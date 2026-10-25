//! Opening a document must stay responsive: the read-only nested-table
//! presentation repair that runs on open is time-bounded. Before the bound, a
//! document with many clipped nested tables kept the window busy for minutes.
//! With `HWPCRAFT_OPEN_BUDGET_OUT` set, the generated document is also written
//! there so other builds can be timed against it.
use hwpcraft_doc::model::{control::Control, table::Table};
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const ROOTS: usize = 24;
/// Body paragraphs after each root table: every repair step re-paginates the
/// whole document, so the stall grows with document length.
const FILLER: usize = 12;

fn shorten(t: &mut Table) {
    t.common.height = 1200;
    if t.raw_ctrl_data.len() >= 20 {
        t.raw_ctrl_data[16..20].copy_from_slice(&1200_u32.to_le_bytes());
    }
    for cell in &mut t.cells {
        cell.height = 1200;
        for p in &mut cell.paragraphs {
            for c in &mut p.controls {
                if let Control::Table(t) = c {
                    shorten(t);
                }
            }
        }
    }
}

/// `ROOTS` body tables, each holding a three-level nested table whose stored
/// frames are too short for its text, saved and reopened like an imported file.
fn clipped_nested_document(path: &std::path::Path) -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for i in 0..ROOTS {
        if i > 0 {
            s.run("caret.move", json!({"motion": "document_end"}))?;
            s.execute(Command::Insert("\n".into()))?;
        }
        s.execute(Command::Insert(format!("본문 {i}")))?;
        s.run(
            "table.create",
            json!({"rows":1,"columns":1,"width_mm":110.0,"treat_as_char":true}),
        )?;
        let first: Value =
            serde_json::to_value(hwpcraft_engine::DocumentAddress::from_cursor(s.cursor))?;
        s.run(
            "context.replace",
            json!({"address":first,"start":0,"end":0,"text":"첫째"}),
        )?;
        let middle = s.run(
            "context.table.create",
            json!({"address":first,"rows":1,"columns":1,"width_mm":65.0}),
        )?["address"]
            .clone();
        s.run(
            "context.replace",
            json!({"address":middle,"start":0,"end":0,"text":"둘째"}),
        )?;
        let leaf = s.run(
            "context.table.create",
            json!({"address":middle,"rows":1,"columns":1,"width_mm":30.0}),
        )?["address"]
            .clone();
        s.run(
            "context.replace",
            json!({"address":leaf,"start":0,"end":0,"text":"세 단계 글자"}),
        )?;
        s.run("caret.move", json!({"motion": "document_end"}))?;
        let line = "긴 본문 문장으로 쪽을 채웁니다. ".repeat(12);
        for _ in 0..FILLER {
            s.execute(Command::Insert(format!("\n{line}")))?;
        }
    }
    let mut document = s.core.document().clone();
    for sec in &mut document.sections {
        sec.raw_stream = None;
        for p in &mut sec.paragraphs {
            for c in &mut p.controls {
                if let Control::Table(t) = c {
                    shorten(t);
                }
            }
        }
    }
    let mut core = hwpcraft_doc::DocumentCore::new_empty();
    core.set_document(document);
    hwpcraft_formats::save(&core, path)?;
    // Persist line caches once, as a document saved by an editor would.
    let stored = hwpcraft_formats::open(path)?;
    hwpcraft_formats::save(&stored, path)?;
    Ok(())
}

