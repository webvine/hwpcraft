//! Suspected defects from the RHWP edit review (REVIEW-RHWP-EDIT-0.19 S1–S14, L10) and
//! connectors attached to paragraph-relative objects: each test reproduces one case.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::shape::{LineShape, ShapeObject};
use hwpcraft_engine::{Command, Cursor, ObjectAddress, Session};
use serde_json::{Value, json};

/// Full text of every body paragraph (`Session::entries` shortens long ones).
fn texts(s: &Session) -> Vec<String> {
    s.core
        .document()
        .sections
        .iter()
        .flat_map(|sec| sec.paragraphs.iter().map(|p| p.text.clone()))
        .collect()
}

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

fn address(v: &Value) -> anyhow::Result<ObjectAddress> {
    Ok(serde_json::from_value(v["address"].clone())?)
}

fn line(s: &Session, a: ObjectAddress) -> anyhow::Result<LineShape> {
    match &s.core.document().sections[a.section].paragraphs[a.paragraph].controls[a.control] {
        Control::Shape(shape) => match shape.as_ref() {
            ShapeObject::Line(l) => Ok(l.clone()),
            _ => anyhow::bail!("not a line"),
        },
        _ => anyhow::bail!("not a shape"),
    }
}

/// Page box (HWPUNIT) of an object as the layout draws it.
fn drawn(s: &Session, a: ObjectAddress) -> anyhow::Result<[i32; 4]> {
    let page = s.page_of_object(a)?;
    let layout: Value = serde_json::from_str(&s.page_control_layout(page)?)?;
    let c = layout["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| {
            c["paraIdx"] == a.paragraph
                && c["controlIdx"] == a.control
                && c.get("cellIdx").is_none()
        })
        .ok_or_else(|| anyhow::anyhow!("{a:?} not on page {page}"))?;
    let hu = |k: &str| (c[k].as_f64().unwrap_or(0.0) * 75.0).round() as i32;
    Ok([hu("x"), hu("y"), hu("w"), hu("h")])
}

/// Paper start and end points of a connector.
fn ends(l: &LineShape) -> [(i32, i32); 2] {
    let (x, y) = (
        l.common.horizontal_offset as i32,
        l.common.vertical_offset as i32,
    );
    [(x + l.start.x, y + l.start.y), (x + l.end.x, y + l.end.y)]
}

fn near(a: (i32, i32), b: (i32, i32)) -> bool {
    // The layout reports 0.1 px (7.5 HWPUNIT).
    (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8
}

/// S14: 라벨 문서 만들기 replaced the open document before its page setup could fail, so
/// a refused label size left a blank document instead of the user's one.
#[test]
fn a_refused_label_sheet_keeps_the_open_document() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("kept.hwpx");
    let mut s = Session::new()?;
    s.execute(Command::Insert("지킬 문서".into()))?;
    s.save(&path)?;
    let generation = s.generation;
    // A 5 mm single label passes the label checks but leaves a body under 10 mm.
    let refused = s.run(
        "page.label",
        json!({"rows": 1, "columns": 1, "labelWidthMm": 5.0, "labelHeightMm": 5.0}),
    );
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(texts(&s), ["지킬 문서"]);
    assert_eq!(s.path.as_deref(), Some(path.as_path()));
    assert!(!s.dirty());
    assert_eq!(s.generation, generation);
    // A good label sheet still replaces the document as before.
    let v = s.run(
        "page.label",
        json!({"rows": 2, "columns": 2, "labelWidthMm": 60.0, "labelHeightMm": 30.0}),
    )?;
    assert_eq!(v["labels"], 4);
    assert!(s.path.is_none() && !s.dirty());
    Ok(())
}

