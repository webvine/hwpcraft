//! PDF links inside the document: 차례 entries and cross references become GoTo
//! links to their target's page in every PDF path (save, partial pages, print layout).
use hwpcraft_engine::{Cell, Cursor, Session};
use serde_json::json;
use std::collections::HashMap;

/// (page object ids in page order, GoTo links as (page index holding the link,
/// target page index, left, top in pt)), one per distinct destination in page
/// order (a 차례 line is several link areas: text, leader and page number).
type Gotos = (Vec<u32>, Vec<(usize, usize, f32, f32)>);

fn gotos(pdf: &[u8]) -> Gotos {
    let text = String::from_utf8_lossy(pdf);
    let ids = |s: &str| -> Vec<u32> {
        s.split(" 0 R")
            .filter_map(|r| {
                r.split_whitespace()
                    .last()?
                    .trim_start_matches('[')
                    .parse()
                    .ok()
            })
            .collect()
    };
    let kids_at = text.find("/Kids [").expect("page tree") + 7;
    let kids = ids(&text[kids_at..kids_at + text[kids_at..].find(']').unwrap()]);
    let index: HashMap<u32, usize> = kids.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("/S /GoTo") {
        let at = from + at;
        let start = text[..at].rfind(" obj").unwrap();
        let end = at + text[at..].find("endobj").unwrap();
        let object = &text[start..end];
        let page = |key: &str| -> u32 {
            let k = object.find(key).unwrap() + key.len();
            object[k..]
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        let d = object.find("/D [").unwrap() + 4;
        let parts: Vec<&str> = object[d..].split_whitespace().collect();
        assert_eq!(parts[3], "/XYZ", "{object}");
        out.push((
            index[&page("/P ")],
            index[&parts[0].parse::<u32>().unwrap()],
            parts[4].parse().unwrap(),
            parts[5].parse().unwrap(),
        ));
        from = end;
    }
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2 && a.3 == b.3);
    (kids, out)
}

/// 표지, two chapters on their own pages (the second with a captioned table and a
/// reference to it), then a table of contents with links on the last page.
fn report() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text":"표지"}))?;
    for (c, title) in ["제1장 개요", "제2장 결과"].iter().enumerate() {
        s.run("insert.page_break", json!({}))?;
        s.run("text.insert", json!({ "text": title }))?;
        s.run("style.apply", json!({"id":2}))?;
        s.run("text.insert", json!({"text":"\n"}))?;
        s.run("style.apply", json!({"id":0}))?;
        s.run(
            "text.insert",
            json!({"text": "본문 문장입니다.\n".repeat(3 + c * 4)}),
        )?;
    }
    let table = s.run("insert.table", json!({"rows":2,"columns":2}))?["table"].clone();
    s.run(
        "table.properties",
        json!({"table":table,"props":{"hasCaption":true}}),
    )?;
    s.run("table.caption_text", json!({"table":table,"text":"예산"}))?;
    end(&mut s);
    s.run("text.insert", json!({"text":"\n자세한 내용은 표 "}))?;
    let r = s.run(
        "insert.cross_reference",
        json!({"target":{"table":1},"show":"number"}),
    )?;
    assert_eq!(r["text"], "1", "{r}");
    s.run("text.insert", json!({"text":"을 참조하세요."}))?;
    s.run("insert.page_break", json!({}))?;
    let toc = s.run("tools.toc", json!({"levels":1}))?;
    assert_eq!(toc["entries"], 2, "{toc}");
    Ok(s)
}

fn end(s: &mut Session) {
    let doc = s.core.document();
    let last = doc.sections[0].paragraphs.len() - 1;
    s.cursor = Cursor {
        paragraph: last,
        offset: doc.sections[0].paragraphs[last].text.chars().count(),
        cell: None::<Cell>,
        ..Default::default()
    };
}

fn outline(s: &Session) -> anyhow::Result<Vec<(String, usize)>> {
    let o = serde_json::from_str::<serde_json::Value>(
        &s.core
            .get_outline_navigation_native()
            .map_err(anyhow::Error::msg)?,
    )?;
    Ok(o["outline"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            (
                h["title"].as_str().unwrap_or_default().to_owned(),
                h["page"].as_u64().unwrap_or(0) as usize,
            )
        })
        .collect())
}

