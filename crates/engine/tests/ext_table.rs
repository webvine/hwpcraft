use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn info(s: &mut Session) -> anyhow::Result<Value> {
    s.run("table.info", json!({}))
}
fn new_table(rows: u16, columns: u16) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows":rows,"columns":columns}))?;
    Ok(s)
}
fn cell_text(s: &mut Session, row: u16, col: u16, text: &str) -> anyhow::Result<()> {
    let v = info(s)?;
    let a = v["table"].clone();
    let index = v["cells"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["row"] == row && c["col"] == col))
        .and_then(|c| c["index"].as_u64())
        .ok_or_else(|| anyhow::anyhow!("cell {row},{col}"))?;
    let mut c = s.cursor;
    c.section = a["section"].as_u64().unwrap_or(0) as usize;
    c.paragraph = a["paragraph"].as_u64().unwrap_or(0) as usize;
    let mut cell = c.cell.ok_or_else(|| anyhow::anyhow!("not in table"))?;
    cell.index = index as usize;
    cell.paragraph = 0;
    c.cell = Some(cell);
    c.offset = 0;
    s.select(c, false)?;
    s.execute(Command::Insert(text.into()))
}
fn reopen(s: &mut Session, ext: &str) -> anyhow::Result<Session> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(format!("table.{ext}"));
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    let first = r
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("{ext}: table lost"))?
        .0;
    r.select(first, false)?;
    Ok(r)
}

#[test]
fn create_with_hangul_options_places_caret_in_first_cell() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞".into()))?;
    let v = s.run(
        "table.create",
        json!({"rows":3,"columns":4,"width_mm":120.0,"height_mm":30.0}),
    )?;
    assert!(v["table"].is_object());
    let i = info(&mut s)?;
    assert_eq!(
        (i["rows"].as_u64(), i["columns"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(i["current"], 0);
    let widths: Vec<u64> = serde_json::from_value(i["columnWidths"].clone())?;
    let total: u64 = widths.iter().sum();
    assert!(
        (total as f64 - 120.0 * 7200.0 / 25.4).abs() < 8.0,
        "{widths:?}"
    );
    let heights: Vec<u64> = serde_json::from_value(i["rowHeights"].clone())?;
    assert!(
        heights
            .iter()
            .all(|h| (*h as f64 - 10.0 * 7200.0 / 25.4).abs() < 3.0),
        "{heights:?}"
    );
    s.execute(Command::Undo)?;
    assert!(s.cursor.cell.is_none());
    assert!(s.run("table.info", json!({})).is_err());

    let mut inline = Session::new()?;
    inline.execute(Command::Insert("글자 ".into()))?;
    inline.run(
        "table.create",
        json!({"rows":1,"columns":2,"treat_as_char":true,"column_widths_mm":[20.0,30.0]}),
    )?;
    let p = inline.run("table.properties.get", json!({}))?;
    assert_eq!(p["treatAsChar"], true);
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut inline, ext)?;
        assert_eq!(
            r.run("table.properties.get", json!({}))?["treatAsChar"],
            true,
            "{ext}"
        );
    }
    Ok(())
}

#[test]
fn invalid_table_requests_keep_document_and_history() -> anyhow::Result<()> {
    let mut s = new_table(2, 2)?;
    let generation = s.generation;
    let undo = s.can_undo();
    for (id, params) in [
        ("table.create", json!({"rows":0,"columns":2})),
        ("table.create", json!({"rows":201,"columns":2})),
        (
            "table.create",
            json!({"rows":2,"columns":2,"width_mm":-5.0}),
        ),
        ("table.properties", json!({"props":{"pageBreak":7}})),
        ("table.properties", json!({"props":{"unknown":1}})),
        ("table.properties", json!({"props":{"textWrap":"Sideways"}})),
        (
            "table.cell_properties",
            json!({"props":{"fillColor":"red"}}),
        ),
        (
            "table.cell_properties",
            json!({"borders":{"diagonal":{"type":1,"width":1,"color":"#000000"}}}),
        ),
        (
            "table.cell_properties",
            json!({"props":{"borderLeft":{"type":99,"width":1,"color":"#000000"}}}),
        ),
        ("table.merge", json!({})),
        (
            "table.merge",
            json!({"range":{"start_row":0,"start_col":0,"end_row":5,"end_col":0}}),
        ),
        ("table.split", json!({"rows":1,"columns":1})),
        ("table.split", json!({"rows":500,"columns":1})),
        ("table.equal_width", json!({})),
        ("table.resize", json!({"vertical":false,"delta_mm":0.0})),
        ("table.resize", json!({"vertical":false,"delta_mm":-500.0})),
        ("table.formula", json!({"formula":""})),
        (
            "table.formula",
            json!({"formula":"=SUM(A1:A2", "row":1, "col":1}),
        ),
        (
            "table.formula",
            json!({"formula":"=NOPE(A1)", "row":1, "col":1}),
        ),
        ("table.block_calc", json!({"op":"median"})),
        ("table.block_calc", json!({"op":"sum"})),
    ] {
        assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(s.generation, generation, "{id} {params}");
        assert_eq!(s.can_undo(), undo);
        assert!(s.paragraph(s.cursor).is_ok());
    }
    let mut body = Session::new()?;
    assert!(body.run("table.merge", json!({})).is_err());
    Ok(())
}