/// S9: 쪽 붙이기 after a page whose last paragraph runs on to the next page put the copy
/// in front of that paragraph, pushing the end of the current page behind the copy.
#[test]
fn a_page_pasted_after_a_running_paragraph_follows_the_page_text() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 문단".into()))?;
    s.execute(Command::Split)?;
    let long: String = (0..900).map(|i| format!("낱말{i} ")).collect();
    s.execute(Command::Insert(long.clone()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("붙일 쪽".into()))?;
    let pages = s.page_count()?;
    assert!(pages >= 3, "{pages}");
    // The first page ends inside the long paragraph.
    let second = s.page_first_position(1).expect("page 2");
    assert_eq!(
        (second.paragraph, second.offset > 0),
        (1, true),
        "{second:?}"
    );
    let page_one_text = format!(
        "첫 문단{}",
        long.chars().take(second.offset).collect::<String>()
    );
    s.run("page.copy", json!({"page": pages}))?;
    let r = s.run("page.paste", json!({"page": 1}))?;
    assert_eq!(r["afterPage"], 1, "{r}");
    let after = texts(&s);
    let pasted = after
        .iter()
        .position(|t| t == "붙일 쪽")
        .expect("pasted paragraph");
    assert_eq!(
        after[..pasted].concat(),
        page_one_text,
        "the first page's text comes before the pasted page"
    );
    // The rest of the long paragraph follows the copy; no text is lost.
    let rest: String = long.chars().skip(second.offset).collect();
    assert_eq!(after[pasted + 1], rest);
    assert_eq!(after.last().map(String::as_str), Some("붙일 쪽"));
    assert_eq!(s.page_of_object_text_start(pasted)?, 1);
    s.execute(Command::Undo)?;
    assert_eq!(
        texts(&s),
        ["첫 문단".to_owned(), long, "붙일 쪽".to_owned()]
    );
    Ok(())
}

trait PageOfParagraph {
    fn page_of_object_text_start(&self, paragraph: usize) -> anyhow::Result<u64>;
}

impl PageOfParagraph for Session {
    /// Zero-based page that shows the start of a body paragraph.
    fn page_of_object_text_start(&self, paragraph: usize) -> anyhow::Result<u64> {
        Ok(self.caret_at(at(paragraph, 0))?["pageIndex"]
            .as_u64()
            .unwrap_or(u64::MAX))
    }
}

/// Two paragraph-relative shapes joined by a connector, in a document with a page
/// number field (so positions come from the page-field layout).
fn connected_paragraph_shapes(s: &mut Session) -> anyhow::Result<[ObjectAddress; 3]> {
    s.run("insert.auto_text", json!({"kind": "page_number"}))?;
    s.execute(Command::Insert(" 쪽\n\n문단 기준 개체\n".into()))?;
    let shape = |s: &mut Session, x: i64| -> anyhow::Result<ObjectAddress> {
        let a = address(&s.run(
            "shape.insert",
            json!({"kind":"rectangle","width_mm":30.0,"height_mm":20.0,"x_mm":90.0,"y_mm":120.0}),
        )?)?;
        s.run(
            "object.properties",
            json!({"address": a, "props": {"vertRelTo": "Para", "horzRelTo": "Column", "vertOffset": 1000, "horzOffset": x}}),
        )?;
        Ok(a)
    };
    let left = shape(s, 2000)?;
    let right = shape(s, 30000)?;
    let conn = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight","width_mm":1.0,"height_mm":1.0,
            "start":{"address":left,"point":1},"end":{"address":right,"point":3}}),
    )?)?;
    Ok([left, right, conn])
}

fn assert_connected(
    s: &Session,
    [left, right, conn]: [ObjectAddress; 3],
    label: &str,
) -> anyhow::Result<()> {
    let [lx, ly, lw, lh] = drawn(s, left)?;
    let [rx, ry, _, rh] = drawn(s, right)?;
    let got = ends(&line(s, conn)?);
    assert!(
        near(got[0], (lx + lw, ly + lh / 2)),
        "{label} start {got:?} left {:?}",
        (lx + lw, ly + lh / 2)
    );
    assert!(
        near(got[1], (rx, ry + rh / 2)),
        "{label} end {got:?} right {:?}",
        (rx, ry + rh / 2)
    );
    Ok(())
}

/// L10: moving several objects in one step routed each connector from the layout read
/// before the later objects moved.
#[test]
fn objects_moved_together_route_their_connector_from_the_new_layout() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let objects = connected_paragraph_shapes(&mut s)?;
    assert_connected(&s, objects, "made")?;
    let [left, right, _] = objects;
    s.run(
        "object.move_by",
        json!({"addresses": [left, right], "dx_mm": 10.0, "dy_mm": 15.0}),
    )?;
    assert_connected(&s, objects, "moved together")?;
    s.run(
        "object.properties",
        json!({"address": right, "props": {"vertOffset": 6000}}),
    )?;
    assert_connected(&s, objects, "one moved")?;
    // Both move to the middle of the pair.
    let v = s.run(
        "object.align",
        json!({"targets": [left, right], "mode": "middle"}),
    )?;
    assert_eq!(v["moved"], 2, "{v}");
    assert_connected(&s, objects, "aligned")?;
    Ok(())
}

