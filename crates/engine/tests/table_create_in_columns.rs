//! A new table in a section with several columns is as wide as the caret's column,
//! as in Hangul, not the whole body: its pieces stay in their columns.
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::{Value, json};

fn create(s: &mut Session) -> anyhow::Result<[f64; 2]> {
    s.run("table.create", json!({"rows": 3, "columns": 2}))?;
    let a: ObjectAddress =
        serde_json::from_value(s.run("table.info", json!({}))?["table"].clone())?;
    let page = s.caret()?["pageIndex"].as_u64().unwrap_or(0) as u32;
    let v: Value = serde_json::from_str(&s.page_control_layout(page)?)?;
    let t = v["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| ObjectAddress::top_level_table(t) == Some(a))
        .expect("the new table is on the caret's page");
    Ok([t["x"].as_f64().unwrap(), t["w"].as_f64().unwrap()])
}

fn columns(s: &Session) -> anyhow::Result<Vec<(f64, f64)>> {
    Ok(s.page_info(0)?["columns"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| (c["x"].as_f64().unwrap(), c["width"].as_f64().unwrap()))
        .collect())
}

#[test]
fn a_table_made_in_two_columns_fits_the_caret_column() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("page.columns.count", json!({"count": 2}))?;
    let cols = columns(&s)?;
    assert_eq!(cols.len(), 2);
    let [x, w] = create(&mut s)?;
    // RHWP's outer margins (283 HU a side) as in one column.
    let inner = cols[0].1 - 2.0 * 283.0 / 75.0;
    assert!((w - inner).abs() < 1.0, "first column {cols:?}: {x} {w}");
    assert!(x >= cols[0].0 - 0.5 && x + w <= cols[0].0 + cols[0].1 + 0.5);

    // In the second column.
    let mut s = Session::new()?;
    s.run("page.columns.count", json!({"count": 2}))?;
    s.run("text.insert", json!({"text": "첫 단"}))?;
    s.run("insert.column_break", json!({}))?;
    let [x, w] = create(&mut s)?;
    let inner = cols[1].1 - 2.0 * 283.0 / 75.0;
    assert!((w - inner).abs() < 1.0, "second column {cols:?}: {x} {w}");
    assert!(x >= cols[1].0 - 0.5 && x + w <= cols[1].0 + cols[1].1 + 0.5);
    Ok(())
}

#[test]
fn a_table_made_in_one_column_still_fills_the_body() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let info = s.page_info(0)?;
    let body = info["bodyRight"].as_f64().unwrap() - info["bodyLeft"].as_f64().unwrap();
    let [_, w] = create(&mut s)?;
    assert!((w - (body - 2.0 * 283.0 / 75.0)).abs() < 1.0, "{body} {w}");
    Ok(())
}
