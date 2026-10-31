//! A document opened from a Hangul HWPX keeps its HWPX source through every save
//! path (page and auto numbers serialize from a model-only core): its HWPX copy is
//! not marked as converted from HWP5, so tables treated as characters read back the
//! same and the reopened pages match the live ones.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, Session};

const SAMPLE: &str = "../../../rhwp/samples/2025년 기부·답례품 실적 지자체 보고서_양식.hwpx";
/// A Hangul HWPX with footnotes, their auto numbers and a page number.
const NUMBERED: &str = "../../../rhwp/samples/issue6044/156513948.hwpx";

fn sample_at(relative: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    path.exists().then_some(path)
}

fn sample() -> Option<std::path::PathBuf> {
    sample_at(SAMPLE)
}

/// Each body table's (paragraph, attr, treat as character, flow with text).
fn tables(s: &Session) -> Vec<(usize, u32, bool, bool)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            p.controls.iter().filter_map(move |c| match c {
                Control::Table(t) => {
                    Some((i, t.attr, t.common.treat_as_char, t.common.flow_with_text))
                }
                _ => None,
            })
        })
        .collect()
}

fn entries(path: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    Ok(zip.file_names().map(str::to_owned).collect())
}

#[test]
fn a_hangul_hwpx_saved_again_is_not_marked_as_converted_from_hwp5() -> anyhow::Result<()> {
    let Some(path) = sample() else {
        return Ok(());
    };
    assert!(
        !entries(&path)?
            .iter()
            .any(|e| e.contains("rhwp-hwp5-origin"))
    );
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.open(&path)?;
    let opened = tables(&s);
    let copy = dir.path().join("copy.hwpx");
    s.save(&copy)?;
    assert!(
        !entries(&copy)?
            .iter()
            .any(|e| e.contains("rhwp-hwp5-origin")),
        "{:?}",
        entries(&copy)?
    );
    let mut r = Session::new()?;
    r.open(&copy)?;
    assert_eq!(tables(&r), opened, "tables read back the same");
    assert_eq!(r.page_count()?, s.page_count()?);
    // With a table inserted (a new table paragraph among Hangul's rows), the
    // reopened copy still paginates like the live document.
    s.select(
        hwpcraft_engine::Cursor {
            section: 0,
            paragraph: 6,
            offset: 0,
            cell: None,
        },
        false,
    )?;
    s.execute(Command::Split)?;
    s.run("table.create", serde_json::json!({"rows": 3, "columns": 3}))?;
    let pages = |s: &Session| -> anyhow::Result<Vec<u32>> {
        (0..s.core.document().sections[0].paragraphs.len())
            .map(|p| {
                s.page_of_object(hwpcraft_engine::ObjectAddress {
                    section: 0,
                    paragraph: p,
                    control: 0,
                })
            })
            .collect()
    };
    let live = pages(&s)?;
    let edited = dir.path().join("edited.hwpx");
    s.save(&edited)?;
    let mut back = Session::new()?;
    back.open(&edited)?;
    assert_eq!(pages(&back)?, live);
    Ok(())
}

/// Every paragraph's text (table cells and notes included) and the control count.
fn content(s: &Session) -> (String, usize) {
    fn walk(paragraphs: &[hwpcraft_doc::model::paragraph::Paragraph], text: &mut String) -> usize {
        let mut controls = 0;
        for p in paragraphs {
            text.push_str(&p.text);
            text.push('\n');
            controls += p.controls.len();
            for c in &p.controls {
                controls += match c {
                    Control::Table(t) => t.cells.iter().map(|c| walk(&c.paragraphs, text)).sum(),
                    Control::Footnote(n) => walk(&n.paragraphs, text),
                    Control::Endnote(n) => walk(&n.paragraphs, text),
                    _ => 0,
                };
            }
        }
        controls
    }
    let mut text = String::new();
    let controls = s
        .core
        .document()
        .sections
        .iter()
        .map(|sec| walk(&sec.paragraphs, &mut text))
        .sum();
    (text, controls)
}

#[test]
fn a_numbered_hangul_hwpx_saves_as_hwp_and_hwpx_with_the_same_content() -> anyhow::Result<()> {
    let Some(path) = sample_at(NUMBERED) else {
        return Ok(());
    };
    let mut s = Session::new()?;
    s.open(&path)?;
    let live = content(&s);
    let pages = s.page_count()?;
    let dir = tempfile::tempdir()?;
    // The HWP save serializes the adapted snapshot once (its auto numbers through
    // `prepare_hwp`); the HWPX save keeps the HWPX source unmarked.
    for ext in ["hwp", "hwpx"] {
        let copy = dir.path().join(format!("copy.{ext}"));
        s.save(&copy)?;
        let mut r = Session::new()?;
        r.open(&copy)?;
        assert_eq!(content(&r), live, "{ext}");
        assert_eq!(r.page_count()?, pages, "{ext}");
        if ext == "hwpx" {
            assert!(
                !entries(&copy)?
                    .iter()
                    .any(|e| e.contains("rhwp-hwp5-origin"))
            );
        }
    }
    Ok(())
}