#[test]
fn table_and_cell_properties_roundtrip() -> anyhow::Result<()> {
    let mut s = new_table(2, 3)?;
    let before = s.run("table.properties.get", json!({}))?;
    s.run(
        "table.properties",
        json!({"props":{"pageBreak":1,"repeatHeader":true,"paddingLeft":283,"outerLeft":567,
            "outerBottom":850,"hasCaption":true,"captionDirection":2,"tableWidth":28346}}),
    )?;
    let p = s.run("table.properties.get", json!({}))?;
    assert_eq!(p["pageBreak"], 1);
    assert_eq!(p["repeatHeader"], true);
    assert_eq!(p["paddingLeft"], 283);
    assert_eq!(p["outerLeft"], 567);
    assert_eq!(p["outerBottom"], 850);
    assert_eq!(p["hasCaption"], true);
    assert_eq!(p["captionDirection"], 2);
    assert!(
        (p["tableWidth"].as_i64().unwrap_or(0) - 28346).abs() <= 3,
        "{p}"
    );
    s.execute(Command::Undo)?;
    assert_eq!(s.run("table.properties.get", json!({}))?, before);
    s.execute(Command::Redo)?;

    let range = json!({"start_row":0,"start_col":0,"end_row":1,"end_col":1});
    let thick = json!({"type":1,"width":5,"color":"#ff0000"});
    let dotted = json!({"type":3,"width":1,"color":"#0000ff"});
    s.run(
        "table.cell_properties",
        json!({"range":range,"props":{"fillType":"solid","fillColor":"#ffff00","verticalAlign":1,
            "cellProtect":true,"paddingTop":141},
            "borders":{"left":thick,"right":thick,"top":thick,"bottom":thick,
                "inner_vertical":dotted,"inner_horizontal":dotted}}),
    )?;
    let corner = s.run(
        "table.cell_properties.get",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}}),
    )?;
    assert_eq!(corner["fillColor"], "#ffff00");
    assert_eq!(corner["verticalAlign"], 1);
    assert_eq!(corner["cellProtect"], true);
    assert_eq!(corner["borderLeft"]["width"], 5);
    assert_eq!(corner["borderTop"]["color"], "#ff0000");
    assert_eq!(corner["borderRight"]["type"], 3, "inner edge: {corner}");
    let far = s.run(
        "table.cell_properties.get",
        json!({"range":{"start_row":1,"start_col":1,"end_row":1,"end_col":1}}),
    )?;
    assert_eq!(far["borderRight"]["width"], 5);
    assert_eq!(far["borderBottom"]["width"], 5);
    assert_eq!(far["borderLeft"]["type"], 3);
    let outside = s.run(
        "table.cell_properties.get",
        json!({"range":{"start_row":0,"start_col":2,"end_row":0,"end_col":2}}),
    )?;
    assert_ne!(outside["fillColor"], "#ffff00");

    s.run(
        "table.cell_properties",
        json!({"props":{"height":2834,"width":5669}}),
    )?;
    let i = info(&mut s)?;
    assert_eq!(i["columnWidths"][0], 5669);
    assert_eq!(i["rowHeights"][0], 2834);

    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let p = r.run("table.properties.get", json!({}))?;
        assert_eq!(p["pageBreak"], 1, "{ext}");
        assert_eq!(p["repeatHeader"], true, "{ext}");
        assert_eq!(p["hasCaption"], true, "{ext}");
        let c = r.run("table.cell_properties.get", json!({}))?;
        assert_eq!(c["fillColor"], "#ffff00", "{ext}");
        assert_eq!(c["cellProtect"], true, "{ext}");
        assert_eq!(c["borderLeft"]["width"], 5, "{ext}");
    }
    Ok(())
}

#[test]
fn block_merge_split_equalize_and_resize() -> anyhow::Result<()> {
    let mut s = new_table(3, 3)?;
    cell_text(&mut s, 0, 0, "가")?;
    s.run(
        "table.merge",
        json!({"range":{"start_row":0,"start_col":0,"end_row":1,"end_col":1}}),
    )?;
    let i = info(&mut s)?;
    let merged = &i["cells"][i["current"].as_u64().unwrap_or(0) as usize];
    assert_eq!(
        (merged["rowSpan"].as_u64(), merged["colSpan"].as_u64()),
        (Some(2), Some(2))
    );
    // A block touching a merged cell widens to cover all of it.
    let b = s.run(
        "table.info",
        json!({"range":{"start_row":1,"start_col":1,"end_row":1,"end_col":2}}),
    )?;
    assert_eq!(
        b["block"],
        json!({"start_row":0,"start_col":0,"end_row":1,"end_col":2})
    );
    s.execute(Command::Undo)?;
    assert_eq!(info(&mut s)?["cells"].as_array().map(Vec::len), Some(9));

    s.run("table.split", json!({"rows":2,"columns":3}))?;
    let i = info(&mut s)?;
    assert_eq!(i["cells"].as_array().map(Vec::len), Some(9 - 1 + 6));
    s.execute(Command::Undo)?;
    s.run(
        "table.split",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1},"rows":1,"columns":2}),
    )?;
    assert_eq!(info(&mut s)?["columns"], 5);
    s.execute(Command::Undo)?;
    s.run(
        "table.split",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":1},"rows":2,"columns":1,"merge_first":true}),
    )?;
    assert_eq!(info(&mut s)?["rows"], 4);
    s.execute(Command::Undo)?;

    let all = json!({"start_row":0,"start_col":0,"end_row":2,"end_col":2});
    s.run("table.resize", json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0},"vertical":false,"delta_mm":10.0}))?;
    s.run("table.resize", json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0},"vertical":true,"delta_mm":5.0}))?;
    let i = info(&mut s)?;
    let w: Vec<i64> = serde_json::from_value(i["columnWidths"].clone())?;
    let h: Vec<i64> = serde_json::from_value(i["rowHeights"].clone())?;
    assert!(w[0] - w[1] > 2800, "{w:?}");
    assert!(h[0] - h[1] > 1400, "{h:?}");
    s.run("table.equal_width", json!({"range":all}))?;
    s.run("table.equal_height", json!({"range":all}))?;
    let i = info(&mut s)?;
    let w: Vec<i64> = serde_json::from_value(i["columnWidths"].clone())?;
    let h: Vec<i64> = serde_json::from_value(i["rowHeights"].clone())?;
    assert!(w.iter().all(|x| (x - w[0]).abs() <= 1), "{w:?}");
    assert!(h.iter().all(|x| (x - h[0]).abs() <= 1), "{h:?}");
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let w2: Vec<i64> = serde_json::from_value(info(&mut r)?["columnWidths"].clone())?;
        assert_eq!(w, w2, "{ext}");
    }
    Ok(())
}

