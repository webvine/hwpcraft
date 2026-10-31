//! A document written in HwpCraft from a blank page carries no layout cache that
//! Hangul made: an HWPX save must not hand out RHWP's working rows as if it did,
//! or Hangul and RHWP trust stale positions on reopen and paginate differently.
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn report(s: &mut Session, png: &std::path::Path) -> anyhow::Result<()> {
    s.execute(Command::Insert("2026년 사업 보고서\n작성: 기획팀\n".into()))?;
    s.run("insert.page_break", json!({}))?;
    for c in 1..=2 {
        s.execute(Command::Insert(format!("제{c}장 추진 현황")))?;
        s.run("style.apply", json!({"id": 2}))?;
        s.execute(Command::Insert("\n".into()))?;
        s.run("style.apply", json!({"id": 0}))?;
        for k in 1..=4 {
            s.execute(Command::Insert(format!(
                "{c}-{k} 이번 분기에는 계획한 과제를 차질 없이 진행했으며 세부 일정과 예산 집행률을 함께 점검했습니다.\n"
            )))?;
        }
        s.run("insert.table", json!({"rows": 2, "columns": 2}))?;
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.execute(Command::Insert("\n".into()))?;
        s.run(
            "picture.insert",
            json!({"path": png, "width_mm": 60.0, "treat_as_char": true}),
        )?;
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.execute(Command::Insert("\n".into()))?;
        if c == 1 {
            s.run(
                "note.insert",
                json!({"endnote": false, "paragraphs": ["결산 기준입니다."]}),
            )?;
            s.run("chart.create", json!({"chart": {"kind": "column", "title": "매출",
                "categories": ["1분기", "2분기"], "series": [{"name": "매출", "values": [12, 19]}]}}))?;
            s.run("caret.move", json!({"motion": "document_end"}))?;
            s.execute(Command::Insert("\n".into()))?;
        }
    }
    Ok(())
}

#[test]
fn authored_document_reopens_with_the_same_pages_in_both_formats() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let png = dir.path().join("f.png");
    image::RgbImage::from_pixel(48, 30, image::Rgb([68, 114, 196])).save(&png)?;
    let mut s = Session::new()?;
    report(&mut s, &png)?;
    let live: Vec<String> = (0..s.page_count()?)
        .map(|p| s.render_page_svg(p))
        .collect::<anyhow::Result<_>>()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("r.{ext}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(
            reopened.page_count()?,
            live.len() as u32,
            "{ext}: page count"
        );
    }
    // No RHWP working rows in the HWPX package.
    let bytes = std::fs::read(dir.path().join("r.hwpx"))?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("Contents/section0.xml")?, &mut xml)?;
    assert!(
        !xml.contains("<hp:linesegarray>"),
        "authored HWPX carries layout caches"
    );
    // The live document keeps its rows: saving does not change what is drawn.
    let after: Vec<String> = (0..s.page_count()?)
        .map(|p| s.render_page_svg(p))
        .collect::<anyhow::Result<_>>()?;
    assert_eq!(after, live);
    Ok(())
}

fn section_xml(path: &std::path::Path) -> anyhow::Result<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("Contents/section0.xml")?, &mut xml)?;
    Ok(xml)
}

#[test]
fn a_failed_save_keeps_the_document_and_the_target() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 문단\n둘째 문단\n".into()))?;
    let before: Vec<String> = (0..s.page_count()?)
        .map(|p| s.render_page_svg(p))
        .collect::<anyhow::Result<_>>()?;
    // A folder where the file should go: the save fails and the folder stays.
    let blocked = dir.path().join("blocked.hwpx");
    std::fs::create_dir(&blocked)?;
    std::fs::write(blocked.join("keep.txt"), b"keep")?;
    assert!(s.save(&blocked).is_err());
    assert_eq!(std::fs::read(blocked.join("keep.txt"))?, b"keep");
    let after: Vec<String> = (0..s.page_count()?)
        .map(|p| s.render_page_svg(p))
        .collect::<anyhow::Result<_>>()?;
    assert_eq!(after, before, "the rows' tags are back after a failed save");
    // The next save still works and still hands out no caches.
    let ok = dir.path().join("ok.hwpx");
    s.save(&ok)?;
    assert!(!section_xml(&ok)?.contains("<hp:linesegarray>"));
    Ok(())
}

#[test]
fn reopened_authored_hwpx_stays_cache_free_and_hangul_files_keep_theirs() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("HwpCraft에서 쓴 문서\n".into()))?;
    let first = dir.path().join("first.hwpx");
    s.save(&first)?;
    let mut reopened = Session::new()?;
    reopened.open(&first)?;
    reopened.execute(Command::Insert("다시 열어 고친 문단\n".into()))?;
    let second = dir.path().join("second.hwpx");
    reopened.save(&second)?;
    assert!(!section_xml(&second)?.contains("<hp:linesegarray>"));
    // A document Hangul laid out keeps its stored rows on an HWPX save.
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/hwpx/aift.hwpx");
    if sample.exists() {
        let mut hangul = Session::new()?;
        hangul.open(&sample)?;
        let out = dir.path().join("aift.hwpx");
        hangul.save(&out)?;
        assert!(section_xml(&out)?.contains("<hp:linesegarray>"));
    }
    Ok(())
}

/// A row: start, vpos, line height, text height, spacing and tag.
type Row = (u32, i32, i32, i32, i32, u32);

/// The rows of every body paragraph.
fn rows(s: &Session) -> Vec<Vec<Row>> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| {
            p.line_segs
                .iter()
                .map(|l| {
                    (
                        l.text_start,
                        l.vertical_pos,
                        l.line_height,
                        l.text_height,
                        l.line_spacing,
                        l.tag,
                    )
                })
                .collect()
        })
        .collect()
}

/// A style set changes sizes after the rows were laid out; an HWP save writes the
/// rows its HWPX would get. Those rows are swapped in only for the write: after a
/// failed save the live rows are back unchanged, and a later save reopens with the
/// live pages.
#[test]
fn an_hwp_save_writes_fresh_rows_and_a_failed_one_leaves_the_live_rows() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "제목 문단\n본문 문단입니다. 계획한 일을 차례로 진행했습니다.\n".repeat(30),
    ))?;
    s.run("format.style.set.apply", json!({"name": "보고서"}))?;
    let before = rows(&s);
    let pages = s.page_count()?;
    let blocked = dir.path().join("blocked.hwp");
    std::fs::create_dir(&blocked)?;
    assert!(s.save(&blocked).is_err());
    assert!(blocked.is_dir());
    assert_eq!(
        rows(&s),
        before,
        "the live rows are back after a failed save"
    );
    let path = dir.path().join("fresh.hwp");
    s.save(&path)?;
    assert_eq!(rows(&s), before, "and after a good one");
    let mut r = Session::new()?;
    r.open(&path)?;
    assert_eq!(r.page_count()?, pages);
    let caret = |s: &Session| -> anyhow::Result<Vec<serde_json::Value>> {
        (0..s.core.document().sections[0].paragraphs.len())
            .step_by(7)
            .map(|p| {
                s.caret_at(hwpcraft_engine::Cursor {
                    section: 0,
                    paragraph: p,
                    offset: 0,
                    cell: None,
                })
            })
            .collect()
    };
    assert_eq!(
        caret(&r)?,
        caret(&s)?,
        "the reopened HWP lays out as the live one"
    );
    Ok(())
}