/// L9: a paragraph-relative shape moves with its paragraph when text is typed above it;
/// the connector attached to it has to follow.
#[test]
fn a_connector_follows_a_paragraph_object_moved_by_text() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let objects = connected_paragraph_shapes(&mut s)?;
    let before = drawn(&s, objects[0])?;
    s.select(at(0, 0), false)?;
    s.execute(Command::Insert("위에 넣은 줄\n둘째 줄\n".into()))?;
    let moved = objects.map(|a| ObjectAddress {
        paragraph: a.paragraph + 2,
        ..a
    });
    assert_ne!(drawn(&s, moved[0])?[1], before[1], "the shape moved down");
    assert_connected(&s, moved, "typed above")?;
    s.execute(Command::Undo)?;
    assert_connected(&s, objects, "undone")?;
    Ok(())
}

/// Line rows (line height, count) of the first header paragraph and the first text box
/// paragraph of section 0.
fn header_and_text_box_rows(s: &Session) -> [(i32, usize); 2] {
    let mut header = (0, 0);
    let mut text_box = (0, 0);
    for para in &s.core.document().sections[0].paragraphs {
        for control in &para.controls {
            match control {
                Control::Header(h) => {
                    if let Some(p) = h.paragraphs.first() {
                        header = (
                            p.line_segs.first().map_or(0, |l| l.line_height),
                            p.line_segs.len(),
                        );
                    }
                }
                Control::Shape(shape) => {
                    if let Some(p) = shape
                        .drawing()
                        .and_then(|d| d.text_box.as_ref())
                        .and_then(|t| t.paragraphs.first())
                    {
                        text_box = (
                            p.line_segs.first().map_or(0, |l| l.line_height),
                            p.line_segs.len(),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    [header, text_box]
}

/// S1: editing a style re-broke the lines of body paragraphs and first-level cells only;
/// a header or text box paragraph in the style kept its old line rows.
#[test]
fn a_style_edit_rebreaks_header_and_text_box_lines() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문"}))?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리말 글"}),
    )?;
    s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 40.0, "height_mm": 30.0,
            "paragraphs": ["글상자 글 글상자 글 글상자 글"]}),
    )?;
    s.run("format.style.create", json!({"name": "검토 문단"}))?;
    let id = s.core.document().doc_info.styles.len() - 1;
    for para in &mut s.core.document_mut().sections[0].paragraphs {
        for control in &mut para.controls {
            match control {
                Control::Header(h) => h.paragraphs.iter_mut().for_each(|p| p.style_id = id as u8),
                Control::Shape(shape) => {
                    if let Some(tb) = shape.drawing_mut().and_then(|d| d.text_box.as_mut()) {
                        tb.paragraphs.iter_mut().for_each(|p| p.style_id = id as u8);
                    }
                }
                _ => {}
            }
        }
    }
    s.core.document_mut().sections[0].raw_stream = None;
    let [header, text_box] = header_and_text_box_rows(&s);
    assert!(header.0 > 0 && text_box.0 > 0, "{header:?} {text_box:?}");
    s.run(
        "format.style.update",
        json!({"id": id, "character": {"fontSize": 3000}}),
    )?;
    let [h, t] = header_and_text_box_rows(&s);
    assert!(h.0 >= header.0 * 2, "header rows {header:?} -> {h:?}");
    assert!(
        t.0 >= text_box.0 * 2 && t.1 > text_box.1,
        "text box rows {text_box:?} -> {t:?}"
    );
    // Deleting the style gives them 바탕글's shapes and rows again.
    s.run("format.style.delete", json!({"id": id, "replacement": 0}))?;
    assert_eq!(header_and_text_box_rows(&s), [header, text_box]);
    Ok(())
}

/// (top, bottom) of the first and the last caret of each body paragraph on page 0.
fn paragraph_spans(s: &Session) -> anyhow::Result<Vec<(f64, f64)>> {
    let paragraphs = &s.core.document().sections[0].paragraphs;
    (0..paragraphs.len())
        .map(|p| {
            let len = paragraphs[p].text.chars().count();
            let first = s.caret_at(at(p, 0))?;
            let last = s.caret_at(at(p, len))?;
            let y = |v: &Value, k: &str| v[k].as_f64().unwrap_or(f64::NAN);
            Ok((y(&first, "y"), y(&last, "y") + y(&last, "height")))
        })
        .collect()
}