#[test]
fn formulas_and_block_calculations() -> anyhow::Result<()> {
    let mut s = new_table(3, 3)?;
    for (r, c, v) in [(0, 0, "1"), (0, 1, "2"), (1, 0, "3"), (1, 1, "4")] {
        cell_text(&mut s, r, c, v)?;
    }
    let v = s.run(
        "table.formula",
        json!({"formula":"=SUM(A1:B2)","row":2,"col":2}),
    )?;
    assert_eq!(v["result"], 10.0);
    let n = s.run(
        "table.block_calc",
        json!({"op":"sum","range":{"start_row":0,"start_col":0,"end_row":2,"end_col":1}}),
    )?;
    assert_eq!(n["cells"], 2);
    let i = info(&mut s)?;
    let text = |r: u64, c: u64| {
        i["cells"]
            .as_array()
            .and_then(|v| v.iter().find(|x| x["row"] == r && x["col"] == c))
            .map(|x| x["text"].as_str().unwrap_or("").to_owned())
    };
    assert_eq!(text(2, 0).as_deref(), Some("4"));
    assert_eq!(text(2, 1).as_deref(), Some("6"));
    assert_eq!(text(2, 2).as_deref(), Some("10"));
    s.execute(Command::Undo)?;
    let n = s.run(
        "table.block_calc",
        json!({"op":"average","range":{"start_row":0,"start_col":0,"end_row":1,"end_col":2}}),
    )?;
    assert_eq!(n["cells"], 2);
    let i = info(&mut s)?;
    let avg = i["cells"]
        .as_array()
        .and_then(|v| v.iter().find(|x| x["row"] == 1 && x["col"] == 2))
        .map(|x| x["text"].clone());
    assert_eq!(avg, Some(json!("3.5")));
    // Protected cells refuse formula output.
    s.run("table.cell_properties", json!({"range":{"start_row":2,"start_col":0,"end_row":2,"end_col":0},"props":{"cellProtect":true}}))?;
    assert!(
        s.run(
            "table.formula",
            json!({"formula":"=SUM(A1:A2)","row":2,"col":0})
        )
        .is_err()
    );
    let mut r = reopen(&mut s, "hwpx")?;
    assert!(
        r.run("table.info", json!({}))?["cells"]
            .as_array()
            .is_some()
    );
    Ok(())
}

#[test]
fn delete_table_and_undo() -> anyhow::Result<()> {
    let mut s = new_table(2, 2)?;
    s.run("table.delete", json!({}))?;
    assert!(s.cursor.cell.is_none());
    assert!(
        !s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .any(|c| matches!(c, Control::Table(_)))
    );
    s.execute(Command::Undo)?;
    assert!(s.cursor.cell.is_some());
    assert_eq!(info(&mut s)?["rows"], 2);
    Ok(())
}

#[test]
fn transpose_split_merge_tables_and_thousands() -> anyhow::Result<()> {
    let mut s = new_table(2, 3)?;
    cell_text(&mut s, 0, 2, "1234567")?;
    cell_text(&mut s, 1, 0, "-9876.5")?;
    cell_text(&mut s, 1, 1, "글자")?;
    let all = json!({"start_row":0,"start_col":0,"end_row":1,"end_col":2});
    assert_eq!(s.run("table.thousands", json!({"range":all}))?["cells"], 2);
    let texts = |s: &mut Session| -> anyhow::Result<Vec<String>> {
        Ok(info(s)?["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| c["text"].as_str().unwrap_or("").to_owned())
            .collect())
    };
    let t = texts(&mut s)?;
    assert!(
        t.contains(&"1,234,567".to_owned()) && t.contains(&"-9,876.5".to_owned()),
        "{t:?}"
    );
    assert_eq!(
        s.run("table.thousands", json!({"range":all,"remove":true}))?["cells"],
        2
    );
    assert!(texts(&mut s)?.contains(&"1234567".to_owned()));

    s.run("table.transpose", json!({}))?;
    let i = info(&mut s)?;
    assert_eq!(
        (i["rows"].as_u64(), i["columns"].as_u64()),
        (Some(3), Some(2))
    );
    s.execute(Command::Undo)?;
    assert_eq!(info(&mut s)?["rows"], 2);

    let tables = |s: &Session| {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter(|c| matches!(c, Control::Table(_)))
            .count()
    };
    assert!(
        s.run(
            "table.split_table",
            json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}})
        )
        .is_err()
    );
    s.run(
        "table.split_table",
        json!({"range":{"start_row":1,"start_col":0,"end_row":1,"end_col":0}}),
    )?;
    assert_eq!(tables(&s), 2);
    assert!(s.paragraph(s.cursor).is_ok());
    s.run("table.merge_table", json!({}))?;
    assert_eq!(tables(&s), 1);
    assert_eq!(info(&mut s)?["rows"], 2);
    s.execute(Command::Undo)?;
    assert_eq!(tables(&s), 2);
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(tables(&r), 2, "{ext}");
    }
    Ok(())
}

