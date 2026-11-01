//! A row of a new table (282 HU, as in Hangul) grown by 1 mm shrinks back to where it
//! started; the resize used to refuse anything under 283 HU.
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn rows(s: &mut Session) -> anyhow::Result<Value> {
    Ok(s.run("table.info", json!({}))?["rowHeights"].clone())
}

#[test]
fn a_new_row_grown_and_shrunk_by_1_mm_is_back_where_it_started() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows": 2, "columns": 2}))?;
    let before = rows(&mut s)?;
    s.run("table.resize", json!({"vertical": true, "delta_mm": 1.0}))?;
    assert_ne!(rows(&mut s)?, before);
    s.run("table.resize", json!({"vertical": true, "delta_mm": -1.0}))?;
    assert_eq!(rows(&mut s)?, before);
    // Below a new row's height is still refused.
    assert!(
        s.run("table.resize", json!({"vertical": true, "delta_mm": -1.0}))
            .is_err()
    );
    Ok(())
}
