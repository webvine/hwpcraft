//! Owner: format area. 상호 참조 and 날짜 필드 go into a table cell as into the body:
//! listed, refreshed and kept on save. Before, both were refused in a cell.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Cell, Cursor, Session};
use serde_json::json;

/// (paragraph, control) of the n-th table in body order.
fn table(s: &Session, n: usize) -> (usize, usize) {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(p, para)| {
            para.controls
                .iter()
                .enumerate()
                .filter(|(_, c)| matches!(c, Control::Table(_)))
                .map(move |(c, _)| (p, c))
        })
        .nth(n)
        .expect("table")
}

fn in_cell(s: &mut Session, n: usize, index: usize) {
    let (paragraph, control) = table(s, n);
    s.cursor = Cursor {
        section: 0,
        paragraph,
        offset: 0,
        cell: Some(Cell {
            control,
            index,
            paragraph: 0,
        }),
    };
}

fn cell_text(s: &Session, n: usize, index: usize) -> String {
    let (paragraph, control) = table(s, n);
    match &s.core.document().sections[0].paragraphs[paragraph].controls[control] {
        Control::Table(t) => t.cells[index].paragraphs[0].text.clone(),
        _ => unreachable!(),
    }
}

#[test]
fn cross_references_and_date_fields_work_in_a_table_cell() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    // Page 1: a 2x1 table to write into. Page 2: the captioned table 1 it points at.
    s.run("text.insert", json!({"text": "요약\n"}))?;
    s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("insert.page_break", json!({}))?;
    s.run("text.insert", json!({"text": "결과\n"}))?;
    s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
    in_cell(&mut s, 1, 0);
    s.run("table.properties", json!({"props": {"hasCaption": true}}))?;

    in_cell(&mut s, 0, 0);
    let made = s.run("insert.cross_reference", json!({"target": {"table": 1}}))?;
    assert_eq!(made["text"], "2", "{made}");
    assert_eq!(cell_text(&s, 0, 0), "2");
    in_cell(&mut s, 0, 1);
    let date = s.run("insert.date_field", json!({"kind": "created"}))?;
    assert!(!date["text"].as_str().unwrap_or("").is_empty(), "{date}");

    let list = s.run("tools.cross_reference.list", json!({}))?;
    assert_eq!(list["items"][0]["cell"]["index"], 0, "{list}");

    // Push table 1 to page 3 and refresh: the cell follows.
    s.cursor = Cursor {
        section: 0,
        paragraph: table(&s, 1).0 - 1,
        offset: 0,
        cell: None,
    };
    s.run("insert.page_break", json!({}))?;
    let refreshed = s.run("tools.cross_reference.update", json!({}))?;
    assert_eq!(refreshed["updated"], 1, "{refreshed}");
    assert_eq!(cell_text(&s, 0, 0), "3");
    // One undo takes the refresh back.
    s.run("edit.undo", json!({}))?;
    assert_eq!(cell_text(&s, 0, 0), "2");
    s.run("edit.redo", json!({}))?;
    assert_eq!(cell_text(&s, 0, 0), "3");
    let dates = s.run("tools.date_field.update", json!({}))?;
    assert_eq!(dates["fields"], 1, "the cell date field is found: {dates}");

    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("cells.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        let items = back.cross_references();
        assert_eq!(items.len(), 1, "{ext}: {items:?}");
        assert_eq!(items[0]["text"], "3", "{ext}");
        assert_eq!(cell_text(&back, 0, 0), "3", "{ext}");
    }
    Ok(())
}