#[test]
fn protected_cells_refuse_text_edits() -> anyhow::Result<()> {
    let mut s = new_table(1, 2)?;
    cell_text(&mut s, 0, 0, "보호")?;
    cell_text(&mut s, 0, 1, "보호")?;
    s.run(
        "table.cell_properties",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0},"props":{"cellProtect":true}}),
    )?;
    cell_text(&mut s, 0, 1, "열림 ")?;
    let mut c = s.cursor;
    if let Some(cell) = c.cell.as_mut() {
        cell.index = info(&mut s)?["cells"]
            .as_array()
            .and_then(|v| v.iter().find(|x| x["row"] == 0 && x["col"] == 0))
            .and_then(|x| x["index"].as_u64())
            .unwrap_or(0) as usize;
    }
    c.offset = 1;
    s.select(c, false)?;
    let generation = s.generation;
    for (id, params) in [
        ("text.insert", json!({"text":"x"})),
        ("text.backspace", json!({})),
        ("text.delete", json!({})),
    ] {
        assert!(s.run(id, params).is_err(), "{id}");
        assert_eq!(s.generation, generation, "{id}");
    }
    s.run(
        "edit.replace_all",
        json!({"query":"보호","replacement":"변경"}),
    )?;
    let texts: Vec<String> = info(&mut s)?["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|x| x["text"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(texts[0], "보호", "{texts:?}");
    assert!(texts[1].contains("변경"), "{texts:?}");
    Ok(())
}

#[test]
fn partial_border_or_fill_edits_keep_the_other_sides() -> anyhow::Result<()> {
    let mut s = new_table(2, 2)?;
    let cell = json!({"start_row":0,"start_col":0,"end_row":0,"end_col":0});
    let before = s.run("table.cell_properties.get", json!({"range":cell}))?;
    assert_eq!(before["borderLeft"]["type"], 1, "{before}");
    // Only the bottom edge and the fill change, like a header row style.
    s.run(
        "table.cell_properties",
        json!({"range":cell,"props":{"fillType":"solid","fillColor":"#dbe5f1"},
            "borders":{"bottom":{"type":8,"width":3,"color":"#1f4e79"}}}),
    )?;
    let after = s.run("table.cell_properties.get", json!({"range":cell}))?;
    for side in ["borderLeft", "borderTop", "borderRight"] {
        assert_eq!(after[side], before[side], "{side} must survive: {after}");
    }
    assert_eq!(after["borderBottom"]["type"], 8);
    // A fill-only edit keeps all four edges, including the new bottom.
    s.run(
        "table.cell_properties",
        json!({"range":cell,"props":{"fillColor":"#ffeeaa"}}),
    )?;
    let fill = s.run("table.cell_properties.get", json!({"range":cell}))?;
    for side in ["borderLeft", "borderTop", "borderRight", "borderBottom"] {
        assert_eq!(fill[side], after[side], "{side}: {fill}");
    }
    assert_eq!(fill["fillColor"], "#ffeeaa");
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let c = r.run("table.cell_properties.get", json!({"range":cell}))?;
        assert_eq!(c["borderLeft"], before["borderLeft"], "{ext}");
        assert_eq!(c["borderBottom"]["type"], 8, "{ext}");
    }
    Ok(())
}

