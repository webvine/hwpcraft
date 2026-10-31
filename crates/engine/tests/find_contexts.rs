//! Owner: review-tools area. 모두 찾기 / 모두 바꾸기 over every context (`include`):
//! headers, footers, footnotes, text boxes and nested cells as well as the body, with
//! version 2 addresses, one undo step and a round trip. Without `include` the old
//! scope stays, but a hit in a text box, a nested cell or an equation's script is no
//! longer given out as a caret position it is not.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

/// Body "사과", a header, a footnote, a text box, a table cell with a nested table
/// cell, and an equation whose script holds the word too.
fn document() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문 사과\n둘째 사과 문단"}))?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리말 사과"}),
    )?;
    s.run("insert.footnote", json!({"text": "각주 사과"}))?;
    s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 40.0, "height_mm": 15.0, "paragraphs": ["글상자 사과"]}),
    )?;
    s.run(
        "insert.equation",
        json!({"script": "사과 over 2", "size_pt": 12}),
    )?;
    s.run("text.insert", json!({"text": "\n"}))?;
    let t = s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    let table = t["table"].clone();
    let cell = json!({"section": table["section"], "paragraph": table["paragraph"], "offset": 0,
        "cell": {"control": table["control"], "index": 1, "paragraph": 0}});
    s.run("selection.set", json!({"cursor": cell}))?;
    s.run("text.insert", json!({"text": "셀 사과"}))?;
    let created = s.run(
        "context.table.create",
        json!({"address": {"section": 0, "paragraph": table["paragraph"],
            "path": [{"control": table["control"], "cell": 0, "paragraph": 0}]},
            "rows": 1, "columns": 1}),
    )?;
    let generation = s.generation;
    s.run(
        "context.replace",
        json!({"address": created["address"], "start": 0, "end": 0, "text": "중첩 사과",
            "expected_generation": generation}),
    )?;
    Ok(s)
}

/// (kind, text) of every context paragraph holding `word`.
fn holding(s: &mut Session, word: &str) -> anyhow::Result<Vec<(String, String)>> {
    let list = s.run("context.v2.list", json!({"limit": 1000}))?;
    Ok(list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["text"].as_str().is_some_and(|t| t.contains(word)))
        .map(|i| {
            (
                i["kind"].as_str().unwrap_or_default().to_string(),
                i["text"].as_str().unwrap_or_default().trim().to_string(),
            )
        })
        .collect())
}

