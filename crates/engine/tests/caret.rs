//! Core area: Hangul-style caret, selection, clipboard, deletion, special
//! characters, auto text, navigation and statistics commands.
use hwpcraft_engine::{Command, Cursor, Motion, Session};
use serde_json::{Value, json};

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}
fn texts(s: &Session) -> Vec<String> {
    s.entries()
        .into_iter()
        .map(|(c, _)| s.paragraph(c).unwrap().text.clone())
        .collect()
}
fn session(text: &str) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert(text.into()))?;
    Ok(s)
}
fn run(s: &mut Session, id: &str, params: Value) -> anyhow::Result<Value> {
    s.run(id, params)
}

#[test]
fn word_paragraph_and_list_motions_match_hangul() -> anyhow::Result<()> {
    let mut s = session("둘째 문단 alpha beta gamma\n셋째")?;
    // DeleteWord/MoveWordEnd semantics come from the Hangul probe (word end = before spaces).
    s.select(at(0, 8), false)?; // "alp|ha"
    s.move_caret(Motion::WordEnd, false)?;
    assert_eq!(s.cursor, at(0, 11));
    s.move_caret(Motion::WordEnd, false)?;
    assert_eq!(s.cursor, at(0, 16)); // end of beta
    s.move_caret(Motion::WordStart, false)?;
    assert_eq!(s.cursor, at(0, 12));
    s.move_caret(Motion::WordStart, false)?;
    assert_eq!(s.cursor, at(0, 6)); // previous word when already at a start
    s.move_caret(Motion::NextParagraphStart, false)?;
    assert_eq!(s.cursor, at(1, 0));
    s.move_caret(Motion::PrevParagraphStart, false)?;
    assert_eq!(s.cursor, at(0, 0));
    s.select(at(1, 1), false)?;
    s.move_caret(Motion::PrevParagraphStart, false)?;
    assert_eq!(s.cursor, at(1, 0)); // first to the paragraph start
    s.move_caret(Motion::PrevParagraphStart, false)?;
    assert_eq!(s.cursor, at(0, 0));
    s.move_caret(Motion::ListEnd, false)?;
    assert_eq!(s.cursor, at(1, 2));
    s.move_caret(Motion::ListStart, true)?;
    assert_eq!(s.cursor, at(0, 0));
    assert_eq!(s.selected_text(), "둘째 문단 alpha beta gamma\n셋째");
    s.move_caret(Motion::PageEnd, false)?;
    assert_eq!(s.cursor, at(1, 2));
    s.move_caret(Motion::PageStart, false)?;
    assert_eq!(s.cursor, at(0, 0));
    assert_eq!(s.page_of(at(1, 0)), Some(0));

    // The same motions through the shared command.
    let r = run(
        &mut s,
        "caret.move",
        json!({"motion":"word_right","extend":true}),
    )?;
    assert_eq!(r["cursor"]["offset"], 3);
    assert_eq!(r["anchor"], 0);
    assert_eq!(r["moved"], true);
    assert!(run(&mut s, "caret.move", json!({"motion":"sideways"})).is_err());
    Ok(())
}

#[test]
fn previous_paragraph_end_preserves_boundaries_and_selection() -> anyhow::Result<()> {
    let mut s = session("앞😀 문단\n\n뒤 문단")?;
    let original = texts(&s);
    for (start, expected, selected) in [
        (at(0, 0), at(0, 0), ""),
        (at(0, 3), at(0, 0), "앞😀 "),
        (at(1, 0), at(0, 5), "\n"),
        (at(2, 0), at(1, 0), "\n"),
        (at(2, 2), at(1, 0), "\n뒤 "),
    ] {
        s.select(start, false)?;
        s.move_caret(Motion::PrevParagraphEnd, false)?;
        assert_eq!(s.cursor, expected, "start={start:?}");
        assert!(!s.has_selection());
        s.select(start, false)?;
        s.run(
            "caret.move",
            json!({"motion":"prev_paragraph_end","extend":true}),
        )?;
        assert_eq!(s.cursor, expected, "extended start={start:?}");
        assert_eq!(s.selected_text(), selected);
    }
    assert_eq!(texts(&s), original);
    // Navigation must not add an edit to history.
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), [""]);

    let mut s = Session::new()?;
    s.run("table.create", json!({"rows":1,"columns":2}))?;
    s.execute(Command::Insert("셀 첫째\n둘째".into()))?;
    let second = s.cursor;
    assert!(second.cell.is_some());
    let mut first_end = second;
    first_end.cell.as_mut().unwrap().paragraph = 0;
    first_end.offset = 4;
    s.move_caret(Motion::PrevParagraphEnd, true)?;
    assert_eq!(s.cursor, first_end);
    assert_eq!(s.selected_text(), "\n둘째");
    s.move_caret(Motion::PrevParagraphEnd, false)?;
    assert_eq!(s.cursor.cell, first_end.cell);
    assert_eq!(s.cursor.offset, 0); // never leaves the first paragraph of a cell
    assert!(!s.has_selection());
    Ok(())
}

