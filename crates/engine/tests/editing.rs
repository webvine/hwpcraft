use hwpcraft_engine::{Command, Cursor, Motion, Session};
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
fn typing_is_one_undo_step_per_word() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for ch in "안녕하세요".chars() {
        s.execute(Command::Insert(ch.to_string()))?;
    }
    for ch in " 한글".chars() {
        s.execute(Command::Insert(ch.to_string()))?;
    }
    assert_eq!(s.text(), "안녕하세요 한글");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "안녕하세요");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "");
    assert!(!s.can_undo());
    s.execute(Command::Redo)?;
    assert_eq!(s.text(), "안녕하세요");
    Ok(())
}

#[test]
fn span_selection_deletes_formats_and_undoes() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫째\n둘째\n셋째".into()))?;
    s.select(at(0, 1), false)?;
    s.select(at(2, 1), true)?;
    assert!(s.has_selection());
    assert_eq!(s.selected_text(), "째\n둘째\n셋");
    s.execute(Command::Character(json!({"bold": true})))?;
    s.select(at(1, 0), false)?;
    assert_eq!(s.character_properties()?["bold"], true);
    s.select(at(0, 0), false)?;
    assert_eq!(s.character_properties()?["bold"], false);

    s.select(at(2, 1), false)?;
    s.select(at(0, 1), true)?;
    assert_eq!(s.selected_text(), "째\n둘째\n셋");
    s.execute(Command::Backspace)?;
    assert_eq!(texts(&s), ["첫째"]);
    assert_eq!(s.cursor, at(0, 1));
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["첫째", "둘째", "셋째"]);
    // Typing over a span replaces it.
    s.select(at(0, 2), false)?;
    s.select(at(1, 1), true)?;
    s.execute(Command::Insert("X".into()))?;
    assert_eq!(texts(&s), ["첫째X째", "셋째"]);
    Ok(())
}

#[test]
fn paragraph_format_applies_to_every_selected_paragraph() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("하나\n둘\n셋".into()))?;
    s.select(at(0, 0), false)?;
    s.select(at(1, 1), true)?;
    s.execute(Command::Paragraph(json!({"alignment": "center"})))?;
    for (p, expected) in [(0, "center"), (1, "center"), (2, "justify")] {
        s.select(at(p, 0), false)?;
        assert_eq!(s.paragraph_properties()?["alignment"], expected);
    }
    Ok(())
}

#[test]
fn copy_cut_paste_keep_paragraphs() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나\n다라".into()))?;
    s.select(at(0, 1), false)?;
    s.select(at(1, 1), true)?;
    let text = s.copy()?;
    assert_eq!(text, "나\n다");
    s.select(at(1, 2), false)?;
    s.paste(&text)?;
    assert_eq!(texts(&s), ["가나", "다라나", "다"]);
    assert_eq!(s.cursor, at(2, 1));
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["가나", "다라"]);
    // Text from another program is inserted as plain text.
    s.paste("외부")?;
    assert_eq!(s.text(), "다라외부");
    s.select(at(0, 0), false)?;
    s.select(at(1, 2), true)?;
    assert_eq!(s.cut()?, "가나\n다라");
    assert_eq!(texts(&s), ["외부"]);
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["가나", "다라외부"]);
    Ok(())
}

#[test]
fn caret_motions_cross_paragraphs_and_words() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("hello world\n둘째 문단".into()))?;
    s.select(at(1, 0), false)?;
    s.move_caret(Motion::Left, false)?;
    assert_eq!(s.cursor, at(0, 11));
    s.move_caret(Motion::Right, false)?;
    assert_eq!(s.cursor, at(1, 0));
    s.select(at(0, 0), false)?;
    s.move_caret(Motion::WordRight, false)?;
    assert_eq!(s.cursor, at(0, 6));
    s.move_caret(Motion::WordLeft, false)?;
    assert_eq!(s.cursor, at(0, 0));
    s.move_caret(Motion::DocumentEnd, true)?;
    assert_eq!(s.cursor, at(1, 5));
    assert_eq!(s.selected_text(), "hello world\n둘째 문단");
    // An arrow collapses the selection to its edge.
    s.move_caret(Motion::Left, false)?;
    assert_eq!(s.cursor, at(0, 0));
    assert!(!s.has_selection());
    s.select(at(0, 3), false)?;
    s.move_caret(Motion::LineEnd, false)?;
    assert_eq!(s.cursor, at(0, 11));
    s.move_caret(Motion::LineStart, false)?;
    assert_eq!(s.cursor, at(0, 0));
    s.move_caret(Motion::Down, false)?;
    assert_eq!(s.cursor.paragraph, 1);
    Ok(())
}

#[test]
fn select_all_word_and_overwrite() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("하나 둘\n셋".into()))?;
    s.select(at(0, 4), false)?;
    s.select_word()?;
    assert_eq!(s.selected_text(), "둘");
    s.select_all()?;
    assert_eq!(s.selected_text(), "하나 둘\n셋");
    s.execute(Command::Delete)?;
    assert_eq!(texts(&s), [""]);
    s.execute(Command::Undo)?;
    s.select(at(0, 0), false)?;
    s.overwrite = true;
    s.execute(Command::Insert("두개".into()))?;
    assert_eq!(s.text(), "두개 둘");
    Ok(())
}

#[test]
fn column_break_after_inline_object_splits_at_caret() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞".into()))?;
    s.run("insert.equation", json!({"script": "x+1", "size_pt": 10.0}))?;
    s.execute(Command::Insert("뒤".into()))?;
    assert_eq!(s.text(), "앞 뒤");
    s.select(at(0, 2), false)?;
    s.run("insert.column_break", json!({}))?;
    assert_eq!(texts(&s), ["앞 ", "뒤"]);
    Ok(())
}
