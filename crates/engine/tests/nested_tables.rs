//! Owner: shell/page area. `context.table.move_border`: a nested table's grid line moves
//! like a body table's (whole, or only for a cell block), with undo and a round trip.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::table::Table;
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

/// The nested table at `host` (cell-paragraph address) and `control`.
fn nested<'a>(s: &'a Session, host: &Value, control: usize) -> anyhow::Result<&'a Table> {
    let doc = s.core.document();
    let mut para = &doc.sections[0].paragraphs[host["paragraph"].as_u64().unwrap_or(0) as usize];
    for step in host["path"].as_array().into_iter().flatten() {
        let n = |k: &str| step[k].as_u64().unwrap_or(0) as usize;
        let Some(Control::Table(t)) = para.controls.get(n("control")) else {
            anyhow::bail!("path table");
        };
        para = &t.cells[n("cell")].paragraphs[n("paragraph")];
    }
    match para.controls.get(control) {
        Some(Control::Table(t)) => Ok(t),
        _ => anyhow::bail!("nested table"),
    }
}

/// Cell widths of a table, row by row.
fn widths(t: &Table) -> Vec<Vec<u32>> {
    (0..t.row_count)
        .map(|r| {
            let mut cells: Vec<_> = t.cells.iter().filter(|c| c.row == r).collect();
            cells.sort_by_key(|c| c.col);
            cells.iter().map(|c| c.width).collect()
        })
        .collect()
}

#[test]
fn a_nested_table_border_moves_whole_or_for_a_block() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    let created = s.run(
        "context.table.create",
        json!({"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
               "rows":3,"columns":2}),
    )?;
    // The first cell of the new table: its last step names the table.
    let first = &created["address"];
    let steps = first["path"].as_array().cloned().unwrap_or_default();
    anyhow::ensure!(steps.len() == 2, "{created}");
    let host = json!({"section": 0, "paragraph": first["paragraph"], "path": [steps[0].clone()]});
    let control = steps[1]["control"].as_u64().unwrap_or(0) as usize;
    let before = widths(nested(&s, &host, control)?);
    let (a, b) = (before[0][0], before[0][1]);

    // The whole border: 5 mm (1417 HWPUNIT) from column 2 to column 1 in every row.
    s.run(
        "context.table.move_border",
        json!({"address": host, "control": control, "vertical": true, "boundary": 1, "delta_mm": 5.0}),
    )?;
    let step = 1417;
    let moved = widths(nested(&s, &host, control)?);
    assert!(
        moved.iter().all(|row| row == &[a + step, b - step]),
        "{moved:?}"
    );
    s.execute(Command::Undo)?;
    assert_eq!(widths(nested(&s, &host, control)?), before);

    // A block (row 2 of the nested table): only that row's border moves.
    s.run(
        "context.table.move_border",
        json!({"address": host, "control": control, "vertical": true, "boundary": 1, "delta_mm": 5.0,
               "range": {"start_row": 1, "start_col": 0, "end_row": 1, "end_col": 1}}),
    )?;
    let check = |s: &Session| -> anyhow::Result<()> {
        let t = nested(s, &host, control)?;
        let w = widths(t);
        assert_eq!(w[0], [a, b], "{w:?}");
        assert_eq!(w[1], [a + step, b - step], "{w:?}");
        assert_eq!(w[2], [a, b], "{w:?}");
        Ok(())
    };
    check(&s)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("nested.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        check(&r)?;
    }
    // A body table is not a nested one; a horizontal block move is refused.
    assert!(
        s.run(
            "context.table.move_border",
            json!({"address":{"section":0,"paragraph":0,"path":[]},"control":2,"vertical":true,"boundary":1,"delta_mm":3.0}),
        )
        .is_err()
    );
    assert!(
        s.run(
            "context.table.move_border",
            json!({"address": host, "control": control, "vertical": false, "boundary": 1, "delta_mm": 1.0,
                   "range": {"start_row": 0, "start_col": 0, "end_row": 0, "end_col": 0}}),
        )
        .is_err()
    );
    Ok(())
}
