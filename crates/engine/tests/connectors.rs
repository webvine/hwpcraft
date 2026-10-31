//! Connectors attached to objects that are not floating paper-relative ones: a
//! paragraph-relative shape and a shape treated as a character. The connector ends
//! where the page shows those objects, follows a move, and keeps its subjects.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::shape::{LineShape, ShapeObject};
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

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
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let c = layout["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| {
            c["paraIdx"] == a.paragraph
                && c["controlIdx"] == a.control
                && c.get("cellIdx").is_none()
        })
        .ok_or_else(|| anyhow::anyhow!("{a:?} not on page 0"))?;
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

#[test]
fn connectors_attach_to_paragraph_relative_and_inline_objects() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("글자처럼 ".into()))?;
    let inline = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0,"treat_as_char":true}),
    )?)?;
    s.execute(Command::Insert(" 뒤\n\n\n문단 기준 개체\n".into()))?;
    let para = address(&s.run(
        "shape.insert",
        json!({"kind":"ellipse","width_mm":30.0,"height_mm":20.0,"x_mm":90.0,"y_mm":120.0}),
    )?)?;
    s.run(
        "object.properties",
        json!({"address": para, "props": {"vertRelTo": "Para", "horzRelTo": "Column", "vertOffset": 1000, "horzOffset": 20000}}),
    )?;
    let conn = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight","width_mm":1.0,"height_mm":1.0,
            "start":{"address":inline,"point":2},"end":{"address":para,"point":0}}),
    )?)?;
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let [ix, iy, iw, ih] = drawn(s, inline)?;
        let [px, py, pw, _] = drawn(s, para)?;
        let got = ends(&line(s, conn)?);
        // Bottom of the inline rectangle to the top of the paragraph-relative ellipse.
        assert!(
            near(got[0], (ix + iw / 2, iy + ih)),
            "{label} start {got:?}"
        );
        assert!(near(got[1], (px + pw / 2, py)), "{label} end {got:?}");
        Ok(())
    };
    check(&s, "made")?;
    // Moving the paragraph-relative ellipse re-routes the connector from the layout.
    s.run(
        "object.properties",
        json!({"address": para, "props": {"horzOffset": 30000}}),
    )?;
    check(&s, "moved")?;
    let k = line(&s, conn)?
        .connector
        .ok_or_else(|| anyhow::anyhow!("no connector"))?;
    assert_ne!(k.start_subject_id, 0);
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("connector.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let back = line(&r, conn)?;
        assert_eq!(
            back.connector.as_ref().map(|c| c.start_subject_id),
            Some(k.start_subject_id),
            "{ext}"
        );
        assert_eq!(ends(&back), ends(&line(&s, conn)?), "{ext} geometry");
        // HWP keeps the line layout; an HWPX from a new document is laid out afresh
        // on reading, which places everything as the live layout does.
        check(&r, ext)?;
    }
    s.execute(Command::Undo)?;
    check(&s, "undone")?;
    Ok(())
}

#[test]
fn objects_on_different_pages_cannot_be_connected() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽 ".into()))?;
    let first = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0,"treat_as_char":true}),
    )?)?;
    s.run("insert.page_break", json!({}))?;
    s.execute(Command::Insert("둘째 쪽 ".into()))?;
    let second = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0,"treat_as_char":true}),
    )?)?;
    let generation = s.generation;
    let refused = s.run(
        "shape.insert",
        json!({"kind":"connector_straight","width_mm":1.0,"height_mm":1.0,
            "start":{"address":first,"point":2},"end":{"address":second,"point":0}}),
    );
    assert!(refused.is_err_and(|e| e.to_string().contains("같은 쪽")));
    assert_eq!(s.generation, generation);
    Ok(())
}

/// A paper-relative connector shows on its anchor paragraph's page, so one made while
/// the caret is on another page is anchored after its start object instead.
#[test]
fn a_connector_made_from_another_page_is_anchored_with_its_objects() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽 ".into()))?;
    let first = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0,"treat_as_char":true}),
    )?)?;
    s.execute(Command::Insert(" 그리고 ".into()))?;
    let second = address(&s.run(
        "shape.insert",
        json!({"kind":"ellipse","width_mm":20.0,"height_mm":10.0,"treat_as_char":true}),
    )?)?;
    s.run("insert.page_break", json!({}))?;
    s.execute(Command::Insert("둘째 쪽의 커서".into()))?;
    let conn = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight","width_mm":1.0,"height_mm":1.0,
            "start":{"address":first,"point":1},"end":{"address":second,"point":3}}),
    )?)?;
    assert_eq!(conn.paragraph, first.paragraph);
    assert_eq!(s.page_of_object(conn)?, 0);
    let [fx, fy, fw, fh] = drawn(&s, first)?;
    let got = ends(&line(&s, conn)?);
    assert!(near(got[0], (fx + fw, fy + fh / 2)), "{got:?}");
    Ok(())
}
