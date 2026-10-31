use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};
fn table(s: &mut Session) -> anyhow::Result<Value> {
    Ok(s.run("table.info", json!({}))?["table"].clone())
}
fn create() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run(
        "table.draw_create",
        json!({"width_mm":100.0,"height_mm":60.0,"x_mm":30.0,"y_mm":40.0}),
    )?;
    Ok(s)
}
fn pen(s: &mut Session, start: [f64; 2], end: [f64; 2], erase: bool) -> anyhow::Result<Value> {
    let a = table(s)?;
    s.run(
        "table.pen",
        json!({"table":a,"start":start,"end":end,"erase":erase}),
    )
}
#[test]
fn draw_grid_at_pointer_position_and_erase_with_single_undo() -> anyhow::Result<()> {
    let mut s = create()?;
    s.execute(Command::Insert("본문 보존".into()))?;
    pen(&mut s, [30.0, 0.0], [30.0, 60.0], false)?;
    let i = s.run("table.info", json!({}))?;
    assert_eq!(i["cells"].as_array().unwrap().len(), 2);
    assert!((i["columnWidths"][0].as_f64().unwrap() - 30.0 * 7200.0 / 25.4).abs() < 3.0);
    pen(&mut s, [0.0, 20.0], [100.0, 20.0], false)?;
    let i = s.run("table.info", json!({}))?;
    assert_eq!(i["cells"].as_array().unwrap().len(), 4);
    pen(&mut s, [30.0, 0.0], [30.0, 60.0], true)?;
    let i = s.run("table.info", json!({}))?;
    assert_eq!(i["cells"].as_array().unwrap().len(), 2);
    assert!(i.to_string().contains("본문 보존"));
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("table.info", json!({}))?["cells"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    s.execute(Command::Redo)?;
    assert_eq!(
        s.run("table.info", json!({}))?["cells"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    Ok(())
}
#[test]
fn partial_row_drawing_keeps_other_row_merged_and_roundtrips() -> anyhow::Result<()> {
    let mut s = create()?;
    pen(&mut s, [0.0, 20.0], [100.0, 20.0], false)?;
    pen(&mut s, [35.0, 0.0], [35.0, 20.0], false)?;
    let before = s.run("table.info", json!({}))?;
    assert_eq!(before["cells"].as_array().unwrap().len(), 3);
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("pen.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let c = r
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
            .unwrap()
            .0;
        r.select(c, false)?;
        let after = r.run("table.info", json!({}))?;
        assert_eq!(after["columnWidths"], before["columnWidths"], "{ext}");
        assert_eq!(after["cells"].as_array().unwrap().len(), 3);
        assert_eq!(after["cells"][2]["colSpan"], 2);
    }
    Ok(())
}
#[test]
fn outer_edge_and_diagonal_erasure_preserve_cells() -> anyhow::Result<()> {
    let mut s = create()?;
    pen(&mut s, [0.0, 0.0], [100.0, 60.0], false)?;
    assert_eq!(
        s.run("table.cell_properties.get", json!({}))?["diagonalBackSlash"],
        1
    );
    pen(&mut s, [0.0, 0.0], [100.0, 60.0], true)?;
    assert_eq!(
        s.run("table.cell_properties.get", json!({}))?["diagonalBackSlash"],
        0
    );
    pen(&mut s, [0.0, 0.0], [100.0, 0.0], true)?;
    let c = s.run("table.cell_properties.get", json!({}))?;
    assert_eq!(c["borderTop"]["type"], 0);
    assert_ne!(c["borderBottom"]["type"], 0);
    assert_eq!(
        s.run("table.info", json!({}))?["cells"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    Ok(())
}
#[test]
fn invalid_strokes_and_protected_cells_are_atomic() -> anyhow::Result<()> {
    let mut s = create()?;
    let before = s.run("table.info", json!({}))?;
    let generation = s.generation;
    for (start, end, erase) in [
        ([20.0, 10.0], [20.0, 30.0], false),
        ([40.0, 0.0], [40.0, 60.0], true),
    ] {
        assert!(pen(&mut s, start, end, erase).is_err());
        assert_eq!(s.generation, generation);
        assert_eq!(s.run("table.info", json!({}))?, before);
    }
    s.run(
        "table.cell_properties",
        json!({"props":{"cellProtect":true}}),
    )?;
    let generation = s.generation;
    assert!(pen(&mut s, [25.0, 0.0], [25.0, 60.0], false).is_err());
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn repeated_uneven_cuts_keep_spans_dimensions_and_text() -> anyhow::Result<()> {
    let mut s = create()?;
    s.execute(Command::Insert("한글😀\n둘째 문단".into()))?;
    pen(&mut s, [25.0, 0.0], [25.0, 60.0], false)?;
    pen(&mut s, [70.0, 0.0], [70.0, 60.0], false)?;
    pen(&mut s, [25.0, 22.0], [100.0, 22.0], false)?;
    let i = s.run("table.info", json!({}))?;
    assert_eq!(i["cells"].as_array().unwrap().len(), 5);
    assert_eq!(i["cells"][0]["rowSpan"], 2);
    assert!(i.to_string().contains("한글😀"));
    let widths = i["columnWidths"].as_array().unwrap();
    for (i, w) in [25.0, 45.0, 30.0].into_iter().enumerate() {
        assert!((widths[i].as_f64().unwrap() / 7200.0 * 25.4 - w).abs() < 0.03);
    }
    pen(&mut s, [25.0, 22.0], [100.0, 22.0], true)?;
    assert_eq!(
        s.run("table.info", json!({}))?["cells"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    Ok(())
}
#[test]
fn failed_creation_does_not_leave_table_or_history_and_pen_preserves_fill() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    assert!(s.run("table.draw_create",json!({"width_mm":100.0,"height_mm":30.0,"x_mm":30.0,"y_mm":30.0,"line":{"type":99,"width":0,"color":"#000000"}})).is_err());
    assert!(!s.can_undo());
    assert!(!s.dirty());
    let mut s = create()?;
    s.run(
        "table.cell_properties",
        json!({"props":{"fillType":"solid","fillColor":"#ffeecc"}}),
    )?;
    let a = table(&mut s)?;
    s.run("table.pen",json!({"table":a,"start":[30.0,0.0],"end":[30.0,60.0],"line":{"type":8,"width":5,"color":"#cc0000"}}))?;
    let p = s.run("table.cell_properties.get", json!({}))?;
    assert_eq!(p["fillColor"], "#ffeecc");
    assert_eq!(p["borderRight"]["type"], 8);
    assert_eq!(p["borderRight"]["width"], 5);
    assert_ne!(p["borderLeft"]["type"], 8);
    Ok(())
}

fn layout(s: &Session) -> anyhow::Result<Value> {
    let v: Value = serde_json::from_str(
        &s.core
            .get_page_control_layout_native(0)
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    )?;
    Ok(v["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type"] == "table")
        .unwrap()
        .clone())
}
#[test]
fn move_inline_and_floating_table_preserves_size_content_and_undo() -> anyhow::Result<()> {
    for inline in [true, false] {
        let mut s = create()?;
        s.execute(Command::Insert("이동 전 내용 🦀".into()))?;
        let a = table(&mut s)?;
        s.run(
            "table.properties",
            json!({"table":a,"props":{"treatAsChar":inline,"outerLeft":420,"outerTop":280}}),
        )?;
        let before = layout(&s)?;
        let info = s.run("table.info", json!({}))?;
        s.run(
            "table.move",
            json!({"table":a,"page":0,"dx_mm":12,"dy_mm":15}),
        )?;
        let after = layout(&s)?;
        for (key, delta) in [("x", 12.0 * 96.0 / 25.4), ("y", 15.0 * 96.0 / 25.4)] {
            assert!(
                (after[key].as_f64().unwrap() - before[key].as_f64().unwrap() - delta).abs() < 0.5,
                "inline={inline} {key}: {before} -> {after}"
            );
        }
        assert_eq!(after["w"], before["w"]);
        assert_eq!(s.run("table.info", json!({}))?["cells"], info["cells"]);
        s.execute(Command::Undo)?;
        assert_eq!(layout(&s)?, before);
        assert_eq!(
            s.run("table.properties.get", json!({}))?["treatAsChar"],
            inline
        );
        s.execute(Command::Redo)?;
        assert_eq!(layout(&s)?, after);
        for ext in ["hwp", "hwpx"] {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join(format!("move.{ext}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let r = layout(&reopened)?;
            for key in ["x", "y", "w", "h"] {
                assert!(
                    (r[key].as_f64().unwrap() - after[key].as_f64().unwrap()).abs() < 0.6,
                    "{ext} {key}: {r} vs {after}"
                );
            }
        }
        let generation = s.generation;
        assert!(
            s.run(
                "table.move",
                json!({"table":a,"page":0,"dx_mm":900,"dy_mm":0})
            )
            .is_err()
        );
        assert_eq!(s.generation, generation);
        assert_eq!(layout(&s)?, after);
    }
    Ok(())
}
/// Cell spacing (셀 간격) leaves a gap between neighbouring cells. The pen works on such
/// a table on the cell grid of `table.info` (the desktop maps a gap onto its edge).
#[test]
fn the_pen_draws_and_erases_on_a_table_with_cell_spacing() -> anyhow::Result<()> {
    let mut s = create()?;
    let a = table(&mut s)?;
    // 2 mm between cells.
    s.run(
        "table.properties",
        json!({"table":a,"props":{"cellSpacing":567}}),
    )?;
    let columns = |s: &mut Session| -> anyhow::Result<Vec<i64>> {
        let i = s.run("table.info", json!({}))?;
        Ok(i["columnWidths"]
            .as_array()
            .map(|w| {
                w.iter()
                    .map(|v| (v.as_f64().unwrap_or(0.0) * 25.4 / 7200.0).round() as i64)
                    .collect()
            })
            .unwrap_or_default())
    };
    let cells = |s: &mut Session| -> anyhow::Result<usize> {
        Ok(s.run("table.info", json!({}))?["cells"]
            .as_array()
            .map_or(0, Vec::len))
    };
    pen(&mut s, [30.0, 0.0], [30.0, 60.0], false)?;
    assert_eq!(columns(&mut s)?, [30, 70]);
    pen(&mut s, [0.0, 20.0], [100.0, 20.0], false)?;
    assert_eq!(cells(&mut s)?, 4);
    pen(&mut s, [70.0, 0.0], [70.0, 60.0], false)?;
    assert_eq!(columns(&mut s)?, [30, 40, 30]);
    pen(&mut s, [30.0, 0.0], [30.0, 60.0], true)?;
    assert_eq!(cells(&mut s)?, 4);
    assert_eq!(columns(&mut s)?.last(), Some(&30));
    let props = s.run("table.properties.get", json!({}))?;
    assert_eq!(props["cellSpacing"], 567, "the spacing stays");
    s.execute(Command::Undo)?;
    assert_eq!(cells(&mut s)?, 6);
    Ok(())
}
/// Where each table sits: (paragraph, text position) of every body table.
fn tables(s: &Session) -> Vec<(usize, usize)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            p.controls
                .iter()
                .zip(p.control_text_positions())
                .filter(|(c, _)| matches!(c, hwpcraft_doc::model::control::Control::Table(_)))
                .map(move |(_, pos)| (i, pos))
        })
        .collect()
}
/// A table treated as a character moves to another text position like other inline
/// objects (Hangul's drag); its cells and text stay, in one undo step.
#[test]
fn an_inline_table_moves_to_another_text_position() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞 문장 뒤 문장\n".into()))?;
    s.run("table.create", json!({"rows": 2, "columns": 2}))?;
    s.execute(Command::Insert("셀 내용".into()))?;
    let a = table(&mut s)?;
    s.run(
        "table.properties",
        json!({"table": a, "props": {"treatAsChar": true}}),
    )?;
    let before = tables(&s);
    let cells = s.run("table.info", json!({"table": a}))?["cells"].clone();
    let to = json!({"section": 0, "paragraph": 0, "offset": 3});
    let moved = s.run("object.move_inline", json!({"address": a, "to": to}))?["address"].clone();
    assert_eq!(moved["paragraph"], 0);
    assert_eq!(tables(&s), [(0, 3)]);
    assert_eq!(
        s.run("table.info", json!({"table": moved}))?["cells"],
        cells
    );
    let text = s.run("document.text", json!({}))?.to_string();
    assert!(text.contains("앞 문장 뒤 문장"), "{text}");
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("inline-table.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(tables(&r), [(0, 3)], "{ext}");
        assert_eq!(
            r.run("table.info", json!({"table": moved}))?["cells"],
            cells
        );
    }
    s.execute(Command::Undo)?;
    assert_eq!(tables(&s), before);
    // A floating table keeps moving by position (table.move), not by text position.
    let a = table(&mut s)?;
    s.run(
        "table.properties",
        json!({"table": a, "props": {"treatAsChar": false}}),
    )?;
    let generation = s.generation;
    assert!(
        s.run("object.move_inline", json!({"address": a, "to": to}))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    Ok(())
}
