//! Owner: format area. A field that begins in one paragraph and ends in a later one
//! (Hangul's multi-paragraph 누름틀 and memos) keeps its end where the text puts it
//! when the last paragraph is edited. RHWP moved field ranges with text edits but not
//! the end marker of a field that began in an earlier paragraph, so text typed in the
//! last paragraph fell out of the field and was saved that way.
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

/// (paragraph, end position, text) of each field end whose begin is in an earlier paragraph.
fn ends(s: &Session) -> Vec<(usize, usize, String)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(p, para)| {
            para.orphan_field_ends
                .iter()
                .map(move |o| (p, o.char_idx, para.text.clone()))
        })
        .collect()
}

#[test]
fn typing_in_the_last_paragraph_of_a_field_keeps_it_inside() -> anyhow::Result<()> {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/task2279/36378481_gyeoljae.hwpx");
    if !sample.exists() {
        return Ok(());
    }
    let mut s = Session::new()?;
    s.open(&sample)?;
    let before = ends(&s);
    let Some(&(paragraph, at, _)) = before.first() else {
        panic!("the sample has a multi-paragraph field: {before:?}");
    };
    s.cursor = Cursor {
        section: 0,
        paragraph,
        offset: at,
        cell: None,
    };
    s.run("text.insert", json!({"text": "추가"}))?;
    assert_eq!(ends(&s)[0].1, at + 2, "the typed text is inside the field");
    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("field.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(ends(&back)[0].1, at + 2, "{ext}");
    }
    s.run("edit.undo", json!({}))?;
    assert_eq!(ends(&s)[0].1, at);
    Ok(())
}
