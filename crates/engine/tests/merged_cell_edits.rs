//! A row below or a column right of a merged cell goes after the merge, as in
//! Hangul, and Tab in the last cell appends a row even when the last row's first
//! cell is merged from above.
use hwpcraft_doc::{Cell, Cursor};
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn span_at(info: &Value, row: u64, col: u64) -> (Value, Value) {
    info["cells"]
        .as_array()
        .and_then(|cells| cells.iter().find(|c| c["row"] == row && c["col"] == col))
        .map(|c| (c["rowSpan"].clone(), c["colSpan"].clone()))
        .unwrap_or_default()
}

fn merged_table(s: &mut Session, range: Value) -> anyhow::Result<Value> {
    let table = s.run("insert.table", json!({"rows": 3, "columns": 3}))?["table"].clone();
    s.run("table.merge", json!({"table": table, "range": range}))?;
    Ok(table)
}

#[test]
fn a_row_below_a_merged_cell_goes_after_the_merge() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let range = json!({"start_row": 0, "start_col": 0, "end_row": 1, "end_col": 0});
    let table = merged_table(&mut s, range)?;
    s.run(
        "table.edit",
        json!({"action": "row_below", "table": table, "row": 0, "col": 0}),
    )?;
    let info = s.run("table.info", json!({"table": table}))?;
    assert_eq!(info["rows"], 4);
    assert_eq!(
        span_at(&info, 0, 0),
        (json!(2), json!(1)),
        "merge unchanged"
    );
    assert_eq!(span_at(&info, 2, 0), (json!(1), json!(1)), "new row at 2");
    Ok(())
}

#[test]
fn a_column_right_of_a_merged_cell_goes_after_the_merge() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let range = json!({"start_row": 0, "start_col": 0, "end_row": 0, "end_col": 1});
    let table = merged_table(&mut s, range)?;
    s.run(
        "table.edit",
        json!({"action": "column_right", "table": table, "row": 0, "col": 0}),
    )?;
    let info = s.run("table.info", json!({"table": table}))?;
    assert_eq!(info["columns"], 4);
    assert_eq!(
        span_at(&info, 0, 0),
        (json!(1), json!(2)),
        "merge unchanged"
    );
    assert_eq!(
        span_at(&info, 0, 2),
        (json!(1), json!(1)),
        "new column at 2"
    );
    Ok(())
}

#[test]
fn tab_in_the_last_cell_appends_a_row_after_a_merge_from_above() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let range = json!({"start_row": 1, "start_col": 0, "end_row": 2, "end_col": 0});
    let table = merged_table(&mut s, range)?;
    // Caret to the last cell (bottom row, last column), then Tab.
    let info = s.run("table.info", json!({"table": table}))?;
    let index = info["cells"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["row"] == 2 && c["col"] == 2))
        .and_then(|c| c["index"].as_u64())
        .ok_or_else(|| anyhow::anyhow!("{info}"))?;
    let at = |k: &str| table[k].as_u64().unwrap_or(0) as usize;
    s.select(
        Cursor {
            section: at("section"),
            paragraph: at("paragraph"),
            offset: 0,
            cell: Some(Cell {
                control: at("control"),
                index: index as usize,
                paragraph: 0,
            }),
        },
        false,
    )?;
    let rows = 3;
    let moved = s.run("table.cell_move", json!({"table": table, "motion": "next"}))?;
    assert_eq!(moved["appended"], true, "{moved}");
    let info = s.run("table.info", json!({"table": table}))?;
    assert_eq!(info["rows"], rows + 1);
    assert_eq!(
        span_at(&info, 1, 0),
        (json!(2), json!(1)),
        "merge unchanged"
    );
    assert_eq!(moved["row"], rows, "{moved}");
    Ok(())
}
