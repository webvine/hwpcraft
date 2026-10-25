use anyhow::Result;
use hwpcraft_doc::{
    DocumentCore,
    model::{
        control::{AutoNumber, AutoNumberType, Control},
        paragraph::Paragraph,
    },
};
use hwpcraft_formats::auto_number_formats;
use std::io::{Cursor, Read};

fn source(formats: &[u8]) -> Result<DocumentCore> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut doc = core.document().clone();
    doc.sections[0].paragraphs = formats
        .iter()
        .enumerate()
        .map(|(i, format)| Paragraph {
            text: " 뒤".into(),
            char_offsets: vec![0, 8],
            char_count: 10,
            controls: vec![Control::AutoNumber(AutoNumber {
                number_type: AutoNumberType::Page,
                format: *format,
                number: (i + 2) as u16,
                assigned_number: (i + 2) as u16,
                user_symbol: '★',
                prefix_char: '[',
                suffix_char: ']',
                superscript: i % 2 == 1,
            })],
            ..Default::default()
        })
        .collect();
    core.set_document(doc);
    Ok(core)
}

fn numbers(core: &DocumentCore) -> Vec<AutoNumber> {
    core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| p.controls.iter())
        .filter_map(|c| match c {
            Control::AutoNumber(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn all_public_shapes_survive_native_hwpx_and_hwp_roundtrips() -> Result<()> {
    let formats: Vec<u8> = (0..=14).chain([0x80, 0x81]).collect();
    let core = source(&formats)?;
    let before = numbers(&core);
    let bytes = auto_number_formats::export(&core, None)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes))?;
    let mut section = String::new();
    archive
        .by_name("Contents/section0.xml")?
        .read_to_string(&mut section)?;
    for format in &formats {
        assert!(
            section.contains(&format!(
                "type=\"{}\"",
                auto_number_formats::format_token(*format)?
            )),
            "format {format}"
        );
    }
    let reopened = auto_number_formats::open(&bytes, None)?;
    let after = numbers(&reopened);
    assert_eq!(after.len(), before.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(
            (
                a.format,
                a.number,
                a.superscript,
                a.user_symbol,
                a.prefix_char,
                a.suffix_char
            ),
            (
                b.format,
                b.number,
                b.superscript,
                b.user_symbol,
                b.prefix_char,
                b.suffix_char
            )
        );
    }
    let hwp = reopened
        .export_hwp_with_adapter_snapshot()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let via_hwp = auto_number_formats::open(&hwp, None)?;
    assert_eq!(
        numbers(&via_hwp)
            .iter()
            .map(|n| n.format)
            .collect::<Vec<_>>(),
        formats
    );
    assert_eq!(
        numbers(&core).iter().map(|n| n.number).collect::<Vec<_>>(),
        before.iter().map(|n| n.number).collect::<Vec<_>>(),
        "export mutated source numbers"
    );
    Ok(())
}

#[test]
fn encrypted_hwpx_retains_canonical_number_shapes() -> Result<()> {
    let core = source(&[6, 7, 8, 12, 13, 14, 0x81])?;
    let bytes = auto_number_formats::export(&core, Some(b"fixture-password"))?;
    assert!(auto_number_formats::open(&bytes, None).is_err());
    assert!(auto_number_formats::open(&bytes, Some(b"wrong-password")).is_err());
    let reopened = auto_number_formats::open(&bytes, Some(b"fixture-password"))?;
    assert_eq!(
        numbers(&reopened)
            .iter()
            .map(|n| n.format)
            .collect::<Vec<_>>(),
        [6, 7, 8, 12, 13, 14, 0x81]
    );
    Ok(())
}

#[test]
fn nested_header_and_note_numbers_are_matched_by_identity() -> Result<()> {
    let mut core = source(&[6])?;
    let mut doc = core.document().clone();
    let mut nested = doc.sections[0].paragraphs[0].clone();
    if let Control::AutoNumber(n) = &mut nested.controls[0] {
        n.format = 13;
        n.number = 42;
        n.assigned_number = 42;
    }
    doc.sections[0].paragraphs[0].controls.insert(
        0,
        Control::Header(Box::new(hwpcraft_doc::model::header_footer::Header {
            paragraphs: vec![nested.clone()],
            ..Default::default()
        })),
    );
    if let Control::AutoNumber(n) = &mut nested.controls[0] {
        n.format = 7;
        n.number = 9;
        n.assigned_number = 9;
    }
    doc.sections[0].paragraphs[0]
        .controls
        .push(Control::Footnote(Box::new(
            hwpcraft_doc::model::footnote::Footnote {
                paragraphs: vec![nested],
                ..Default::default()
            },
        )));
    core.set_document(doc);
    let bytes = auto_number_formats::export(&core, None)?;
    let reopened = auto_number_formats::open(&bytes, None)?;
    let p = &reopened.document().sections[0].paragraphs[0];
    let header = p
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Header(h) => Some(h),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("header missing"))?;
    let note = p
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Footnote(n) => Some(n),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("note missing"))?;
    assert!(
        matches!(&header.paragraphs[0].controls[0],Control::AutoNumber(n) if n.format==13 && n.number==42)
    );
    assert!(
        matches!(&note.paragraphs[0].controls[0],Control::AutoNumber(n) if n.format==7 && n.number==9)
    );
    Ok(())
}

