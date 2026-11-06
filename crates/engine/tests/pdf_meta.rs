//! PDF document information and bookmarks: every export path (save, partial pages, the
//! background job, printing) names the document and HwpCraft, and turns the outline
//! headings (개요) into bookmarks that go to their page; a print layout that reorders
//! pages keeps the information but leaves the bookmarks out.
use hwpcraft_engine::{Cursor, PrintLayout, Session, fixed_clock};
use serde_json::json;

/// PDF text string as `pdf_meta` writes it: UTF-16BE hex with a byte order mark.
fn text_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

/// Page object ids in page order (the page tree's `/Kids`).
fn kids(text: &str) -> Vec<u32> {
    let at = text.find("/Kids [").expect("page tree") + 7;
    text[at..at + text[at..].find(']').unwrap()]
        .split(" 0 R")
        .filter_map(|r| r.split_whitespace().last()?.parse().ok())
        .collect()
}

/// (title, page index) of every bookmark, in the order written.
fn bookmarks(pdf: &[u8]) -> Vec<(String, usize)> {
    let text = String::from_utf8_lossy(pdf);
    let pages = kids(&text);
    let mut out = Vec::new();
    for item in text.split(" 0 obj").filter(|o| o.contains("/Dest [")) {
        let title = item
            .split("/Title ")
            .nth(1)
            .unwrap()
            .split('\n')
            .next()
            .unwrap();
        let page: u32 = item
            .split("/Dest [")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        out.push((
            title.to_owned(),
            pages.iter().position(|&p| p == page).unwrap(),
        ));
    }
    out
}

/// 서론 (개요 1) on page 1, 본론 (개요 1) with 세부 항목 (개요 2) on page 2, saved as
/// 업무 보고.hwpx so the file name is the title. Bookmarks carry the outline numbers.
fn report(dir: &std::path::Path) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run(
        "text.insert",
        json!({"text":"머리 문단\n서론\n본문 하나\n본론\n세부 항목\n본문 둘"}),
    )?;
    for (paragraph, level) in [(1, 1), (3, 1), (4, 2)] {
        s.cursor = Cursor {
            paragraph,
            ..Default::default()
        };
        s.run("style.apply", json!({"id": 1 + level}))?;
    }
    s.cursor = Cursor {
        paragraph: 2,
        offset: "본문 하나".chars().count(),
        ..Default::default()
    };
    s.run("insert.page_break", json!({}))?;
    s.save(&dir.join("업무 보고.hwpx"))?;
    Ok(s)
}

#[test]
fn every_pdf_path_names_the_document_and_bookmarks_its_headings() -> anyhow::Result<()> {
    fixed_clock(Some(1_793_000_000));
    let dir = tempfile::tempdir()?;
    let mut s = report(dir.path())?;
    assert_eq!(s.page_count()?, 2);
    let expected = vec![
        (text_string("1. 서론"), 0),
        (text_string("2. 본론"), 1),
        (text_string("가. 세부 항목"), 1),
    ];
    let path = dir.path().join("업무 보고.pdf");
    s.save(&path)?;
    let saved = std::fs::read(&path)?;
    let job = s.pdf_job(&[0, 1])?.convert()?;
    let (_, print) = s.print_pdf_job("all", 0)?;
    let printed = print.convert()?;
    for (name, pdf) in [("save", &saved), ("job", &job), ("print", &printed)] {
        let text = String::from_utf8_lossy(pdf);
        assert!(
            text.contains(&format!("/Title {}", text_string("업무 보고"))),
            "{name}"
        );
        assert!(
            text.contains(&format!(
                "/Creator {}",
                text_string(&format!("HwpCraft {}", hwpcraft_doc::VERSION.trim()))
            )),
            "{name}"
        );
        assert!(text.contains("/CreationDate (D:20261026073320Z)"), "{name}");
        assert!(text.contains("/PageMode /UseOutlines"), "{name}");
        // Two top-level bookmarks; 본론 opens to show 세부 항목.
        assert!(text.contains("/Type /Outlines\n/Count 3"), "{name}");
        assert_eq!(bookmarks(pdf), expected, "{name}");
    }

    // A PDF of page 2 alone bookmarks only what is on it, at page index 0.
    let second = s.render_pdf(&[1])?;
    assert_eq!(
        bookmarks(&second),
        [
            (text_string("2. 본론"), 0),
            (text_string("가. 세부 항목"), 0)
        ]
    );

    // Two pages on a sheet: the information stays, the bookmarks go.
    let prep = {
        let mut prep = s.pdf_prepare(&[0, 1])?;
        s.pdf_prepare_step(&mut prep, std::time::Duration::MAX)?;
        prep
    };
    let layout = PrintLayout {
        n_up: 2,
        ..PrintLayout::default()
    };
    let sheet = s.pdf_finish_with(prep, &[0, 1], &layout)?.convert()?;
    let text = String::from_utf8_lossy(&sheet);
    assert!(text.contains(&format!("/Title {}", text_string("업무 보고"))));
    assert!(!text.contains("/Outlines") && bookmarks(&sheet).is_empty());
    Ok(())
}

#[test]
fn a_document_without_headings_gets_information_but_no_outline() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text":"본문만 있는 문서"}))?;
    let pdf = s.render_pdf(&[0])?;
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Producer ") && text.contains("/CreationDate (D:"));
    assert!(!text.contains("/Outlines") && !text.contains("/PageMode"));
    // A new document has no file name and no title.
    assert!(!text.contains("/Title <FEFF"));
    Ok(())
}
