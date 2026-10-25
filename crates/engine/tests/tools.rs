use hwpcraft_engine::{Command, Cursor, MarkKind, Session};
use serde_json::json;

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}
fn texts(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect()
}

#[test]
fn sort_selected_paragraphs_text_number_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("제목\n다\n가\nB\na\n나".into()))?;
    s.select(at(1, 0), false)?;
    s.select(at(5, 1), true)?;
    s.run("edit.sort", json!({}))?;
    assert_eq!(texts(&s), ["제목", "a", "B", "가", "나", "다"]);
    assert_eq!(s.selected_text(), "a\nB\n가\n나\n다");
    s.run("edit.sort", json!({"descending": true}))?;
    assert_eq!(texts(&s), ["제목", "다", "나", "가", "B", "a"]);
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["제목", "다", "가", "B", "a", "나"]);

    let mut n = Session::new()?;
    n.execute(Command::Insert("머리\n10번\n없음\n2번\n-3점".into()))?;
    n.select(at(1, 0), false)?;
    n.select(at(4, 2), true)?;
    n.run("edit.sort", json!({"key": "number"}))?;
    assert_eq!(texts(&n), ["머리", "-3점", "2번", "10번", "없음"]);
    // Sorted paragraphs survive a save and reopen.
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("sorted.hwpx");
    n.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    assert_eq!(texts(&reopened), ["머리", "-3점", "2번", "10번", "없음"]);
    assert!(n.run("edit.sort", json!({"key": "bogus"})).is_err());
    Ok(())
}

#[test]
fn delete_ctrl_marks_removes_chosen_kinds_only() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문 ".into()))?;
    s.run("insert.equation", json!({"script": "a+b", "size_pt": 10.0}))?;
    s.run("insert.footnote", json!({"text": "주석"}))?;
    s.run("bookmark.add", json!({"name": "표시"}))?;
    let counts = s.ctrl_mark_counts()?;
    let count = |k| counts.iter().find(|(x, _)| *x == k).map(|x| x.1);
    assert_eq!(count(MarkKind::Equation), Some(1));
    assert_eq!(count(MarkKind::Footnote), Some(1));
    assert_eq!(count(MarkKind::Bookmark), Some(1));
    let v = s.run(
        "edit.delete_ctrl_marks",
        json!({"kinds": ["equation", "bookmark"]}),
    )?;
    assert_eq!(v["deleted"], 2);
    let counts = s.ctrl_mark_counts()?;
    let count = |k| counts.iter().find(|(x, _)| *x == k).map(|x| x.1);
    assert_eq!(count(MarkKind::Equation), Some(0));
    assert_eq!(count(MarkKind::Bookmark), Some(0));
    assert_eq!(count(MarkKind::Footnote), Some(1));
    s.execute(Command::Undo)?;
    let counts = s.ctrl_mark_counts()?;
    assert_eq!(
        counts
            .iter()
            .find(|(x, _)| *x == MarkKind::Equation)
            .map(|x| x.1),
        Some(1)
    );
    assert!(
        s.run("edit.delete_ctrl_marks", json!({"kinds": []}))
            .is_err()
    );
    Ok(())
}

#[test]
fn sort_whole_section_with_section_definition() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("다\n가\n나".into()))?;
    s.select_all()?;
    s.run("edit.sort", json!({}))?;
    assert_eq!(texts(&s), ["가", "나", "다"]);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("whole.hwp");
    s.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    assert_eq!(texts(&reopened), ["가", "나", "다"]);
    Ok(())
}
