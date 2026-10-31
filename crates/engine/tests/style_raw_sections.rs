//! Restyling keeps the stored stream of every section it did not change: RHWP reads a
//! sealed section without one as edited and drops the stored line rules for the whole
//! document, so a Hangul document laid out with one page more or less after reopening.
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

fn raw(s: &Session) -> Vec<bool> {
    s.core
        .document()
        .sections
        .iter()
        .map(|sec| sec.raw_stream.is_some())
        .collect()
}

/// Two sections read back from HWP (so both carry their stored stream); only the
/// second uses 개요 1.
fn two_sections() -> anyhow::Result<(tempfile::TempDir, Session)> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "첫 구역 본문"}))?;
    s.run("page.section_break", json!({}))?;
    s.run("text.insert", json!({"text": "둘째 구역 제목"}))?;
    s.run("style.apply", json!({"id": 2}))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("two.hwp");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    assert_eq!(back.core.document().sections.len(), 2);
    assert_eq!(raw(&back), [true, true], "both sections read from the file");
    Ok((dir, back))
}

#[test]
fn restyling_one_section_keeps_the_others_stored_stream() -> anyhow::Result<()> {
    // Style set: 개요 1 changes, used only in the second section.
    let (_dir, mut s) = two_sections()?;
    s.run(
        "format.style.import",
        json!({"styles": [{"name": "개요 1", "character": {"fontSize": 1600}}]}),
    )?;
    assert_eq!(raw(&s), [true, false]);

    // Deleting or moving styles no paragraph uses changes no section.
    let (_dir, mut s) = two_sections()?;
    let last = s
        .run("format.style.list", json!({}))?
        .as_array()
        .map_or(0, Vec::len)
        - 1;
    s.cursor = Cursor::default();
    s.run("format.style.move", json!({"id": last, "direction": "up"}))?;
    assert_eq!(raw(&s), [true, true], "move");
    s.run("format.style.delete", json!({"id": last}))?;
    assert_eq!(raw(&s), [true, true], "delete");
    Ok(())
}

/// The case 03 found: the 보고서 style set on the 381-page handbook laid out 380 pages
/// live and 381 after the HWP was reopened. Skipped without the sample.
#[test]
fn a_style_set_on_a_hangul_document_reopens_with_the_same_pages() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/2025 행정업무운영 편람(최종).hwp");
    if !path.exists() {
        return Ok(());
    }
    let mut s = Session::new()?;
    s.open(&path)?;
    s.run("format.style.set.apply", json!({"name": "보고서"}))?;
    let live = s.page_count()?;
    let dir = tempfile::tempdir()?;
    let saved = dir.path().join("handbook.hwp");
    s.save(&saved)?;
    let mut back = Session::new()?;
    back.open(&saved)?;
    assert_eq!(back.page_count()?, live);
    Ok(())
}
