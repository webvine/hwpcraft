//! Owner: shell/page area. `table.move_border` with a cell block: only the block's part of
//! the border moves (Hangul's ragged edge), with one undo step and an HWP/HWPX round trip.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

const TABLE: &str = r#"{"section":0,"paragraph":0,"control":2}"#;

/// Laid-out (x, w) of the cell at (row, col) on page 1.
fn cell_box(s: &Session, row: u64, col: u64) -> anyhow::Result<(f64, f64)> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let table = layout["controls"]
        .as_array()
        .and_then(|c| c.iter().find(|t| t["type"] == "table"))
        .ok_or_else(|| anyhow::anyhow!("table in layout"))?;
    let cell = table["cells"]
        .as_array()
        .and_then(|cells| {
            cells
                .iter()
                .find(|c| c["row"].as_u64() == Some(row) && c["col"].as_u64() == Some(col))
        })
        .ok_or_else(|| anyhow::anyhow!("cell {row},{col}"))?;
    Ok((
        cell["x"].as_f64().unwrap_or(0.0),
        cell["w"].as_f64().unwrap_or(0.0),
    ))
}

#[test]
fn a_border_moves_only_for_the_cell_block() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 3, "columns": 3}))?;
    let table: Value = serde_json::from_str(TABLE)?;
    let (_, w0) = cell_box(&s, 0, 0)?;
    // Row 2 only: the border between columns 1 and 2 moves 10 mm right.
    s.run(
        "table.move_border",
        json!({"table": table, "vertical": true, "boundary": 1, "delta_mm": 10.0,
               "range": {"start_row": 1, "start_col": 0, "end_row": 1, "end_col": 2}}),
    )?;
    let check = |s: &Session| -> anyhow::Result<()> {
        let px = 10.0 * 96.0 / 25.4;
        let (_, top) = cell_box(s, 0, 0)?;
        let (_, moved) = cell_box(s, 1, 0)?;
        let (_, below) = cell_box(s, 2, 0)?;
        assert!(
            (top - w0).abs() < 1.0,
            "row 1 keeps its border: {top} vs {w0}"
        );
        assert!(
            (below - w0).abs() < 1.0,
            "row 3 keeps its border: {below} vs {w0}"
        );
        assert!(
            (moved - (w0 + px)).abs() < 1.0,
            "row 2 moved: {moved} vs {}",
            w0 + px
        );
        // The neighbour in row 2 shrank by as much; the row still ends where it did.
        let (x1, w1) = cell_box(s, 1, 2)?;
        let (x0, w) = cell_box(s, 0, 1)?;
        assert!(((x1 + w1) - (x0 + w)).abs() < 1.0, "same right edge");
        Ok(())
    };
    check(&s)?;
    // Typing in a cell of the ragged table keeps its layout.
    s.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: Some(hwpcraft_doc::Cell {
                control: 2,
                index: 0,
                paragraph: 0,
            }),
        },
        false,
    )?;
    s.execute(Command::Insert("글".into()))?;
    check(&s)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("ragged.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        check(&r)?;
    }
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    let (_, again) = cell_box(&s, 1, 0)?;
    assert!((again - w0).abs() < 1.0, "undone");
    // A block that covers every row moves the whole border, as before.
    s.run(
        "table.move_border",
        json!({"table": table, "vertical": true, "boundary": 1, "delta_mm": 5.0,
               "range": {"start_row": 0, "start_col": 0, "end_row": 2, "end_col": 0}}),
    )?;
    let (_, a) = cell_box(&s, 0, 0)?;
    let (_, b) = cell_box(&s, 2, 0)?;
    assert!((a - b).abs() < 1.0 && (a - w0).abs() > 10.0);
    // The table's outer edge cannot be moved for part of it.
    assert!(
        s.run(
            "table.move_border",
            json!({"table": table, "vertical": true, "boundary": 3, "delta_mm": 5.0,
                   "range": {"start_row": 1, "start_col": 0, "end_row": 1, "end_col": 2}}),
        )
        .is_err()
    );
    Ok(())
}

/// Right edges of the cells of each laid-out row on page 1, sorted.
fn row_edges(s: &Session) -> anyhow::Result<Vec<Vec<i64>>> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let table = layout["controls"]
        .as_array()
        .and_then(|c| c.iter().find(|t| t["type"] == "table"))
        .ok_or_else(|| anyhow::anyhow!("table in layout"))?;
    let rows = table["rowCount"].as_u64().unwrap_or(0);
    Ok((0..rows)
        .map(|r| {
            let mut edges: Vec<i64> = table["cells"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["row"].as_u64() == Some(r))
                .map(|c| {
                    (c["x"].as_f64().unwrap_or(0.0) + c["w"].as_f64().unwrap_or(0.0)).round() as i64
                })
                .collect();
            edges.sort();
            edges
        })
        .collect())
}

#[test]
fn rows_and_columns_go_into_a_ragged_table_on_its_grid() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 3, "columns": 3}))?;
    let table: Value = serde_json::from_str(TABLE)?;
    s.run(
        "table.move_border",
        json!({"table": table, "vertical": true, "boundary": 1, "delta_mm": 10.0,
               "range": {"start_row": 1, "start_col": 0, "end_row": 1, "end_col": 2}}),
    )?;
    let before = row_edges(&s)?;
    // A row below the first one copies the first row's edges (RHWP patch 0027: the
    // thin grid column's width comes from the merged cells, not a 1800 guess).
    s.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: Some(hwpcraft_doc::Cell {
                control: 2,
                index: 0,
                paragraph: 0,
            }),
        },
        false,
    )?;
    s.run("table.edit", json!({"action": "row_below"}))?;
    let after = row_edges(&s)?;
    assert_eq!(after.len(), 4);
    assert_eq!(after[0], before[0], "first row unchanged");
    assert_eq!(after[2], before[1], "moved row unchanged");
    assert_eq!(after[3], before[2], "last row unchanged");
    // The new row lies on the table grid: every edge is one some other row has.
    let known: Vec<i64> = after
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .flat_map(|(_, row)| row.clone())
        .collect();
    assert!(
        after[1]
            .iter()
            .all(|e| known.iter().any(|k| (k - e).abs() <= 1)),
        "{after:?}"
    );
    assert_eq!(after[1].last(), after[0].last(), "same right edge");
    // A column right of the first one: every row still ends at one right edge.
    s.run("table.edit", json!({"action": "column_right"}))?;
    let widened = row_edges(&s)?;
    let right = widened[0].last().copied();
    assert!(
        widened.iter().all(|row| row.last().copied() == right),
        "{widened:?}"
    );
    Ok(())
}
