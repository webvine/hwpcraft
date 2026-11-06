//! The paragraph after a table that is not treated as a character starts below the
//! table's box: its outer margins and its caption (RHWP 0105). The layout while
//! editing and the layouts of the saved HWP and HWPX, read back, agree.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::{Value, json};

const LEAD: &str = "제목\n1. 배경\n본문 문단이다.\n2. 과제\n";

/// Page 1: each table's (top, bottom including its caption) and the next paragraph's top.
fn geometry(s: &Session, caption: Option<&str>) -> anyhow::Result<(Vec<(f64, f64)>, f64)> {
    let controls: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let f = |c: &Value, k: &str| c[k].as_f64().unwrap_or(f64::NAN);
    let mut tables: Vec<(f64, f64)> = controls["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["type"] == "table" && c.get("cellIdx").is_none())
        .map(|t| (f(t, "y"), f(t, "y") + f(t, "h")))
        .collect();
    tables.sort_by(|a, b| a.0.total_cmp(&b.0));
    let text: Value = serde_json::from_str(
        &s.core
            .get_page_text_layout_native(0)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
    )?;
    let runs = text["runs"].as_array().cloned().unwrap_or_default();
    if let Some(caption) = caption {
        for run in runs
            .iter()
            .filter(|r| r["text"].as_str().is_some_and(|t| t.contains(caption)))
        {
            // A caption belongs to the nearest table above or below it.
            let (y, bottom) = (f(run, "y"), f(run, "y") + f(run, "h"));
            if let Some(t) = tables.iter_mut().min_by(|a, b| {
                (a.0 - y)
                    .abs()
                    .min((a.1 - y).abs())
                    .total_cmp(&(b.0 - y).abs().min((b.1 - y).abs()))
            }) {
                t.0 = t.0.min(y);
                t.1 = t.1.max(bottom);
            }
        }
    }
    let next = runs
        .iter()
        .find(|r| r["text"].as_str().is_some_and(|t| t.starts_with("3.")))
        .ok_or_else(|| anyhow::anyhow!("no next paragraph run"))?;
    Ok((tables, f(next, "y")))
}

/// `lead` text, then `tables` tables (each with a caption `direction` when given:
/// 3 below, 2 above), then a paragraph; checked live and after HWP/HWPX save+reopen.
fn check(lead: &str, tables: usize, caption: Option<u8>) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    if !lead.is_empty() {
        s.run("text.insert", json!({ "text": lead }))?;
    }
    // Each table's outer margins (top, bottom) in px, top to bottom.
    let mut margins = Vec::new();
    for _ in 0..tables {
        let t = s.run("insert.table", json!({"rows": 3, "columns": 3}))?;
        let table = t["table"].clone();
        let a: ObjectAddress = serde_json::from_value(table.clone())?;
        let Some(Control::Table(t)) = s.core.document().sections[a.section].paragraphs[a.paragraph]
            .controls
            .get(a.control)
        else {
            anyhow::bail!("no table at {a:?}");
        };
        let px = |hu: i16| f64::from(hu) * 96.0 / 7200.0;
        margins.push((px(t.outer_margin_top), px(t.outer_margin_bottom)));
        s.run(
            "table.cells.set",
            json!({"table": table,
                   "cells": [["과제", "기간", "담당"], ["가", "나", "다"], ["라", "마", "바"]]}),
        )?;
        if let Some(direction) = caption {
            s.run(
                "table.properties",
                json!({"table": table, "props": {"hasCaption": true, "captionDirection": direction}}),
            )?;
            s.run(
                "table.caption_text",
                json!({"table": table, "text": "캡션 글"}),
            )?;
        }
        s.run("caret.move", json!({"motion": "document_end"}))?;
    }
    s.run("text.insert", json!({"text": "3. 효과\n끝 문단."}))?;
    let caption_text = caption.map(|_| "캡션 글");
    let live = geometry(&s, caption_text)?;
    let mut all = vec![("live", live.clone())];
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("t.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        all.push((ext, geometry(&r, caption_text)?));
    }
    let case = format!(
        "lead {} tables {tables} caption {caption:?}",
        !lead.is_empty()
    );
    for (name, (boxes, next)) in &all {
        assert_eq!(boxes.len(), tables, "{case} {name}: {boxes:?}");
        // Stacked tables keep the upper one's bottom and the lower one's top margin
        // (the layout reports positions to 0.1 px).
        for (pair, m) in boxes.windows(2).zip(margins.windows(2)) {
            let need = m[0].1 + m[1].0;
            assert!(
                need > 0.0 && pair[1].0 - pair[0].1 >= need - 0.1,
                "{case} {name}: gap {} < margins {need}: {boxes:?}",
                pair[1].0 - pair[0].1
            );
        }
        let last = boxes.last().map_or(f64::NAN, |b| b.1);
        assert!(
            *next >= last,
            "{case} {name}: box bottom {last}, next {next}"
        );
        for (b, l) in boxes.iter().zip(&live.0) {
            assert!(
                (b.0 - l.0).abs() < 0.5,
                "{case} {name}: {boxes:?} vs live {:?}",
                live.0
            );
        }
        assert!(
            (next - live.1).abs() < 0.5,
            "{case} {name}: next {next} vs live {}",
            live.1
        );
    }
    Ok(())
}

#[test]
fn the_paragraph_after_a_table_starts_below_its_outer_margin() -> anyhow::Result<()> {
    check(LEAD, 1, None)
}

#[test]
fn the_paragraph_after_a_captioned_table_starts_below_the_caption() -> anyhow::Result<()> {
    check(LEAD, 1, Some(3))?;
    check(LEAD, 1, Some(2))
}

#[test]
fn stacked_tables_keep_their_margins() -> anyhow::Result<()> {
    check(LEAD, 2, None)
}

#[test]
fn a_table_at_the_top_of_the_page_keeps_the_next_paragraph_below_it() -> anyhow::Result<()> {
    check("", 1, None)
}
