//! 머리말·꼬리말 안 개체: the page layout lists them with their place inside the
//! header or footer (`paraIdx`, `controlIdx`) and its control (`headerFooter`). Those
//! numbers are not a body address; read as one, they name whatever body object sits
//! at the same numbers.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

fn png(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let path = dir.join("logo.png");
    image::RgbImage::from_pixel(60, 30, image::Rgb([68, 114, 196])).save(&path)?;
    Ok(path)
}

/// A saved and reopened document whose header (or footer) has a picture at control 0
/// of its second paragraph, and whose body paragraph 1 has another picture at control
/// 0: the header picture's layout numbers are the body picture's address.
fn picture_over_body_picture(dir: &std::path::Path, footer: bool) -> anyhow::Result<Session> {
    let logo = png(dir)?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문 첫 문단".into()))?;
    s.execute(Command::Split)?;
    s.run(
        "picture.insert",
        json!({"path": logo, "width_mm": 30.0, "treat_as_char": false}),
    )?;
    let command = if footer { "page.footer" } else { "page.header" };
    s.run(command, json!({"apply_to": "both", "template": 0}))?;
    let doc = s.core.document_mut();
    let picture = doc.sections[0].paragraphs[1].clone();
    assert!(matches!(
        picture.controls.first(),
        Some(Control::Picture(_))
    ));
    let paragraphs = doc.sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
        .find_map(|c| match c {
            Control::Header(h) if !footer => Some(&mut h.paragraphs),
            Control::Footer(f) if footer => Some(&mut f.paragraphs),
            _ => None,
        })
        .expect("a header or footer");
    if paragraphs.is_empty() {
        paragraphs.push(Paragraph::new_empty());
    }
    paragraphs.truncate(1);
    paragraphs.push(picture);
    let path = dir.join("header.hwpx");
    s.save(&path)?;
    let mut s = Session::new()?;
    s.open(&path)?;
    Ok(s)
}

fn layout_numbers_name_a_body_picture(footer: bool) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let s = picture_over_body_picture(dir.path(), footer)?;
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let image = layout["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["type"] == "image" && c.get("headerFooter").is_some())
        .expect("the header picture is laid out");
    let kind = if footer { "footer" } else { "header" };
    assert_eq!(image["headerFooter"]["kind"], kind, "{image}");
    let numbers = ObjectAddress {
        section: image["secIdx"].as_u64().unwrap_or(9) as usize,
        paragraph: image["paraIdx"].as_u64().unwrap_or(9) as usize,
        control: image["controlIdx"].as_u64().unwrap_or(9) as usize,
    };
    assert_eq!(
        numbers,
        ObjectAddress {
            section: 0,
            paragraph: 1,
            control: 0
        }
    );
    assert_eq!(s.object_properties(numbers)?["kind"], "picture");
    Ok(())
}

#[test]
fn header_picture_layout_numbers_name_a_body_picture() -> anyhow::Result<()> {
    layout_numbers_name_a_body_picture(false)
}

#[test]
fn footer_picture_layout_numbers_name_a_body_picture() -> anyhow::Result<()> {
    layout_numbers_name_a_body_picture(true)
}

/// The body picture's box on page 0, never the header picture with the same numbers.
fn body_box(s: &Session) -> anyhow::Result<[f64; 4]> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let c = layout["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["type"] == "image" && c.get("headerFooter").is_none())
        .ok_or_else(|| anyhow::anyhow!("no body picture"))?;
    let f = |k: &str| c[k].as_f64().unwrap_or(f64::NAN);
    Ok([f("x"), f("y"), f("w"), f("h")])
}

