//! Owner: format area. Typed text may hold tabs and line breaks but no other control
//! characters: a HWP file reads codes 1–31 as inline-control slots, so "앞\u{1}뒤"
//! reopened as "앞" (MCP 2nd review D1). Refused before anything changes.
use hwpcraft_engine::Session;
use serde_json::json;

fn text(s: &Session) -> String {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn control_characters_are_refused_and_tabs_kept() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문"}))?;
    let generation = s.generation;
    for bad in [
        "앞\u{1}뒤",
        "\u{0}",
        "a\u{12}b",
        "\u{1f}",
        "x\u{fffe}",
        "\u{ffff}",
    ] {
        let refused = s.run("text.insert", json!({ "text": bad }));
        let message = refused.expect_err(bad).to_string();
        assert!(message.contains("제어 문자"), "{bad:?}: {message}");
        assert_eq!(s.generation, generation, "{bad:?}");
        assert_eq!(text(&s), "본문", "{bad:?}");
    }
    let t = s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    let generation = s.generation;
    assert!(
        s.run(
            "table.cells.set",
            json!({"table": t["table"], "row": 0, "col": 0, "cells": [["좋음", "나쁨\u{2}"]]}),
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    // Tabs and line breaks still go in and survive a HWP round trip.
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("text.insert", json!({"text": "\n가\t나\r\n다"}))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("tabs.hwp");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    let reopened = text(&back);
    assert!(reopened.contains("가\t나\n다"), "{reopened:?}");
    Ok(())
}
