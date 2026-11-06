//! Typing into the empty paragraph that holds a paragraph-relative table, in a file
//! whose rows Hangul stored, keeps the next paragraph below the table (RHWP 0112) and,
//! as in Hangul, draws the typed text below the table with the next paragraph after it
//! (RHWP 0113). Undo and redo restore the layouts, and the saved file reopens alike.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::paragraph::LineSeg;
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const HOST: usize = 1;

/// A Hangul-like file: a title, an empty host with a 3×3 table (moved down by
/// `offset` HWPUNIT when given), the next paragraph. Its rows are not RHWP's own, and
/// the next paragraph sits at host + table box, as Hangul stores it.
fn hangul_file(dir: &Path, offset: Option<i32>) -> anyhow::Result<PathBuf> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "제목\n"}))?;
    s.run("insert.table", json!({"rows": 3, "columns": 3}))?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("text.insert", json!({"text": "다음 문단\n끝 문단"}))?;
    if let Some(offset) = offset {
        s.core
            .set_table_properties_native(0, HOST, 0, &json!({"vertOffset": offset}).to_string())
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    let paragraphs = &mut s.core.document_mut().sections[0].paragraphs;
    let box_end = paragraphs[HOST]
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Table(t) => Some(
                (t.common.vertical_offset as i32).max(0)
                    + i32::from(t.outer_margin_top)
                    + t.common.height as i32
                    + i32::from(t.outer_margin_bottom),
            ),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("no table in the host"))?;
    let (mut vpos, mut host) = (0, 0);
    for (pi, para) in paragraphs.iter_mut().enumerate() {
        if pi == HOST {
            host = vpos;
        }
        if pi == HOST + 1 {
            vpos = vpos.max(host + box_end);
        }
        for line in &mut para.line_segs {
            line.tag &= !LineSeg::TAG_IMPLEMENTATION_PROPERTY;
            line.vertical_pos = vpos;
            vpos += line.line_height + line.line_spacing;
        }
    }
    // Written by RHWP itself: a HwpCraft save would mark a new document's rows its own.
    let path = dir.join("hangul.hwp");
    std::fs::write(
        &path,
        s.core
            .export_hwp_native()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
    )?;
    Ok(path)
}

