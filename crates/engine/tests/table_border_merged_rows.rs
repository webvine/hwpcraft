//! `table.move_border` with a cell block on a border that only the block's rows have:
//! every other row's merged cell crosses it (the RFP form: title boxes of different
//! widths over full-width rows). The other rows must keep their layout.
use hwpcraft_engine::Session;
use serde_json::{Value, json};

/// Laid-out (x, w) of every cell on page 1, by (row, col).
fn boxes(s: &Session) -> anyhow::Result<Vec<(u64, u64, f64, f64)>> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let table = layout["controls"]
        .as_array()
        .and_then(|c| c.iter().find(|t| t["type"] == "table"))
        .ok_or_else(|| anyhow::anyhow!("table in layout"))?;
    Ok(table["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            (
                c["row"].as_u64().unwrap_or(0),
                c["col"].as_u64().unwrap_or(0),
                c["x"].as_f64().unwrap_or(0.0),
                c["w"].as_f64().unwrap_or(0.0),
            )
        })
        .collect())
}

fn cell(b: &[(u64, u64, f64, f64)], row: u64, col: u64) -> (f64, f64) {
    b.iter()
        .find(|c| c.0 == row && c.1 == col)
        .map(|c| (c.2, c.3))
        .unwrap_or((f64::NAN, f64::NAN))
}

#[test]
fn a_block_border_that_other_rows_cross_moves_without_breaking_them() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.table", json!({"rows": 3, "columns": 4}))?;
    let table = json!({"section": 0, "paragraph": 0, "control": 2});
    // Row 0: 1 + 3 cells, row 1: 2 + 2, row 2: 3 + 1. Only row 2 has an edge on border 3.
    for (r, c0, c1) in [(0, 1, 3), (1, 0, 1), (1, 2, 3), (2, 0, 2)] {
        s.run(
            "table.merge",
            json!({"table": table, "range": {"start_row": r, "start_col": c0, "end_row": r, "end_col": c1}}),
        )?;
    }
    let before = boxes(&s)?;
    s.run(
        "table.move_border",
        json!({"table": table, "vertical": true, "boundary": 3, "delta_mm": 5.0,
               "range": {"start_row": 2, "start_col": 0, "end_row": 2, "end_col": 3}}),
    )?;
    let after = boxes(&s)?;
    let px = 5.0 * 96.0 / 25.4;
    // Rows 0 and 1 are laid out exactly as before.
    for (r, c) in [(0, 0), (0, 1), (1, 0), (1, 2)] {
        let (x0, w0) = cell(&before, r, c);
        let (x1, w1) = cell(&after, r, c);
        assert!(
            (x0 - x1).abs() < 1.0 && (w0 - w1).abs() < 1.0,
            "cell {r},{c}: {x0},{w0} -> {x1},{w1}"
        );
    }
    // Row 2's box grew by 5 mm and its neighbour gave the space back.
    let (_, grown) = cell(&after, 2, 0);
    let (_, was) = cell(&before, 2, 0);
    assert!((grown - (was + px)).abs() < 1.0, "{was} -> {grown}");
    let right = |b: &[(u64, u64, f64, f64)], r: u64| {
        b.iter()
            .filter(|c| c.0 == r)
            .map(|c| c.2 + c.3)
            .fold(0.0, f64::max)
    };
    for r in 0..3 {
        assert!(
            (right(&after, r) - right(&before, 0)).abs() < 1.0,
            "row {r} ends where the table did"
        );
    }
    let info = s.run("table.info", json!({"table": table}))?;
    let widths: u64 = info["columnWidths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .sum();
    let row0: u64 = info["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["row"] == 0)
        .filter_map(|c| c["width"].as_u64())
        .sum();
    assert_eq!(
        widths, row0,
        "column widths add up to the table width: {info}"
    );
    Ok(())
}