/// S2: moving a paragraph swapped the paragraphs with their stored rows, whose
/// vertical positions belong to the old order.
#[test]
fn a_moved_paragraph_is_laid_out_in_its_new_place() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let long: String = (0..60).map(|i| format!("긴 문단 {i} ")).collect();
    s.execute(Command::Insert(format!(
        "첫 문단\n{long}\n둘째 짧은 문단\n끝"
    )))?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("moved.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        r.select(at(2, 0), false)?;
        r.run("format.paragraph.up", json!({}))?;
        assert_eq!(
            r.core.document().sections[0].paragraphs[1].text,
            "둘째 짧은 문단"
        );
        let spans = paragraph_spans(&r)?;
        for pair in spans.windows(2) {
            assert!(
                pair[1].0 >= pair[0].1 - 0.5,
                "{ext}: paragraphs overlap {spans:?}"
            );
        }
        // The same as the moved document laid out afresh.
        let again = dir.path().join(format!("again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        assert_eq!(paragraph_spans(&fresh)?, spans, "{ext}");
    }
    Ok(())
}

fn first_table(s: &Session) -> Option<(usize, usize)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Table(_)))
                .map(|c| (p, c))
        })
}

fn in_cell(table: (usize, usize), paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph: table.0,
        offset,
        cell: Some(hwpcraft_engine::Cell {
            control: table.1,
            index: 0,
            paragraph,
        }),
    }
}

/// S2 in a table cell: paragraphs of the first cell keep a cell-relative layout.
#[test]
fn a_moved_cell_paragraph_is_laid_out_in_its_new_place() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("표 앞".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Table {
        rows: 1,
        columns: 2,
    })?;
    let table = first_table(&s).expect("table");
    s.select(in_cell(table, 0, 0), false)?;
    let long: String = (0..12).map(|i| format!("셀 문단 {i} ")).collect();
    s.execute(Command::Insert(format!("{long}\n짧은 셀 문단\n셀 끝")))?;
    let dir = tempfile::tempdir()?;
    let spans = |s: &Session| -> anyhow::Result<Vec<(f64, f64)>> {
        let t = first_table(s).expect("table");
        let Control::Table(tb) = &s.core.document().sections[0].paragraphs[t.0].controls[t.1]
        else {
            anyhow::bail!("no table");
        };
        (0..tb.cells[0].paragraphs.len())
            .map(|k| {
                let len = tb.cells[0].paragraphs[k].text.chars().count();
                let first = s.caret_at(in_cell(t, k, 0))?;
                let last = s.caret_at(in_cell(t, k, len))?;
                let y = |v: &Value, k: &str| v[k].as_f64().unwrap_or(f64::NAN);
                Ok((y(&first, "y"), y(&last, "y") + y(&last, "height")))
            })
            .collect()
    };
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("cell.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let t = first_table(&r).expect("table");
        r.select(in_cell(t, 1, 0), false)?;
        r.run("format.paragraph.up", json!({}))?;
        let got = spans(&r)?;
        for pair in got.windows(2) {
            assert!(pair[1].0 >= pair[0].1 - 0.5, "{ext}: overlap {got:?}");
        }
        let again = dir.path().join(format!("cell-again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        assert_eq!(spans(&fresh)?, got, "{ext}");
    }
    Ok(())
}

/// Drawn rows (text runs grouped by baseline) of page 0 as (y, text).
fn drawn_rows(s: &Session) -> anyhow::Result<Vec<(f64, String)>> {
    let svg = s.render_page_svg(0)?;
    let mut rows: Vec<(f64, String)> = Vec::new();
    for part in svg.split("<text").skip(1) {
        let Some(y) = part
            .split(" y=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .and_then(|v| v.parse::<f64>().ok())
        else {
            continue;
        };
        let content = part
            .split_once('>')
            .and_then(|(_, rest)| rest.split_once("</text>"))
            .map_or("", |(text, _)| text);
        match rows.iter_mut().find(|(ry, _)| (*ry - y).abs() < 0.5) {
            Some((_, row)) => row.push_str(content),
            None => rows.push((y, content.to_owned())),
        }
    }
    Ok(rows)
}

/// Drawn caption rows of a 40 mm picture whose caption is set to `text`, after the
/// caption first held "매출 비교" on two rows stored by the file (`stored`) or on none.
fn caption_drawn(
    dir: &std::path::Path,
    stored: bool,
    text: &str,
) -> anyhow::Result<Vec<(f64, String)>> {
    let png = dir.join("p.png");
    image::RgbImage::from_pixel(48, 30, image::Rgb([68, 114, 196])).save(&png)?;
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "앞 문단
"
        .into(),
    ))?;
    let made = s.run(
        "picture.insert",
        json!({"path": png, "width_mm": 40.0, "treat_as_char": true}),
    )?;
    let a = address(&made)?;
    s.run(
        "object.properties",
        json!({"address": a, "props": {"hasCaption": true}}),
    )?;
    s.run(
        "object.caption_text",
        json!({"address": a, "text": "매출 비교"}),
    )?;
    if stored {
        // A caption read from a Hangul file carries the rows Hangul stored for its text:
        // "그림 1 " and "매출 비교".
        let doc = s.core.document_mut();
        let mut row = doc.sections[0].paragraphs[0].line_segs[0].clone();
        row.tag &= !hwpcraft_doc::model::paragraph::LineSeg::TAG_IMPLEMENTATION_PROPERTY;
        row.text_start = 0;
        row.vertical_pos = 0;
        let mut second = row.clone();
        second.text_start = 12;
        second.vertical_pos = row.line_height + row.line_spacing;
        if let Control::Picture(p) =
            &mut doc.sections[a.section].paragraphs[a.paragraph].controls[a.control]
            && let Some(cap) = p.caption.as_mut()
        {
            cap.paragraphs[0].line_segs = vec![row, second];
        }
        doc.sections[a.section].raw_stream = None;
        s.core.set_document(s.core.document().clone());
    }
    s.run("object.caption_text", json!({"address": a, "text": text}))?;
    Ok(drawn_rows(&s)?
        .into_iter()
        .filter(|(_, t)| !t.contains("앞문단"))
        .collect())
}