/// The drawn table's top, the bottom of the table and the host's text, and the top
/// of the "다음" paragraph on page 0.
fn geometry(s: &Session) -> anyhow::Result<((f64, f64), f64)> {
    let controls: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let f = |c: &Value, k: &str| c[k].as_f64().unwrap_or(f64::NAN);
    let table = controls["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["type"] == "table" && c.get("cellIdx").is_none())
        .map(|t| (f(t, "y"), f(t, "y") + f(t, "h")))
        .ok_or_else(|| anyhow::anyhow!("no table drawn"))?;
    let text: Value = serde_json::from_str(
        &s.core
            .get_page_text_layout_native(0)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
    )?;
    let runs = text["runs"].as_array().into_iter().flatten();
    let bottom = runs
        .clone()
        .filter(|r| r["paraIdx"] == HOST && r.get("cellIdx").is_none())
        .map(|r| f(r, "y") + f(r, "h"))
        .fold(table.1, f64::max);
    let next = runs
        .clone()
        .find(|r| r["text"].as_str().is_some_and(|t| t.starts_with("다음")))
        .map(|r| f(r, "y"))
        .ok_or_else(|| anyhow::anyhow!("no next paragraph drawn"))?;
    Ok(((table.0, bottom), next))
}

/// The top of the host's first drawn text and the bottom of the drawn table on page 0.
fn host_text_top_and_table_bottom(s: &Session) -> anyhow::Result<(f64, f64)> {
    let controls: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let table = controls["controls"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["type"] == "table" && c.get("cellIdx").is_none())
        .and_then(|t| Some(t["y"].as_f64()? + t["h"].as_f64()?))
        .ok_or_else(|| anyhow::anyhow!("no table drawn"))?;
    let text: Value = serde_json::from_str(
        &s.core
            .get_page_text_layout_native(0)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
    )?;
    let top = text["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["paraIdx"] == HOST && r.get("cellIdx").is_none())
        .filter_map(|r| r["y"].as_f64())
        .fold(f64::INFINITY, f64::min);
    Ok((top, table))
}

/// The host's typed text is drawn below the table, as Hangul places text that does
/// not fit in the table's vertical offset.
fn assert_text_below_table(s: &Session, what: &str) -> anyhow::Result<()> {
    let (text, table) = host_text_top_and_table_bottom(s)?;
    assert!(
        text >= table - 0.5,
        "{what}: host text top {text}, table bottom {table}"
    );
    Ok(())
}

fn next_vpos(s: &Session) -> i32 {
    s.core.document().sections[0].paragraphs[HOST + 1].line_segs[0].vertical_pos
}

/// Page 0's controls and text, as drawn.
fn page(s: &Session) -> anyhow::Result<(String, String)> {
    Ok((
        s.page_control_layout(0)?,
        s.core
            .get_page_text_layout_native(0)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
    ))
}

fn host_len(s: &Session) -> usize {
    s.core.document().sections[0].paragraphs[HOST]
        .text
        .chars()
        .count()
}

/// The drawn next paragraph is below the table and the host's text, without a gap.
fn assert_next_below(s: &Session, what: &str) -> anyhow::Result<()> {
    let ((_, bottom), next) = geometry(s)?;
    assert!(
        next >= bottom - 0.5 && next < bottom + 40.0,
        "{what}: table and host text bottom {bottom}, next {next}"
    );
    Ok(())
}

fn type_into_host(text: &str, offset: Option<i32>) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.open(&hangul_file(dir.path(), offset)?)?;
    let stored = next_vpos(&s);
    let ((_, bottom), next) = geometry(&s)?;
    assert!(
        next >= bottom,
        "as stored: table bottom {bottom}, next {next}"
    );
    let before = page(&s)?;

    s.cursor = Cursor {
        section: 0,
        paragraph: HOST,
        offset: host_len(&s),
        cell: None,
    };
    s.execute(Command::Insert(text.into()))?;
    let host = &s.core.document().sections[0].paragraphs[HOST].line_segs;
    let last = host
        .last()
        .map_or(0, |l| l.vertical_pos + l.line_height + l.line_spacing);
    // Never above the stored place, nor above the host's own lines.
    assert!(
        next_vpos(&s) >= stored.max(last),
        "next {} (stored {stored}, host end {last})",
        next_vpos(&s)
    );
    assert_next_below(&s, "after typing")?;
    assert_text_below_table(&s, "after typing")?;
    let after = page(&s)?;

    s.execute(Command::Undo)?;
    assert_eq!(next_vpos(&s), stored, "undo");
    assert_eq!(page(&s)?, before, "undo restores the page");
    s.execute(Command::Redo)?;
    assert_eq!(page(&s)?, after, "redo");

    let live = geometry(&s)?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("typed.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let ((top, bottom), next) = geometry(&r)?;
        assert!(
            (top - live.0.0).abs() < 0.5
                && (bottom - live.0.1).abs() < 0.5
                && (next - live.1).abs() < 0.5,
            "{ext}: {:?} vs live {live:?}",
            ((top, bottom), next)
        );
        assert_text_below_table(&r, ext)?;
    }
    Ok(())
}

#[test]
fn typing_into_a_table_host_keeps_the_next_paragraph_below_the_table() -> anyhow::Result<()> {
    type_into_host("가나", None)
}

#[test]
fn a_table_host_that_grows_past_the_table_pushes_the_next_paragraph() -> anyhow::Result<()> {
    type_into_host(&"가나다라마바사아자차".repeat(60), None)
}

/// A table moved down a little (less than a line) still takes the typed lines below
/// it — RHWP drew one host line above such a table and the rest over it.
#[test]
fn lines_typed_into_a_host_whose_table_has_a_small_offset_go_below_the_table() -> anyhow::Result<()>
{
    type_into_host(&"가나다라마바사아자차".repeat(20), Some(150))
}

/// Deleting what was typed brings the next paragraph back to where the file put it,
/// not to where the longer text had pushed it.
#[test]
fn deleting_what_was_typed_into_a_table_host_brings_the_next_paragraph_back() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.open(&hangul_file(dir.path(), None)?)?;
    let stored = next_vpos(&s);
    let len = host_len(&s);
    s.cursor = Cursor {
        section: 0,
        paragraph: HOST,
        offset: len,
        cell: None,
    };
    s.execute(Command::Insert("가나다라마바사아자차".repeat(60)))?;
    assert!(next_vpos(&s) > stored, "typing pushed the next paragraph");
    s.anchor = Some(len);
    s.execute(Command::Backspace)?;
    assert_eq!(host_len(&s), len);
    assert_eq!(next_vpos(&s), stored, "back at the stored place");
    assert_next_below(&s, "after deleting")?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("deleted.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_next_below(&r, ext)?;
    }
    Ok(())
}