fn kinds(matches: &Value) -> Vec<String> {
    let mut k: Vec<String> = matches["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| m["kind"].as_str().unwrap_or_default().to_string())
        .collect();
    k.sort();
    k
}

#[test]
fn find_all_without_include_gives_no_caret_for_text_boxes_nested_cells_or_equations()
-> anyhow::Result<()> {
    let mut s = document()?;
    let found = s.run("edit.find_all", json!({"query": "사과"}))?;
    let matches = found["matches"].as_array().cloned().unwrap_or_default();
    // Body ×2, the text box, the equation script, the cell and the nested cell.
    assert_eq!(kinds(&found).len(), 6, "{found}");
    for m in &matches {
        let kind = m["kind"].as_str().unwrap_or_default();
        let caret = !m["cursor"].is_null();
        let equation = !m["equationControl"].is_null();
        let depth = m["address"]["path"].as_array().map_or(0, Vec::len);
        // A caret only for body text and first-level cell text.
        let expected = !equation && (kind == "body" || (kind == "table_cell" && depth == 1));
        assert_eq!(caret, expected, "{m}");
        assert_eq!(m["nested"] == true, !expected, "{m}");
        if kind == "textbox" {
            assert_eq!(m["address"]["path"][0]["kind"], "textbox", "{m}");
        }
    }
    assert!(
        matches.iter().any(|m| !m["equationControl"].is_null()),
        "{found}"
    );
    // Every caret it gives selects the word.
    for m in matches.iter().filter(|m| !m["cursor"].is_null()) {
        let c = m["cursor"].clone();
        let mut end = c.clone();
        end["offset"] = json!(c["offset"].as_u64().unwrap_or(0) + 2);
        s.run(
            "selection.set",
            json!({"cursor": end, "anchor": c["offset"]}),
        )?;
        assert_eq!(s.selected_text(), "사과", "{m}");
    }
    Ok(())
}

#[test]
fn find_all_with_include_reaches_every_context() -> anyhow::Result<()> {
    let mut s = document()?;
    let all = s.run("edit.find_all", json!({"query": "사과", "include": []}))?;
    assert_eq!(
        kinds(&all),
        [
            "body",
            "body",
            "footnote",
            "header",
            "table_cell",
            "table_cell",
            "textbox"
        ],
        "{all}"
    );
    assert_eq!(all["total"], 7);
    // Each address reads back the word at its offset.
    for m in all["matches"].as_array().into_iter().flatten() {
        let got = s.run("context.v2.get", json!({"address": m["address"]}))?;
        let text: String = got["text"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .skip(m["offset"].as_u64().unwrap_or(0) as usize)
            .take(2)
            .collect();
        assert_eq!(text, "사과", "{m}");
    }
    let some = s.run(
        "edit.find_all",
        json!({"query": "사과", "include": ["header", "footnote"]}),
    )?;
    assert_eq!(kinds(&some), ["footnote", "header"], "{some}");
    assert!(
        s.run(
            "edit.find_all",
            json!({"query": "사과", "include": ["memo"]})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn replace_all_with_include_changes_every_context_in_one_undo_step() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = document()?;
    let before = holding(&mut s, "사과")?;
    assert_eq!(before.len(), 7, "{before:?}");
    // The old scope: body and first-level cells only.
    let r = s.run(
        "edit.replace_all",
        json!({"query": "사과", "replacement": "포도"}),
    )?;
    assert_eq!(r["replaced"], 3, "{r}");
    let left: Vec<String> = holding(&mut s, "사과")?.into_iter().map(|k| k.0).collect();
    assert_eq!(
        left,
        ["header", "footnote", "textbox", "table_cell"],
        "{left:?}"
    );
    s.execute(Command::Undo)?;
    assert_eq!(holding(&mut s, "사과")?, before);

    let r = s.run(
        "edit.replace_all",
        json!({"query": "사과", "replacement": "포도", "include": []}),
    )?;
    assert_eq!(r["replaced"], 7, "{r}");
    assert_eq!(r["skippedProtected"], 0, "{r}");
    assert!(holding(&mut s, "사과")?.is_empty());
    assert_eq!(holding(&mut s, "포도")?.len(), 7);
    let after = holding(&mut s, "포도")?;
    for name in ["everywhere.hwp", "everywhere.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(holding(&mut back, "포도")?, after, "{name}");
    }
    // One step back restores every context.
    s.execute(Command::Undo)?;
    assert_eq!(holding(&mut s, "사과")?, before);
    assert!(holding(&mut s, "포도")?.is_empty());

    // Only the kinds named, and nothing to do is not an error.
    let r = s.run(
        "edit.replace_all",
        json!({"query": "사과", "replacement": "배", "include": ["header"]}),
    )?;
    assert_eq!(r["replaced"], 1, "{r}");
    let r = s.run(
        "edit.replace_all",
        json!({"query": "없는 말", "replacement": "배", "include": []}),
    )?;
    assert_eq!(r["replaced"], 0, "{r}");
    Ok(())
}

#[test]
fn replace_all_with_include_leaves_protected_cells_and_counts_them() -> anyhow::Result<()> {
    let mut s = document()?;
    let found = s.run("edit.find_all", json!({"query": "셀 사과", "include": []}))?;
    let cell = found["matches"][0]["address"].clone();
    let table = json!({"section": cell["section"], "paragraph": cell["paragraph"],
        "control": cell["path"][0]["control"]});
    s.run(
        "table.cell_properties",
        json!({"table": table, "range": {"start_row": 0, "start_col": 1, "end_row": 0, "end_col": 1},
            "props": {"cellProtect": true}}),
    )?;
    let r = s.run(
        "edit.replace_all",
        json!({"query": "사과", "replacement": "포도", "include": []}),
    )?;
    assert_eq!(r["replaced"], 6, "{r}");
    assert_eq!(r["skippedProtected"], 1, "{r}");
    assert_eq!(
        holding(&mut s, "사과")?,
        [("table_cell".to_string(), "셀 사과".to_string())]
    );
    Ok(())
}
