//! Owner: format area. A cross reference to an outline heading's number or text,
//! pointed at the heading paragraph by its instance id, as Hangul 2024 does
//! (probe: `?#<paragraph InstId>;5;1;0;0` shows "1.", `;5;2;` the heading text).
use hwpcraft_engine::Session;
use serde_json::{Value, json};

/// "참조: " then three outline headings with a body paragraph after each.
fn document() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "참조: \n"}))?;
    for title in ["가 제목", "나 제목", "다 제목"] {
        s.run("text.insert", json!({ "text": title }))?;
        s.run("select.text", json!({ "text": title }))?;
        s.run("style.apply", json!({"id": 2}))?;
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("text.insert", json!({"text": "\n"}))?;
        s.run("style.apply", json!({"id": 0}))?;
        s.run("text.insert", json!({"text": "본문\n"}))?;
    }
    Ok(s)
}

fn at_reference(s: &mut Session) -> anyhow::Result<()> {
    s.cursor = hwpcraft_engine::Cursor {
        paragraph: 0,
        offset: s.core.document().sections[0].paragraphs[0]
            .text
            .chars()
            .count(),
        ..Default::default()
    };
    Ok(())
}

fn references(s: &Session) -> Vec<Value> {
    s.cross_references()
}

#[test]
fn heading_number_and_text_references_follow_the_heading() -> anyhow::Result<()> {
    let mut s = document()?;
    at_reference(&mut s)?;
    let number = s.run(
        "insert.cross_reference",
        json!({"target": {"heading": "나 제목"}, "show": "number"}),
    )?;
    let id = number["heading"].as_u64().unwrap_or(0);
    assert!(id >= 0x4000_0000, "{number}");
    s.run("text.insert", json!({"text": " "}))?;
    let text = s.run(
        "insert.cross_reference",
        json!({"target": {"heading": "나 제목"}, "show": "caption"}),
    )?;
    assert_eq!(text["text"], "나 제목", "{text}");
    assert_eq!(text["heading"], id);
    let listed = references(&s);
    assert_eq!(listed.len(), 2, "{listed:?}");
    for item in &listed {
        assert_eq!(item["heading"], id, "{item}");
        assert_eq!(item["expected"], item["text"], "{item}");
    }
    // The second heading, shown as Hangul shows it.
    assert_eq!(number["text"], "2.", "{number}");

    // A heading added in front renumbers it; a refresh follows.
    s.cursor = hwpcraft_engine::Cursor {
        paragraph: 1,
        offset: 0,
        ..Default::default()
    };
    s.run("text.insert", json!({"text": "새 제목\n"}))?;
    s.cursor = hwpcraft_engine::Cursor {
        paragraph: 1,
        offset: 0,
        ..Default::default()
    };
    s.run("select.text", json!({"text": "새 제목"}))?;
    s.run("style.apply", json!({"id": 2}))?;
    let refreshed = s.run("tools.cross_reference.update", json!({}))?;
    assert_eq!(refreshed["updated"], 1, "{refreshed}");
    let after = references(&s);
    assert_eq!(after[0]["text"], "3.", "{after:?}");
    assert_eq!(after[1]["text"], "나 제목");

    // HWP keeps the paragraph's instance id: the references still resolve.
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("headings.hwp");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    let reopened = references(&back);
    assert_eq!(reopened.len(), 2, "{reopened:?}");
    for item in &reopened {
        assert_eq!(item["expected"], item["text"], "{item}");
    }
    let unchanged = back.run("tools.cross_reference.update", json!({}))?;
    assert_eq!(unchanged["missing"], json!([]), "{unchanged}");
    Ok(())
}

#[test]
fn heading_references_survive_hwpx() -> anyhow::Result<()> {
    let mut s = document()?;
    at_reference(&mut s)?;
    s.run(
        "insert.cross_reference",
        json!({"target": {"heading": "다 제목"}, "show": "caption"}),
    )?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("headings.hwpx");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    let reopened = references(&back);
    assert_eq!(reopened.len(), 1, "{reopened:?}");
    assert_eq!(reopened[0]["expected"], "다 제목", "{reopened:?}");
    Ok(())
}
