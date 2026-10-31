//! Owner: object area. 개체 보호 모두 풀기 (`object.unlock_all`) rewrites only the sections
//! with a locked object: a section none of whose objects changed keeps its stored stream
//! (REVIEW-RHWP-EDIT-0.19 M11). RHWP reads a sealed section without one as edited, which
//! turns off the stored line rules for the whole document and can change its pages.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::json;

/// Two sections from an HWP file: a locked rectangle in the first, an unlocked table in
/// the second.
fn opened(dir: &std::path::Path) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "첫 구역\n"}))?;
    let first = s.run(
        "shape.insert",
        json!({"kind": "rectangle", "width_mm": 20.0, "height_mm": 10.0}),
    )?;
    s.run(
        "object.lock",
        json!({"address": first["address"], "locked": true}),
    )?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("text.insert", json!({"text": "둘째 구역"}))?;
    s.run("page.section_break", json!({}))?;
    s.run("text.insert", json!({"text": "표\n"}))?;
    s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    let path = dir.join("two.hwp");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    Ok(back)
}

/// Locked tables, pictures and shapes in body paragraphs.
fn locked(s: &mut Session) -> anyhow::Result<usize> {
    Ok(s.core
        .document()
        .sections
        .iter()
        .flat_map(|x| x.paragraphs.iter())
        .flat_map(|p| p.controls.iter())
        .filter(|c| match c {
            Control::Table(t) => t.common.locked,
            Control::Picture(p) => p.common.locked,
            Control::Shape(s) => s.common().locked,
            _ => false,
        })
        .count())
}

#[test]
fn unlocking_all_keeps_the_stream_of_a_section_without_locked_objects() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = opened(dir.path())?;
    let sections = s.core.document().sections.len();
    assert_eq!(sections, 2);
    assert!(
        s.core
            .document()
            .sections
            .iter()
            .all(|x| x.raw_stream.is_some()),
        "both sections as read from the file"
    );
    assert_eq!(locked(&mut s)?, 1);
    let r = s.run("object.unlock_all", json!({}))?;
    assert_eq!(r["unlocked"], 1, "{r}");
    assert_eq!(locked(&mut s)?, 0);
    let doc = s.core.document();
    assert!(
        doc.sections[0].raw_stream.is_none(),
        "the changed section is written again"
    );
    assert!(
        doc.sections[1].raw_stream.is_some(),
        "the other keeps its stream"
    );
    // Saved and opened again, nothing is locked.
    let path = dir.path().join("unlocked.hwp");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    assert_eq!(locked(&mut back)?, 0);
    assert_eq!(back.page_count()?, s.page_count()?);
    Ok(())
}
