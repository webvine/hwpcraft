//! `insert.table` hands back the new table's address for follow-up commands.
use hwpcraft_engine::Session;
use serde_json::json;

#[test]
fn insert_table_returns_an_address_table_commands_accept() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 문단"}))?;
    s.run("paragraph.split", json!({}))?;
    let result = s.run("insert.table", json!({"rows": 2, "columns": 3}))?;
    let table = result["table"].clone();
    assert!(table.is_object(), "{result}");
    // Move away, then address the table explicitly.
    s.cursor = Default::default();
    let info = s.run("table.info", json!({"table": table}))?;
    assert_eq!(info["rows"], 2, "{info}");
    assert_eq!(info["columns"], 3, "{info}");
    Ok(())
}

/// `table.edit` takes the table and cell too, so an MCP client need not place the
/// caret in the cell first; a refused edit leaves the document and caret alone.
#[test]
fn table_edit_takes_a_table_address_and_cell() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 문단"}))?;
    s.run("paragraph.split", json!({}))?;
    let result = s.run("insert.table", json!({"rows": 2, "columns": 3}))?;
    let table = result["table"].clone();
    s.cursor = Default::default();
    s.run(
        "table.edit",
        json!({"action": "row_below", "table": table, "row": 1, "col": 2}),
    )?;
    s.run(
        "table.edit",
        json!({"action": "delete_column", "table": table, "row": 0, "col": 0}),
    )?;
    let info = s.run("table.info", json!({"table": table}))?;
    assert_eq!(
        (info["rows"].clone(), info["columns"].clone()),
        (json!(3), json!(2))
    );
    s.cursor = Default::default();
    let generation = s.generation;
    for params in [
        json!({"action": "row_below", "table": table, "row": 5, "col": 0}),
        json!({"action": "row_below", "table": table, "row": 0, "col": 2}),
        json!({"action": "row_below", "table": table, "row": 0}),
        json!({"action": "row_below", "table": table}),
    ] {
        assert!(s.run("table.edit", params.clone()).is_err(), "{params}");
        assert_eq!(s.generation, generation, "{params}");
        assert_eq!(s.cursor, Default::default(), "{params}");
    }
    Ok(())
}
