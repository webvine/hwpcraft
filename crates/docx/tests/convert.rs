//! Word ↔ HWP conversion: what a Word document brings into the HWP model, and what an
//! HWP document keeps through DOCX and back.
use anyhow::{Context, Result};
use hwpcraft_doc::DocumentCore;
use hwpcraft_doc::model::control::{AutoNumberType, Control};
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_doc::model::style::{Alignment, HeadType};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wordcraft_doc as wc;
use wordcraft_doc::props::{Border, BorderStyle, Borders, CellProps, LineSpacing, VMerge};
use wordcraft_doc::{Block, CharProps, InlineObject, Part, PartKind, Rgb, TextColor};

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../rhwp/samples").join(name)
}

fn para(text: &str, props: CharProps) -> Arc<Block> {
    wc::para_block(wc::Paragraph::with_text(text, props))
}

/// A Word document of `blocks`, written as `.docx` and opened as HWP.
fn import(doc: &wc::Document) -> Result<(DocumentCore, Vec<String>)> {
    let bytes = wordcraft_docx::write(doc).map_err(|e| anyhow::anyhow!("{e}"))?;
    let imported = hwpcraft_docx::import(&bytes, None)?;
    Ok((imported.core, imported.omitted))
}

fn body(core: &DocumentCore) -> &[Paragraph] {
    &core.document().sections[0].paragraphs
}

