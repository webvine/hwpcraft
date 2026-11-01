//! 표를 문자열로 바꾸면 글이 표가 있던 자리에 들어간다: "앞[표]뒤"는 "앞", 표의 글,
//! "뒤" 순서가 된다(문단 끝에 붙지 않는다).
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

fn at(s: &mut Session, offset: usize) -> anyhow::Result<()> {
    s.select(
        Cursor {
            section: 0,
            paragraph: 0,
            offset,
            cell: None,
        },
        false,
    )
}

#[test]
fn table_text_goes_where_the_table_was() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞뒤".into()))?;
    at(&mut s, 1)?;
    s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
    // Join "앞", the table's paragraph and "뒤" into one paragraph: 앞[표]뒤.
    for _ in 0..3 {
        at(&mut s, 1)?;
        s.execute(Command::Delete)?;
    }
    let host = &s.core.document().sections[0].paragraphs[0];
    assert_eq!(host.text, "앞뒤");
    let control = host.controls.len() - 1;
    assert_eq!(host.control_text_positions()[control], 1, "앞[표]뒤");
    let table = json!({"section": 0, "paragraph": 0, "control": control});
    s.run(
        "table.cells.set",
        json!({"table": table, "row": 0, "col": 0, "cells": [["A", "B"], ["C", "D"]]}),
    )?;
    s.run("table.to_text", json!({"table": table, "delimiter": "tab"}))?;
    let body: Vec<String> = s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect();
    assert_eq!(body, vec!["앞A\tB", "C\tD뒤"]);
    Ok(())
}
