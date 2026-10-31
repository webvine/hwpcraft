//! `table.cells.set` fills a block of cells in one command and one undo step, the
//! way an MCP client writes a report table: table address, start cell, rows of text.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn texts(s: &mut Session, table: &Value) -> anyhow::Result<Vec<Vec<String>>> {
    let info = s.run("table.info", json!({"table": table}))?;
    let rows = info["rows"].as_u64().unwrap_or(0) as usize;
    let cols = info["columns"].as_u64().unwrap_or(0) as usize;
    let mut grid = vec![vec![String::new(); cols]; rows];
    for c in info["cells"].as_array().into_iter().flatten() {
        let (r, k) = (
            c["row"].as_u64().unwrap_or(0),
            c["col"].as_u64().unwrap_or(0),
        );
        grid[r as usize][k as usize] = c["text"].as_str().unwrap_or_default().to_owned();
    }
    Ok(grid)
}

#[test]
fn a_block_of_cells_fills_in_one_undo_step_and_survives_both_formats() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "분기별 실적"}))?;
    s.run("paragraph.split", json!({}))?;
    let table = s.run("insert.table", json!({"rows": 3, "columns": 3}))?["table"].clone();
    s.cursor = Default::default();
    let done = s.run(
        "table.cells.set",
        json!({"table": table, "cells": [["구분", "실적", "비고"], ["1분기", "120", null]]}),
    )?;
    assert_eq!(done["cells"], 5, "{done}");
    // A start cell, and a line break that makes a second cell paragraph.
    s.run(
        "table.cells.set",
        json!({"table": table, "row": 2, "col": 1, "cells": [["135", "목표\n초과"]]}),
    )?;
    let want = vec![
        vec!["구분", "실적", "비고"],
        vec!["1분기", "120", ""],
        vec![
            "",
            "135",
            "목표
초과",
        ],
    ];
    assert_eq!(texts(&mut s, &table)?, want);
    // Writing a cell again replaces its paragraphs.
    s.run(
        "table.cells.set",
        json!({"table": table, "row": 2, "col": 2, "cells": [["달성"]]}),
    )?;
    assert_eq!(texts(&mut s, &table)?[2][2], "달성");
    // Each command is one undo step.
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(texts(&mut s, &table)?[2], vec!["", "", ""]);
    s.execute(Command::Redo)?;
    s.execute(Command::Redo)?;
    assert_eq!(texts(&mut s, &table)?[2][2], "달성");
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("cells.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(texts(&mut r, &table)?, texts(&mut s, &table)?, "{ext}");
    }
    Ok(())
}

#[test]
fn a_block_that_does_not_fit_changes_nothing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let table = s.run("insert.table", json!({"rows": 2, "columns": 2}))?["table"].clone();
    s.run("table.merge", json!({"table": table, "range": {"start_row": 0, "start_col": 0, "end_row": 0, "end_col": 1}}))?;
    s.run(
        "table.cells.set",
        json!({"table": table, "row": 1, "cells": [["가", "나"]]}),
    )?;
    let generation = s.generation;
    let before = texts(&mut s, &table)?;
    for params in [
        json!({"table": table, "row": 1, "cells": [["다", "라", "마"]]}),
        json!({"table": table, "row": 1, "cells": [["다"], ["라"]]}),
        json!({"table": table, "cells": [["머리", "가려진 칸"]]}),
        json!({"table": table, "cells": [["탭\t글"]]}),
        json!({"table": table, "cells": []}),
        json!({"table": table, "row": 1}),
    ] {
        assert!(
            s.run("table.cells.set", params.clone()).is_err(),
            "{params}"
        );
        assert_eq!(s.generation, generation, "{params}");
        assert_eq!(texts(&mut s, &table)?, before, "{params}");
    }
    // A merged cell takes its text at its own position; the covered one stays null.
    s.run(
        "table.cells.set",
        json!({"table": table, "cells": [["머리", null]]}),
    )?;
    assert_eq!(texts(&mut s, &table)?[0][0], "머리");
    Ok(())
}
