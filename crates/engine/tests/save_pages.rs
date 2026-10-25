//! Exact editable/text page exports must not silently broaden a partial cut.
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};

fn texts(core: &hwpcraft_doc::DocumentCore) -> Vec<String> {
    core.document()
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .map(|p| p.text.clone())
        .collect()
}

fn state(s: &Session) -> anyhow::Result<Value> {
    Ok(
        json!({"document":format!("{:?}",s.core.document()),"info":s.info(),"cursor":s.cursor,
        "path":s.path,"generation":s.generation,"dirty":s.dirty(),
        "undo":s.can_undo(),"redo":s.can_redo(),"character":s.character_properties()?}),
    )
}

fn long_paragraph() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라마".repeat(1800)))?;
    assert!(s.page_count()? >= 3);
    Ok(s)
}

#[test]
fn partial_paragraph_and_partial_table_cuts_refuse_without_changing_source_or_target()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut table = Session::new()?;
    table.run(
        "table.create",
        json!({"rows":120,"columns":1,"width_mm":100.0,"treat_as_char":true}),
    )?;
    table.execute(Command::Insert("첫 셀의 자체 문구".into()))?;
    assert!(table.page_count()? >= 2);
    for (index, mut s) in [long_paragraph()?, table].into_iter().enumerate() {
        let before = state(&s)?;
        for ext in ["hwp", "hwpx", "txt"] {
            let path = dir.path().join(format!("existing-{index}.{ext}"));
            std::fs::write(&path, b"existing output must survive")?;
            let error = s
                .run("file.save_pages", json!({"path":path,"from":1,"to":1}))
                .unwrap_err();
            assert!(error.to_string().contains("문단 또는 표"), "{error:#}");
            assert_eq!(std::fs::read(path)?, b"existing output must survive");
            assert_eq!(state(&s)?, before);
        }
    }
    Ok(())
}

#[test]
fn all_pages_of_a_multisheet_paragraph_are_allowed_inside_a_partial_document_range()
-> anyhow::Result<()> {
    let mut s = long_paragraph()?;
    let kept = s.core.document().sections[0].paragraphs[0].text.clone();
    let to = s.page_count()?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("범위 밖의 마지막 문단".into()))?;
    assert!(s.page_count()? > to);
    let before = state(&s)?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx", "txt"] {
        let path = dir.path().join(format!("whole-paragraph.{ext}"));
        let report = s.run("file.save_pages", json!({"path":path,"from":1,"to":to}))?;
        assert_eq!(report["paragraphsKept"], 1, "{ext} {report}");
        if ext == "txt" {
            assert_eq!(std::fs::read_to_string(&path)?.trim(), kept);
        } else {
            let reopened = hwpcraft_formats::open(&path)?;
            assert_eq!(texts(&reopened).as_slice(), std::slice::from_ref(&kept));
        }
        assert_eq!(state(&s)?, before);
    }
    Ok(())
}

#[test]
fn paragraph_boundary_range_still_exports_exactly_the_selected_page() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for index in 1..=3 {
        if index > 1 {
            s.execute(Command::PageBreak)?;
        }
        s.execute(Command::Insert(format!("{index}쪽 자체 문구")))?;
    }
    let before = state(&s)?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx", "txt"] {
        let path = dir.path().join(format!("second.{ext}"));
        let result = s.run("file.save_pages", json!({"path":path,"from":2,"to":2}))?;
        assert_eq!(result["pagesAfter"], 1);
        if ext == "txt" {
            assert_eq!(std::fs::read_to_string(path)?.trim(), "2쪽 자체 문구");
        } else {
            let reopened = hwpcraft_formats::open(&path)?;
            assert_eq!(texts(&reopened), ["2쪽 자체 문구"]);
        }
        assert_eq!(state(&s)?, before);
    }
    Ok(())
}

#[test]
fn a_whole_multipage_table_is_allowed_and_keeps_nested_content() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("표를 담을 문단".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("제외할 끝 문단".into()))?;
    s.select(Cursor::default(), false)?;
    s.run(
        "table.create",
        json!({"rows":120,"columns":1,"width_mm":100.0,"treat_as_char":true}),
    )?;
    s.execute(Command::Insert("선택 표의 첫 셀 😀".into()))?;
    let covering = s
        .core
        .pages_covering_paragraphs(&[(0, 0)])
        .expect("table coverage");
    let to = covering.last().copied().unwrap() + 1;
    assert!(to > 1 && to < s.page_count()?);
    let before = state(&s)?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("whole-table.{ext}"));
        s.run("file.save_pages", json!({"path":path,"from":1,"to":to}))?;
        let reopened = hwpcraft_formats::open(&path)?;
        let tables: Vec<_> = reopened
            .document()
            .sections
            .iter()
            .flat_map(|s| &s.paragraphs)
            .flat_map(|p| &p.controls)
            .filter_map(|c| {
                if let hwpcraft_doc::model::control::Control::Table(t) = c {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].cells.len(), 120);
        assert_eq!(tables[0].cells[0].paragraphs[0].text, "선택 표의 첫 셀 😀");
        assert!(!texts(&reopened).iter().any(|t| t.contains("제외할")));
        assert_eq!(state(&s)?, before);
    }
    Ok(())
}

