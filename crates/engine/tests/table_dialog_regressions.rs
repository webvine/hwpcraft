use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn table(rows: u16, columns: u16) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows":rows,"columns":columns}))?;
    Ok(s)
}

fn props(s: &mut Session) -> anyhow::Result<(Value, Value)> {
    Ok((
        s.run("table.properties.get", json!({}))?,
        s.run("table.cell_properties.get", json!({}))?,
    ))
}

#[test]
fn table_dialog_applies_all_tabs_in_one_undo_step() -> anyhow::Result<()> {
    let mut s = table(2, 2)?;
    let before = props(&mut s)?;
    s.run(
        "table.properties.apply",
        json!({
            "table_props":{"outerLeft":567,"horzOffset":-1417},
            "cell_props":{"fillType":"solid","fillColor":"#ffcc00"}
        }),
    )?;
    let after = props(&mut s)?;
    assert_eq!(after.0["outerLeft"], 567);
    assert_eq!(after.0["horzOffset"], -1417);
    assert_eq!(after.1["fillColor"], "#ffcc00");
    s.execute(Command::Undo)?;
    assert_eq!(props(&mut s)?, before);
    s.execute(Command::Redo)?;
    assert_eq!(props(&mut s)?, after);
    Ok(())
}

#[test]
fn failed_table_dialog_preserves_document_and_redo() -> anyhow::Result<()> {
    let mut s = table(2, 2)?;
    s.run(
        "table.cell_properties",
        json!({"props":{"fillColor":"#abcdef"}}),
    )?;
    let after = props(&mut s)?;
    s.execute(Command::Undo)?;
    let before = props(&mut s)?;
    let generation = s.generation;
    assert!(
        s.run(
            "table.properties.apply",
            json!({
                "table_props":{"outerLeft":567}, "cell_props":{"fillColor":"invalid"}
            })
        )
        .is_err()
    );
    assert_eq!(props(&mut s)?, before);
    assert_eq!(s.generation, generation);
    s.execute(Command::Redo)?;
    assert_eq!(props(&mut s)?, after);
    Ok(())
}

#[test]
fn unchanged_table_dialog_adds_no_history() -> anyhow::Result<()> {
    let mut s = table(2, 2)?;
    let generation = s.generation;
    s.run("table.properties.apply", json!({}))?;
    assert_eq!(s.generation, generation);
    s.execute(Command::Undo)?;
    assert!(s.run("table.info", json!({})).is_err());
    Ok(())
}

#[test]
fn merge_before_split_uses_the_whole_selected_block() -> anyhow::Result<()> {
    let mut s = table(2, 3)?;
    s.run(
        "table.cells.set",
        json!({"cells":[["가","나","밖"],["다","라","끝"]]}),
    )?;
    let before = s.run("table.info", json!({}))?;
    let range = json!({"start_row":0,"start_col":0,"end_row":0,"end_col":1});
    let width = before["cells"][0]["width"].as_u64().unwrap()
        + before["cells"][1]["width"].as_u64().unwrap();
    s.run(
        "table.split",
        json!({"range":range,"rows":2,"columns":1,"merge_first":true}),
    )?;
    let after = s.run("table.info", json!({}))?;
    let cells = after["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 6, "merge two cells, then split into two");
    let split: Vec<_> = cells
        .iter()
        .filter(|c| c["col"] == 0 && c["row"].as_u64().unwrap() < 2)
        .collect();
    assert_eq!(split.len(), 2);
    assert!(
        split
            .iter()
            .all(|c| c["width"] == width && c["colSpan"] == 2),
        "{after}"
    );
    let texts: Vec<_> = s.entries().into_iter().map(|(_, text)| text).collect();
    for text in ["가", "나", "밖", "다", "라", "끝"] {
        assert!(
            texts.iter().any(|t| t.contains(text)),
            "lost {text}: {texts:?}"
        );
    }
    s.execute(Command::Undo)?;
    assert_eq!(s.run("table.info", json!({}))?, before);
    Ok(())
}

#[test]
fn splitting_a_merged_cell_at_the_column_limit_is_allowed() -> anyhow::Result<()> {
    let mut s = table(1, 64)?;
    s.run(
        "table.merge",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":63}}),
    )?;
    s.run("table.split", json!({"rows":1,"columns":64}))?;
    let info = s.run("table.info", json!({}))?;
    assert_eq!(info["columns"], 64);
    assert_eq!(info["cells"].as_array().unwrap().len(), 64);
    Ok(())
}

#[test]
fn every_custom_column_must_meet_the_minimum_width() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let generation = s.generation;
    assert!(
        s.run(
            "table.create",
            json!({"rows":1,"columns":2,"column_widths_mm":[0.1,20.0]})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert!(!s.can_undo());
    Ok(())
}

#[test]
fn total_height_accepts_the_same_range_as_table_creation() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":100,"columns":2,"height_mm":700.0}),
    )?;
    let before = props(&mut s)?.0;
    let created_height = (700.0_f64 * 7200.0 / 25.4).round() as i64;
    assert!((before["tableHeight"].as_i64().unwrap() - created_height).abs() <= 100);
    let target = (800.0_f64 * 7200.0 / 25.4).round() as i64;
    s.run(
        "table.properties.apply",
        json!({"table_props":{"tableHeight":target}}),
    )?;
    let after = props(&mut s)?.0;
    assert!((after["tableHeight"].as_i64().unwrap() - target).abs() <= 100);
    s.execute(Command::Undo)?;
    assert_eq!(props(&mut s)?.0, before);
    Ok(())
}

#[test]
fn combined_properties_survive_hwp_and_hwpx_roundtrip() -> anyhow::Result<()> {
    let mut s = table(2, 3)?;
    s.run(
        "table.properties.apply",
        json!({
            "range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1},
            "table_props":{"horzOffset":-1417,"vertOffset":-567,"outerLeft":567},
            "cell_props":{"fillType":"solid","fillColor":"#ffcc00"},
            "borders":{"bottom":{"type":1,"width":5,"color":"#123456"}}
        }),
    )?;
    let before = props(&mut s)?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("properties.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let first = r
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
            .unwrap()
            .0;
        r.select(first, false)?;
        let after = props(&mut r)?;
        for key in ["horzOffset", "vertOffset", "outerLeft"] {
            assert_eq!(after.0[key], before.0[key], "{ext}: {key}");
        }
        for key in ["fillColor", "borderBottom"] {
            assert_eq!(after.1[key], before.1[key], "{ext}: {key}");
        }
    }
    Ok(())
}
