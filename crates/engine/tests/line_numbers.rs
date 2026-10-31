//! Owner: shell/page area. 줄 번호 settings survive a save as HWPX or HWP (RHWP patch
//! 0020); RHWP 0037 draws them.
use hwpcraft_engine::{Command, Session};
use std::io::{Read, Write};

const SHAPE: &str =
    r#"<hp:lineNumberShape restartType="1" countBy="5" distance="850" startNumber="3"/>"#;

/// `path` with section 0's line numbering replaced by [`SHAPE`] and shown.
fn with_line_numbers(path: &std::path::Path, out: &std::path::Path) -> anyhow::Result<()> {
    let mut zin = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let mut zout = zip::ZipWriter::new(std::fs::File::create(out)?);
    for i in 0..zin.len() {
        let mut entry = zin.by_index(i)?;
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        if name == "Contents/section0.xml" {
            let text = String::from_utf8(data)?;
            let start = text.find("<hp:lineNumberShape").expect("lineNumberShape");
            let end = start + text[start..].find("/>").expect("element end") + 2;
            let text = format!("{}{SHAPE}{}", &text[..start], &text[end..]).replacen(
                r#"showLineNumber="0""#,
                r#"showLineNumber="1""#,
                1,
            );
            data = text.into_bytes();
        }
        let options =
            zip::write::SimpleFileOptions::default().compression_method(entry.compression());
        zout.start_file(name, options)?;
        zout.write_all(&data)?;
    }
    zout.finish()?;
    Ok(())
}

fn section0(path: &std::path::Path) -> anyhow::Result<String> {
    let mut zin = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let mut text = String::new();
    zin.by_name("Contents/section0.xml")?
        .read_to_string(&mut text)?;
    Ok(text)
}

#[test]
fn hwpx_line_number_settings_are_kept_when_saving() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let plain = temp.path().join("plain.hwpx");
    let mut s = Session::new()?;
    s.execute(Command::Insert("줄 번호가 있는 문서".into()))?;
    s.save(&plain)?;
    let numbered = temp.path().join("numbered.hwpx");
    with_line_numbers(&plain, &numbered)?;
    let mut doc = Session::new()?;
    doc.open(&numbered)?;
    doc.execute(Command::Insert(" 고침".into()))?;
    let saved = temp.path().join("saved.hwpx");
    doc.save(&saved)?;
    let xml = section0(&saved)?;
    assert!(xml.contains(SHAPE), "line numbering kept");
    assert!(xml.contains(r#"showLineNumber="1""#), "shown");
    // HWP5 keeps them in the section definition (flags bit 24 and the record tail):
    // HWPX → HWP → HWPX gives the same settings back.
    let hwp = temp.path().join("saved.hwp");
    doc.save(&hwp)?;
    let mut reopened = Session::new()?;
    reopened.open(&hwp)?;
    let back = temp.path().join("back.hwpx");
    reopened.save(&back)?;
    let xml = section0(&back)?;
    assert!(xml.contains(SHAPE), "line numbering kept through HWP");
    assert!(xml.contains(r#"showLineNumber="1""#), "shown through HWP");
    // A document without line numbers still writes the plain defaults.
    let xml = section0(&plain)?;
    assert!(xml.contains(
        r#"<hp:lineNumberShape restartType="0" countBy="0" distance="0" startNumber="0"/>"#
    ));
    assert!(xml.contains(r#"showLineNumber="0""#));
    Ok(())
}

#[test]
fn page_line_numbers_turns_numbering_on_and_saves_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = hwpcraft_engine::Session::new()?;
    s.execute(hwpcraft_engine::Command::Insert("첫 줄\n둘째 줄".into()))?;
    let r = s.run(
        "page.line_numbers",
        serde_json::json!({"show": true, "count_by": 5, "distance_mm": 5, "restart": 2}),
    )?;
    assert_eq!(r["show"], true);
    assert_eq!(r["countBy"], 5);
    assert_eq!(r["start"], 1, "a missing start number becomes 1");
    assert_eq!(r["restart"], 2);
    assert!(
        (r["distanceMm"].as_f64().unwrap() - 5.0).abs() < 0.01,
        "{r}"
    );
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("numbers.{ext}"));
        s.save(&path)?;
        let mut back = hwpcraft_engine::Session::new()?;
        back.open(&path)?;
        let got = back.run("page.line_numbers.get", serde_json::json!({}))?;
        assert_eq!(got, r, "{ext}");
    }
    // Turned on without a distance, the numbers keep 5 mm from the text; 0 is kept when given.
    let mut fresh = hwpcraft_engine::Session::new()?;
    fresh.execute(hwpcraft_engine::Command::Insert("본문".into()))?;
    let on = fresh.run("page.line_numbers", serde_json::json!({"show": true}))?;
    assert!(
        (on["distanceMm"].as_f64().unwrap() - 5.0).abs() < 0.01,
        "{on}"
    );
    let flush = fresh.run(
        "page.line_numbers",
        serde_json::json!({"show": true, "distance_mm": 0}),
    )?;
    assert_eq!(flush["distanceMm"], 0.0);
    fresh.run("page.line_numbers", serde_json::json!({"show": false}))?;
    let again = fresh.run("page.line_numbers", serde_json::json!({"show": true}))?;
    assert!(
        (again["distanceMm"].as_f64().unwrap() - 5.0).abs() < 0.01,
        "{again}"
    );
    // Turning it off keeps the settings; bad values are refused.
    let off = s.run("page.line_numbers", serde_json::json!({"show": false}))?;
    assert_eq!(
        (off["show"].clone(), off["countBy"].clone()),
        (serde_json::json!(false), serde_json::json!(5))
    );
    for bad in [
        serde_json::json!({"show": true, "start": 0}),
        serde_json::json!({"show": true, "count_by": 0}),
        serde_json::json!({"show": true, "distance_mm": 101}),
        serde_json::json!({"show": true, "restart": 4}),
    ] {
        assert!(s.run("page.line_numbers", bad.clone()).is_err(), "{bad}");
    }
    Ok(())
}

#[test]
fn a_paragraph_can_skip_line_numbering() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "첫 줄
둘째 줄
셋째 줄"
            .into(),
    ))?;
    s.run("page.line_numbers", serde_json::json!({"show": true}))?;
    let numbered = s.render_page_svg(0)?;
    s.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 1,
            offset: 0,
            cell: None,
        },
        false,
    )?;
    let skipped = s.run("page.line_numbers.skip", serde_json::json!({"skip": true}))?;
    assert_eq!(skipped["paragraphs"], 1);
    assert_eq!(
        s.run("page.line_numbers.skip.get", serde_json::json!({}))?["skip"],
        true
    );
    let without = s.render_page_svg(0)?;
    assert_ne!(without, numbered, "the skipped line has no number");
    assert!(!without.contains(">3<"), "two numbered lines");
    // Kept in HWPX, lost in HWP (its HWP place is not public).
    for (ext, kept) in [("hwpx", true), ("hwp", false)] {
        let path = dir.path().join(format!("skip.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        back.select(
            hwpcraft_doc::Cursor {
                section: 0,
                paragraph: 1,
                offset: 0,
                cell: None,
            },
            false,
        )?;
        let got = back.run("page.line_numbers.skip.get", serde_json::json!({}))?;
        assert_eq!(got["skip"], kept, "{ext}");
    }
    s.run("page.line_numbers.skip", serde_json::json!({"skip": false}))?;
    assert_eq!(s.render_page_svg(0)?, numbered, "numbered again");
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("page.line_numbers.skip.get", serde_json::json!({}))?["skip"],
        true
    );
    Ok(())
}
