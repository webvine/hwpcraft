use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

#[test]
fn korean_unicode_history_and_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한글 문서 🦀 café".into()))?;
    let original = s.text().to_owned();
    s.cursor.offset = 2;
    s.execute(Command::Insert(" 편집".into()))?;
    assert_eq!(s.text(), "한글 편집 문서 🦀 café");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), original);
    s.execute(Command::Redo)?;
    assert_eq!(s.text(), "한글 편집 문서 🦀 café");
    s.anchor = Some(0);
    s.cursor.offset = 2;
    s.execute(Command::Character(json!({"bold":true,"fontSize":1800})))?;
    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("문서.{ext}"));
        s.save(&path)?;
        assert!(!s.dirty());
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(reopened.text(), s.text());
        assert!(reopened.core.page_count() > 0);
        let props: serde_json::Value = serde_json::from_str(
            &reopened
                .core
                .get_char_properties_at_native(0, 0, 0)
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        )?;
        assert_eq!(props["bold"], true, "{props}");
    }
    Ok(())
}
#[test]
fn multiline_split_merge_selection_and_find() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 문단\n둘째 문단\n마지막".into()))?;
    assert_eq!(s.core.document().sections[0].paragraphs.len(), 3);
    s.cursor.offset = 0;
    s.execute(Command::Backspace)?;
    assert_eq!(s.text(), "둘째 문단마지막");
    assert!(s.find_next("첫")?);
    assert_eq!(s.selected_text(), "첫");
    s.execute(Command::Insert("새".into()))?;
    assert_eq!(s.text(), "새 문단");
    s.replace_paragraph_text("새로운 한글 문단")?;
    assert_eq!(s.text(), "새로운 한글 문단");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "새 문단");
    Ok(())
}
#[test]
fn table_cell_save_reopen_and_history() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Table {
        rows: 2,
        columns: 2,
    })?;
    let c = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("missing cell"))?
        .0;
    s.select(c, false)?;
    s.execute(Command::Insert("표 안의 한글".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("둘째 줄".into()))?;
    s.cursor.offset = 0;
    s.execute(Command::Backspace)?;
    assert_eq!(s.text(), "표 안의 한글둘째 줄");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "둘째 줄");
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let p = dir.path().join(format!("table.{ext}"));
        s.save(&p)?;
        let mut r = Session::new()?;
        r.open(&p)?;
        assert_eq!(
            r.entries().iter().filter(|(c, _)| c.cell.is_some()).count(),
            5
        );
        assert!(r.entries().iter().any(|(_, t)| t.contains("표 안의 한글")));
    }
    Ok(())
}
#[test]
fn failed_open_and_failed_command_preserve_document() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("원본 보존".into()))?;
    let dir = tempfile::tempdir()?;
    let bad = dir.path().join("bad.hwp");
    std::fs::write(&bad, b"invalid")?;
    assert!(s.open(&bad).is_err());
    assert_eq!(s.text(), "원본 보존");
    assert!(s.dirty());
    assert!(
        s.execute(Command::Table {
            rows: 0,
            columns: 3
        })
        .is_err()
    );
    assert_eq!(s.text(), "원본 보존");
    let good = dir.path().join("good.hwpx");
    s.save(&good)?;
    let before = std::fs::read(&good)?;
    assert!(s.save(&dir.path().join("missing.unsupported")).is_err());
    assert_eq!(std::fs::read(&good)?, before);
    s.execute(Command::Undo)?;
    assert!(s.text().is_empty());
    assert!(s.dirty());
    s.execute(Command::Redo)?;
    assert!(!s.dirty());
    Ok(())
}
#[test]
fn bounded_history_handles_many_edits_and_new_branch() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for _ in 0..110 {
        // Moving the caret ends a typing group, so each insert is its own undo step.
        let c = s.cursor;
        s.select(c, false)?;
        s.execute(Command::Insert("가".into()))?;
    }
    for _ in 0..40 {
        s.execute(Command::Undo)?;
    }
    assert_eq!(s.len(), 70);
    assert!(!s.can_undo());
    for _ in 0..40 {
        s.execute(Command::Redo)?;
    }
    assert_eq!(s.len(), 110);
    s.execute(Command::Undo)?;
    s.execute(Command::Insert("나".into()))?;
    assert!(!s.can_redo());
    s.select(Cursor::default(), false)?;
    assert!(s.text().ends_with('나'));
    Ok(())
}

#[test]
fn replace_all_unicode_body_cells_and_single_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가🦀가🦀 끝\n가🦀".into()))?;
    s.execute(Command::Table {
        rows: 1,
        columns: 2,
    })?;
    let cells: Vec<_> = s
        .entries()
        .into_iter()
        .filter_map(|(c, _)| c.cell.map(|_| c))
        .collect();
    for c in &cells {
        s.select(*c, false)?;
        s.execute(Command::Insert("셀 가🦀 가🦀".into()))?;
    }
    s.select(
        Cursor {
            offset: 4,
            ..Default::default()
        },
        false,
    )?;
    s.anchor = Some(0);
    let before: Vec<_> = s
        .entries()
        .into_iter()
        .map(|(c, _)| s.paragraph(c).map(|p| p.text.clone()))
        .collect::<anyhow::Result<_>>()?;
    let cursor = s.cursor;
    assert_eq!(s.replace_all("가🦀", "새😀문서")?, 7);
    assert_eq!(
        s.core.document().sections[0].paragraphs[0].text,
        "새😀문서새😀문서 끝"
    );
    for c in &cells {
        assert_eq!(s.paragraph(*c)?.text, "셀 새😀문서 새😀문서");
    }
    s.execute(Command::Undo)?;
    assert_eq!(s.cursor, cursor);
    assert_eq!(s.selected_text(), "가🦀가🦀");
    let after: Vec<_> = s
        .entries()
        .into_iter()
        .map(|(c, _)| s.paragraph(c).map(|p| p.text.clone()))
        .collect::<anyhow::Result<_>>()?;
    assert_eq!(before, after);
    s.execute(Command::Redo)?;
    assert!(s.entries().iter().all(|(_, text)| !text.contains("가🦀")));
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("replace.{ext}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        for c in &cells {
            assert_eq!(reopened.paragraph(*c)?.text, "셀 새😀문서 새😀문서");
        }
    }
    Ok(())
}