#[test]
fn unknown_model_number_shape_cannot_silently_become_digit() -> Result<()> {
    let core = source(&[127])?;
    assert!(auto_number_formats::export(&core, None).is_err());
    assert_eq!(numbers(&core)[0].format, 127);
    Ok(())
}

#[test]
fn nested_table_textbox_and_masterpage_shapes_survive_by_identity() -> Result<()> {
    use hwpcraft_doc::model::{
        header_footer::MasterPage,
        shape::{RectangleShape, ShapeObject, TextBox},
        table::{Cell, Table},
    };
    let mut core = source(&[6])?;
    let template = core.document().sections[0].paragraphs[0].clone();
    let field = |format, number| {
        let mut p = template.clone();
        if let Control::AutoNumber(n) = &mut p.controls[0] {
            n.format = format;
            n.number = number;
            n.assigned_number = number;
        }
        p
    };
    let table = |paragraphs: Vec<Paragraph>| {
        let mut t = Table {
            row_count: 1,
            col_count: 1,
            row_sizes: vec![1],
            cell_grid: vec![Some(0)],
            cells: vec![Cell {
                row_span: 1,
                col_span: 1,
                width: 12000,
                height: 4000,
                paragraphs,
                ..Default::default()
            }],
            ..Default::default()
        };
        t.common.width = 12000;
        t.common.height = 4000;
        t
    };
    let nested = table(vec![field(12, 31)]);
    let mut cell = field(7, 19);
    cell.text = "  ".into();
    cell.char_offsets = vec![0, 8, 16];
    cell.char_count = 17;
    cell.controls.push(Control::Table(Box::new(nested)));
    let mut rect = RectangleShape::default();
    rect.common.width = 16000;
    rect.common.height = 6000;
    rect.drawing.text_box = Some(TextBox {
        paragraphs: vec![field(14, 27)],
        max_width: 16000,
        ..Default::default()
    });
    let mut doc = core.document().clone();
    doc.sections[0].paragraphs = vec![Paragraph {
        text: "  ".into(),
        char_offsets: vec![0, 8, 16],
        char_count: 17,
        controls: vec![
            Control::Table(Box::new(table(vec![cell]))),
            Control::Shape(Box::new(ShapeObject::Rectangle(rect))),
        ],
        ..Default::default()
    }];
    doc.sections[0].section_def.master_pages = vec![MasterPage {
        paragraphs: vec![field(13, 42)],
        ..Default::default()
    }];
    core.set_document(doc);
    let bytes = auto_number_formats::export(&core, None)?;
    let reopened = auto_number_formats::open(&bytes, None)?;
    let doc = reopened.document();
    let p = &doc.sections[0].paragraphs[0];
    let t = p
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("table missing"))?;
    assert!(
        t.cells[0].paragraphs[0]
            .controls
            .iter()
            .any(|c| matches!(c,Control::AutoNumber(n) if n.format==7 && n.number==19))
    );
    let nested = t.cells[0].paragraphs[0]
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("nested table missing"))?;
    assert!(
        nested.cells[0].paragraphs[0]
            .controls
            .iter()
            .any(|c| matches!(c,Control::AutoNumber(n) if n.format==12 && n.number==31))
    );
    let box_p = p
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Shape(s) => s.drawing().and_then(|d| d.text_box.as_ref()),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("textbox missing"))?;
    assert!(
        box_p.paragraphs[0]
            .controls
            .iter()
            .any(|c| matches!(c,Control::AutoNumber(n) if n.format==14 && n.number==27))
    );
    assert!(
        doc.sections[0].section_def.master_pages[0].paragraphs[0]
            .controls
            .iter()
            .any(|c| matches!(c,Control::AutoNumber(n) if n.format==13 && n.number==42))
    );
    Ok(())
}

