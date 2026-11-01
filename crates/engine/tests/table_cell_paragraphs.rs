//! Owner: format area. New table cells start as 바탕글 paragraphs, as in Hangul: a
//! table made in a paragraph with spacing before and an indent gets cells without
//! them (Hangul 2024 probe, 10-31: ParaShape 0 cells). RHWP copied the caret
//! paragraph's shape; its spacing before was left out while editing but counted
//! once a saved HWPX was opened again, so every single-line row grew.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::json;

fn cell_ys(s: &Session) -> Vec<f64> {
    let layout = s.core.get_page_text_layout_native(0).unwrap();
    let v: serde_json::Value = serde_json::from_str(&layout).unwrap();
    v["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["text"].as_str().is_some_and(|t| t.starts_with("셀")))
        .map(|r| r["y"].as_f64().unwrap())
        .collect()
}

#[test]
fn new_cells_are_plain_paragraphs_and_keep_their_rows_on_reopen() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("format.style.set.apply", json!({"name": "보고서"}))?;
    s.run(
        "format.paragraph",
        json!({"spacingBefore": 1200, "spacingAfter": 300, "marginLeft": 1000}),
    )?;
    s.run("text.insert", json!({"text": "앞 문단\n"}))?;
    let t = s.run("insert.table", json!({"rows": 3, "columns": 1}))?;
    s.run(
        "table.cells.set",
        json!({"table": t["table"], "row": 0, "col": 0, "cells": [["셀1"], ["셀2"], ["셀3"]]}),
    )?;
    let doc = s.core.document();
    let plain = doc.doc_info.styles[0].para_shape_id;
    for para in &doc.sections[0].paragraphs {
        for control in &para.controls {
            if let Control::Table(table) = control {
                for cell in &table.cells {
                    for p in &cell.paragraphs {
                        assert_eq!(p.para_shape_id, plain, "cell takes 바탕글's shape");
                        assert_eq!(p.style_id, 0);
                    }
                }
            }
        }
    }
    let live = cell_ys(&s);
    assert_eq!(live.len(), 3);
    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("cells.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        let reopened = cell_ys(&back);
        // Row pitch (the table may sit a little differently once the body is laid
        // out afresh; that is not the cells).
        let pitch = |ys: &[f64]| ys.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
        for (a, b) in pitch(&live).iter().zip(pitch(&reopened)) {
            assert!(
                (a - b).abs() < 0.6,
                "{ext}: live {live:?} reopened {reopened:?}"
            );
        }
    }
    Ok(())
}

/// Cells made in 바탕글's shape at creation (RHWP 0098) are the same as cells made in
/// the caret paragraph's shape and changed afterwards one by one, which cost a style
/// resolution per cell: same cell paragraphs, rows and drawn page. Only the stored
/// line width of the empty cell paragraph differs: made at once it keeps the value
/// RHWP writes for a new cell after Hangul's own blank table (cell width − 282, as in
/// 0.19); changed afterwards, the per-cell relayout replaced it with its own measure.
#[test]
fn cells_made_plain_match_cells_changed_one_by_one() -> anyhow::Result<()> {
    let setup = || -> anyhow::Result<Session> {
        let mut s = Session::new()?;
        s.run("format.style.set.apply", json!({"name": "보고서"}))?;
        s.run(
            "format.paragraph",
            json!({"spacingBefore": 1200, "spacingAfter": 300, "marginLeft": 1000}),
        )?;
        s.run("text.insert", json!({"text": "앞 문단\n"}))?;
        Ok(s)
    };
    let table = |s: &Session| -> String {
        let doc = s.core.document();
        doc.sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .find_map(|c| match c {
                Control::Table(t) => Some(format!(
                    "{:?} {:?}",
                    t.row_sizes,
                    t.cells
                        .iter()
                        .map(|cell| {
                            (
                                cell.height,
                                cell.paragraphs
                                    .iter()
                                    .map(|p| {
                                        let rows: Vec<_> = p
                                            .line_segs
                                            .iter()
                                            .map(|l| {
                                                (l.vertical_pos, l.line_height, l.line_spacing)
                                            })
                                            .collect();
                                        (p.para_shape_id, p.style_id, rows)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>()
                )),
                _ => None,
            })
            .unwrap_or_default()
    };

    // Now: one creation with the cell shape given.
    let mut made = setup()?;
    made.run("insert.table", json!({"rows": 3, "columns": 2}))?;
    let _ = made.page_count()?;

    // Before: the caret paragraph's shape, then each cell paragraph changed.
    let mut changed = setup()?;
    let c = changed.cursor;
    let plain = changed.core.document().doc_info.styles[0].para_shape_id;
    let created = changed
        .core
        .create_table_native(c.section, c.paragraph, c.offset, 3, 2)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let v: serde_json::Value = serde_json::from_str(&created)?;
    let (para, control) = (
        v["paraIdx"].as_u64().unwrap_or_default() as usize,
        v["controlIdx"].as_u64().unwrap_or_default() as usize,
    );
    for cell in 0..6 {
        changed
            .core
            .set_cell_para_shape_id_native(c.section, para, control, cell, 0, plain)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    let _ = changed.page_count()?;

    assert_eq!(table(&made), table(&changed));
    assert_eq!(made.render_page_svg(0)?, changed.render_page_svg(0)?);
    let doc = made.core.document();
    let t = doc.sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("표가 없습니다"))?;
    for cell in &t.cells {
        assert_eq!(
            cell.paragraphs[0].line_segs[0].segment_width,
            cell.width as i32 - 282
        );
    }
    Ok(())
}

/// MCP `table.create`, inline or not, also starts cells as 바탕글 paragraphs, made so
/// at creation (RHWP 0098/0100) rather than cell by cell.
#[test]
fn mcp_table_create_makes_plain_cells_inline_or_not() -> anyhow::Result<()> {
    for inline in [false, true] {
        let mut s = Session::new()?;
        s.run(
            "format.paragraph",
            json!({"spacingBefore": 1200, "marginLeft": 1000}),
        )?;
        s.run("text.insert", json!({"text": "앞 "}))?;
        s.run(
            "table.create",
            json!({"rows": 2, "columns": 2, "treat_as_char": inline}),
        )?;
        let doc = s.core.document();
        let plain = doc.doc_info.styles[0].para_shape_id;
        let tables: Vec<_> = doc.sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter_map(|c| match c {
                Control::Table(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tables.len(), 1, "inline {inline}");
        for cell in &tables[0].cells {
            for p in &cell.paragraphs {
                assert_eq!(p.para_shape_id, plain, "inline {inline}");
                assert_eq!(p.style_id, 0);
            }
        }
    }
    Ok(())
}