#[test]
fn dragging_grid_lines_moves_borders() -> anyhow::Result<()> {
    let mut s = new_table(3, 3)?;
    let widths = |s: &mut Session| -> anyhow::Result<Vec<i64>> {
        Ok(serde_json::from_value(info(s)?["columnWidths"].clone())?)
    };
    let heights = |s: &mut Session| -> anyhow::Result<Vec<i64>> {
        Ok(serde_json::from_value(info(s)?["rowHeights"].clone())?)
    };
    let w0 = widths(&mut s)?;
    let step = (10.0f64 * 7200.0 / 25.4).round() as i64;
    // Inner column line: the two neighbours trade width, the table width stays.
    s.run(
        "table.move_border",
        json!({"vertical":true,"boundary":1,"delta_mm":10.0}),
    )?;
    let w1 = widths(&mut s)?;
    assert_eq!(w1[0], w0[0] + step, "{w1:?}");
    assert_eq!(w1[1], w0[1] - step, "{w1:?}");
    assert_eq!(w1.iter().sum::<i64>(), w0.iter().sum::<i64>());
    // Outer right edge grows the table.
    s.run(
        "table.move_border",
        json!({"vertical":true,"boundary":3,"delta_mm":10.0}),
    )?;
    assert_eq!(widths(&mut s)?[2], w0[2] + step);
    // Horizontal line resizes the row above.
    let h0 = heights(&mut s)?;
    s.run(
        "table.move_border",
        json!({"vertical":false,"boundary":2,"delta_mm":10.0}),
    )?;
    let h1 = heights(&mut s)?;
    assert_eq!(h1[1], h0[1] + step, "{h1:?}");
    assert_eq!((h1[0], h1[2]), (h0[0], h0[2]));
    s.execute(Command::Undo)?;
    assert_eq!(heights(&mut s)?, h0);
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        assert_eq!(widths(&mut r)?[0], w1[0], "{ext}");
        assert_eq!(heights(&mut r)?[1], h1[1], "{ext}");
    }
    let generation = s.generation;
    for p in [
        json!({"vertical":true,"boundary":0,"delta_mm":1.0}),
        json!({"vertical":true,"boundary":9,"delta_mm":1.0}),
        json!({"vertical":true,"boundary":1,"delta_mm":-500.0}),
        json!({"vertical":true,"boundary":1,"delta_mm":f64::MAX}),
        json!({"vertical":true,"boundary":1,"delta_mm":1.0,"cell":[0,0]}),
    ] {
        assert!(s.run("table.move_border", p.clone()).is_err(), "{p}");
        assert_eq!(s.generation, generation, "{p}");
    }
    Ok(())
}

fn texts(s: &mut Session) -> anyhow::Result<Vec<Vec<String>>> {
    let v = info(s)?;
    let rows = v["rows"].as_u64().unwrap_or(0);
    let cols = v["columns"].as_u64().unwrap_or(0);
    let cells = v["cells"].as_array().cloned().unwrap_or_default();
    Ok((0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    cells
                        .iter()
                        .find(|x| x["row"] == r && x["col"] == c)
                        .map(|x| x["text"].as_str().unwrap_or("").to_owned())
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect())
}

#[test]
fn text_to_table_and_back() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("이름,점수\n가,10\n나,7".into()))?;
    let end = s.cursor;
    let mut start = end;
    start.paragraph = 0;
    start.offset = 0;
    s.select(start, false)?;
    s.select(end, true)?;
    s.run("table.from_text", json!({"delimiter":"comma"}))?;
    assert_eq!(
        texts(&mut s)?,
        vec![vec!["이름", "점수"], vec!["가", "10"], vec!["나", "7"]]
    );
    assert!(s.cursor.cell.is_some());
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        assert_eq!(texts(&mut r)?[2], vec!["나", "7"], "{ext}");
    }
    s.run("table.to_text", json!({"delimiter":"tab"}))?;
    assert!(s.cursor.cell.is_none());
    let body: Vec<String> = s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .filter(|t| !t.is_empty())
        .collect();
    assert_eq!(body, vec!["이름\t점수", "가\t10", "나\t7"]);
    s.execute(Command::Undo)?;
    assert_eq!(texts(&mut s)?[1], vec!["가", "10"]);
    s.execute(Command::Undo)?;
    assert!(s.run("table.info", json!({})).is_err());
    let generation = s.generation;
    for d in ["", "\n", "123456789"] {
        assert!(
            s.run("table.from_text", json!({"delimiter":d})).is_err(),
            "{d:?}"
        );
    }
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn sort_auto_fill_caption_and_styles() -> anyhow::Result<()> {
    let mut s = new_table(4, 3)?;
    for (r, c, t) in [
        (0, 0, "이름"),
        (0, 1, "점수"),
        (1, 0, "다"),
        (1, 1, "7"),
        (2, 0, "가"),
        (2, 1, "12"),
        (3, 0, "나"),
        (3, 1, "9"),
    ] {
        cell_text(&mut s, r, c, t)?;
    }
    let body = json!({"start_row":1,"start_col":0,"end_row":3,"end_col":1});
    s.run(
        "table.sort",
        json!({"range":body,"column":1,"descending":true}),
    )?;
    let t = texts(&mut s)?;
    assert_eq!(
        t.iter().skip(1).map(|r| r[1].clone()).collect::<Vec<_>>(),
        ["12", "9", "7"],
        "numeric descending: {t:?}"
    );
    assert_eq!(t[1][0], "가", "rows move together: {t:?}");
    // Ascending by score really reorders (a no-op sort leaves no undo step).
    s.run("table.sort", json!({"range":body,"column":1}))?;
    assert_eq!(
        texts(&mut s)?
            .iter()
            .skip(1)
            .map(|r| r[0].clone())
            .collect::<Vec<_>>(),
        ["다", "나", "가"]
    );
    let generation = s.generation;
    s.run("table.sort", json!({"range":body,"column":1}))?;
    assert_eq!(
        s.generation, generation,
        "sorting sorted rows changes nothing"
    );
    s.execute(Command::Undo)?;
    assert_eq!(texts(&mut s)?[1][1], "12");
    assert!(
        s.run("table.sort", json!({"range":body,"column":2}))
            .is_err()
    );

    // Auto fill: weekdays down the third column, numbers with a step across.
    cell_text(&mut s, 0, 2, "월요일")?;
    let n = s.run(
        "table.auto_fill",
        json!({"range":{"start_row":0,"start_col":2,"end_row":3,"end_col":2}}),
    )?;
    assert_eq!(n["cells"], 3);
    assert_eq!(
        texts(&mut s)?
            .iter()
            .map(|r| r[2].clone())
            .collect::<Vec<_>>(),
        ["월요일", "화요일", "수요일", "목요일"]
    );
    let mut s2 = new_table(1, 4)?;
    cell_text(&mut s2, 0, 0, "1.5")?;
    cell_text(&mut s2, 0, 1, "2")?;
    s2.run(
        "table.auto_fill",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":3}}),
    )?;
    assert_eq!(texts(&mut s2)?[0], vec!["1.5", "2", "2.5", "3.0"]);
    assert!(
        s2.run(
            "table.auto_fill",
            json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}})
        )
        .is_err()
    );

    // Caption text after the automatic number.
    assert!(s.run("table.caption_text", json!({"text":"x"})).is_err());
    s.run(
        "table.properties",
        json!({"props":{"hasCaption":true,"captionDirection":3}}),
    )?;
    assert_eq!(s.run("table.caption_text.get", json!({}))?["text"], "");
    s.run("table.caption_text", json!({"text":"분기별 판매 현황"}))?;
    assert_eq!(
        s.run("table.caption_text.get", json!({}))?["text"],
        "분기별 판매 현황"
    );
    s.run("table.caption_text", json!({"text":"수정한 캡션"}))?;
    assert!(
        s.run("table.caption_text", json!({"text":"줄\n바꿈"}))
            .is_err()
    );
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        assert_eq!(
            r.run("table.caption_text.get", json!({}))?["text"],
            "수정한 캡션",
            "{ext}"
        );
    }
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("table.caption_text.get", json!({}))?["text"],
        "분기별 판매 현황"
    );

    // 표 마당 presets are border/fill only, one undo step each.
    for preset in ["basic", "blue_header", "gray_grid", "striped"] {
        s.run("table.style", json!({"preset":preset}))?;
    }
    let striped_row = s.run(
        "table.cell_properties.get",
        json!({"range":{"start_row":2,"start_col":0,"end_row":2,"end_col":0}}),
    )?;
    assert_eq!(striped_row["fillColor"], "#f2f2f2");
    let header = s.run(
        "table.cell_properties.get",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}}),
    )?;
    assert_eq!(header["borderBottom"]["width"], 3);
    s.execute(Command::Undo)?;
    let gray = s.run(
        "table.cell_properties.get",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}}),
    )?;
    assert_eq!(gray["fillColor"], "#d9d9d9");
    assert!(s.run("table.style", json!({"preset":"neon"})).is_err());
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let c = r.run(
            "table.cell_properties.get",
            json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}}),
        )?;
        assert_eq!(c["fillColor"], "#d9d9d9", "{ext}");
    }
    Ok(())
}