#[test]
fn native_hwp_first_number_after_structural_gap_is_a_real_control() -> Result<()> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut doc = core.document().clone();
    let p = &mut doc.sections[0].paragraphs[0];
    p.text = " 뒤".into();
    p.char_offsets = vec![16, 24];
    p.char_count = 26;
    p.controls.push(Control::AutoNumber(AutoNumber {
        number_type: AutoNumberType::Page,
        number: 1,
        assigned_number: 1,
        ..Default::default()
    }));
    core.set_document(doc);
    let before = core.document().sections[0].paragraphs[0].clone();
    let prepared = auto_number_formats::prepare_hwp(&core)?
        .ok_or_else(|| anyhow::anyhow!("no prepared snapshot"))?;
    let bytes = prepared
        .prepare_hwp_export_snapshot()
        .serialize()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut compound = cfb::CompoundFile::open(Cursor::new(&bytes))?;
    let mut compressed = Vec::new();
    compound
        .open_stream("/BodyText/Section0")?
        .read_to_end(&mut compressed)?;
    let mut raw = Vec::new();
    flate2::read::DeflateDecoder::new(compressed.as_slice()).read_to_end(&mut raw)?;
    let records = hwpcraft_doc::HwpRecord::read_all(&raw).map_err(|e| anyhow::anyhow!("{e}"))?;
    let text = &records
        .iter()
        .find(|r| r.tag_id == 67)
        .ok_or_else(|| anyhow::anyhow!("no PARA_TEXT"))?
        .data;
    let units: Vec<_> = text
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(
        units[16], 0x12,
        "first body slot must be native AutoNumber, not a literal space"
    );
    assert!(
        !units.contains(&0xfffc),
        "transient placeholder leaked into saved text"
    );
    let reopened = DocumentCore::from_bytes(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    let p = &reopened.document().sections[0].paragraphs[0];
    assert_eq!(p.text, before.text);
    assert_eq!(p.char_offsets[1] - p.char_offsets[0], 8);
    assert_eq!(core.document().sections[0].paragraphs[0].text, before.text);
    assert_eq!(
        core.document().sections[0].paragraphs[0].char_offsets,
        before.char_offsets
    );
    Ok(())
}

#[test]
fn hwpx_number_attribute_preserves_number_instead_of_assigned_value() -> Result<()> {
    let mut core = source(&[0])?;
    if let Control::AutoNumber(n) = &mut core.document_mut().sections[0].paragraphs[0].controls[0] {
        n.number = 0;
        n.assigned_number = 9;
    }
    let bytes = auto_number_formats::export(&core, None)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes))?;
    let mut xml = String::new();
    zip.by_name("Contents/section0.xml")?
        .read_to_string(&mut xml)?;
    assert!(xml.contains("autoNum num=\"0\""));
    assert_eq!(numbers(&core)[0].assigned_number, 9);
    assert_eq!(
        numbers(&auto_number_formats::open(&bytes, None)?)[0].number,
        0
    );
    Ok(())
}
