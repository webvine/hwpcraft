use hwpcraft_engine::{Command, Cursor, FindDirection, FindOptions, Session};
use serde_json::json;

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

#[test]
fn find_options_case_word_and_direction() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("Rust rusty RUST\n한글 rust".into()))?;
    s.select(at(0, 0), false)?;
    let o = FindOptions::default();
    assert!(s.find("rust", o)?);
    assert_eq!((s.cursor, s.anchor), (at(0, 4), Some(0)));
    assert!(s.find("rust", o)?);
    assert_eq!(s.anchor, Some(5));
    let word = FindOptions {
        whole_word: true,
        ..o
    };
    assert!(s.find("rust", word)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (0, Some(11)));
    assert!(s.find("rust", word)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (1, Some(3)));
    // Down wraps to the document start.
    assert!(s.find("rust", word)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (0, Some(0)));
    let exact = FindOptions {
        case_sensitive: true,
        ..o
    };
    assert!(s.find("RUST", exact)?);
    assert_eq!(s.anchor, Some(11));
    let up = FindOptions {
        direction: FindDirection::Up,
        ..o
    };
    assert!(s.find("rust", up)?);
    assert_eq!(s.anchor, Some(5));
    assert!(!s.find("없음", o)?);
    Ok(())
}

#[test]
fn replace_next_and_replace_all_with_options() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("Cat cat category CAT".into()))?;
    s.select(at(0, 0), false)?;
    let o = FindOptions {
        whole_word: true,
        ..Default::default()
    };
    assert!(s.find("cat", o)?);
    assert!(s.replace_next("cat", "개", o)?);
    assert_eq!(s.text(), "개 cat category CAT");
    assert_eq!(s.selected_text(), "cat");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "Cat cat category CAT");

    let count = s.replace_all_with("cat", "dog", o)?;
    assert_eq!(count, 3);
    assert_eq!(s.text(), "dog dog category dog");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "Cat cat category CAT");

    let v = s.run(
        "edit.replace_all",
        json!({"query":"cat","replacement":"x","options":{"case_sensitive":true}}),
    )?;
    assert_eq!(v["replaced"], 2);
    assert_eq!(s.text(), "Cat x xegory CAT");
    let v = s.run(
        "edit.find",
        json!({"query":"CAT","options":{"direction":"all"}}),
    )?;
    assert_eq!(v["found"], true);
    assert_eq!(v["anchor"], 0);
    assert!(
        s.run("edit.find", json!({"query":"a","options":{"bogus":1}}))
            .is_err()
    );
    Ok(())
}

#[test]
fn extended_options_find_and_replace() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("대 한 민국 韓國 제12조\n제1조 비고".into()))?;
    s.select(at(0, 0), false)?;
    // 띄어쓰기 무시 selects the spaced run.
    let spaces = FindOptions {
        ignore_spaces: true,
        ..Default::default()
    };
    assert!(s.find("대한민국", spaces)?);
    assert_eq!((s.anchor, s.cursor.offset), (Some(0), 6));
    // 한글로 한자 찾기 continues to 韓國.
    let hanja = FindOptions {
        hanja_by_hangul: true,
        ..Default::default()
    };
    assert!(s.find("한국", hanja)?);
    assert_eq!((s.anchor, s.cursor.offset), (Some(7), 9));
    // 아무개 문자 `?` is one character, so 제12조 is skipped for 제1조.
    let wild = FindOptions {
        wildcards: true,
        ..Default::default()
    };
    assert!(s.find("제?조", wild)?);
    assert_eq!((s.cursor.paragraph, s.anchor), (1, Some(0)));
    // 조건식 with an invalid pattern is an error, not a crash.
    let regex = FindOptions {
        regex: true,
        ..Default::default()
    };
    assert!(s.find("[", regex).is_err());
    assert!(s.find(r"제\d+조", regex)?);
    // Through the command protocol, unknown options are still rejected.
    let v = s.run(
        "edit.find",
        json!({"query":"비 고","options":{"ignore_spaces":true,"direction":"all"}}),
    )?;
    assert_eq!(v["found"], true);
    assert!(
        s.run("edit.find", json!({"query":"a","options":{"regexp":true}}))
            .is_err()
    );
    // 모두 바꾸기 with a wildcard replaces variable-length matches in one undo step.
    let count = s.replace_all_with("제*조", "조항", wild)?;
    assert_eq!(count, 2);
    assert_eq!(s.entries()[0].1, "대 한 민국 韓國 조항");
    assert_eq!(s.entries()[1].1, "조항 비고");
    s.execute(Command::Undo)?;
    assert_eq!(s.entries()[1].1, "제1조 비고");
    Ok(())
}

