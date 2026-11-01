//! An automatic number right after a control slot without a character (a bookmark, an
//! inline object, a field start) is written as a number control in an HWP file wherever
//! the paragraph is: body, table cell, header or text box (REVIEW-RHWP-EDIT-0.19 S8).
use anyhow::Result;
use hwpcraft_doc::{
    DocumentCore,
    model::{
        control::{AutoNumber, AutoNumberType, Bookmark, Control},
        paragraph::Paragraph,
    },
};

/// " 뒤" behind a bookmark slot: the number's placeholder space follows a gap.
fn numbered(paragraph: &mut Paragraph) {
    paragraph.text = " 뒤".into();
    paragraph.char_offsets = vec![8, 16];
    paragraph.char_count = 18;
    paragraph.controls = vec![
        Control::Bookmark(Bookmark { name: "앞".into() }),
        Control::AutoNumber(AutoNumber {
            number_type: AutoNumberType::Page,
            number: 1,
            assigned_number: 1,
            ..Default::default()
        }),
    ];
    paragraph.ctrl_data_records = vec![None, None];
}

fn numbers(paragraph: &Paragraph) -> usize {
    paragraph
        .controls
        .iter()
        .filter(|c| matches!(c, Control::AutoNumber(_)))
        .count()
}

fn e(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!("{error}")
}

#[test]
fn a_number_after_a_slot_in_a_cell_or_header_stays_a_number() -> Result<()> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().map_err(e)?;
    core.split_paragraph_native(0, 0, 0, None).map_err(e)?;
    core.create_table_native(0, 1, 0, 1, 1).map_err(e)?;
    core.create_header_footer_native(0, true, 0).map_err(e)?;
    let mut doc = core.document().clone();
    let section = &mut doc.sections[0];
    section.raw_stream = None;
    let mut found = (false, false);
    for para in &mut section.paragraphs {
        for control in &mut para.controls {
            match control {
                Control::Table(t) => {
                    numbered(&mut t.cells[0].paragraphs[0]);
                    found.0 = true;
                }
                Control::Header(h) => {
                    numbered(&mut h.paragraphs[0]);
                    found.1 = true;
                }
                _ => {}
            }
        }
    }
    assert_eq!(found, (true, true));
    core.set_document(doc);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("nested-number.hwp");
    hwpcraft_formats::save(&core, &path)?;
    let back = hwpcraft_formats::open(&path)?;
    let mut seen = Vec::new();
    for para in &back.document().sections[0].paragraphs {
        for control in &para.controls {
            match control {
                Control::Table(t) => {
                    let p = &t.cells[0].paragraphs[0];
                    seen.push(("cell", numbers(p), p.text.clone()));
                }
                Control::Header(h) => {
                    let p = &h.paragraphs[0];
                    seen.push(("header", numbers(p), p.text.clone()));
                }
                _ => {}
            }
        }
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            ("cell", 1, " 뒤".to_owned()),
            ("header", 1, " 뒤".to_owned())
        ]
    );
    Ok(())
}
