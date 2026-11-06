//! Adding a row or column copies the reference row's horizontal merges (the
//! reference column's vertical merges) into the new one, as Hangul 2024 does
//! (measured 11-03; RHWP patch 0111). A merge that straddles the insertion point
//! still grows. Each insert is one undo step and survives HWP and HWPX.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

/// (row, col, rowSpan, colSpan) of every cell, row-major.
fn layout(info: &Value) -> Vec<(u64, u64, u64, u64)> {
    let mut v: Vec<_> = info["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            let n = |k: &str| c[k].as_u64().unwrap_or(u64::MAX);
            (n("row"), n("col"), n("rowSpan"), n("colSpan"))
        })
        .collect();
    v.sort();
    v
}

/// The cell boxes the layout draws for every table on page 0.
fn drawn(s: &Session) -> anyhow::Result<Vec<Value>> {
    let v: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    Ok(v["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["type"] == "table")
        .map(|c| c["cells"].clone())
        .collect())
}

/// The table as `info` reports it, without the caret's cell and block.
fn table_only(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.remove("block");
        o.remove("current");
    }
    v
}

/// Save as HWP and HWPX and reopen: the reopened table equals the live one. The
/// reopened layout equals the live one (`live_layout`), or at least the HWP and HWPX
/// reopens agree.
fn round_trip(
    s: &mut Session,
    info: impl Fn(&mut Session) -> anyhow::Result<Value>,
    live_layout: bool,
) -> anyhow::Result<()> {
    let live = info(s)?;
    let live_drawn = drawn(s)?;
    let mut reopened = Vec::new();
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("merges.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let again = info(&mut r)?;
        assert_eq!(layout(&again), layout(&live), "{ext}");
        assert_eq!(again["columnWidths"], live["columnWidths"], "{ext}");
        assert_eq!(again["rowHeights"], live["rowHeights"], "{ext}");
        let d = drawn(&r)?;
        if live_layout {
            assert_eq!(d, live_drawn, "{ext}: reopened layout");
        }
        reopened.push(d);
    }
    assert_eq!(reopened[0], reopened[1], "HWP and HWPX reopen alike");
    Ok(())
}

const T: &str = r#"{"section":0,"paragraph":0,"control":2}"#;

fn body_table(rows: u16, cols: u16, merges: &[[u16; 4]]) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows": rows, "columns": cols}))?;
    let t: Value = serde_json::from_str(T)?;
    let cells: Vec<Vec<String>> = (0..rows)
        .map(|r| (0..cols).map(|c| format!("{r}{c}")).collect())
        .collect();
    s.run("table.cells.set", json!({"table": t, "cells": cells}))?;
    for [r0, c0, r1, c1] in merges {
        s.run(
            "table.merge",
            json!({"table": t, "range": {"start_row": r0, "start_col": c0, "end_row": r1, "end_col": c1}}),
        )?;
    }
    Ok(s)
}

fn body_info(s: &mut Session) -> anyhow::Result<Value> {
    let t: Value = serde_json::from_str(T)?;
    s.run("table.info", json!({"table": t}))
}

/// Run `table.edit`, check the result, undo back to the very same table, redo, and
/// round-trip it.
fn body_case(
    merges: &[[u16; 4]],
    size: (u16, u16),
    action: &str,
    at: (u16, u16),
    expect: &[(u64, u64, u64, u64)],
) -> anyhow::Result<()> {
    let mut s = body_table(size.0, size.1, merges)?;
    let before = body_info(&mut s)?;
    let t: Value = serde_json::from_str(T)?;
    s.run(
        "table.edit",
        json!({"table": t, "action": action, "row": at.0, "col": at.1}),
    )?;
    let after = body_info(&mut s)?;
    let got = layout(&after);
    for e in expect {
        assert!(
            got.contains(e),
            "{action} at {at:?}: missing {e:?} in {got:?}"
        );
    }
    // Every grid slot is covered exactly once.
    let (rows, cols) = (
        after["rows"].as_u64().unwrap(),
        after["columns"].as_u64().unwrap(),
    );
    let mut seen = vec![0; (rows * cols) as usize];
    for (r, c, rs, cs) in &got {
        for rr in *r..r + rs {
            for cc in *c..c + cs {
                seen[(rr * cols + cc) as usize] += 1;
            }
        }
    }
    assert!(seen.iter().all(|n| *n == 1), "{got:?}");

    s.execute(Command::Undo)?;
    assert_eq!(
        table_only(body_info(&mut s)?),
        table_only(before),
        "{action}: one undo restores the table"
    );
    s.execute(Command::Redo)?;
    assert_eq!(layout(&body_info(&mut s)?), got, "{action}: redo");
    round_trip(&mut s, body_info, true)
}