#[test]
fn opening_many_clipped_nested_tables_stays_within_the_repair_budget() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = match std::env::var("HWPCRAFT_OPEN_BUDGET_OUT") {
        Ok(out) => std::path::PathBuf::from(out),
        Err(_) => dir.path().join("clipped-nested.hwp"),
    };
    clipped_nested_document(&path)?;
    let started = Instant::now();
    let mut s = Session::new()?;
    s.open(&path)?;
    let pages = s.page_count()?;
    let elapsed = started.elapsed();
    let report = s.presentation_report()?;
    let nested = &report["nestedTables"];
    eprintln!("open + first layout: {elapsed:?}, {pages} pages, report {nested}");
    assert!(pages >= 1);
    assert_eq!(nested["roots_seen"], ROOTS as u64, "{report}");
    // The repair pass stops on its work budget (deterministic), not on the clock.
    // The time bound is generous for slow debug runs; the unbounded pass needed
    // about 30 s of release time on this 39-page document.
    assert!(
        elapsed < Duration::from_secs(60),
        "open + first layout took {elapsed:?}: {report}"
    );
    assert_eq!(nested["scan_truncated"], true, "{report}");
    // Unrepaired roots are reported with a reason, never silently dropped.
    let adjusted = nested["roots_adjusted"].as_u64().unwrap_or(0);
    let skipped = nested["skipped"].as_array().cloned().unwrap_or_default();
    assert_eq!(adjusted + skipped.len() as u64, ROOTS as u64, "{report}");
    let reason = |s: &Value, text: &str| s["reason"].as_str().is_some_and(|r| r.contains(text));
    assert!(
        skipped
            .iter()
            .all(|s| reason(s, "작업 한도") || reason(s, "시간 한도")),
        "{report}"
    );
    // Same document, same presentation: the work budget does not depend on
    // timing. A slow (debug) build may hit the wall-clock safety stop first;
    // only then can the outcome vary between runs.
    // The reopen may reuse the stored verdict; it must be the same verdict.
    if skipped.iter().all(|s| reason(s, "작업 한도")) {
        let without_reuse = |mut v: Value| {
            if let Some(o) = v.as_object_mut() {
                o.remove("reused_previous");
            }
            v
        };
        let mut again = Session::new()?;
        again.open(&path)?;
        assert_eq!(
            without_reuse(again.presentation_report()?["nestedTables"].clone()),
            without_reuse(nested.clone())
        );
    }
    Ok(())
}

/// Timing probe for one typed character on a long document. Run with
/// `HWPCRAFT_LATENCY_DOC=<path> cargo test --release --test open_budget -- --ignored --nocapture`.
#[test]
#[ignore = "timing probe on a local long document"]
fn edit_latency_probe() -> anyhow::Result<()> {
    let Ok(path) = std::env::var("HWPCRAFT_LATENCY_DOC") else {
        return Ok(());
    };
    let started = Instant::now();
    let mut s = Session::new()?;
    s.open(std::path::Path::new(&path))?;
    eprintln!("open + first page_count: {:?}", started.elapsed());
    for round in 0..3 {
        let t = Instant::now();
        s.execute(Command::Insert("가".into()))?;
        let typed = t.elapsed();
        let t = Instant::now();
        let core_pages = s.core.page_count();
        let core = t.elapsed();
        let t = Instant::now();
        let pages = s.page_count()?;
        let layout = t.elapsed();
        let t = Instant::now();
        s.page_count()?;
        let cached = t.elapsed();
        eprintln!(
            "round {round}: insert {typed:?}, core.page_count {core:?} ({core_pages}), \
             page_count after edit {layout:?} ({pages}), cached {cached:?}, report {}",
            s.presentation_report()?["pageFields"]
        );
    }
    Ok(())
}

/// Timing probe for PDF export: page SVG generation (session thread) versus the
/// whole export. Run with `HWPCRAFT_LATENCY_DOC=<path> cargo test --release
/// --test open_budget pdf_split_probe -- --ignored --nocapture`.
#[test]
#[ignore = "timing probe on a local long document"]
fn pdf_split_probe() -> anyhow::Result<()> {
    let Ok(path) = std::env::var("HWPCRAFT_LATENCY_DOC") else {
        return Ok(());
    };
    let mut s = Session::new()?;
    s.open(std::path::Path::new(&path))?;
    let pages = s.page_count()?;
    let t = Instant::now();
    for page in 0..pages {
        s.core
            .render_page_svg_layer_with_profile_native(page, hwpcraft_doc::RenderProfile::Print)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    eprintln!("{pages} pages: SVG generation {:?}", t.elapsed());
    Ok(())
}