#[test]
fn replacement_boundaries_reject_before_mutation() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("aaaa 한글".into()))?;
    let generation = s.generation;
    assert_eq!(s.replace_all("없는 말", "새 말")?, 0);
    assert_eq!(s.replace_all("한글", "한글")?, 0);
    assert!(s.replace_all("", "x").is_err());
    assert!(s.replace_all("한글", "a\nb").is_err());
    assert_eq!(s.generation, generation);
    assert_eq!(s.replace_all("aa", "a")?, 2);
    assert_eq!(s.text(), "aa 한글");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "aaaa 한글");
    s.replace_paragraph_text(&"가".repeat(10_001))?;
    let generation = s.generation;
    assert!(s.replace_all("가", "나").is_err());
    assert_eq!(s.len(), 10_001);
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn replace_preserves_unaffected_style() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞 단어 뒤".into()))?;
    s.anchor = Some(5);
    s.cursor.offset = 6;
    s.execute(Command::Character(json!({"bold":true})))?;
    assert_eq!(s.replace_all("단어", "한글문서")?, 1);
    s.select(
        Cursor {
            offset: 7,
            ..Default::default()
        },
        false,
    )?;
    // Inspect the preserved character, rather than the insertion shape to its left.
    s.select(
        Cursor {
            offset: 8,
            ..s.cursor
        },
        true,
    )?;
    assert_eq!(s.character_properties()?["bold"], true);
    assert_eq!(s.text(), "앞 한글문서 뒤");
    Ok(())
}

#[test]
fn page_setup_break_save_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫째쪽둘째쪽".into()))?;
    s.cursor.offset = 3;
    s.execute(Command::PageBreak)?;
    assert_eq!(s.core.page_count(), 2);
    assert_eq!(s.text(), "둘째쪽");
    assert_eq!(s.core.document().sections[0].paragraphs[0].text, "첫째쪽");
    let mut setup = s.page_setup()?;
    setup.landscape = true;
    setup.left_mm = 18.0;
    setup.right_mm = 18.0;
    s.execute(Command::PageSetup(setup))?;
    assert!(s.page_setup()?.landscape);
    assert!((s.page_setup()?.left_mm - 18.0).abs() < 0.005);
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("page.{ext}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(reopened.core.page_count(), 2);
        assert!(reopened.page_setup()?.landscape);
        assert!((reopened.page_setup()?.left_mm - 18.0).abs() < 0.005);
    }
    s.execute(Command::Undo)?;
    assert!(!s.page_setup()?.landscape);
    s.execute(Command::Undo)?;
    assert_eq!(s.core.page_count(), 1);
    assert_eq!(s.text(), "첫째쪽둘째쪽");
    Ok(())
}

#[test]
fn invalid_page_and_table_page_break_leave_state_intact() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1001.0] {
        let mut p = s.page_setup()?;
        p.width_mm = value;
        assert!(s.execute(Command::PageSetup(p)).is_err());
        assert!(!s.dirty());
    }
    let mut p = s.page_setup()?;
    p.left_mm = 200.0;
    assert!(s.execute(Command::PageSetup(p)).is_err());
    s.execute(Command::Table {
        rows: 1,
        columns: 1,
    })?;
    let cell = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("cell"))?
        .0;
    s.select(cell, false)?;
    s.execute(Command::Insert("보존".into()))?;
    let generation = s.generation;
    assert!(s.execute(Command::PageBreak).is_err());
    assert_eq!(s.text(), "보존");
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn command_protocol_validates_before_editing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text":"가나다"}))?;
    let generation = s.generation;
    for (id, value) in [
        ("unknown", json!({})),
        ("text.insert", json!({"wrong":"x"})),
        ("page.setup", json!({"width_mm":0})),
        ("page.setup", json!({"typo":10})),
        (
            "selection.set",
            json!({"cursor":{"section":0,"paragraph":0,"offset":99,"cell":null}}),
        ),
    ] {
        assert!(s.run(id, value).is_err(), "{id}");
        assert_eq!(s.text(), "가나다");
        assert_eq!(s.generation, generation);
    }
    s.run("page.setup", json!({"landscape":true}))?;
    assert!(s.page_setup()?.landscape);
    assert_eq!(
        s.run(
            "edit.replace_all",
            json!({"query":"나","replacement":"한글"})
        )?["replaced"],
        1
    );
    assert_eq!(s.text(), "가한글다");
    Ok(())
}