/// S3: replacing a caption's text kept the rows the file stored for the old text, so
/// the new text was broken where the old one was.
#[test]
fn a_replaced_caption_text_drops_the_stored_line_breaks() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for text in ["분기별 매출", "분기별 매출 추이와 지역별 비중 비교"] {
        let fresh = caption_drawn(dir.path(), false, text)?;
        let stored = caption_drawn(dir.path(), true, text)?;
        // The same line breaks (a stored row's own line metrics may shift the baseline).
        let lines =
            |rows: &[(f64, String)]| rows.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>();
        assert_eq!(lines(&stored), lines(&fresh), "{text}");
    }
    Ok(())
}

/// Rows of every body paragraph: (count, first line height).
fn body_rows(s: &Session) -> Vec<(usize, i32)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| {
            (
                p.line_segs.len(),
                p.line_segs.first().map_or(0, |l| l.line_height),
            )
        })
        .collect()
}

/// S3: moving an inline object re-breaks the lines of both paragraphs, as a file
/// written afterwards lays them out.
#[test]
fn an_inline_object_moved_into_a_full_line_rebreaks_it() -> anyhow::Result<()> {
    let line: String = (0..14).map(|i| format!("줄 {i} ")).collect();
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("개체 앞 ".into()))?;
        let a = address(&s.run(
            "shape.insert",
            json!({"kind":"rectangle","width_mm":60.0,"height_mm":15.0,"treat_as_char":true}),
        )?)?;
        s.execute(Command::Insert(format!(" 개체 뒤\n{line}")))?;
        let path = dir.path().join(format!("inline.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        r.move_inline_object(a, at(1, 6))?;
        let spans = paragraph_spans(&r)?;
        eprintln!("{ext} {:?} {spans:?}", body_rows(&r));
        let again = dir.path().join(format!("again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        assert_eq!(spans, paragraph_spans(&fresh)?, "{ext}");
    }
    Ok(())
}

/// S3: a form object put into a full line moves the rest of it to a new line, as a file
/// written afterwards lays it out.
#[test]
fn a_form_object_in_a_full_line_rebreaks_it() -> anyhow::Result<()> {
    let line: String = (0..14).map(|i| format!("줄 {i} ")).collect();
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let mut s = Session::new()?;
        s.execute(Command::Insert(format!("{line}\n끝 문단")))?;
        let path = dir.path().join(format!("form.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let before = paragraph_spans(&r)?;
        r.select(at(0, 6), false)?;
        r.run(
            "form.insert",
            json!({"kind": "edit", "text": "아주 긴 입력 칸의 기본 글", "width_mm": 80.0}),
        )
        .or_else(|_| {
            r.run(
                "form.insert",
                json!({"kind": "edit", "text": "아주 긴 입력 칸의 기본 글"}),
            )
        })?;
        let spans = paragraph_spans(&r)?;
        assert_ne!(spans, before, "{ext}: the line did not change");
        let again = dir.path().join(format!("again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        assert_eq!(spans, paragraph_spans(&fresh)?, "{ext}");
    }
    Ok(())
}

/// (top, bottom) of the first and last caret of the first paragraph of cell `index`.
fn cell_span(s: &Session, table: (usize, usize), index: usize) -> anyhow::Result<(f64, f64)> {
    let Control::Table(t) = &s.core.document().sections[0].paragraphs[table.0].controls[table.1]
    else {
        anyhow::bail!("no table");
    };
    let len = t.cells[index].paragraphs[0].text.chars().count();
    let cursor = |offset| Cursor {
        section: 0,
        paragraph: table.0,
        offset,
        cell: Some(hwpcraft_engine::Cell {
            control: table.1,
            index,
            paragraph: 0,
        }),
    };
    let first = s.caret_at(cursor(0))?;
    let last = s.caret_at(cursor(len))?;
    let y = |v: &Value, k: &str| v[k].as_f64().unwrap_or(f64::NAN);
    Ok((y(&first, "y"), y(&last, "y") + y(&last, "height")))
}

/// S3: moving a border for a cell block narrows the block's cells; their text breaks
/// again, as a file written afterwards lays it out.
#[test]
fn a_block_border_move_rebreaks_the_narrowed_cell() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let mut s = Session::new()?;
        s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
        let table = first_table(&s).expect("table");
        s.select(
            Cursor {
                section: 0,
                paragraph: table.0,
                offset: 0,
                cell: Some(hwpcraft_engine::Cell {
                    control: table.1,
                    index: 2,
                    paragraph: 0,
                }),
            },
            false,
        )?;
        s.execute(Command::Insert(
            (0..7).map(|i| format!("셀 {i} ")).collect(),
        ))?;
        let path = dir.path().join(format!("block.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let before = cell_span(&r, table, 2)?;
        r.run(
            "table.move_border",
            json!({"table": {"section": 0, "paragraph": table.0, "control": table.1},
                "vertical": true, "boundary": 1, "delta_mm": -40.0,
                "range": {"start_row": 1, "start_col": 0, "end_row": 1, "end_col": 1}}),
        )?;
        let live = cell_span(&r, table, 2)?;
        let again = dir.path().join(format!("again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        let reopened = cell_span(&fresh, table, 2)?;
        assert!(
            reopened.1 - reopened.0 > (before.1 - before.0) * 1.5,
            "{ext}: {before:?} {reopened:?}"
        );
        assert_eq!(live, reopened, "{ext}");
    }
    Ok(())
}

/// S3: the same for a block of a table inside a table cell.
#[test]
fn a_nested_block_border_move_rebreaks_the_narrowed_cell() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let mut s = Session::new()?;
        s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
        let created = s.run(
            "context.table.create",
            json!({"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
                   "rows":2,"columns":2}),
        )?;
        let first = &created["address"];
        let steps = first["path"].as_array().cloned().unwrap_or_default();
        let host =
            json!({"section": 0, "paragraph": first["paragraph"], "path": [steps[0].clone()]});
        let control = steps[1]["control"].clone();
        let mut cell = steps[1].clone();
        cell["cell"] = json!(2);
        let text: String = (0..7).map(|i| format!("셀 {i} ")).collect();
        s.run(
            "context.replace",
            json!({"address": {"section": 0, "paragraph": first["paragraph"], "path": [steps[0].clone(), cell]},
                   "start": 0, "end": 0, "text": text}),
        )?;
        let path = dir.path().join(format!("nested.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let before = drawn_rows(&r)?.len();
        r.run(
            "context.table.move_border",
            json!({"address": host, "control": control, "vertical": true, "boundary": 1, "delta_mm": -50.0,
                   "range": {"start_row": 1, "start_col": 0, "end_row": 1, "end_col": 1}}),
        )?;
        let live = drawn_rows(&r)?;
        // The same as laying the whole document out again.
        let mut whole = Session::new()?;
        whole.core.set_document(r.core.document().clone());
        assert_eq!(live, drawn_rows(&whole)?, "{ext}");
        let again = dir.path().join(format!("again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        let reopened = drawn_rows(&fresh)?;
        assert!(reopened.len() > before, "{ext}: {before} {reopened:?}");
        // A reopened file places the nested table's rows differently even without the
        // move (208.0 -> 204.2 px), an HWPX one more so after it; the line breaks are
        // the same.
        let lines =
            |rows: &[(f64, String)]| rows.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>();
        assert_eq!(lines(&live), lines(&reopened), "{ext}");
    }
    Ok(())
}

fn auto_numbers(s: &Session, paragraph: usize) -> usize {
    s.core.document().sections[0].paragraphs[paragraph]
        .controls
        .iter()
        .filter(|c| matches!(c, Control::AutoNumber(_)))
        .count()
}

/// S5: `context.split` at the end of a paragraph that ends with an automatic number
/// moved the number into the new paragraph; Enter (`Command::Split`) keeps it.
#[test]
fn a_context_split_after_a_trailing_number_keeps_it() -> anyhow::Result<()> {
    for context in [false, true] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("그림 ".into()))?;
        s.run("insert.auto_number", json!({"kind": "picture"}))?;
        assert_eq!(auto_numbers(&s, 0), 1);
        let len = s.core.document().sections[0].paragraphs[0]
            .text
            .chars()
            .count();
        if context {
            s.run(
                "context.split",
                json!({"address": {"section": 0, "paragraph": 0, "path": []}, "offset": len}),
            )?;
        } else {
            s.select(at(0, len), false)?;
            s.execute(Command::Split)?;
        }
        assert_eq!(
            (auto_numbers(&s, 0), auto_numbers(&s, 1)),
            (1, 0),
            "context {context}"
        );
    }
    Ok(())
}

/// Height of the outer table's only cell (row 0).
fn outer_cell_height(s: &Session) -> u32 {
    match &s.core.document().sections[0].paragraphs[0].controls[2] {
        Control::Table(t) => t.cells[0].height,
        _ => 0,
    }
}

/// S6: lines added in a table inside a table cell grow the outer row; taking them out
/// again left the outer row grown.
#[test]
fn an_outer_row_shrinks_back_when_nested_lines_go() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
    let created = s.run(
        "context.table.create",
        json!({"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
               "rows":1,"columns":1}),
    )?;
    let first = created["address"].clone();
    // The first nested edit sizes the rows for one line of text.
    s.run(
        "context.replace",
        json!({"address": first, "start": 0, "end": 0, "text": "가"}),
    )?;
    let original = outer_cell_height(&s);
    let rows = drawn_rows(&s)?;
    for _ in 0..8 {
        s.run("context.split", json!({"address": first, "offset": 0}))?;
    }
    let grown = outer_cell_height(&s);
    assert!(grown > original, "{original} -> {grown}");
    for k in (1..=8).rev() {
        let mut address = first.clone();
        let steps = address["path"].as_array_mut().expect("path");
        let last = steps.len() - 1;
        steps[last]["paragraph"] = json!(k);
        s.run("context.merge", json!({"address": address}))?;
    }
    assert_eq!(outer_cell_height(&s), original, "grown {grown}");
    assert_eq!(drawn_rows(&s)?, rows);
    // A row taller than its content (as a file may store it) keeps its height.
    let mut doc = s.core.document().clone();
    if let Control::Table(t) = &mut doc.sections[0].paragraphs[0].controls[2] {
        t.cells[0].height = 20000;
        t.common.height += 20000 - original;
    }
    doc.sections[0].raw_stream = None;
    s.core.set_document(doc);
    s.run("context.split", json!({"address": first, "offset": 0}))?;
    let mut second = first.clone();
    let steps = second["path"].as_array_mut().expect("path");
    let last = steps.len() - 1;
    steps[last]["paragraph"] = json!(1);
    s.run("context.merge", json!({"address": second}))?;
    assert_eq!(outer_cell_height(&s), 20000);
    Ok(())
}

/// S7: a paragraph `context.table.create` puts into the body keeps the rows it got at
/// the end of a scratch copy; the live layout has to place it where it now is.
#[test]
fn a_context_table_paragraph_is_laid_out_in_place() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let long: String = (0..30).map(|i| format!("문단 {i} ")).collect();
    for ext in ["hwp", "hwpx"] {
        let mut s = Session::new()?;
        s.execute(Command::Insert(format!("첫 문단\n{long}\n{long}\n끝 문단")))?;
        let path = dir.path().join(format!("ctx.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        r.run(
            "context.table.create",
            json!({"address": {"section": 0, "paragraph": 1, "path": []}, "rows": 2, "columns": 2,
                   "position": "after"}),
        )?;
        let live = paragraph_spans(&r)?;
        let mut whole = Session::new()?;
        whole.core.set_document(r.core.document().clone());
        assert_eq!(live, paragraph_spans(&whole)?, "{ext}");
        let again = dir.path().join(format!("again.{ext}"));
        r.save(&again)?;
        let mut fresh = Session::new()?;
        fresh.open(&again)?;
        // Rows as RHWP lays them out afresh: the table paragraph after the one before
        // it, the paragraphs after it moved down.
        let rows_of = |s: &Session| -> Vec<Vec<(i32, i32)>> {
            s.core.document().sections[0]
                .paragraphs
                .iter()
                .map(|p| {
                    p.line_segs
                        .iter()
                        .map(|l| (l.vertical_pos, l.line_height))
                        .collect()
                })
                .collect()
        };
        assert_eq!(rows_of(&r), rows_of(&fresh), "{ext}");
        // The drawn page still differs by 4 px after the table: RHWP reads the inline
        // table's outer margins back into other fields (common margin 0 -> 283), which
        // this context-made table's row then counts differently. Not part of S7.
    }
    Ok(())
}

/// S11: the HWP bullet record keeps the picture's id twice, once in a single byte; a
/// document with more than 255 embedded pictures got a byte that names another one.
/// A free id under 256 avoids it (with none free the byte still wraps).
#[test]
fn a_picture_bullet_id_fits_the_hwp_byte() -> anyhow::Result<()> {
    use hwpcraft_doc::model::bin_data::{
        BinData, BinDataBytes, BinDataCompression, BinDataContent, BinDataStatus, BinDataType,
    };
    let dir = tempfile::tempdir()?;
    let png = dir.path().join("bullet.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 0, 0])).save(&png)?;
    let filler = dir.path().join("filler.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([0, 0, 255])).save(&filler)?;
    let filler = std::fs::read(&filler)?;
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "항목"}))?;
    // 300 embedded pictures already (ids 1–301 without 7).
    let doc = s.core.document_mut();
    for id in (1..=301u16).filter(|id| *id != 7) {
        doc.bin_data_content.push(BinDataContent {
            id,
            data: BinDataBytes::from_shared(filler.clone()),
            extension: "png".into(),
        });
        doc.doc_info.bin_data_list.push(BinData {
            raw_data: None,
            attr: 0x0101,
            data_type: BinDataType::Embedding,
            compression: BinDataCompression::Default,
            status: BinDataStatus::Success,
            abs_path: None,
            rel_path: None,
            storage_id: id,
            extension: Some("png".into()),
        });
    }
    doc.doc_info.raw_stream = None;
    doc.doc_info.raw_stream_dirty = true;
    let id = s.run("format.bullet.picture", json!({"path": png}))?["id"]
        .as_u64()
        .unwrap_or(0) as usize;
    let bullet = s.core.document().doc_info.bullets[id - 1].clone();
    assert_eq!(bullet.image_bullet, 7);
    assert_eq!(
        i32::from(bullet.image_data[3]),
        bullet.image_bullet,
        "the byte names the bullet's picture"
    );
    let path = dir.path().join("bullets.hwp");
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    let doc = r.core.document();
    let b = &doc.doc_info.bullets[id - 1];
    let image = doc
        .bin_data_content
        .iter()
        .find(|c| i32::from(c.id) == b.image_bullet)
        .ok_or_else(|| anyhow::anyhow!("bullet image lost"))?;
    assert_eq!(
        image.data.load_shared().as_ref(),
        std::fs::read(&png)?.as_slice()
    );
    Ok(())
}
