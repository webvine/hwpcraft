//! What DOCX export keeps of an HWP page layout, checked in the written parts: page
//! numbers that start again and pages that hide them, the middle dot's width, a table
//! set like a character in a hanging paragraph, and a box drawn as one tall cell.
use hwpcraft_engine::{Command, Session};
use serde_json::json;
use std::io::Read;

fn part(path: &std::path::Path, name: &str) -> anyhow::Result<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let mut text = String::new();
    zip.by_name(name)?.read_to_string(&mut text)?;
    Ok(text)
}

/// Each `<w:sectPr>` of the body in order.
fn sections(document: &str) -> Vec<&str> {
    document
        .match_indices("<w:sectPr")
        .map(|(at, _)| &document[at..at + document[at..].find("</w:sectPr>").unwrap()])
        .collect()
}

/// The text of the part a section's `kind` (header/footer) reference of `ty` points at.
fn referenced(
    path: &std::path::Path,
    section: &str,
    kind: &str,
    ty: &str,
) -> anyhow::Result<Option<String>> {
    let tag = format!("<w:{kind}Reference w:type=\"{ty}\" r:id=\"");
    let Some(at) = section.find(&tag) else {
        return Ok(None);
    };
    let id = &section[at + tag.len()..];
    let id = &id[..id.find('"').unwrap()];
    let rels = part(path, "word/_rels/document.xml.rels")?;
    let rel = &rels[rels.find(&format!("Id=\"{id}\"")).unwrap()..];
    let target = rel
        .split("Target=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    Ok(Some(part(path, &format!("word/{target}"))?))
}

fn hide(s: &mut Session, header: bool) -> anyhow::Result<()> {
    s.run(
        "page.hide",
        json!({"header": header, "footer": false, "master_page": false, "border": false,
               "fill": false, "page_number": true}),
    )?;
    Ok(())
}

/// A cover that hides its header and number, a contents page that hides its number,
/// and a body whose numbers start again at 1 after a page break: Word gets a section
/// for the cover and contents (a first-page footer, a default footer without a number)
/// and a section from the body page that starts at 1 and shows the number.
#[test]
fn page_numbers_start_again_and_hidden_ones_stay_hidden() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "page.number",
        json!({"position": "bottom_center", "format": 0, "dash": true}),
    )?;
    s.execute(Command::Insert("표지".into()))?;
    hide(&mut s, true)?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("목차".into()))?;
    hide(&mut s, false)?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("본문 첫 쪽".into()))?;
    s.run("page.new_number", json!({"start": 1}))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("본문 둘째 쪽".into()))?;
    assert_eq!(s.page_count()?, 4);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("numbers.docx");
    s.export_docx(&path, false)?;
    let document = part(&path, "word/document.xml")?;
    let parts = sections(&document);
    assert_eq!(parts.len(), 2, "cover and contents, then the body");
    let (front, body) = (parts[0], parts[1]);
    assert!(front.contains("<w:titlePg/>"), "{front}");
    assert!(!front.contains("w:start="));
    let front_footer = referenced(&path, front, "footer", "default")?.unwrap_or_default();
    assert!(
        !front_footer.contains("PAGE"),
        "contents page hides its number"
    );
    assert!(body.contains("w:start=\"1\""), "{body}");
    assert!(!body.contains("<w:titlePg/>"));
    let body_footer = referenced(&path, body, "footer", "default")?.expect("body footer");
    assert!(body_footer.contains("PAGE"));
    // The body starts its page through the section break, not a second page break.
    let at = document.find("본문 첫 쪽").unwrap();
    let start = document[..at].rfind("<w:p>").unwrap();
    assert!(!document[start..at].contains("pageBreakBefore"));
    Ok(())
}

/// Word draws U+00B7 full width from a Korean face; it is written in Times New Roman,
/// whose dot is as narrow as Hangul's.
#[test]
fn the_middle_dot_keeps_its_width() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("행정·생활".into()))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("dot.docx");
    s.export_docx(&path, false)?;
    let document = part(&path, "word/document.xml")?;
    let at = document.find(">·<").expect("the dot in a run of its own");
    let run = &document[document[..at].rfind("<w:r>").unwrap()..at];
    assert!(run.contains("w:eastAsia=\"Times New Roman\""), "{run}");
    assert!(document.contains(">행정<") && document.contains(">생활<"));
    Ok(())
}

/// A table set like a character in a hanging paragraph (내어쓰기) sits where the first
/// line starts, at the margin, not at the indent of the other lines.
#[test]
fn an_inline_table_in_a_hanging_paragraph_sits_at_its_first_line() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("format.paragraph", json!({"indent": -4000}))?;
    s.run(
        "table.create",
        json!({"rows": 1, "columns": 2, "treat_as_char": true, "width_mm": 100.0}),
    )?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("inline.docx");
    s.export_docx(&path, false)?;
    let document = part(&path, "word/document.xml")?;
    let table =
        &document[document.find("<w:tblPr>").unwrap()..document.find("</w:tblPr>").unwrap()];
    assert!(
        !table.contains("<w:tblInd") || table.contains("<w:tblInd w:w=\"0\""),
        "{table}"
    );
    Ok(())
}

/// A one-row table taller than half a page that fits on it (a frame drawn as one tall
/// cell) keeps its height in Word.
#[test]
fn a_tall_one_row_box_keeps_its_height() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "table.create",
        json!({"rows": 1, "columns": 1, "width_mm": 160.0, "height_mm": 200.0}),
    )?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("box.docx");
    s.export_docx(&path, false)?;
    let document = part(&path, "word/document.xml")?;
    let at = document
        .find("<w:trHeight")
        .expect("the row height is kept");
    let value: f32 = document[at..]
        .split("w:val=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()?;
    // 200 mm is 11,339 twips.
    assert!((value - 11339.0).abs() < 60.0, "{value}");
    Ok(())
}