/// The baseline (CSS px) of the first SVG text drawing `ch` on `page`.
fn baseline(s: &Session, page: u32, ch: char) -> anyhow::Result<f64> {
    let svg = s.render_page_svg(page)?;
    let at = svg
        .find(&format!(">{ch}<"))
        .ok_or_else(|| anyhow::anyhow!("no {ch} on page {page}"))?;
    let tag = &svg[svg[..at].rfind("<text").unwrap()..at];
    let y = tag
        .split(" y=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    Ok(y.parse()?)
}

#[test]
fn toc_and_cross_reference_links_go_to_their_pages_in_every_pdf_path() -> anyhow::Result<()> {
    let mut s = report()?;
    let heads = outline(&s)?;
    assert_eq!(heads.len(), 2, "{heads:?}");
    let pages = s.page_count()? as usize;
    let dir = tempfile::tempdir()?;

    // The real save path (compacted PDF).
    let path = dir.path().join("report.pdf");
    s.save(&path)?;
    let (kids, links) = gotos(&std::fs::read(&path)?);
    assert_eq!(kids.len(), pages);
    let toc_page = pages - 1;
    let toc_links: Vec<_> = links.iter().filter(|l| l.0 == toc_page).collect();
    assert_eq!(toc_links.len(), 2, "{links:?}");
    for ((title, page), link) in heads.iter().zip(&toc_links) {
        assert_eq!(link.1 + 1, *page, "{title}: {links:?}");
        // The destination's top is at the heading line, just above its text.
        let height_pt = 841.9_f32; // A4
        let top_px = f64::from(height_pt - link.3) / 0.75;
        let base = baseline(&s, link.1 as u32, title.chars().next().unwrap())?;
        assert!(
            top_px <= base && base - top_px < 30.0,
            "{title}: top {top_px} px, baseline {base} px"
        );
    }
    // The cross reference on 제2장's page goes to the table's page.
    let reference: Vec<_> = links.iter().filter(|l| l.0 != toc_page).collect();
    assert_eq!(reference.len(), 1, "{links:?}");
    assert_eq!(reference[0].1 + 1, heads[1].1, "{links:?}");

    // Only some pages: targets are renumbered, missing ones leave no link.
    let part = s.render_pdf(&[(heads[1].1 - 1) as u32, toc_page as u32])?;
    let (kids, links) = gotos(&part);
    assert_eq!(kids.len(), 2);
    assert!(
        links.iter().any(|l| l.0 == 1 && l.1 == 0),
        "TOC → 제2장 is now page 1 of 2: {links:?}"
    );
    assert!(
        links.iter().all(|l| l.1 < 2),
        "제1장 is not in this PDF: {links:?}"
    );
    assert_eq!(links.iter().filter(|l| l.0 == 1).count(), 1, "{links:?}");

    // 인쇄 상세: reversed order through file.print_pdf.
    let printed = dir.path().join("reversed.pdf");
    s.run(
        "file.print_pdf",
        json!({"path": printed, "pages": "all", "reverse": true}),
    )?;
    let (kids, links) = gotos(&std::fs::read(&printed)?);
    assert_eq!(kids.len(), pages);
    let reversed = |i: usize| pages - 1 - i;
    let toc_links: Vec<_> = links.iter().filter(|l| l.0 == reversed(toc_page)).collect();
    assert_eq!(toc_links.len(), 2, "{links:?}");
    for ((_, page), link) in heads.iter().zip(&toc_links) {
        assert_eq!(link.1, reversed(page - 1), "{links:?}");
    }
    // 모아 찍기 keeps web links only.
    let two_up = dir.path().join("two-up.pdf");
    s.run(
        "file.print_pdf",
        json!({"path": two_up, "pages": "all", "nUp": 2}),
    )?;
    assert!(gotos(&std::fs::read(&two_up)?).1.is_empty());
    // Two copies: collated, a target is its page in the first set; uncollated, the
    // first sheet of that page.
    for (collate, sheet_of) in [
        (true, Box::new(|i: usize| i) as Box<dyn Fn(usize) -> usize>),
        (false, Box::new(|i: usize| 2 * i)),
    ] {
        let path = dir.path().join(format!("copies-{collate}.pdf"));
        s.run(
            "file.print_pdf",
            json!({"path": path, "pages": "all", "copies": 2, "collate": collate}),
        )?;
        let (kids, links) = gotos(&std::fs::read(&path)?);
        assert_eq!(kids.len(), 2 * pages);
        let first_toc = sheet_of(toc_page);
        let toc_links: Vec<_> = links.iter().filter(|l| l.0 == first_toc).collect();
        assert_eq!(toc_links.len(), 2, "collate {collate}: {links:?}");
        for ((_, page), link) in heads.iter().zip(&toc_links) {
            assert_eq!(link.1, sheet_of(page - 1), "collate {collate}: {links:?}");
        }
    }
    // A print range without 제1장's page: only the link to 제2장 is left.
    let range = dir.path().join("range.pdf");
    s.run(
        "file.print_pdf",
        json!({"path": range, "pages": format!("{}-{}", heads[1].1, pages)}),
    )?;
    let (kids, links) = gotos(&std::fs::read(&range)?);
    let toc_sheet = kids.len() - 1;
    let toc_links: Vec<_> = links.iter().filter(|l| l.0 == toc_sheet).collect();
    assert_eq!(toc_links.len(), 1, "{links:?}");
    assert_eq!(toc_links[0].1, 0, "{links:?}");
    Ok(())
}

/// Past 64 pages RHWP converts PDF pages in two passes without keeping the first
/// pass's trees; the links must still be there.
#[test]
fn a_long_document_keeps_its_toc_links() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text":"표지"}))?;
    for n in 1..=70 {
        s.run("insert.page_break", json!({}))?;
        s.run("text.insert", json!({ "text": format!("{n}절 제목") }))?;
        s.run("style.apply", json!({"id":2}))?;
    }
    s.run("insert.page_break", json!({}))?;
    let toc = s.run("tools.toc", json!({"levels":1}))?;
    assert_eq!(toc["entries"], 70, "{toc}");
    let pages = s.page_count()? as usize;
    assert!(pages > 70, "{pages}");
    let heads = outline(&s)?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("long.pdf");
    s.save(&path)?;
    let (kids, links) = gotos(&std::fs::read(&path)?);
    assert_eq!(kids.len(), pages);
    let targets: Vec<usize> = links
        .iter()
        .filter(|l| l.0 >= 71)
        .map(|l| l.1 + 1)
        .collect();
    // The outline also lists the 차례 title itself.
    let expected: Vec<usize> = heads
        .iter()
        .filter(|(title, _)| title.ends_with("절 제목"))
        .map(|(_, page)| *page)
        .collect();
    assert_eq!(expected.len(), 70);
    assert_eq!(targets, expected);
    Ok(())
}

#[test]
fn a_pdf_with_web_links_only_has_no_goto() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "insert.hyperlink",
        json!({"uri": "https://example.com/", "text": "누리집"}),
    )?;
    let pdf = s.render_pdf(&[0])?;
    assert!(gotos(&pdf).1.is_empty());
    assert!(String::from_utf8_lossy(&pdf).contains("/URI (https://example.com/)"));
    Ok(())
}
