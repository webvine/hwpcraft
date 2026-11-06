//! A table on one page moves part of a horizontal border although a table nested in
//! another table on the next page carries the same section/paragraph/control numbers
//! in the page layout: that nested table is not a piece of it.
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::{Value, json};

#[test]
fn a_nested_table_with_the_same_numbers_is_not_the_next_piece() -> anyhow::Result<()> {
    // T1 (paragraph 1) on page 1; T2 on page 2 holds a nested table whose layout
    // entry reads section 0, paragraph 1, control 0, as T1.
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞"}))?;
    s.run("paragraph.split", json!({}))?;
    s.run("paragraph.split", json!({}))?;
    s.run(
        "text.insert",
        json!({"text": "쪽을 넘기는 긴 문단입니다. ".repeat(130)}),
    )?;
    s.run("paragraph.split", json!({}))?;
    s.run("table.create", json!({"rows": 2, "columns": 2}))?;
    s.run(
        "selection.set",
        json!({"cursor":{"section":0,"paragraph":1,"offset":0}}),
    )?;
    s.run("table.create", json!({"rows": 2, "columns": 2}))?;
    let a: ObjectAddress =
        serde_json::from_value(s.run("table.info", json!({}))?["table"].clone())?;
    s.run(
        "context.table.create",
        json!({"address":{"section":0,"paragraph":4,
            "path":[{"control":0,"cell":0,"paragraph":0}]},"rows":2,"columns":2}),
    )?;
    let v: Value = serde_json::from_str(&s.page_control_layout(1)?)?;
    assert!(
        v["controls"].as_array().into_iter().flatten().any(|t| {
            t["secIdx"] == a.section && t["paraIdx"] == a.paragraph && t["controlIdx"] == a.control
        }),
        "the nested table's entry reads as T1"
    );
    // Taller rows than the text needs, so the edge has room to move.
    s.run(
        "table.move_border",
        json!({"table": a, "vertical": false, "boundary": 2, "delta_mm": 10.0}),
    )?;
    // The first column only: the edge under row 0 moves 3 mm down.
    s.run(
        "table.move_border",
        json!({"table": a, "vertical": false, "boundary": 1, "delta_mm": 3.0,
               "range": {"start_row": 0, "start_col": 0, "end_row": 0, "end_col": 0}}),
    )?;
    Ok(())
}
