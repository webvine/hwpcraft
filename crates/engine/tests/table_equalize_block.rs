use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn info(s: &mut Session) -> anyhow::Result<Value> {
    s.run("table.info", json!({}))
}
fn widths(s: &mut Session) -> anyhow::Result<Vec<u64>> {
    Ok(info(s)?["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["width"].as_u64().unwrap())
        .collect())
}

#[test]
fn equal_width_changes_only_selected_physical_cells() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":2,"columns":3,"column_widths_mm":[20.0,40.0,60.0]}),
    )?;
    let before = info(&mut s)?;
    let old = widths(&mut s)?;
    s.run(
        "table.equal_width",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1}}),
    )?;
    let after = widths(&mut s)?;
    assert!(after[0].abs_diff(after[1]) <= 1);
    assert_eq!(after[0] + after[1], old[0] + old[1]);
    assert_eq!(
        &after[2..],
        &old[2..],
        "unselected row and column must keep their widths"
    );
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("equal.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let c = r
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
            .unwrap()
            .0;
        r.select(c, false)?;
        assert_eq!(widths(&mut r)?, after, "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_eq!(info(&mut s)?, before);
    s.execute(Command::Redo)?;
    assert_eq!(widths(&mut s)?, after);
    Ok(())
}

#[test]
fn equal_width_removes_a_ragged_border_without_squeezing_other_rows() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":3,"columns":3,"width_mm":120.0}),
    )?;
    let before = widths(&mut s)?;
    s.run(
        "table.move_border",
        json!({"vertical":true,"boundary":1,"delta_mm":1.0,
        "range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1}}),
    )?;
    assert_eq!(info(&mut s)?["columns"], 4);
    s.run(
        "table.equal_width",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":2}}),
    )?;
    assert_eq!(widths(&mut s)?, before);
    assert_eq!(info(&mut s)?["columns"], 3);
    Ok(())
}

#[test]
fn equal_height_preserves_the_other_column() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":3,"columns":2,"width_mm":80.0,"height_mm":30.0}),
    )?;
    s.run(
        "table.move_border",
        json!({"vertical":false,"boundary":1,"delta_mm":10.0}),
    )?;
    let before = info(&mut s)?;
    s.run(
        "table.equal_height",
        json!({"range":{"start_row":0,"start_col":0,"end_row":2,"end_col":0}}),
    )?;
    let after = info(&mut s)?;
    let old = before["cells"].as_array().unwrap();
    let new = after["cells"].as_array().unwrap();
    for i in [1, 3, 5] {
        assert_eq!(new[i]["height"], old[i]["height"]);
    }
    let heights: Vec<_> = [0, 2, 4]
        .into_iter()
        .map(|i| new[i]["height"].as_u64().unwrap())
        .collect();
    assert!(
        heights.iter().all(|h| h.abs_diff(heights[0]) <= 1),
        "{heights:?}"
    );
    s.execute(Command::Undo)?;
    assert_eq!(info(&mut s)?, before);
    Ok(())
}

#[test]
fn partial_equalization_noop_and_rejection_keep_history() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows":2,"columns":2}))?;
    let range = json!({"start_row":0,"start_col":0,"end_row":0,"end_col":1});
    let generation = s.generation;
    s.run("table.equal_width", json!({"range":range}))?;
    assert_eq!(s.generation, generation);
    s.run("table.properties", json!({"props":{"cellSpacing":100}}))?;
    let before = info(&mut s)?;
    let generation = s.generation;
    assert!(s.run("table.equal_width", json!({"range":range})).is_err());
    assert_eq!(s.generation, generation);
    assert_eq!(info(&mut s)?, before);
    Ok(())
}

#[test]
fn equal_height_refuses_to_shrink_below_cell_contents() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows":2,"columns":2,"height_mm":20.0}),
    )?;
    s.run(
        "table.cells.set",
        json!({"cells":[["가\n나\n다\n라\n마\n바\n사\n아","옆"],["아래","끝"]]}),
    )?;
    let before = info(&mut s)?;
    let generation = s.generation;
    assert!(
        s.run(
            "table.equal_height",
            json!({"range":{"start_row":0,"start_col":0,"end_row":1,"end_col":0}})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert_eq!(info(&mut s)?, before);
    Ok(())
}