#[test]
fn identical_body_text_does_not_hide_an_excluded_empty_object() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("같은 본문".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("같은 본문".into()))?;
    for paragraph in [0, 1] {
        s.select(
            Cursor {
                paragraph,
                ..Cursor::default()
            },
            false,
        )?;
        s.run(
            "table.create",
            json!({"rows":1,"columns":1,"width_mm":70.0,"treat_as_char":true}),
        )?;
        if paragraph == 0 {
            s.execute(Command::Insert("선택 셀 😀".into()))?;
        }
    }
    assert_eq!(s.page_count()?, 2);
    let before = state(&s)?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("one-object.hwp");
    s.run("file.save_pages", json!({"path":path,"from":1,"to":1}))?;
    let reopened = hwpcraft_formats::open(&path)?;
    let tables: Vec<_> = reopened
        .document()
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .flat_map(|p| &p.controls)
        .filter_map(|c| {
            if let hwpcraft_doc::model::control::Control::Table(t) = c {
                Some(t)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(tables.len(), 1, "the excluded empty table must not survive");
    assert_eq!(tables[0].cells[0].paragraphs[0].text, "선택 셀 😀");
    assert_eq!(state(&s)?, before);
    Ok(())
}

#[test]
fn excluded_section_last_paragraph_cannot_leak_into_the_output() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("선택할 구역".into()))?;
    let mut document = s.core.document().clone();
    document.sections.push(document.sections[0].clone());
    s.core.set_document(document);
    s.core
        .delete_range_native(1, 0, 0, 0, "선택할 구역".chars().count(), None)
        .map_err(anyhow::Error::msg)?;
    s.core
        .insert_text_native(1, 0, 0, "제외 구역의 자체 기밀 문구")
        .map_err(anyhow::Error::msg)?;
    assert_eq!(s.page_count()?, 2);
    // Demonstrate the native reproducer API's documented behavior: the section
    // outside the requested page still keeps its final paragraph.
    let mut native = hwpcraft_doc::DocumentCore::new_empty();
    native.set_document(s.core.document().clone());
    native
        .extract_page_range(1, 1)
        .map_err(anyhow::Error::msg)?;
    assert!(texts(&native).iter().any(|t| t.contains("제외 구역")));
    let before = state(&s)?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx", "txt"] {
        let path = dir.path().join(format!("preserved.{ext}"));
        std::fs::write(&path, b"must not be replaced by excluded text")?;
        let error = s
            .run("file.save_pages", json!({"path":path,"from":1,"to":1}))
            .unwrap_err();
        assert!(error.to_string().contains("선택 밖"), "{error:#}");
        assert_eq!(
            std::fs::read(path)?,
            b"must not be replaced by excluded text"
        );
        assert_eq!(state(&s)?, before);
    }
    Ok(())
}

fn changed_display_boundaries() -> anyhow::Result<Session> {
    for length in 35..=44 {
        let mut s = Session::new()?;
        s.execute(Command::Insert(format!("\n{}", "가".repeat(length))))?;
        s.run("insert.auto_text", json!({"kind":"page_number"}))?;
        let mut document = s.core.document().clone();
        let first = document.sections[0].paragraphs[0].clone();
        let paragraph = document.sections[0].paragraphs[1].clone();
        document.sections[0].paragraphs = std::iter::once(first)
            .chain(std::iter::repeat_n(paragraph, 90))
            .collect();
        s.core.set_document(document);
        s.run("page.new_number", json!({"start":10000}))?;
        if s.page_count()? > s.core.page_count() {
            return Ok(s);
        }
    }
    anyhow::bail!("self-created field fixture did not change a displayed page boundary")
}

#[test]
fn all_displayed_pages_preserve_complete_source_and_pending_style_for_every_format()
-> anyhow::Result<()> {
    let mut s = changed_display_boundaries()?;
    let dir = tempfile::tempdir()?;
    let original = dir.path().join("original.hwp");
    s.save(&original)?;
    let original_bytes = std::fs::read(&original)?;
    s.execute(Command::Character(json!({"bold":true,"fontSize":1700})))?;
    let expected_text = texts(&s.core);
    let expected_ga = expected_text
        .iter()
        .flat_map(|t| t.chars())
        .filter(|c| *c == '가')
        .count();
    let before = state(&s)?;
    let pages = s.page_count()?;
    assert!(pages > s.core.page_count());
    for ext in ["hwp", "hwpx", "txt"] {
        let path = dir.path().join(format!("all.{ext}"));
        let report = s.run("file.save_pages", json!({"path":path,"from":1,"to":pages}))?;
        assert_eq!(report["paragraphsRemoved"], 0);
        if ext == "txt" {
            assert_eq!(
                std::fs::read_to_string(&path)?
                    .chars()
                    .filter(|c| *c == '가')
                    .count(),
                expected_ga
            );
        } else {
            let reopened = hwpcraft_formats::open(&path)?;
            assert_eq!(texts(&reopened), expected_text);
        }
        assert_eq!(state(&s)?, before, "{ext}");
        assert_eq!(std::fs::read(&original)?, original_bytes);
    }
    s.execute(Command::Insert("입력 모양 유지".into()))?;
    let style = s.character_properties()?;
    assert_eq!(style["bold"], true);
    assert_eq!(style["fontSize"], 1700);
    Ok(())
}