fn texts(paragraphs: &[Paragraph], out: &mut Vec<String>) {
    for p in paragraphs {
        let t: String = p.text.chars().filter(|c| !c.is_control()).collect();
        if !t.trim().is_empty() {
            out.push(t.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        for c in &p.controls {
            if let Control::Table(t) = c {
                for cell in &t.cells {
                    texts(&cell.paragraphs, out);
                }
            }
        }
    }
}

#[test]
fn runs_keep_their_formatting_and_paragraphs_theirs() -> Result<()> {
    let mut doc = wc::Document::new();
    let bold = CharProps {
        bold: Some(true),
        size: Some(14.0),
        color: Some(TextColor::Rgb(Rgb(0xC0, 0x10, 0x20))),
        font: Some("Arial".into()),
        font_ea: Some("맑은 고딕".into()),
        ..Default::default()
    };
    let mut title = wc::Paragraph::with_text("보고서 제목", bold);
    title.props.align = Some(wc::Align::Center);
    title.props.space_after = Some(12.0);
    let mut body_para = wc::Paragraph::with_text("본문 첫 줄", CharProps::default());
    body_para.props.indent_left = Some(36.0);
    body_para.props.indent_first = Some(-18.0);
    body_para.props.line_spacing = Some(LineSpacing::Exactly(20.0));
    doc.body = vec![wc::para_block(title), wc::para_block(body_para)];
    let (core, _) = import(&doc)?;
    let info = &core.document().doc_info;
    let paras = body(&core);
    assert_eq!(paras[0].text, "보고서 제목");
    let cs = &info.char_shapes[paras[0].char_shapes[0].char_shape_id as usize];
    assert!(cs.bold);
    assert_eq!(cs.base_size, 1400);
    assert_eq!(cs.text_color & 0x00FF_FFFF, 0x0020_10C0, "COLORREF is BGR");
    let ps = &info.para_shapes[usize::from(paras[0].para_shape_id)];
    assert_eq!(ps.alignment, Alignment::Center);
    assert_eq!(ps.spacing_after, 12 * 200, "paragraph shapes keep twice HWPUNIT");
    // Word hangs the first line 18 pt left of a 36 pt indent: HWP's 내어쓰기 from 18 pt.
    let ps = &info.para_shapes[usize::from(paras[1].para_shape_id)];
    assert_eq!((ps.margin_left, ps.indent), (18 * 200, -18 * 200));
    assert_eq!(ps.line_spacing, 20 * 200);
    Ok(())
}

#[test]
fn tables_keep_merged_cells_borders_and_fills() -> Result<()> {
    let mut doc = wc::Document::new();
    let line = Some(Border {
        style: BorderStyle::Single,
        width: 0.5,
        color: Some(Rgb(0, 0, 0)),
        space: 0.0,
    });
    let cell = |text: &str, span: u32, vmerge: VMerge, fill: Option<Rgb>| wc::Cell {
        props: CellProps {
            span,
            vmerge,
            shading: fill,
            ..Default::default()
        },
        blocks: vec![para(text, CharProps::default())],
        ..Default::default()
    };
    let rows = vec![
        wc::Row {
            cells: vec![cell("머리", 2, VMerge::None, Some(Rgb(0xDD, 0xEE, 0xFF))), cell("세로", 1, VMerge::Restart, None)],
            ..Default::default()
        },
        wc::Row {
            cells: vec![cell("가", 1, VMerge::None, None), cell("나", 1, VMerge::None, None), cell("", 1, VMerge::Continue, None)],
            ..Default::default()
        },
    ];
    let table = wc::Table {
        props: wc::props::TableProps {
            borders: Some(Borders {
                top: line,
                left: line,
                bottom: line,
                right: line,
                between: line,
                inside_v: line,
            }),
            ..Default::default()
        },
        grid: vec![100.0, 100.0, 100.0],
        rows,
        ..Default::default()
    };
    doc.body = vec![para("앞", CharProps::default()), Arc::new(Block::Table(table)), para("뒤", CharProps::default())];
    let (core, _) = import(&doc)?;
    let paras = body(&core);
    let table = paras
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .context("table")?;
    assert_eq!((table.row_count, table.col_count), (2, 3));
    assert_eq!(table.cells.len(), 4, "the merged cells are one cell each");
    let head = table.cells.iter().find(|c| c.row == 0 && c.col == 0).context("head")?;
    assert_eq!(head.col_span, 2);
    assert_eq!(head.width, 20_000, "two 100 pt columns");
    let tall = table.cells.iter().find(|c| c.row == 0 && c.col == 2).context("tall")?;
    assert_eq!(tall.row_span, 2);
    let fill = &core.document().doc_info.border_fills[usize::from(head.border_fill_id) - 1];
    assert_eq!(fill.fill.solid.map(|s| s.background_color & 0x00FF_FFFF), Some(0x00FF_EEDD));
    assert!(fill.borders.iter().all(|b| b.line_type != hwpcraft_doc::model::style::BorderLineType::None));
    // A Word table holds its own place in the text, so it breaks across pages as Word's.
    assert!(!table.common.treat_as_char);
    let mut all = Vec::new();
    texts(paras, &mut all);
    assert_eq!(all, ["앞", "머리", "세로", "가", "나", "뒤"]);
    Ok(())
}

#[test]
fn headers_page_numbers_and_notes_become_hwp_controls() -> Result<()> {
    let mut doc = wc::Document::new();
    let mut header = wc::Paragraph::new();
    header.text.push(wc::para::OBJ);
    header.runs.push(wc::Run {
        len: wc::para::OBJ.len_utf8(),
        props: CharProps::default(),
    });
    header.objects.push(InlineObject::Field {
        instr: " PAGE ".into(),
        result: "1".into(),
        locked: false,
        dirty: false,
        code: false,
        form: String::new(),
    });
    doc.parts.insert(
        1,
        Part {
            kind: PartKind::Header,
            blocks: vec![wc::para_block(header)],
            ..Default::default()
        },
    );
    doc.last_section.headers.default = Some(1);
    doc.parts.insert(
        2,
        Part {
            kind: PartKind::Footnote,
            blocks: vec![para("각주 내용", CharProps::default())],
            ..Default::default()
        },
    );
    let mut text = wc::Paragraph::with_text("본문", CharProps::default());
    text.text.push(wc::para::OBJ);
    text.runs.push(wc::Run {
        len: wc::para::OBJ.len_utf8(),
        props: CharProps::default(),
    });
    text.objects.push(InlineObject::NoteRef {
        kind: wc::para::NoteKind::Footnote,
        id: 2,
        custom: String::new(),
    });
    doc.body = vec![wc::para_block(text)];
    let (core, _) = import(&doc)?;
    let controls: Vec<&Control> = body(&core).iter().flat_map(|p| &p.controls).collect();
    let header = controls
        .iter()
        .find_map(|c| match c {
            Control::Header(h) => Some(h),
            _ => None,
        })
        .context("header")?;
    assert!(header.paragraphs.iter().flat_map(|p| &p.controls).any(|c| matches!(c, Control::AutoNumber(n) if n.number_type == AutoNumberType::Page)));
    let note = controls
        .iter()
        .find_map(|c| match c {
            Control::Footnote(n) => Some(n),
            _ => None,
        })
        .context("footnote")?;
    assert!(note.paragraphs.iter().any(|p| p.text.contains("각주 내용")));
    Ok(())
}

#[test]
fn pictures_and_lists_come_through() -> Result<()> {
    let mut doc = wc::Document::new();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 200, 30, 255])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let media = doc.add_media(png, "png");
    let mut pic = wc::Paragraph::new();
    pic.text.push(wc::para::OBJ);
    pic.runs.push(wc::Run {
        len: wc::para::OBJ.len_utf8(),
        props: CharProps::default(),
    });
    pic.objects.push(InlineObject::Image {
        media,
        w: 80.0,
        h: 40.0,
        alt: "초록 상자".into(),
        float: Default::default(),
        crop: [0.0; 4],
        ole: None,
    });
    let numbering = Arc::make_mut(&mut doc.numbering);
    numbering.abstracts.push(wc::numbering::AbstractNum {
        id: 0,
        levels: vec![wc::numbering::Level::default()],
        ..Default::default()
    });
    numbering.nums.push(wc::numbering::Num {
        id: 1,
        abstract_id: 0,
        ..Default::default()
    });
    let mut item = wc::Paragraph::with_text("첫째 항목", CharProps::default());
    item.props.numbering = Some(wc::props::NumRef { num: 1, level: 0 });
    doc.body = vec![wc::para_block(pic), wc::para_block(item)];
    let (core, _) = import(&doc)?;
    let paras = body(&core);
    let picture = paras
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Picture(p) => Some(p),
            _ => None,
        })
        .context("picture")?;
    assert!(picture.common.treat_as_char);
    assert_eq!((picture.common.width, picture.common.height), (8000, 4000));
    assert!(!core.document().bin_data_content.is_empty());
    let item = paras.iter().find(|p| p.text == "첫째 항목").context("item")?;
    let ps = &core.document().doc_info.para_shapes[usize::from(item.para_shape_id)];
    assert_eq!(ps.head_type, HeadType::Number);
    // Word's level: text at 36 pt, number hanging 18 pt.
    assert_eq!((ps.margin_left, ps.indent), (18 * 200, -18 * 200));
    Ok(())
}