#[test]
fn a_new_row_copies_the_reference_rows_horizontal_merge() -> anyhow::Result<()> {
    // 3×3 with r1 c0-1 merged: below, above, and from the unmerged r1c2.
    body_case(
        &[[1, 0, 1, 1]],
        (3, 3),
        "row_below",
        (1, 0),
        &[(2, 0, 1, 2), (2, 2, 1, 1)],
    )?;
    body_case(
        &[[1, 0, 1, 1]],
        (3, 3),
        "row_above",
        (1, 0),
        &[(1, 0, 1, 2), (2, 0, 1, 2)],
    )?;
    body_case(
        &[[1, 0, 1, 1]],
        (3, 3),
        "row_below",
        (1, 2),
        &[(2, 0, 1, 2), (2, 2, 1, 1)],
    )?;
    // Below a 2×2 merge from its bottom row: the new row gets its width.
    body_case(
        &[[1, 1, 2, 2]],
        (4, 4),
        "row_below",
        (2, 0),
        &[(1, 1, 2, 2), (3, 1, 1, 2)],
    )?;
    // A whole-row merge above the first row.
    body_case(
        &[[0, 0, 0, 2]],
        (2, 3),
        "row_above",
        (0, 0),
        &[(0, 0, 1, 3), (1, 0, 1, 3)],
    )?;
    // Inside a vertical merge the merge grows, as before.
    body_case(
        &[[0, 0, 1, 0]],
        (3, 3),
        "row_below",
        (0, 1),
        &[(0, 0, 3, 1)],
    )
}

#[test]
fn a_new_column_copies_the_reference_columns_vertical_merge() -> anyhow::Result<()> {
    // c1 r0-1 merged, column to the right: the new c2 gets [2x1].
    body_case(
        &[[0, 1, 1, 1]],
        (3, 3),
        "column_right",
        (0, 1),
        &[(0, 2, 2, 1), (2, 2, 1, 1)],
    )?;
    // From inside a 2×2 merge: the new column goes right of it, [2x1] at r1.
    body_case(
        &[[1, 1, 2, 2]],
        (4, 4),
        "column_right",
        (1, 1),
        &[(1, 1, 2, 2), (1, 3, 2, 1)],
    )?;
    // First column, to the left.
    body_case(
        &[[0, 0, 1, 0]],
        (3, 3),
        "column_left",
        (0, 0),
        &[(0, 0, 2, 1), (0, 1, 2, 1)],
    )?;
    // Inside a horizontal merge the merge grows, as before.
    body_case(
        &[[0, 0, 0, 1]],
        (3, 3),
        "column_right",
        (1, 0),
        &[(0, 0, 1, 3)],
    )
}

/// A body 2×2 table whose first cell holds a 3×3 table; returns the nested table's
/// host address and control index.
fn nested(s: &mut Session) -> anyhow::Result<(Value, usize)> {
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    let created = s.run(
        "context.table.create",
        json!({"address": {"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
               "rows": 3, "columns": 3}),
    )?;
    let address = &created["address"];
    let steps = address["path"].as_array().cloned().unwrap_or_default();
    anyhow::ensure!(steps.len() == 2, "{created}");
    let control = steps[1]["control"].as_u64().unwrap_or(0) as usize;
    let host = json!({"section": address["section"], "paragraph": address["paragraph"],
                      "path": [steps[0].clone()]});
    Ok((host, control))
}

#[test]
fn a_nested_table_copies_merges_on_insert_too() -> anyhow::Result<()> {
    for (merge, id, extra, expect) in [
        (
            json!({"start_row":1,"start_col":0,"end_row":1,"end_col":1}),
            "context.table.insert_row",
            json!({"row": 1, "below": true}),
            (2, 0, 1, 2),
        ),
        (
            json!({"start_row":0,"start_col":1,"end_row":1,"end_col":1}),
            "context.table.insert_column",
            json!({"col": 1, "right": true}),
            (0, 2, 2, 1),
        ),
    ] {
        let mut s = Session::new()?;
        let (host, control) = nested(&mut s)?;
        let info = |s: &mut Session| {
            s.run(
                "context.table.info",
                json!({"address": host, "control": control}),
            )
        };
        s.run(
            "context.table.merge",
            json!({"address": host, "control": control, "range": merge}),
        )?;
        let before = info(&mut s)?;
        let mut p = json!({"address": host, "control": control});
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            p[k] = v;
        }
        s.run(id, p)?;
        let got = layout(&info(&mut s)?);
        assert!(got.contains(&expect), "{id}: {got:?}");
        s.execute(Command::Undo)?;
        assert_eq!(
            table_only(info(&mut s)?),
            table_only(before.clone()),
            "{id}: one undo"
        );
        s.execute(Command::Redo)?;
        assert_eq!(layout(&info(&mut s)?), got, "{id}: redo");
        // The live layout equals the reopened one (contexts.rs N1).
        round_trip(&mut s, info, true)?;
    }
    Ok(())
}