/// 맞춤 measures the body picture from the page layout; the header picture listed
/// with the same numbers put it off by the distance between the two.
fn aligning_measures_the_body_picture(footer: bool) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = picture_over_body_picture(dir.path(), footer)?;
    let a = json!({"section": 0, "paragraph": 1, "control": 0});
    s.run("object.align", json!({"targets": [a], "mode": "middle"}))?;
    let after = body_box(&s)?;
    let height = s.page_info(0)?["height"].as_f64().unwrap_or(f64::NAN);
    assert!(
        (after[1] + after[3] / 2.0 - height / 2.0).abs() < 1.0,
        "centred on the paper: {after:?}, page height {height}"
    );
    Ok(())
}

#[test]
fn aligning_a_body_picture_measures_it_not_the_header_picture() -> anyhow::Result<()> {
    aligning_measures_the_body_picture(false)
}

#[test]
fn aligning_a_body_picture_measures_it_not_the_footer_picture() -> anyhow::Result<()> {
    aligning_measures_the_body_picture(true)
}

/// The paragraphs of section 0's both-pages header or footer.
fn hf_paragraphs(s: &Session, footer: bool) -> Vec<Paragraph> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Header(h) if !footer => Some(h.paragraphs.clone()),
            Control::Footer(f) if footer => Some(f.paragraphs.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn has_picture(paragraphs: &[Paragraph]) -> bool {
    paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .any(|c| matches!(c, Control::Picture(_)))
}

/// `page.header_footer.text` rebuilds the header or footer from one line of text and
/// fields. With a picture in it the rewrite would drop the picture: the text is read
/// as not lossless and the rewrite is refused, leaving everything as it was.
fn a_rewrite_never_drops_a_picture(footer: bool) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = picture_over_body_picture(dir.path(), footer)?;
    let target = json!({"header": !footer, "apply_to": "both"});
    let got = s.run("page.header_footer.text.get", target.clone())?;
    assert_eq!(got["lossless"], false, "{got}");
    assert!(
        got["lossy"]
            .as_array()
            .is_some_and(|l| l.iter().any(|w| w == "그림")),
        "{got}"
    );
    let before = hf_paragraphs(&s, footer);
    assert!(has_picture(&before));
    let generation = s.generation;
    let mut rewrite = target;
    rewrite["text"] = json!("새 글");
    let err = s
        .run("page.header_footer.text", rewrite)
        .expect_err("refused");
    let kind = if footer { "꼬리말" } else { "머리말" };
    assert!(
        err.to_string().contains(kind) && err.to_string().contains("그림"),
        "{err}"
    );
    assert_eq!(s.generation, generation);
    assert_eq!(hf_paragraphs(&s, footer).len(), before.len());
    assert!(has_picture(&hf_paragraphs(&s, footer)));
    Ok(())
}

#[test]
fn a_header_rewrite_never_drops_its_picture() -> anyhow::Result<()> {
    a_rewrite_never_drops_a_picture(false)
}

#[test]
fn a_footer_rewrite_never_drops_its_picture() -> anyhow::Result<()> {
    a_rewrite_never_drops_a_picture(true)
}

/// A page-number header or footer made from a 마당 and made bold is one line of text
/// and fields: it is lossless, and a rewrite keeps the alignment and the bold font
/// (rebuilt from scratch, it was left-aligned in the default font).
fn a_page_number_line_is_rewritten_in_place(footer: bool) -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let command = if footer { "page.footer" } else { "page.header" };
    // 가운데 쪽 번호 (굵게·밑줄)
    s.run(command, json!({"apply_to": "both", "template": 7}))?;
    s.run(
        "format.hf.character",
        json!({"header": !footer, "format": {"bold": true}}),
    )?;
    let target = json!({"header": !footer, "apply_to": "both"});
    let got = s.run("page.header_footer.text.get", target.clone())?;
    assert_eq!(got["lossless"], true, "{got}");
    let before = hf_paragraphs(&s, footer);
    let shapes = |p: &Paragraph| {
        (
            p.para_shape_id,
            p.char_shapes.first().map(|c| c.char_shape_id),
        )
    };
    assert!(shapes(&before[0]).1.is_some(), "the text has its own font");
    let mut rewrite = target.clone();
    rewrite["text"] = json!("보고서 - ");
    rewrite["fields"] = json!([{"offset": 6, "kind": "page_number"}]);
    s.run("page.header_footer.text", rewrite)?;
    let after = hf_paragraphs(&s, footer);
    assert_eq!(after.len(), 1);
    assert_eq!(shapes(&after[0]), shapes(&before[0]));
    let got = s.run("page.header_footer.text.get", target)?;
    assert_eq!(got["text"], "보고서 - ");
    assert_eq!(got["fields"], json!([{"offset": 6, "kind": "page_number"}]));
    assert_eq!(got["lossless"], true, "{got}");
    Ok(())
}