#[test]
fn a_single_edge_without_fill_is_applied() -> anyhow::Result<()> {
    // RHWP only notices border edits that carry borderLeft/fillType; 03 sends all edges.
    let mut s = new_table(2, 2)?;
    let cell = json!({"start_row":0,"start_col":0,"end_row":0,"end_col":0});
    let before = s.run("table.cell_properties.get", json!({"range":cell}))?;
    for side in ["bottom", "right", "top"] {
        s.run(
            "table.cell_properties",
            json!({"range":cell,"borders":{side:{"type":2,"width":5,"color":"#ff0000"}}}),
        )?;
    }
    let after = s.run("table.cell_properties.get", json!({"range":cell}))?;
    for key in ["borderBottom", "borderRight", "borderTop"] {
        assert_eq!(after[key]["width"], 5, "{key}: {after}");
        assert_eq!(after[key]["type"], 2, "{key}: {after}");
    }
    assert_eq!(after["borderLeft"], before["borderLeft"]);
    assert_eq!(after["fillColor"], before["fillColor"]);
    Ok(())
}

#[test]
fn caption_text_matches_hangul_native_layout() -> anyhow::Result<()> {
    // Observed from a caption typed in Hangul 2024 ("표 1 분기별 판매 현황") and read by
    // RHWP: "표 " + placeholder for the number (8 units) + " " + text; count = units + 1.
    let mut s = new_table(2, 2)?;
    s.run(
        "table.properties",
        json!({"props":{"hasCaption":true,"captionDirection":3}}),
    )?;
    s.run("table.caption_text", json!({"text":"분기별 판매 현황"}))?;
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let p = s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .find_map(|c| match c {
                Control::Table(t) => t.caption.as_ref().map(|c| c.paragraphs[0].clone()),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("{label}: no caption"))?;
        assert_eq!(p.text, "표   분기별 판매 현황", "{label}");
        assert_eq!(
            p.char_offsets,
            vec![0, 1, 2, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
            "{label}"
        );
        assert_eq!(p.char_count, 21, "{label}");
        assert_eq!(p.control_text_positions(), vec![2], "{label}");
        Ok(())
    };
    check(&s, "memory")?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        check(&r, ext)?;
        assert_eq!(
            r.run("table.caption_text.get", json!({}))?["text"],
            "분기별 판매 현황"
        );
    }
    Ok(())
}

