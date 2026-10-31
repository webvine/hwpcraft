//! Commands that cut a paragraph at the caret count an inline object (here a footnote)
//! as RHWP does: one logical position, not its eight-character placeholder. A table
//! made before "D" in "A[note]BCD" splits the text there, not one character early.
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

fn caret(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph,
        offset,
        cell: None,
    }
}

/// "A", a footnote, "BCD"; returns the scalar offset just before "D".
fn with_note(s: &mut Session) -> anyhow::Result<usize> {
    s.run("text.insert", json!({"text": "A"}))?;
    s.run("insert.footnote", json!({"text": "주석"}))?;
    // Back to the body, after the footnote mark.
    let len = s.core.document().sections[0].paragraphs[0]
        .text
        .chars()
        .count();
    s.select(caret(0, len), false)?;
    s.run("text.insert", json!({"text": "BCD"}))?;
    let text = &s.core.document().sections[0].paragraphs[0].text;
    let before_d = text.chars().count() - 1;
    anyhow::ensure!(text.ends_with("BCD"), "{text:?}");
    Ok(before_d)
}

fn body_texts(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.chars().filter(|c| c.is_alphabetic()).collect())
        .collect()
}

#[test]
fn a_table_made_after_a_footnote_splits_at_the_caret() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let before_d = with_note(&mut s)?;
    s.select(caret(0, before_d), false)?;
    s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
    let texts = body_texts(&s);
    assert!(
        texts.iter().any(|t| t == "ABC") && texts.iter().any(|t| t == "D"),
        "{texts:?}"
    );
    Ok(())
}

#[test]
fn a_table_of_contents_after_a_footnote_keeps_the_text_around_it() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let before_d = with_note(&mut s)?;
    s.run("paragraph.split", json!({}))?;
    s.run("text.insert", json!({"text": "제목"}))?;
    s.run("style.apply", json!({"id": 2}))?;
    s.select(caret(0, before_d), false)?;
    s.run("tools.toc", json!({}))?;
    let texts = body_texts(&s);
    assert!(
        texts.iter().any(|t| t == "ABC") && texts.iter().any(|t| t == "D"),
        "{texts:?}"
    );
    Ok(())
}