#[test]
fn a_page_number_header_is_rewritten_with_its_shapes() -> anyhow::Result<()> {
    a_page_number_line_is_rewritten_in_place(false)
}

#[test]
fn a_page_number_footer_is_rewritten_with_its_shapes() -> anyhow::Result<()> {
    a_page_number_line_is_rewritten_in_place(true)
}

/// A footer of two paragraphs reads as not lossless instead of failing.
#[test]
fn a_two_paragraph_footer_reads_as_not_lossless() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.run("page.footer", json!({"apply_to": "both", "template": 2}))?;
    let doc = s.core.document_mut();
    let footer = doc.sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
        .find_map(|c| match c {
            Control::Footer(f) => Some(f),
            _ => None,
        })
        .expect("a footer");
    let second = footer.paragraphs[0].clone();
    footer.paragraphs.push(second);
    let target = json!({"header": false, "apply_to": "both"});
    let got = s.run("page.header_footer.text.get", target.clone())?;
    assert_eq!(got["lossless"], false, "{got}");
    assert_eq!(got["lossy"], json!(["여러 문단"]));
    let mut rewrite = target;
    rewrite["text"] = json!("한 줄");
    assert!(s.run("page.header_footer.text", rewrite).is_err());
    assert_eq!(hf_paragraphs(&s, true).len(), 2);
    Ok(())
}

/// The header (footer) picture's address, from `page.header_footer.object.list`.
fn listed_picture(s: &mut Session, footer: bool) -> anyhow::Result<Value> {
    let list = s.run("page.header_footer.object.list", json!({}))?;
    let objects = list["objects"].as_array().cloned().unwrap_or_default();
    let found = objects
        .iter()
        .find(|o| o["header"] == !footer && o["kind"] == "picture")
        .ok_or_else(|| anyhow::anyhow!("not listed: {list}"))?;
    assert_eq!(found["applyTo"], "both");
    Ok(found["address"].clone())
}

/// The header (footer) picture's layout entry on page 0.
fn hf_image(s: &Session) -> anyhow::Result<Value> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    layout["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["type"] == "image" && c.get("headerFooter").is_some())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no header picture"))
}

fn width_mm(s: &mut Session, address: &Value) -> anyhow::Result<f64> {
    let p = s.run("object.properties.get", json!({"address": address}))?;
    assert_eq!(p["kind"], "picture", "{p}");
    Ok(p["props"]["width"].as_f64().unwrap_or(0.0) * 25.4 / 7200.0)
}

