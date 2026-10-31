//! 각주 번호 `쪽마다 새로` and the note separator length presets.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn texts(svg: &str) -> String {
    let mut out = String::new();
    for part in svg.split("<text").skip(1) {
        if let (Some(gt), Some(end)) = (part.find('>'), part.find("</text>"))
            && gt < end
        {
            out.push_str(&part[gt + 1..end]);
        }
    }
    out
}

/// The `N)` right before each occurrence of `after` in `text`.
fn numbers_before(text: &str, after: &str) -> Vec<u32> {
    let chunks: Vec<&str> = text.split(after).collect();
    chunks[..chunks.len() - 1]
        .iter()
        .filter_map(|chunk| {
            let head = chunk.strip_suffix(')')?;
            let digits: String = head
                .chars()
                .rev()
                .take_while(char::is_ascii_digit)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            digits.parse().ok()
        })
        .collect()
}

/// Footnote area numbers and body reference marks of each page.
fn page_numbers(s: &Session) -> anyhow::Result<Vec<(Vec<u32>, Vec<u32>)>> {
    (0..s.page_count()?)
        .map(|p| {
            let t = texts(&s.render_page_svg(p)?);
            Ok((numbers_before(&t, "주석끝"), numbers_before(&t, "본문")))
        })
        .collect()
}

fn notes_over_pages() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    for _ in 0..40 {
        s.execute(Command::Insert("문단입니다 ".repeat(30)))?;
        s.run(
            "note.insert",
            json!({"endnote": false, "paragraphs": ["주석끝"]}),
        )?;
        s.execute(Command::Insert("\n본문 ".into()))?;
    }
    Ok(s)
}

#[test]
fn footnotes_restart_on_every_page_in_the_area_and_the_body() -> anyhow::Result<()> {
    let mut s = notes_over_pages()?;
    assert!(s.page_count()? >= 3);
    let continued = page_numbers(&s)?;
    assert!(continued[1].0[0] > 1, "{continued:?}");
    s.run(
        "note.shape",
        json!({"kind": "footnote", "props": {"numbering": "page"}}),
    )?;
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let pages = page_numbers(s)?;
        for (page, (area, body)) in pages.iter().enumerate() {
            let expected: Vec<u32> = (1..=area.len() as u32).collect();
            assert!(!area.is_empty(), "{label} page {page}: {pages:?}");
            assert_eq!(area, &expected, "{label} page {page} area");
            // A page's last mark may have no 본문 after it on the same page.
            assert!(
                body.len() + 1 >= area.len(),
                "{label} page {page}: {pages:?}"
            );
            assert_eq!(
                &body[..],
                &expected[..body.len()],
                "{label} page {page} body"
            );
        }
        Ok(())
    };
    check(&s, "live")?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("notes.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(
            r.run("note.shape.get", json!({"kind": "footnote"}))?["numbering"],
            "page"
        );
        check(&r, ext)?;
    }
    // Back to continuous numbering: the second page continues from the first.
    s.run(
        "note.shape",
        json!({"kind": "footnote", "props": {"numbering": "continue"}}),
    )?;
    assert_eq!(page_numbers(&s)?, continued);
    Ok(())
}

#[test]
fn separator_lengths_are_the_ones_hwp_can_keep() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.run(
        "note.insert",
        json!({"endnote": false, "paragraphs": ["각주"]}),
    )?;
    s.run(
        "note.insert",
        json!({"endnote": true, "paragraphs": ["미주"]}),
    )?;
    let length = |s: &mut Session, kind: &str| -> anyhow::Result<Value> {
        Ok(s.run("note.shape.get", json!({"kind": kind}))?["separatorLength"].clone())
    };
    let set = |s: &mut Session, kind: &str, n: i64| {
        s.run(
            "note.shape",
            json!({"kind": kind, "props": {"separatorLength": n}}),
        )
    };
    // 16-bit HWP field: a longer line would wrap on save, and -4 (column width) is
    // an endnote preset only.
    let generation = s.generation;
    for (kind, n) in [
        ("footnote", 32_768),
        ("footnote", -4),
        ("footnote", -2),
        ("endnote", 40_000),
        ("endnote", -3),
    ] {
        assert!(set(&mut s, kind, n).is_err(), "{kind} {n}");
    }
    assert_eq!(s.generation, generation);
    set(&mut s, "footnote", 32_767)?;
    set(&mut s, "endnote", -4)?;
    assert_eq!(length(&mut s, "footnote")?, 32_767);
    assert_eq!(length(&mut s, "endnote")?, -4);
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("lines.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(length(&mut r, "footnote")?, 32_767, "{ext}");
        assert_eq!(length(&mut r, "endnote")?, -4, "{ext}");
    }
    // The 5 cm preset stays a preset on endnotes too (not 0, which is no line).
    set(&mut s, "endnote", -1)?;
    assert_eq!(length(&mut s, "endnote")?, -1);
    set(&mut s, "footnote", -1)?;
    assert_eq!(length(&mut s, "footnote")?, -1);
    Ok(())
}

/// The PDF is converted from each page's print SVG, and those SVGs are built where
/// the page is laid out (RHWP's parallel PDF step only converts finished SVG text).
/// Several threads laying out pages at once, in different orders, each get their own
/// page's numbers, and the PDF of every page still builds.
#[test]
fn per_page_numbers_reach_the_print_output_from_concurrent_threads() -> anyhow::Result<()> {
    let mut s = notes_over_pages()?;
    s.run(
        "note.shape",
        json!({"kind": "footnote", "props": {"numbering": "page"}}),
    )?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("notes.hwp");
    s.save(&path)?;
    let pages = s.page_count()?;
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let path = &path;
                scope.spawn(move || -> anyhow::Result<()> {
                    let mut r = Session::new()?;
                    r.open(path)?;
                    let order: Vec<u32> = (0..pages).map(|i| (i + worker) % pages).collect();
                    for &page in order.iter().chain(order.iter().rev()) {
                        let svg = r
                            .core
                            .render_page_svg_layer_with_profile_native(
                                page,
                                hwpcraft_doc::RenderProfile::Print,
                            )
                            .map_err(|e| anyhow::anyhow!("{e}"))?;
                        let text = texts(&svg);
                        let area = numbers_before(&text, "주석끝");
                        // Body marks use the numbers of the page being laid out.
                        let body = numbers_before(&text, "본문");
                        let expected: Vec<u32> = (1..=area.len() as u32).collect();
                        assert!(!area.is_empty(), "worker {worker} page {page}");
                        assert_eq!(area, expected, "worker {worker} page {page}");
                        assert!(body.len() + 1 >= area.len(), "worker {worker} page {page}");
                        assert_eq!(
                            body[..],
                            expected[..body.len()],
                            "worker {worker} page {page}"
                        );
                    }
                    let all: Vec<u32> = (0..pages).collect();
                    assert!(r.render_pdf(&all)?.starts_with(b"%PDF-"));
                    Ok(())
                })
            })
            .collect();
        workers
            .into_iter()
            .try_for_each(|w| w.join().map_err(|_| anyhow::anyhow!("worker panicked"))?)
    })
}