/// A rule drawn as a thin table row (an empty paragraph with an exact 1 pt line in a 6 pt
/// row) stays that thin in HWP, and a header picture placed from the page corner lands
/// where HWP draws a header's objects: in the header area, offset from its corner.
#[test]
fn thin_rule_rows_stay_thin_and_header_pictures_keep_their_place() -> Result<()> {
    use wordcraft_doc::para::{Anchor, Float, Wrap};
    let mut doc = wc::Document::new();
    let section = &mut doc.last_section;
    section.page_w = 595.3;
    section.page_h = 841.9;
    (section.margin_left, section.margin_right) = (56.7, 56.7);
    (section.margin_top, section.margin_bottom) = (85.0, 85.0);
    (section.header, section.footer) = (42.5, 42.5);
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        4,
        image::Rgba([10, 20, 200, 255]),
    ))
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let media = doc.add_media(png, "png");
    let mut logo = wc::Paragraph::new();
    logo.text.push(wc::para::OBJ);
    logo.runs.push(wc::Run {
        len: wc::para::OBJ.len_utf8(),
        props: CharProps::default(),
    });
    logo.objects.push(InlineObject::Image {
        media,
        w: 82.0,
        h: 33.8,
        alt: "로고".into(),
        float: Float {
            wrap: Wrap::Square,
            h_rel: Anchor::Page,
            v_rel: Anchor::Page,
            x: 456.6,
            y: 41.7,
            ..Default::default()
        },
        crop: [0.0; 4],
        ole: None,
    });
    doc.parts.insert(
        1,
        Part {
            kind: PartKind::Header,
            blocks: vec![wc::para_block(logo)],
            ..Default::default()
        },
    );
    doc.last_section.headers.default = Some(1);
    let mut rule = wc::Paragraph::new();
    rule.props.line_spacing = Some(LineSpacing::Exactly(1.0));
    let table = wc::Table {
        grid: vec![400.0],
        rows: vec![wc::Row {
            props: wordcraft_doc::props::RowProps {
                height: Some(6.0),
                height_rule: wordcraft_doc::props::HeightRule::AtLeast,
                ..Default::default()
            },
            cells: vec![wc::Cell {
                blocks: vec![wc::para_block(rule)],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    doc.body = vec![
        Arc::new(Block::Table(table)),
        para("본문", CharProps::default()),
    ];
    let (core, _) = import(&doc)?;
    let paras = body(&core);
    let table = paras
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .context("table")?;
    let cell = &table.cells[0];
    assert!(
        cell.height <= 700,
        "a 6 pt row, not a line of text: {}",
        cell.height
    );
    let shape = cell.paragraphs[0].char_shape_id_at(0).unwrap_or(0) as usize;
    let size = core.document().doc_info.char_shapes[shape].base_size;
    assert!(
        size <= 100,
        "the empty line's mark fits its 1 pt line: {size}"
    );
    let header = paras
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Header(h) => Some(h),
            _ => None,
        })
        .context("header")?;
    let picture = header
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Picture(p) => Some(p),
            _ => None,
        })
        .context("header picture")?;
    let x = picture.common.horizontal_offset as i32;
    let y = picture.common.vertical_offset as i32;
    assert!((x - 39_990).abs() <= 10, "from the left margin: {x}");
    assert!((y + 80).abs() <= 10, "from the header's top: {y}");
    Ok(())
}

