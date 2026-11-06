//! Equal cell heights for part of a table split across pages is refused with a
//! message about heights (not about moving a border), and nothing changes.
use hwpcraft_engine::Session;
use serde_json::json;

#[test]
fn partial_equal_height_on_a_split_table_says_why() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows": 120, "columns": 2}))?;
    assert!(s.page_count()? > 1, "the table runs across pages");
    let before = s.run("table.info", json!({}))?;
    let generation = s.generation;
    let e = s
        .run(
            "table.equal_height",
            json!({"range":{"start_row":0,"start_col":0,"end_row":2,"end_col":0}}),
        )
        .expect_err("refused")
        .to_string();
    assert!(e.contains("셀 높이"), "{e}");
    assert!(!e.contains("경계선"), "{e}");
    assert_eq!(s.run("table.info", json!({}))?, before);
    assert_eq!(s.generation, generation);
    Ok(())
}
