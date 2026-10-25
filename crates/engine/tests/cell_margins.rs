use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};
#[test]
fn cell_margin_override_preserves_defaults_reflows_and_roundtrips() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows":2,"columns":2}))?;
    s.run("table.properties",json!({"props":{"paddingLeft":567,"paddingRight":850,"paddingTop":1134,"paddingBottom":1417,"tableHeight":17008}}))?;
    s.execute(Command::Insert("셀 여백 테스트".into()))?;
    let a = s.run("table.info", json!({}))?["table"].clone();
    s.select(
        hwpcraft_engine::Cursor {
            offset: 0,
            ..s.cursor
        },
        false,
    )?;
    let old = s.caret()?;
    s.run(
        "table.cell_properties",
        json!({"props":{"applyInnerMargin":true,"paddingLeft":2268}}),
    )?;
    let c = s.run("table.cell_properties.get", json!({}))?;
    assert_eq!(c["paddingLeft"], 2268);
    assert_eq!(c["paddingRight"], 850);
    assert_eq!(c["paddingTop"], 1134);
    assert_eq!(c["paddingBottom"], 1417);
    let after = s.caret()?;
    assert!(
        (after["x"].as_f64().unwrap() - old["x"].as_f64().unwrap() - 1701.0 / 75.0).abs() < 1.0,
        "{old} -> {after}"
    );
    s.execute(Command::Undo)?;
    assert_ne!(
        s.run("table.cell_properties.get", json!({}))?["applyInnerMargin"],
        true
    );
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join(format!("margins.{ext}"));
        s.save(&file)?;
        let mut r = Session::new()?;
        r.open(&file)?;
        let cell = r
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
            .unwrap()
            .0;
        r.select(cell, false)?;
        let actual = r.run("table.cell_properties.get", json!({}))?;
        for k in [
            "paddingLeft",
            "paddingRight",
            "paddingTop",
            "paddingBottom",
            "applyInnerMargin",
        ] {
            assert_eq!(actual[k], c[k], "{ext}: {k}");
        }
    }
    let generation = s.generation;
    let before: Value = s.run("table.info", json!({"table":a}))?;
    assert!(s.run("table.cell_properties",json!({"range":{"start_row":0,"end_row":1,"start_col":0,"end_col":1},"props":{"applyInnerMargin":true,"paddingLeft":14000,"paddingRight":14000}})).is_err());
    assert_eq!(s.generation, generation);
    assert_eq!(s.run("table.info", json!({"table":a}))?, before);
    Ok(())
}