#[test]
fn table_move_follows_hangul_cell_navigation() -> anyhow::Result<()> {
    let mut s = new_table(3, 3)?;
    cell_text(&mut s, 0, 1, "가나")?;
    let mv = |s: &mut Session, m: &str| s.run("table.cell_move", json!({"motion": m}));
    let at = |v: &Value| (v["row"].as_u64(), v["col"].as_u64());
    let v = mv(&mut s, "row_begin")?;
    assert_eq!(at(&v), (Some(0), Some(0)));
    let v = mv(&mut s, "right")?;
    assert_eq!(
        (v["moved"].as_bool(), at(&v), s.cursor.offset),
        (Some(true), (Some(0), Some(1)), 0)
    );
    let v = mv(&mut s, "right")?;
    assert_eq!(at(&v), (Some(0), Some(2)));
    let v = mv(&mut s, "right")?;
    assert_eq!(v["moved"], false, "edge: {v}");
    // Moving left / up lands at the end of the neighbour's text.
    let v = mv(&mut s, "left")?;
    assert_eq!((at(&v), s.cursor.offset), ((Some(0), Some(1)), 2));
    let v = mv(&mut s, "down")?;
    assert_eq!((at(&v), s.cursor.offset), ((Some(1), Some(1)), 0));
    let v = mv(&mut s, "up")?;
    assert_eq!((at(&v), s.cursor.offset), ((Some(0), Some(1)), 2));
    assert_eq!(at(&mv(&mut s, "col_bottom")?), (Some(2), Some(1)));
    assert_eq!(at(&mv(&mut s, "col_top")?), (Some(0), Some(1)));
    assert_eq!(at(&mv(&mut s, "row_end")?), (Some(0), Some(2)));
    // Tab wraps to the next row; Shift+Tab walks back.
    let v = mv(&mut s, "next")?;
    assert_eq!(
        (at(&v), v["appended"].as_bool()),
        ((Some(1), Some(0)), Some(false))
    );
    assert_eq!(at(&mv(&mut s, "prev")?), (Some(0), Some(2)));
    // Tab in the last cell appends a row (Hangul TableRightCellAppend).
    assert_eq!(at(&mv(&mut s, "col_bottom")?), (Some(2), Some(2)));
    let v = mv(&mut s, "next")?;
    assert_eq!(
        (at(&v), v["appended"].as_bool()),
        ((Some(3), Some(0)), Some(true))
    );
    assert_eq!(info(&mut s)?["rows"], 4);
    let mut r = reopen(&mut s, "hwpx")?;
    assert_eq!(r.run("table.info", json!({}))?["rows"], 4);
    s.execute(Command::Undo)?;
    assert_eq!(info(&mut s)?["rows"], 3);
    assert!(s.cursor.cell.is_some());
    // Shift+Tab in the first cell stays; unknown motions are rejected without change.
    mv(&mut s, "col_top")?;
    mv(&mut s, "row_begin")?;
    assert_eq!(mv(&mut s, "prev")?["moved"], false);
    assert!(mv(&mut s, "diagonal").is_err());
    assert_eq!(at(&mv(&mut s, "row_begin")?), (Some(0), Some(0)));
    // Merged cells count as one stop.
    s.run(
        "table.merge",
        json!({"range":{"start_row":0,"start_col":0,"end_row":1,"end_col":1}}),
    )?;
    assert_eq!(at(&mv(&mut s, "row_end")?), (Some(0), Some(2)));
    assert_eq!(at(&mv(&mut s, "left")?), (Some(0), Some(0)));
    assert_eq!(at(&mv(&mut s, "down")?), (Some(2), Some(0)));
    assert_eq!(at(&mv(&mut s, "up")?), (Some(0), Some(0)));
    // Tab / Shift+Tab use the same navigation.
    s.move_cell(false)?;
    assert_eq!(
        info(&mut s)?["current"],
        info(&mut s)?["cells"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["row"] == 0 && c["col"] == 2))
            .and_then(|c| c["index"].as_u64())
            .map(Value::from)
            .unwrap_or(Value::Null)
    );
    s.move_cell(true)?;
    assert_eq!(at(&mv(&mut s, "right")?), (Some(0), Some(2)));
    // Outside a table the command fails.
    let mut body = Session::new()?;
    assert!(
        body.run("table.cell_move", json!({"motion":"right"}))
            .is_err()
    );
    Ok(())
}