#[test]
fn damaged_and_encrypted_packages_are_refused() -> Result<()> {
    assert!(hwpcraft_docx::import(b"not a zip", None).is_err());
    let mut doc = wc::Document::new();
    doc.body = vec![para("비밀", CharProps::default())];
    let plain = wordcraft_docx::write(&doc).map_err(|e| anyhow::anyhow!("{e}"))?;
    let locked = wordcraft_docx::encrypt(&plain, "pw").map_err(|e| anyhow::anyhow!("{e}"))?;
    let refused = hwpcraft_docx::import(&locked, None).err().context("needs a password")?;
    assert!(format!("{refused:#}").contains("비밀번호가 필요한 암호 문서"), "{refused:#}");
    let wrong = hwpcraft_docx::import(&locked, Some("nope")).err().context("wrong password")?;
    assert!(format!("{wrong:#}").contains("비밀번호가 일치하지 않거나"), "{wrong:#}");
    let opened = hwpcraft_docx::import(&locked, Some("pw"))?;
    assert_eq!(body(&opened.core)[0].text, "비밀");
    Ok(())
}

#[test]
fn an_hwp_report_keeps_its_text_tables_and_pictures_in_word() -> Result<()> {
    let path = sample("156457624_210622 7월부터 해외직구 구매대행업체 등록제 시행.hwp");
    let core = hwpcraft_formats::open(&path)?;
    let exported = hwpcraft_docx::export(&core)?;
    let word = wordcraft_docx::read(&exported.bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    let tables = word.body.iter().filter(|b| matches!(&***b, Block::Table(_))).count();
    let hwp_tables = body(&core).iter().flat_map(|p| &p.controls).filter(|c| matches!(c, Control::Table(_))).count();
    assert_eq!(tables, hwp_tables);
    assert_eq!(word.media.len(), 2, "both pictures");
    let text: String = word.body.iter().filter_map(|b| b.as_para()).map(|p| p.plain_text()).collect();
    let all: String = word
        .body
        .iter()
        .filter_map(|b| b.as_table())
        .flat_map(|t| &t.rows)
        .flat_map(|r| &r.cells)
        .flat_map(|c| &c.blocks)
        .filter_map(|b| b.as_para())
        .map(|p| p.plain_text())
        .collect();
    assert!(all.contains("관세청"), "table text");
    let _ = text;
    // Page size and margins: HWP's top margin is to the header, Word's to the body.
    let page = &core.document().sections[0].section_def.page_def;
    assert!((word.last_section.page_w - page.width as f32 / 100.0).abs() < 0.5);
    let top = (page.margin_top + page.margin_header) as f32 / 100.0;
    assert!((word.last_section.margin_top - top).abs() < 0.5);
    Ok(())
}

#[test]
fn hwp_to_word_and_back_keeps_every_line_of_text() -> Result<()> {
    for name in ["156457624_210622 7월부터 해외직구 구매대행업체 등록제 시행.hwp", "table-vpos-01.hwp"] {
        let core = hwpcraft_formats::open(&sample(name))?;
        let exported = hwpcraft_docx::export(&core)?;
        let back = hwpcraft_docx::import(&exported.bytes, None)?;
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for s in &core.document().sections {
            texts(&s.paragraphs, &mut before);
        }
        for s in &back.core.document().sections {
            texts(&s.paragraphs, &mut after);
        }
        before.sort();
        after.sort();
        assert_eq!(before, after, "{name}");
    }
    Ok(())
}