/// A header or footer picture is listed with the address its layout entry gives, and
/// the body's object commands resize, move and delete it through that address, each
/// one undo step, without touching the body picture of the same numbers.
fn header_objects_take_the_body_object_commands(footer: bool) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = picture_over_body_picture(dir.path(), footer)?;
    let a = listed_picture(&mut s, footer)?;
    let image = hf_image(&s)?;
    assert_eq!(
        (
            &a["paragraph"],
            &a["control"],
            &a["header_footer"]["paragraph"],
            &a["header_footer"]["control"]
        ),
        (
            &image["paraIdx"],
            &image["controlIdx"],
            &image["headerFooter"]["outerParaIdx"],
            &image["headerFooter"]["outerControlIdx"]
        ),
        "{a} {image}"
    );
    let body = json!({"section": 0, "paragraph": 1, "control": 0});
    let body_width = width_mm(&mut s, &body)?;
    assert!((width_mm(&mut s, &a)? - 30.0).abs() < 0.5);
    // Resize: the header picture only.
    s.run(
        "object.size",
        json!({"address": a, "width_mm": 45.0, "keep_ratio": true}),
    )?;
    assert!((width_mm(&mut s, &a)? - 45.0).abs() < 0.5);
    assert!((width_mm(&mut s, &body)? - body_width).abs() < 0.01);
    assert!((hf_image(&s)?["w"].as_f64().unwrap() - 45.0 * 96.0 / 25.4).abs() < 1.0);
    // Move.
    let x = hf_image(&s)?["x"].as_f64().unwrap();
    s.run(
        "object.move_by",
        json!({"addresses": [a], "dx_mm": 20.0, "dy_mm": 0.0}),
    )?;
    let moved = hf_image(&s)?["x"].as_f64().unwrap();
    assert!(
        (moved - x - 20.0 * 96.0 / 25.4).abs() < 1.0,
        "{x} -> {moved}"
    );
    // Saved and reopened, the header keeps both edits.
    for name in ["edited.hwpx", "edited.hwp"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let ra = listed_picture(&mut r, footer)?;
        assert!((width_mm(&mut r, &ra)? - 45.0).abs() < 0.5, "{name}");
        assert!(
            (hf_image(&r)?["x"].as_f64().unwrap() - moved).abs() < 1.0,
            "{name}"
        );
    }
    // Delete; the body picture stays. Each edit is one undo step.
    s.run("object.delete", a.clone())?;
    assert!(hf_image(&s).is_err());
    assert_eq!(
        s.object_properties(hwpcraft_engine::ObjectAddress {
            section: 0,
            paragraph: 1,
            control: 0,
        })?["kind"],
        "picture"
    );
    s.execute(Command::Undo)?;
    assert!((hf_image(&s)?["x"].as_f64().unwrap() - moved).abs() < 1.0);
    s.execute(Command::Undo)?;
    assert!((hf_image(&s)?["x"].as_f64().unwrap() - x).abs() < 1.0);
    s.execute(Command::Undo)?;
    assert!((width_mm(&mut s, &a)? - 30.0).abs() < 0.5);
    Ok(())
}

#[test]
fn header_objects_take_the_body_object_commands_in_a_header() -> anyhow::Result<()> {
    header_objects_take_the_body_object_commands(false)
}

#[test]
fn header_objects_take_the_body_object_commands_in_a_footer() -> anyhow::Result<()> {
    header_objects_take_the_body_object_commands(true)
}

/// Header addresses go only to the object commands, never mixed with body objects
/// or another header, and a wrong `header_footer` changes nothing.
#[test]
fn bad_header_object_addresses_change_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = picture_over_body_picture(dir.path(), false)?;
    let a = listed_picture(&mut s, false)?;
    let generation = s.generation;
    let body = json!({"section": 0, "paragraph": 1, "control": 0});
    assert!(
        s.run(
            "object.move_by",
            json!({"addresses": [a, body], "dx_mm": 5.0, "dy_mm": 0.0})
        )
        .is_err()
    );
    assert!(s.run("object.copy", json!({"address": a})).is_err());
    let mut wrong = a.clone();
    wrong["header_footer"]["control"] = json!(0);
    assert!(
        s.run("object.size", json!({"address": wrong, "width_mm": 40.0}))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    Ok(())
}

