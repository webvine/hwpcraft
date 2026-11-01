//! Owner: format area. A cross reference goes into a paragraph that also holds a
//! footnote or bookmark (RHWP 0067), as long as no object sits right at the caret.
//! Refreshing one whose end an object later touches is skipped, not an error, and
//! leaves the document as it was.
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

fn para_text(s: &Session, p: usize) -> String {
    s.core.document().sections[0].paragraphs[p].text.clone()
}

/// Paragraph 0 "앞 문단 끝" with a footnote after "앞"; table 1 with a caption on page 2.
fn document() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 문단 끝"}))?;
    s.cursor = Cursor {
        offset: 1,
        ..Default::default()
    };
    s.run("insert.footnote", json!({"text": "각주"}))?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("insert.page_break", json!({}))?;
    s.run("text.insert", json!({"text": "표 앞\n"}))?;
    let t = s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
    s.run(
        "table.properties",
        json!({"table": t["table"], "props": {"hasCaption": true}}),
    )?;
    Ok(s)
}

#[test]
fn a_cross_reference_beside_a_footnote() -> anyhow::Result<()> {
    let mut s = document()?;
    // Right after the footnote: refused before any change.
    s.cursor = Cursor {
        offset: 1,
        ..Default::default()
    };
    let generation = s.generation;
    assert!(
        s.run("insert.cross_reference", json!({"target": {"table": 1}}))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    // At the paragraph end, away from it: inserted.
    let end = para_text(&s, 0).chars().count();
    s.cursor = Cursor {
        offset: end,
        ..Default::default()
    };
    let made = s.run("insert.cross_reference", json!({"target": {"table": 1}}))?;
    assert_eq!(made["text"], "2", "{made}");
    assert!(para_text(&s, 0).ends_with("끝2"));

    // The table moves to page 3: refreshed in that paragraph.
    let table_para = s.core.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.text == "표 앞")
        .unwrap();
    s.cursor = Cursor {
        paragraph: table_para,
        ..Default::default()
    };
    s.run("insert.page_break", json!({}))?;
    let refreshed = s.run("tools.cross_reference.update", json!({}))?;
    assert_eq!(refreshed["updated"], 1, "{refreshed}");
    assert!(para_text(&s, 0).ends_with("끝3"));
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("beside.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(back.cross_references()[0]["text"], "3", "{ext}");
    }

    // A bookmark right after the reference: RHWP will not rewrite its text, so the
    // refresh skips it and the document stays as it was.
    let end = para_text(&s, 0).chars().count();
    s.cursor = Cursor {
        offset: end,
        ..Default::default()
    };
    s.run("bookmark.add", json!({"name": "끝"}))?;
    s.cursor = Cursor {
        paragraph: table_para + 1,
        ..Default::default()
    };
    s.run("insert.page_break", json!({}))?;
    let before = para_text(&s, 0);
    let skipped = s.run("tools.cross_reference.update", json!({}))?;
    assert_eq!(skipped["updated"], 0, "{skipped}");
    assert_eq!(
        skipped["skipped"].as_array().map(Vec::len),
        Some(1),
        "{skipped}"
    );
    assert_eq!(para_text(&s, 0), before);
    Ok(())
}