#[test]
fn format_conditions_restrict_matches() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("bold plain bold".into()))?;
    s.select(at(0, 0), false)?;
    s.select(at(0, 4), true)?;
    s.execute(Command::Character(json!({"bold": true})))?;
    s.select(at(0, 0), false)?;
    let bold = FindOptions {
        bold: Some(true),
        ..Default::default()
    };
    assert!(s.find("bold", bold)?);
    assert_eq!(s.anchor, Some(0));
    // Only the first "bold" is bold, so the search wraps back to it.
    assert!(s.find("bold", bold)?);
    assert_eq!(s.anchor, Some(0));
    let plain = FindOptions {
        bold: Some(false),
        ..Default::default()
    };
    assert!(s.find("bold", plain)?);
    assert_eq!(s.anchor, Some(11));
    assert_eq!(s.replace_all_with("bold", "B", bold)?, 1);
    assert_eq!(s.text(), "B plain bold");
    Ok(())
}

fn all_text(s: &Session) -> Vec<String> {
    s.cursors()
        .into_iter()
        .filter_map(|c| s.paragraph(c).ok().map(|p| p.text.clone()))
        .collect()
}

#[test]
fn replace_all_in_body_and_cells_is_one_step_with_fresh_layout() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let line = "문서 관리 문서 보관 ".repeat(8);
    s.execute(Command::Insert(vec![line.as_str(); 40].join(
        "
",
    )))?;
    s.execute(Command::Table {
        rows: 2,
        columns: 2,
    })?;
    let cells: Vec<Cursor> = s
        .cursors()
        .into_iter()
        .filter(|c| c.cell.is_some())
        .collect();
    for c in &cells {
        s.select(*c, false)?;
        s.execute(Command::Insert("셀 문서 문서".into()))?;
    }
    let before = all_text(&s);
    let pages = s.page_count()?;
    assert_eq!(
        s.count_matches("문서", FindOptions::default(), usize::MAX)?,
        40 * 16 + 8
    );
    assert_eq!(s.count_matches("문서", FindOptions::default(), 10)?, 10);
    let n = s.replace_all_with("문서", "기록물 문서철", FindOptions::default())?;
    assert_eq!(n, 40 * 16 + 8);
    assert_eq!(
        s.count_matches("기록물 문서철", FindOptions::default(), usize::MAX)?,
        n
    );
    // Pagination was redone once at the end: the longer text needs more pages,
    // and a reopened copy agrees with the live layout.
    let grown = s.page_count()?;
    assert!(grown > pages, "{pages} -> {grown}");
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("replaced.hwpx");
    s.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    assert_eq!(reopened.page_count()?, grown);
    s.execute(Command::Undo)?;
    assert_eq!(all_text(&s), before);
    assert_eq!(s.page_count()?, pages);
    Ok(())
}

#[test]
fn replace_all_skips_matches_that_already_read_as_the_replacement() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("문서 Doc doc".into()))?;
    let generation = s.generation;
    // Case-insensitive 문서 -> 문서 changes nothing and leaves no undo step.
    assert_eq!(
        s.replace_all_with("문서", "문서", FindOptions::default())?,
        1
    );
    assert_eq!(s.generation, generation);
    // Only the match that differs is edited: Doc -> doc, doc stays.
    assert_eq!(s.replace_all_with("doc", "doc", FindOptions::default())?, 2);
    assert_eq!(s.text(), "문서 doc doc");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "문서 Doc doc");
    Ok(())
}