#[test]
fn delete_word_and_line_follow_hangul_probe() -> anyhow::Result<()> {
    let mut s = session("둘째 문단 alpha beta gamma\n셋째")?;
    s.select(at(0, 10), false)?; // "alph|a beta"
    run(&mut s, "text.delete_word", json!({}))?;
    assert_eq!(s.text(), "둘째 문단 alphbeta gamma");
    assert_eq!(s.cursor, at(0, 10));
    run(&mut s, "text.delete_word_back", json!({}))?;
    assert_eq!(s.text(), "둘째 문단 beta gamma");
    assert_eq!(s.cursor, at(0, 6));
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "둘째 문단 alphbeta gamma");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "둘째 문단 alpha beta gamma");

    // Alt+Y: caret to end of line.
    s.select(at(0, 9), false)?;
    run(&mut s, "text.delete_to_line_end", json!({}))?;
    assert_eq!(s.text(), "둘째 문단 alp");
    s.execute(Command::Undo)?;

    // Ctrl+Y on a one-line paragraph removes the paragraph itself.
    s.select(at(0, 3), false)?;
    run(&mut s, "text.delete_line", json!({}))?;
    assert_eq!(texts(&s), ["셋째"]);
    assert_eq!(s.cursor, at(0, 0));
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["둘째 문단 alpha beta gamma", "셋째"]);
    // Last paragraph: its text and the break before it go.
    s.select(at(1, 1), false)?;
    run(&mut s, "text.delete_line", json!({}))?;
    assert_eq!(texts(&s), ["둘째 문단 alpha beta gamma"]);
    assert_eq!(s.cursor, at(0, 22));
    Ok(())
}

#[test]
fn selection_commands_and_clipboard_round_trip() -> anyhow::Result<()> {
    let mut s = session("하나 둘 셋\n넷")?;
    s.select(at(0, 3), false)?;
    let r = run(&mut s, "select.word", json!({}))?;
    assert_eq!(r["text"], "둘");
    let r = run(&mut s, "select.line", json!({}))?;
    assert_eq!(r["text"], "하나 둘 셋");
    let r = run(&mut s, "select.paragraph", json!({}))?;
    assert_eq!(r["hasSelection"], true);
    let r = run(&mut s, "select.clear", json!({}))?;
    assert_eq!(r["hasSelection"], false);
    let r = run(&mut s, "select.all", json!({}))?;
    assert_eq!(r["text"], "하나 둘 셋\n넷");

    s.select(at(0, 3), false)?;
    s.select(at(1, 1), true)?;
    let r = run(&mut s, "edit.copy", json!({}))?;
    assert_eq!(r["text"], "둘 셋\n넷");
    let clip = run(&mut s, "edit.clipboard.get", json!({}))?;
    assert_eq!(clip["text"], "둘 셋\n넷");
    assert_eq!(clip["internal"], true);
    s.select(at(1, 1), false)?;
    run(&mut s, "edit.paste", json!({}))?; // internal clipboard, rich path
    assert_eq!(texts(&s), ["하나 둘 셋", "넷둘 셋", "넷"]);
    s.execute(Command::Undo)?;
    run(
        &mut s,
        "edit.paste",
        json!({"text":"외부 글","mode":"text_only"}),
    )?;
    assert_eq!(texts(&s), ["하나 둘 셋", "넷외부 글"]);
    run(&mut s, "edit.paste", json!({"html":"<p><b>굵게</b></p>"}))?;
    assert!(s.text().contains("굵게"));
    s.select(at(0, 0), false)?;
    s.select(at(0, 2), true)?;
    let r = run(&mut s, "edit.cut", json!({}))?;
    assert_eq!(r["text"], "하나");
    assert_eq!(texts(&s)[0], " 둘 셋");
    assert!(run(&mut s, "edit.paste", json!({"mode":"sideways"})).is_err());
    let mut empty = Session::new()?;
    assert!(run(&mut empty, "edit.paste", json!({})).is_err());
    Ok(())
}