#[test]
fn table_style_options_shade_and_user_auto_fill() -> anyhow::Result<()> {
    let mut s = new_table(4, 3)?;
    let fill = |s: &mut Session, row: u16, col: u16| -> anyhow::Result<(String, String)> {
        let v = s.run(
            "table.cell_properties.get",
            json!({"range":{"start_row":row,"start_col":col,"end_row":row,"end_col":col}}),
        )?;
        Ok((
            v["fillType"].as_str().unwrap_or("").to_ascii_lowercase(),
            v["fillColor"].as_str().unwrap_or("").to_ascii_lowercase(),
        ))
    };
    // Default options: header row filled, body clear.
    s.run("table.style", json!({"preset":"blue_header"}))?;
    assert_eq!(fill(&mut s, 0, 0)?, ("solid".into(), "#dbe5f1".into()));
    assert_eq!(fill(&mut s, 1, 0)?.0, "none");
    // 첫째 칸 + 마지막 줄 on, 제목 줄 off, 회색조: luminance gray of the accent color.
    s.run(
        "table.style",
        json!({"preset":"blue_header","header_row":false,"first_column":true,"last_row":true,"grayscale":true}),
    )?;
    assert_eq!(fill(&mut s, 0, 1)?.0, "none");
    let (t, c) = fill(&mut s, 2, 0)?;
    assert_eq!(t, "solid");
    assert_eq!(c, "#e3e3e3", "gray of #dbe5f1");
    assert_eq!(fill(&mut s, 3, 2)?.0, "solid");
    // 셀 배경 off keeps fills untouched; 테두리 off + 배경 off is rejected.
    s.run("table.style", json!({"preset":"basic","background":false}))?;
    assert_eq!(fill(&mut s, 2, 0)?.1, "#e3e3e3");
    assert!(
        s.run(
            "table.style",
            json!({"preset":"basic","background":false,"borders":false})
        )
        .is_err()
    );
    assert!(
        s.run("table.style", json!({"preset":"basic","stripes":true}))
            .is_err()
    );
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(fill(&mut s, 0, 0)?.0, "none");

    // 셀 음영: Hangul-measured steps from #dbe5f1 and from white.
    s.run(
        "table.cell_properties",
        json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0},"props":{"fillType":"solid","fillColor":"#dbe5f1"}}),
    )?;
    let one = json!({"start_row":0,"start_col":0,"end_row":0,"end_col":0});
    let v = s.run("table.cell_shade", json!({"range":one,"darker":true}))?;
    assert_eq!(v["cells"], 1);
    assert_eq!(fill(&mut s, 0, 0)?.1, "#c8d7e9");
    s.execute(Command::Undo)?;
    s.run("table.cell_shade", json!({"range":one,"darker":false}))?;
    assert_eq!(fill(&mut s, 0, 0)?.1, "#ecf1f7");
    let white = json!({"start_row":1,"start_col":0,"end_row":1,"end_col":1});
    let v = s.run("table.cell_shade", json!({"range":white,"darker":true}))?;
    assert_eq!(v["cells"], 2);
    assert_eq!(fill(&mut s, 1, 1)?.1, "#f2f2f2");
    let v = s.run("table.cell_shade", json!({"range":white,"darker":false}))?;
    assert_eq!(v["cells"], 2);
    assert_eq!(fill(&mut s, 1, 1)?.0, "none");
    assert_eq!(
        s.run("table.cell_shade", json!({"range":white,"darker":false}))?["cells"],
        0
    );
    assert!(s.run("table.cell_shade", json!({"darker":"yes"})).is_err());

    // 사용자 정의 자동 채우기 cycles the list from the first cell's entry.
    cell_text(&mut s, 2, 0, "중")?;
    let row = json!({"start_row":2,"start_col":0,"end_row":3,"end_col":2});
    let v = s.run(
        "table.auto_fill",
        json!({"range":row,"values":["상","중","하"]}),
    )?;
    assert_eq!(v["cells"], 5);
    let texts: Vec<String> = info(&mut s)?["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["row"].as_u64() >= Some(2))
        .map(|c| c["text"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(texts, ["중", "하", "상", "중", "하", "상"]);
    s.execute(Command::Undo)?;
    assert!(
        s.run("table.auto_fill", json!({"range":row,"values":[]}))
            .is_err()
    );
    assert!(
        s.run("table.auto_fill", json!({"range":one,"values":["a"]}))
            .is_err()
    );
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let v = r.run(
            "table.cell_properties.get",
            json!({"range":{"start_row":0,"start_col":0,"end_row":0,"end_col":0}}),
        )?;
        assert_eq!(
            v["fillColor"].as_str().map(str::to_ascii_lowercase),
            Some("#ecf1f7".into()),
            "{ext}"
        );
    }
    Ok(())
}

#[test]
fn deferred_table_edits_leave_layout_matching_a_fresh_reopen() -> anyhow::Result<()> {
    // Cell loops (표 마당, 음영, 채우기, 정렬…) defer RHWP pagination to one pass at
    // the end. The live layout afterwards must equal the layout of the saved file.
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞 문단".into()))?;
    s.run("table.create", json!({"rows":120,"columns":3}))?;
    s.run(
        "table.style",
        json!({"preset":"striped","first_column":true}),
    )?;
    s.run(
        "table.cell_shade",
        json!({"range":{"start_row":0,"start_col":0,"end_row":119,"end_col":0},"darker":true}),
    )?;
    let pages = s.page_count()?;
    assert!(pages >= 2, "long table spans pages: {pages}");
    let far = s
        .entries()
        .into_iter()
        .rfind(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("cells"))?
        .0;
    let live = s.caret_at(far)?;
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("deferred.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(r.page_count()?, pages, "{ext}");
        let reopened = r.caret_at(far)?;
        assert_eq!(reopened["pageIndex"], live["pageIndex"], "{ext}");
        let dy = (reopened["y"].as_f64().unwrap_or(0.0) - live["y"].as_f64().unwrap_or(0.0)).abs();
        assert!(dy < 1.0, "{ext}: {live} vs {reopened}");
    }
    // One undo step per command; the first undo returns the shade, the second the style.
    let fill = |s: &mut Session| -> anyhow::Result<Value> {
        Ok(s.run(
            "table.cell_properties.get",
            json!({"range":{"start_row":5,"start_col":0,"end_row":5,"end_col":0}}),
        )?["fillColor"]
            .clone())
    };
    let shaded = fill(&mut s)?;
    s.execute(Command::Undo)?;
    assert_ne!(fill(&mut s)?, shaded);
    s.execute(Command::Undo)?;
    assert_eq!(s.run("table.info", json!({}))?["rows"], 120);
    Ok(())
}
