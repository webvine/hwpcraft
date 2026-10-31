//! Owner: shell/page area. 사용자 표 마당 (table.user_style.*), in a settings folder of its own.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, Session, set_config_dir};
use serde_json::{Value, json};

/// Address of the `n`-th table in the body of section 0.
fn table(s: &Session, n: usize) -> anyhow::Result<Value> {
    let found = s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(p, para)| {
            para.controls
                .iter()
                .enumerate()
                .filter(|(_, c)| matches!(c, Control::Table(_)))
                .map(move |(c, _)| (p, c))
        })
        .nth(n);
    let (paragraph, control) = found.ok_or_else(|| anyhow::anyhow!("table {n}"))?;
    Ok(json!({"section": 0, "paragraph": paragraph, "control": control}))
}

/// Fill colour and bottom edge of the cell at (row, col) of a table.
fn look(s: &Session, t: &Value, row: u16, col: u16) -> anyhow::Result<(Value, Value)> {
    let (p, c) = (
        t["paragraph"].as_u64().unwrap_or(0) as usize,
        t["control"].as_u64().unwrap_or(0) as usize,
    );
    let Some(Control::Table(table)) = s.core.document().sections[0].paragraphs[p].controls.get(c)
    else {
        anyhow::bail!("table");
    };
    let index = table
        .cell_index_at(row, col)
        .ok_or_else(|| anyhow::anyhow!("cell"))?;
    let v: Value = serde_json::from_str(
        &s.core
            .get_cell_properties_by_cell_path_native(0, p, &[(c, index, 0)], index)
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    )?;
    Ok((v["fillColor"].clone(), v["borderBottom"].clone()))
}

#[test]
fn a_saved_table_look_fits_a_table_of_another_size() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(set_config_dir(dir.path().to_path_buf()));
    let mut s = Session::new()?;
    assert_eq!(
        s.run("table.user_style.list", json!({}))?["styles"],
        json!([])
    );
    s.run("insert.table", json!({"rows": 3, "columns": 3}))?;
    let source = table(&s, 0)?;
    s.run(
        "table.style",
        json!({"table": source, "preset": "blue_header", "first_column": true, "last_row": true}),
    )?;
    let saved = s.run(
        "table.user_style.save",
        json!({"name": "파랑 제목", "table": source}),
    )?;
    assert_eq!(saved["styles"][0]["name"], "파랑 제목");
    assert!(
        dir.path()
            .join("table_styles")
            .join("파랑 제목.json")
            .exists()
    );
    assert!(
        s.run(
            "table.user_style.save",
            json!({"name": "파랑 제목", "table": source})
        )
        .is_err(),
        "an existing name needs force"
    );
    s.run(
        "table.user_style.save",
        json!({"name": "파랑 제목", "table": source, "force": true}),
    )?;

    // A 6×5 table in another document: every cell takes the look of its role.
    let mut other = Session::new()?;
    other.run("insert.table", json!({"rows": 6, "columns": 5}))?;
    let target = table(&other, 0)?;
    let plain = look(&other, &target, 0, 0)?;
    let header = look(&s, &source, 0, 0)?;
    assert!(header.0.is_string() && header.1.is_object(), "{header:?}");
    assert_ne!(plain, header);
    other.run(
        "table.user_style.apply",
        json!({"name": "파랑 제목", "table": target}),
    )?;
    for (row, col, from_row, from_col) in [
        (0, 0, 0, 0), // first row, first column
        (0, 3, 0, 1), // first row, body column
        (2, 2, 1, 1), // body
        (3, 0, 1, 0), // body row, first column
        (5, 4, 2, 2), // last row, last column
        (5, 1, 2, 1), // last row, body column
    ] {
        assert_eq!(
            look(&other, &target, row, col)?,
            look(&s, &source, from_row, from_col)?,
            "cell {row},{col}"
        );
    }
    // One undo step.
    other.execute(Command::Undo)?;
    assert_eq!(look(&other, &target, 0, 0)?, plain, "undone");

    for bad in ["", "a/b", ".hidden"] {
        assert!(
            s.run(
                "table.user_style.save",
                json!({"name": bad, "table": source})
            )
            .is_err()
        );
    }
    assert!(
        other
            .run(
                "table.user_style.apply",
                json!({"name": "없음", "table": target})
            )
            .is_err()
    );
    let deleted = s.run("table.user_style.delete", json!({"name": "파랑 제목"}))?;
    assert_eq!(deleted["styles"], json!([]));
    assert!(
        s.run("table.user_style.delete", json!({"name": "파랑 제목"}))
            .is_err()
    );
    Ok(())
}