/// A table in a footer: listed by `page.header_footer.list` with its address, and its
/// table properties change through that address (one undo step, kept on reopening).
#[test]
fn a_footer_table_takes_table_properties() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Table {
        rows: 1,
        columns: 2,
    })?;
    s.run("page.footer", json!({"apply_to": "both", "template": 0}))?;
    let doc = s.core.document_mut();
    let table = doc.sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.iter().any(|c| matches!(c, Control::Table(_))))
        .expect("a body table");
    let moved = doc.sections[0].paragraphs.remove(table);
    let footer = doc.sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
        .find_map(|c| match c {
            Control::Footer(f) => Some(&mut f.paragraphs),
            _ => None,
        })
        .expect("a footer");
    footer.truncate(1);
    footer.push(moved);
    let path = dir.path().join("footer-table.hwpx");
    s.save(&path)?;
    let mut s = Session::new()?;
    s.open(&path)?;
    let list = s.run("page.header_footer.list", json!({}))?;
    let item = list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|i| i["isHeader"] == false)
        .cloned()
        .expect("the footer is listed");
    let table = item["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| o["kind"] == "table")
        .cloned()
        .unwrap_or_else(|| panic!("the footer table is listed: {list}"));
    let a = table["address"].clone();
    assert_eq!(a["header_footer"], item["header_footer"], "{list}");
    let padding = |s: &mut Session| -> anyhow::Result<Value> {
        let p = s.run("object.properties.get", json!({"address": a}))?;
        assert_eq!(p["kind"], "table", "{p}");
        Ok(p["props"]["paddingLeft"].clone())
    };
    let before = padding(&mut s)?;
    s.run(
        "object.properties",
        json!({"address": a, "props": {"paddingLeft": 300}}),
    )?;
    assert_eq!(padding(&mut s)?, json!(300));
    let edited = dir.path().join("edited.hwp");
    s.save(&edited)?;
    let mut r = Session::new()?;
    r.open(&edited)?;
    assert_eq!(padding(&mut r)?, json!(300));
    s.execute(Command::Undo)?;
    assert_eq!(padding(&mut s)?, before);
    Ok(())
}

/// A header with a 2×2 table in its second paragraph (saved and reopened).
fn header_table(dir: &std::path::Path) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Table {
        rows: 2,
        columns: 2,
    })?;
    s.run("page.header", json!({"apply_to": "both", "template": 0}))?;
    let doc = s.core.document_mut();
    let table = doc.sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.iter().any(|c| matches!(c, Control::Table(_))))
        .expect("a body table");
    let moved = doc.sections[0].paragraphs.remove(table);
    let header = doc.sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
        .find_map(|c| match c {
            Control::Header(h) => Some(&mut h.paragraphs),
            _ => None,
        })
        .expect("a header");
    header.truncate(1);
    header.push(moved);
    let path = dir.join("header-table.hwpx");
    s.save(&path)?;
    let mut s = Session::new()?;
    s.open(&path)?;
    Ok(s)
}

/// The table commands take a header table through `table`: its properties, its cells'
/// properties over a range, and both at once in one undo step.
#[test]
fn header_tables_take_the_table_commands() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = header_table(dir.path())?;
    let list = s.run("page.header_footer.object.list", json!({}))?;
    let a = list["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| o["kind"] == "table")
        .map(|o| o["address"].clone())
        .expect("the header table is listed");
    let t = s.run("table.properties.get", json!({"table": a}))?;
    assert_eq!(
        (t["rows"].clone(), t["columns"].clone()),
        (json!(2), json!(2))
    );
    let all = json!({"start_row": 0, "start_col": 0, "end_row": 1, "end_col": 1});
    s.run(
        "table.cell_properties.get",
        json!({"table": a, "range": all}),
    )?;
    let generation = s.generation;
    s.run(
        "table.properties.apply",
        json!({"table": a, "range": all, "table_props": {"paddingLeft": 400},
            "cell_props": {"paddingTop": 200}}),
    )?;
    assert_eq!(s.generation, generation + 1, "one undo step");
    let t = s.run("table.properties.get", json!({"table": a}))?;
    assert_eq!(t["paddingLeft"], 400, "{t}");
    s.execute(Command::Undo)?;
    let t = s.run("table.properties.get", json!({"table": a}))?;
    assert_ne!(t["paddingLeft"], 400, "{t}");
    Ok(())
}
