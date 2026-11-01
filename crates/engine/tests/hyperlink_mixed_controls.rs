//! A link stays editable when its paragraph also holds a bookmark, footnote or
//! equation outside the link: Hangul edits such links, and real documents put
//! bookmarks on headings and footnotes at sentence ends. Memos (fields made the
//! same way) are added and removed there too, and every edit survives HWP/HWPX.
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn at(offset: usize) -> Value {
    json!({"anchor": null, "cursor": {"cell": null, "offset": offset, "paragraph": 0, "section": 0}})
}

/// "앞 링크 뒤 글" with the link at offset 2, plus `extra` inserted at offset 0.
fn paragraph_with(extra: &str, params: Value) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 뒤 글"}))?;
    s.run("selection.set", at(2))?;
    s.run(
        "insert.hyperlink",
        json!({"text": "링크", "uri": "https://example.com"}),
    )?;
    s.run("selection.set", at(0))?;
    s.run(extra, params)?;
    s.run("selection.set", at(3))?;
    Ok(s)
}

fn extras() -> [(&'static str, Value); 3] {
    [
        ("bookmark.add", json!({"name": "요약"})),
        ("insert.footnote", json!({"text": "각주"})),
        ("insert.equation", json!({"script": "x", "size_pt": 10})),
    ]
}

fn links(s: &mut Session) -> anyhow::Result<Vec<Value>> {
    Ok(s.run("hyperlink.list", json!({}))?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

fn link_id(s: &mut Session) -> anyhow::Result<Value> {
    let list = links(s)?;
    assert_eq!(list.len(), 1, "{list:?}");
    Ok(list[0]["fieldId"].clone())
}

fn reopened(s: &mut Session, name: &str) -> anyhow::Result<Session> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(name);
    s.save(&path)?;
    let mut again = Session::new()?;
    again.open(&path)?;
    again.run("selection.set", at(3))?;
    Ok(again)
}

#[test]
fn links_next_to_other_controls_can_be_updated_and_removed() -> anyhow::Result<()> {
    for (extra, params) in extras() {
        let mut s = paragraph_with(extra, params.clone())?;
        let id = link_id(&mut s)?;
        s.run(
            "hyperlink.update",
            json!({"field_id": id, "text": "새 링크", "uri": "https://example.org"}),
        )
        .map_err(|e| anyhow::anyhow!("update next to {extra}: {e}"))?;
        let list = links(&mut s)?;
        assert_eq!(list[0]["text"], "새 링크", "{extra}: {list:?}");
        assert_eq!(list[0]["uri"], "https://example.org", "{extra}: {list:?}");
        let text = s.run("document.text", json!({}))?;
        for name in ["l.hwpx", "l.hwp"] {
            let mut again = reopened(&mut s, name)?;
            let list = links(&mut again)?;
            assert_eq!(list.len(), 1, "{extra} {name}: {list:?}");
            assert_eq!(list[0]["text"], "새 링크", "{extra} {name}: {list:?}");
            assert_eq!(
                again.run("document.text", json!({}))?,
                text,
                "{extra} {name}"
            );
        }

        let mut s = paragraph_with(extra, params)?;
        let id = link_id(&mut s)?;
        s.run(
            "hyperlink.remove",
            json!({"field_id": id, "restore_format": true}),
        )
        .map_err(|e| anyhow::anyhow!("remove next to {extra}: {e}"))?;
        assert!(links(&mut s)?.is_empty(), "{extra}");
        assert!(
            s.run("document.text", json!({}))?
                .to_string()
                .contains("링크"),
            "{extra}: removing a link keeps its text"
        );
    }
    Ok(())
}

#[test]
fn memos_and_bookmarks_beside_a_footnote_are_added_and_removed() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 뒤 글"}))?;
    s.run("selection.set", at(0))?;
    s.run("insert.footnote", json!({"text": "각주"}))?;
    // Select "뒤" and add a memo there, then remove it.
    let text = s.core.document().sections[0].paragraphs[0].text.clone();
    let start = text.chars().position(|c| c == '뒤').expect("뒤");
    s.run(
        "selection.set",
        json!({"anchor": start,
               "cursor": {"cell": null, "offset": start + 1, "paragraph": 0, "section": 0}}),
    )?;
    let memo = s.run("memo.create", json!({"text": "확인"}))?;
    let id = memo["id"].clone();
    assert_eq!(
        s.run("memo.list", json!({}))?.as_array().map(Vec::len),
        Some(1)
    );
    s.run("memo.delete", json!({"id": id}))?;
    assert_eq!(
        s.run("memo.list", json!({}))?.as_array().map(Vec::len),
        Some(0)
    );
    // A bookmark next to the footnote comes and goes too.
    s.run("selection.set", at(start))?;
    s.run("bookmark.add", json!({"name": "가운데"}))?;
    s.run("bookmark.delete", json!({"name": "가운데"}))?;
    let mut again = reopened(&mut s, "m.hwpx")?;
    assert_eq!(
        again.run("memo.list", json!({}))?.as_array().map(Vec::len),
        Some(0)
    );
    Ok(())
}

/// The same edits in a paragraph with only the link already work (control group).
#[test]
fn a_link_alone_in_its_paragraph_can_be_updated() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 뒤 글"}))?;
    s.run("selection.set", at(2))?;
    s.run(
        "insert.hyperlink",
        json!({"text": "링크", "uri": "https://example.com"}),
    )?;
    let id = link_id(&mut s)?;
    s.run(
        "hyperlink.update",
        json!({"field_id": id, "text": "새 링크"}),
    )?;
    assert_eq!(links(&mut s)?[0]["text"], "새 링크");
    Ok(())
}
