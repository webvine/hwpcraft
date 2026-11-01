//! An edit in a document Hangul wrote leaves the pages before it laid out as they
//! were opened, and a saved HWP reopens like the live pages.
use hwpcraft_doc::Cursor;
use hwpcraft_engine::Session;
use serde_json::json;

/// A Hangul press release whose first page opens with head tables.
const SAMPLE: &str = "../../../rhwp/samples/20250130-hongbo.hwp";

fn sample() -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    path.exists().then_some(path)
}

fn pages(s: &Session) -> anyhow::Result<Vec<String>> {
    (0..s.page_count()?).map(|p| s.render_page_svg(p)).collect()
}

/// Typing at the end used to turn RHWP's stored-geometry rules off for the whole
/// document, so page 1's head tables moved by 8–14 px, and the saved HWP reopened
/// with them back in place.
#[test]
fn typing_at_the_end_keeps_the_pages_before_it() -> anyhow::Result<()> {
    let Some(path) = sample() else {
        return Ok(());
    };
    let mut s = Session::new()?;
    s.open(&path)?;
    let opened = pages(&s)?;
    let last = s.core.document().sections[0].paragraphs.len() - 1;
    let end = s.core.document().sections[0].paragraphs[last]
        .text
        .chars()
        .count();
    s.select(
        Cursor {
            section: 0,
            paragraph: last,
            offset: end,
            cell: None,
        },
        false,
    )?;
    s.run("text.insert", json!({"text": "x"}))?;
    s.run("text.backspace", json!({}))?;
    let live = pages(&s)?;
    assert_eq!(live.len(), opened.len());
    for (page, (now, before)) in live.iter().zip(&opened).enumerate() {
        assert!(
            now == before,
            "page {} moved after an edit at the end",
            page + 1
        );
    }
    let dir = tempfile::tempdir()?;
    let saved = dir.path().join("hongbo.hwp");
    s.save(&saved)?;
    let mut reopened = Session::new()?;
    reopened.open(&saved)?;
    assert_eq!(pages(&reopened)?, live);
    Ok(())
}

/// Hangul tables treated as characters, laid out by the stored rules when opened.
const TABLES: &str = "../../../rhwp/samples/issue1835_tac_stale_height.hwp";

/// Text typed above tables: RHWP applied its edited-document rules from the edited
/// paragraph to the end of the section, while the saved HWP reopened with the stored
/// rules for the tables after it, so they moved on reopening. The rules now apply to
/// the edited paragraphs only, as reading the file back does.
#[test]
fn an_edited_hwp_reopens_like_the_live_pages() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(TABLES);
    if !path.exists() {
        return Ok(());
    }
    let mut s = Session::new()?;
    s.open(&path)?;
    let paragraph = s.core.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.is_empty() && p.text.chars().count() >= 4)
        .expect("a text paragraph");
    let middle = s.core.document().sections[0].paragraphs[paragraph]
        .text
        .chars()
        .count()
        / 2;
    s.select(
        Cursor {
            section: 0,
            paragraph,
            offset: middle,
            cell: None,
        },
        false,
    )?;
    let long = "편집으로 늘어난 문장입니다. 이 문장은 여러 줄에 걸치도록 충분히 길게 씁니다. ";
    s.run("text.insert", json!({"text": long.repeat(3)}))?;
    s.run("paragraph.split", json!({}))?;
    s.run("text.insert", json!({"text": long.repeat(2)}))?;
    let live = pages(&s)?;
    let dir = tempfile::tempdir()?;
    let saved = dir.path().join("tables.hwp");
    s.save(&saved)?;
    let mut reopened = Session::new()?;
    reopened.open(&saved)?;
    let back = pages(&reopened)?;
    assert_eq!(back.len(), live.len());
    for (page, (a, b)) in back.iter().zip(&live).enumerate() {
        assert!(a == b, "page {} differs after reopening", page + 1);
    }
    Ok(())
}

/// A HwpCraft formatting command on an earlier paragraph after typing at the end:
/// RHWP lays the formatted paragraph out with the edited-document rules too (it
/// marked only paragraphs its text edits laid out, so this one kept the stored
/// rules meant for untouched paragraphs).
#[test]
fn a_paragraph_formatted_after_typing_is_an_edited_paragraph() -> anyhow::Result<()> {
    let Some(path) = sample() else {
        return Ok(());
    };
    let mut s = Session::new()?;
    s.open(&path)?;
    let last = s.core.document().sections[0].paragraphs.len() - 1;
    let at = |paragraph| Cursor {
        section: 0,
        paragraph,
        offset: 0,
        cell: None,
    };
    s.select(at(last), false)?;
    s.run("text.insert", json!({"text": "x"}))?;
    s.select(at(9), false)?;
    s.run("format.paragraph", json!({"alignment": "right"}))?;
    let edited: Vec<usize> = s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.edited_in_session.then_some(i))
        .collect();
    assert_eq!(edited, [9, last]);
    Ok(())
}
