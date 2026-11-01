//! A table inside a table cell is merged, split, grown and shrunk by rows and
//! columns and given cell backgrounds like a top-level one (context.table.*), each
//! edit one undo step, and the result survives HWP and HWPX.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

/// A body 2×2 table whose first cell holds a 2×3 table; returns the address of the
/// cell paragraph holding the nested table and the nested table's control index.
fn nested(s: &mut Session) -> anyhow::Result<(Value, usize)> {
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    let created = s.run(
        "context.table.create",
        json!({"address": {"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
               "rows": 2, "columns": 3}),
    )?;
    // The result names the new table's first cell: its last step's control is the
    // table, and the steps before it name the cell paragraph that holds it.
    let address = &created["address"];
    let steps = address["path"].as_array().cloned().unwrap_or_default();
    anyhow::ensure!(steps.len() == 2, "{created}");
    let control = steps[1]["control"].as_u64().unwrap_or(0) as usize;
    let host = json!({"section": address["section"], "paragraph": address["paragraph"],
                      "path": [steps[0].clone()]});
    Ok((host, control))
}

fn info(s: &mut Session, host: &Value, control: usize) -> anyhow::Result<Value> {
    s.run(
        "context.table.info",
        json!({"address": host, "control": control}),
    )
}

fn cell(v: &Value, row: u64, col: u64) -> Value {
    v["cells"]
        .as_array()
        .and_then(|cells| {
            cells
                .iter()
                .find(|c| c["row"] == row && c["col"] == col)
                .cloned()
        })
        .unwrap_or(Value::Null)
}

#[test]
fn a_nested_table_takes_block_edits_like_a_top_level_one() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let (host, control) = nested(&mut s)?;
    let op = |id: &str, extra: Value| {
        let mut p = json!({"address": host, "control": control});
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            p[k] = v;
        }
        (id.to_owned(), p)
    };

    // Merge the first row's first two cells.
    let (id, p) = op(
        "context.table.merge",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1}}),
    );
    s.run(&id, p)?;
    let v = info(&mut s, &host, control)?;
    assert_eq!(cell(&v, 0, 0)["colSpan"], 2, "{v}");
    assert_eq!(cell(&v, 0, 1), Value::Null, "{v}");
    // One undo takes the merge back.
    s.execute(Command::Undo)?;
    assert_eq!(cell(&info(&mut s, &host, control)?, 0, 0)["colSpan"], 1);
    s.execute(Command::Redo)?;

    // Split the merged cell back into two, add a row, drop a column.
    let (id, p) = op(
        "context.table.split",
        json!({"row":0,"col":0,"rows":1,"columns":2}),
    );
    s.run(&id, p)?;
    let v = info(&mut s, &host, control)?;
    assert_eq!(
        (v["rows"].clone(), v["columns"].clone()),
        (json!(2), json!(3)),
        "{v}"
    );
    assert_eq!(cell(&v, 0, 1)["colSpan"], 1, "{v}");
    let (id, p) = op("context.table.insert_row", json!({"row":1}));
    s.run(&id, p)?;
    let (id, p) = op("context.table.delete_column", json!({"col":2}));
    s.run(&id, p)?;
    let v = info(&mut s, &host, control)?;
    assert_eq!(
        (v["rows"].clone(), v["columns"].clone()),
        (json!(3), json!(2)),
        "{v}"
    );

    // A background on a block of cells.
    let (id, p) = op(
        "context.table.cells",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1},
               "style":{"fillType":"solid","fillColor":"#dbe5f1"}}),
    );
    s.run(&id, p)?;

    // Out-of-range and impossible requests change nothing.
    let generation = s.generation;
    for (id, extra) in [
        (
            "context.table.merge",
            json!({"range":{"start_row":0,"start_col":0,"end_row":9,"end_col":0}}),
        ),
        ("context.table.delete_row", json!({})),
        (
            "context.table.cells",
            json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}}),
        ),
    ] {
        let (id, p) = op(id, extra);
        assert!(s.run(&id, p).is_err(), "{id}");
    }
    assert_eq!(s.generation, generation);
    let body = json!({"section":0,"paragraph":0,"path":[]});
    assert!(
        s.run(
            "context.table.merge",
            json!({"address": body, "control": 2,
              "range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1}})
        )
        .is_err(),
        "a body table goes through table.merge"
    );

    let expected = info(&mut s, &host, control)?;
    for name in ["n.hwp", "n.hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut again = Session::new()?;
        again.open(&path)?;
        assert_eq!(info(&mut again, &host, control)?, expected, "{name}");
    }
    Ok(())
}