#[test]
fn special_characters_survive_save_and_reopen() -> anyhow::Result<()> {
    let mut s = session("가나다")?;
    s.select(at(0, 1), false)?;
    run(&mut s, "insert.line_break", json!({}))?;
    run(&mut s, "insert.tab", json!({}))?;
    run(&mut s, "insert.nbsp", json!({}))?;
    run(&mut s, "insert.fixed_width_space", json!({}))?;
    run(&mut s, "insert.soft_hyphen", json!({}))?;
    let expected = "가\n\t\u{a0}\u{2007}\u{ad}나다";
    assert_eq!(s.text(), expected);
    assert_eq!(s.cursor, at(0, 6));
    assert_eq!(
        texts(&s).len(),
        1,
        "a line break stays inside the paragraph"
    );
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "가\n\t\u{a0}\u{2007}나다");
    s.execute(Command::Redo)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = temp.path().join(format!("special.{ext}"));
        s.save(&path)?;
        let mut again = Session::new()?;
        again.open(&path)?;
        assert_eq!(again.text(), expected, "{ext}");
        assert_eq!(texts(&again).len(), 1, "{ext}");
    }
    Ok(())
}

#[test]
fn auto_text_overwrite_and_statistics() -> anyhow::Result<()> {
    let mut s = session("본문")?;
    let r = run(
        &mut s,
        "insert.auto_text",
        json!({"kind":"date","format":"%Y-%m-%d"}),
    )?;
    let date = r["inserted"].as_str().unwrap().to_owned();
    assert_eq!(date.len(), 10);
    assert!(s.text().ends_with(&date));
    let r = run(&mut s, "insert.auto_text", json!({"kind":"date"}))?;
    assert!(r["inserted"].as_str().unwrap().ends_with("요일"));
    run(&mut s, "insert.auto_text", json!({"kind":"file_name"}))?;
    assert!(s.text().ends_with("제목 없음"));
    assert!(run(&mut s, "insert.auto_text", json!({"kind":"moon"})).is_err());
    // Page fields are live controls; the text gets a placeholder character each.
    let before = s.text().chars().count();
    run(&mut s, "insert.auto_text", json!({"kind":"page_of_total"}))?;
    assert_eq!(s.text().chars().count(), before + 3);
    let stats = run(&mut s, "document.statistics", json!({}))?;
    assert_eq!(stats["paragraphs"], 1);
    assert_eq!(stats["pages"], 1);
    assert!(stats["characters"].as_u64().unwrap() > 10);
    s.execute(Command::Undo)?;
    assert_eq!(s.text().chars().count(), before);

    let r = run(&mut s, "edit.overwrite", json!({}))?;
    assert_eq!(r["overwrite"], true);
    let r = run(&mut s, "edit.overwrite", json!({"value":false}))?;
    assert_eq!(r["overwrite"], false);

    let mut t = session("하나\n둘")?;
    t.execute(Command::Table {
        rows: 1,
        columns: 2,
    })?;
    let stats = run(&mut t, "document.statistics", json!({}))?;
    assert_eq!(stats["tables"], 1);
    // Table creation adds its own body paragraphs; the two cells count on top.
    let body = stats["bodyParagraphs"].as_u64().unwrap();
    assert!(body >= 2);
    assert_eq!(stats["paragraphs"].as_u64().unwrap(), body + 2);
    assert_eq!(stats["words"], 2);
    assert_eq!(stats["characters"], 3);
    Ok(())
}

#[test]
fn goto_find_all_and_hyperlink_navigation() -> anyhow::Result<()> {
    let mut s = session("첫 링크 하나\n둘째 링크 둘\n끝")?;
    s.select(at(0, 2), false)?;
    s.select(at(0, 4), true)?;
    s.insert_hyperlink("https://example.com/1", None)?;
    s.select(at(1, 3), false)?;
    s.select(at(1, 5), true)?;
    s.insert_hyperlink("https://example.com/2", None)?;
    s.select(at(0, 0), false)?;
    let r = run(&mut s, "hyperlink.next", json!({}))?;
    assert_eq!(r["found"], true);
    assert_eq!(r["uri"], "https://example.com/1");
    assert_eq!(s.selected_text(), "링크");
    let r = run(&mut s, "hyperlink.jump", json!({}))?;
    assert_eq!(r["uri"], "https://example.com/1");
    let r = run(&mut s, "hyperlink.next", json!({}))?;
    assert_eq!(r["uri"], "https://example.com/2");
    assert_eq!(s.cursor.paragraph, 1);
    let r = run(&mut s, "hyperlink.next", json!({}))?;
    assert_eq!(r["found"], false);
    let r = run(&mut s, "hyperlink.prev", json!({}))?;
    assert_eq!(r["uri"], "https://example.com/1");
    s.select(at(2, 0), false)?;
    assert!(run(&mut s, "hyperlink.jump", json!({})).is_err());

    let r = run(&mut s, "edit.find_all", json!({"query":"링크"}))?;
    assert_eq!(r["total"], 2);
    assert_eq!(r["matches"][1]["cursor"]["paragraph"], 1);
    assert_eq!(r["matches"][1]["cursor"]["offset"], 3);
    assert!(run(&mut s, "edit.find_all", json!({"query":""})).is_err());

    run(&mut s, "bookmark.add", json!({"name":"끝표"}))?;
    s.select(at(0, 0), false)?;
    let r = run(&mut s, "edit.goto", json!({"bookmark":"끝표"}))?;
    assert_eq!(r["cursor"]["paragraph"], 2);
    let r = run(&mut s, "edit.goto", json!({"paragraph":1}))?;
    assert_eq!(s.cursor, at(1, 0));
    assert_eq!(r["page"], 0);
    let r = run(&mut s, "edit.goto", json!({"page":1}))?;
    assert_eq!(r["cursor"]["paragraph"], 0);
    assert!(run(&mut s, "edit.goto", json!({"page":9})).is_err());
    assert!(run(&mut s, "edit.goto", json!({"page":1,"section":0})).is_err());
    assert!(run(&mut s, "document.outline", json!({}))?["outline"].is_array());
    Ok(())
}

