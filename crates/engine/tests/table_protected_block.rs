//! 셀 보호: block commands (계산, 1,000 단위 쉼표, 정렬, 자동 채우기, 여러 셀 글 채우기)
//! refuse a block that holds a protected cell and leave the table as it was.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn texts(s: &Session) -> Vec<String> {
    let doc = s.core.document();
    doc.sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| &t.cells)
        .map(|c| {
            c.paragraphs
                .iter()
                .map(|p| p.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

#[test]
fn block_commands_refuse_a_protected_cell() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let table: Value = s.run("insert.table", json!({"rows": 3, "columns": 3}))?["table"].clone();
    s.run(
        "table.cells.set",
        json!({"table": table, "row": 0, "col": 0,
               "cells": [["3", "1000", ""], ["1", "2000", ""], ["", "", ""]]}),
    )?;
    {
        let doc = s.core.document_mut();
        let p = table["paragraph"].as_u64().unwrap_or(0) as usize;
        let c = table["control"].as_u64().unwrap_or(0) as usize;
        let Control::Table(t) = &mut doc.sections[0].paragraphs[p].controls[c] else {
            anyhow::bail!("no table");
        };
        let i = t
            .cells
            .iter()
            .position(|c| c.row == 1 && c.col == 1)
            .ok_or_else(|| anyhow::anyhow!("cell"))?;
        t.cells[i].set_cell_protect(true);
    }
    let before = texts(&s);
    let range = json!({"start_row": 0, "start_col": 0, "end_row": 2, "end_col": 2});
    for (command, params) in [
        (
            "table.block_calc",
            json!({"table": table, "range": range, "op": "sum"}),
        ),
        (
            "table.thousands",
            json!({"table": table, "range": range, "remove": false}),
        ),
        (
            "table.sort",
            json!({"table": table, "range": range, "column": 0, "descending": false}),
        ),
        ("table.auto_fill", json!({"table": table, "range": range})),
        (
            "table.cells.set",
            json!({"table": table, "row": 1, "col": 1, "cells": [["바꿈"]]}),
        ),
    ] {
        let error = s.run(command, params).expect_err(command);
        assert!(error.to_string().contains("보호"), "{command}: {error}");
        assert_eq!(texts(&s), before, "{command}");
    }
    // A block without the protected cell still works.
    s.run(
        "table.thousands",
        json!({"table": table, "range": {"start_row": 0, "start_col": 1, "end_row": 0, "end_col": 1}, "remove": false}),
    )?;
    assert!(texts(&s).contains(&"1,000".to_owned()));
    Ok(())
}