/// Timing probe for the review-tools paths on a long document. Run with
/// `HWPCRAFT_PERF_SAMPLE=<file.hwp> cargo test --release --test find -- --ignored --nocapture`.
#[test]
#[ignore]
fn perf_review_tools_on_sample() -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("HWPCRAFT_PERF_SAMPLE") else {
        return Ok(());
    };
    let time = |label: &str, f: &mut dyn FnMut() -> anyhow::Result<String>| -> anyhow::Result<()> {
        let t = std::time::Instant::now();
        let out = f()?;
        eprintln!("{label}: {:?} {out}", t.elapsed());
        Ok(())
    };
    let mut s = Session::new()?;
    time("open", &mut || {
        s.open(std::path::Path::new(&path))?;
        Ok(format!("paragraphs {}", s.cursors().len()))
    })?;
    let o = FindOptions::default();
    s.select(at(0, 0), false)?;
    time("find missing", &mut || {
        Ok(s.find("없는낱말쿼리", o)?.to_string())
    })?;
    time("find 10x common", &mut || {
        let mut n = 0;
        for _ in 0..10 {
            n += usize::from(s.find("문서", o)?);
        }
        Ok(n.to_string())
    })?;
    let regex = FindOptions { regex: true, ..o };
    time("find regex missing", &mut || {
        Ok(s.find(r"\d{9}없음", regex)?.to_string())
    })?;
    let hanja = FindOptions {
        hanja_by_hangul: true,
        ..o
    };
    time("find hanja_by_hangul missing", &mut || {
        Ok(s.find("없는한자", hanja)?.to_string())
    })?;
    time("replace_all common", &mut || {
        Ok(s.replace_all_with(
            "문서",
            "문서",
            FindOptions {
                case_sensitive: false,
                ..o
            },
        )?
        .to_string())
    })?;
    time("replace_all changing", &mut || {
        Ok(s.replace_all_with("문서", "서류", o)?.to_string())
    })?;
    s.execute(Command::Undo)?;
    time("page count after undo", &mut || {
        Ok(s.page_count()?.to_string())
    })?;
    time("symbol_search 20 keys", &mut || {
        let mut n = 0;
        for q in [
            "ㄱ",
            "화",
            "화살",
            "화살표",
            "a",
            "ar",
            "arr",
            "arrow",
            "u+2605",
            "별",
        ]
        .iter()
        .cycle()
        .take(20)
        {
            n += hwpcraft_engine::symbol_search(q).sets.len();
        }
        Ok(n.to_string())
    })?;
    time("highlight rects (no markpen)", &mut || {
        Ok(s.highlight_rects().len().to_string())
    })?;
    for round in 0..3 {
        time(&format!("steady edit+render round {round}"), &mut || {
            s.execute(Command::Insert("y".into()))?;
            Ok(s.render_page_svg(0)?.len().to_string())
        })?;
    }
    time("render page 0 after edit", &mut || {
        s.execute(Command::Insert("x".into()))?;
        Ok(s.render_page_svg(0)?.len().to_string())
    })?;
    time("render page 0 cached", &mut || {
        Ok(s.render_page_svg(0)?.len().to_string())
    })?;
    if std::env::var_os("HWPCRAFT_PERF_HIGHLIGHT").is_some() {
        time("highlight whole document", &mut || {
            s.select_all()?;
            s.run("format.highlight", json!({"color": "#ffff00"}))?;
            s.clear_selection();
            Ok(String::new())
        })?;
        time("highlight rects (limited)", &mut || {
            Ok(s.highlight_rects().len().to_string())
        })?;
        time("render page 0 after edit, all highlighted", &mut || {
            s.select(at(0, 0), false)?;
            s.execute(Command::Insert("x".into()))?;
            Ok(s.render_page_svg(0)?.len().to_string())
        })?;
        time("render page 1 next frame", &mut || {
            Ok(s.render_page_svg(1)?.len().to_string())
        })?;
    }
    Ok(())
}