#[test]
fn page_number_controls_keep_their_place_in_text() -> anyhow::Result<()> {
    let mut s = session("쪽 ")?;
    run(&mut s, "insert.auto_text", json!({"kind":"page_number"}))?;
    // Typing right after a trailing number must not push the number behind it.
    s.execute(Command::Insert("abc".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("x ".into()))?;
    run(&mut s, "insert.auto_text", json!({"kind":"page_of_total"}))?;
    s.execute(Command::Insert(" 끝".into()))?;
    let positions = |s: &Session, p: usize| -> Vec<usize> {
        let para = &s.core.document().sections[0].paragraphs[p];
        let n = para.controls.len();
        // Section/column definitions sit at 0; the number controls follow.
        para.control_text_positions()[n.saturating_sub(if p == 0 { 1 } else { 2 })..].to_vec()
    };
    assert_eq!(s.paragraph(at(0, 0))?.text, "쪽  abc");
    assert_eq!(positions(&s, 0), vec![2]);
    assert_eq!(s.paragraph(at(1, 0))?.text, "x  /  끝");
    assert_eq!(positions(&s, 1), vec![2, 4]);
    let temp = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = temp.path().join(format!("numbers.{ext}"));
        s.save(&path)?;
        let mut again = Session::new()?;
        again.open(&path)?;
        assert_eq!(positions(&again, 0), vec![2], "{ext}");
        assert_eq!(positions(&again, 1), vec![2, 4], "{ext}");
        assert_eq!(again.paragraph(at(0, 0))?.text.chars().count(), 6, "{ext}");
    }
    Ok(())
}

#[test]
fn checkpoint_rollback_restores_document_and_history() -> anyhow::Result<()> {
    let mut s = session("기준")?;
    let generation = s.generation;
    let checkpoint = s.checkpoint();
    s.execute(Command::Insert(" 하나".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("둘".into()))?;
    assert_eq!(texts(&s), ["기준 하나", "둘"]);
    s.rollback(checkpoint)?;
    assert_eq!(texts(&s), ["기준"]);
    assert_eq!(s.cursor, at(0, 2));
    assert!(s.generation > generation);
    // The steps since the checkpoint are gone from the undo history.
    assert!(s.can_undo());
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), [""]);
    let checkpoint = s.checkpoint();
    s.execute(Command::Insert("유지".into()))?;
    s.release(checkpoint);
    assert_eq!(s.text(), "유지");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "");
    Ok(())
}

#[test]
fn every_command_documents_its_parameters_and_actions_resolve() {
    let registered: Vec<&str> = hwpcraft_engine::COMMANDS
        .iter()
        .chain(hwpcraft_engine::extension_commands())
        .map(|(id, _, _)| *id)
        .collect();
    for (id, help, example) in hwpcraft_engine::command_help_entries() {
        assert!(registered.contains(&id), "PARAMS for unknown command {id}");
        assert!(!help.is_empty(), "{id}: empty help");
        assert!(example.is_object(), "{id}: example must be a JSON object");
    }
    for (action, command, params, mapping) in hwpcraft_engine::HWP_ACTIONS {
        assert!(
            registered.contains(command),
            "{action} -> unknown command {command}"
        );
        assert!(
            serde_json::from_str::<Value>(params).unwrap().is_object(),
            "{action}"
        );
        assert!(matches!(*mapping, "mapped" | "partial"), "{action}");
        assert_eq!(
            hwpcraft_engine::hwp_action(action).map(|a| a.0),
            Some(*command)
        );
    }
    assert_eq!(
        hwpcraft_engine::hwp_action("MoveLineEnd").map(|a| a.0),
        Some("caret.move")
    );
    assert!(hwpcraft_engine::hwp_action("NoSuchAction").is_none());
}
