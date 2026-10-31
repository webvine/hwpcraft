//! Key macros: recording, replay, rollback and saved macros (`macro.*`).
use hwpcraft_engine::{Command, MacroStep, Motion, Session};
use serde_json::{Value, json};

fn step(command: &str, params: Value) -> MacroStep {
    MacroStep {
        command: command.into(),
        params,
    }
}

/// What the desktop does for: type 제목, select it with Shift+Home, make it bold,
/// Enter, type 본문, then query the selection (not recorded).
fn desktop_edits(s: &mut Session) -> anyhow::Result<()> {
    for ch in ["제", "목"] {
        s.execute(Command::Insert(ch.into()))?;
    }
    s.move_caret(Motion::LineStart, true)?;
    s.execute(Command::Character(json!({"bold": true})))?;
    s.move_caret(Motion::LineEnd, false)?;
    s.execute(Command::Split)?;
    s.run("text.insert", json!({"text": "본문"}))?;
    s.run("select.get", json!({}))?;
    Ok(())
}

#[test]
fn recording_captures_top_level_edits_once() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    assert_eq!(s.macro_recording(), None);
    s.run("macro.record.start", json!({}))?;
    assert!(
        s.run("macro.record.start", json!({})).is_err(),
        "already recording"
    );
    desktop_edits(&mut s)?;
    assert_eq!(s.macro_recording(), Some(6));
    let stopped = s.run("macro.record.stop", json!({}))?;
    let steps: Vec<MacroStep> = serde_json::from_value(stopped["steps"].clone())?;
    assert_eq!(
        steps,
        vec![
            step("text.insert", json!({"text": "제목"})),
            step(
                "caret.move",
                json!({"motion": "line_start", "extend": true})
            ),
            step("format.character", json!({"bold": true})),
            step("caret.move", json!({"motion": "line_end", "extend": false})),
            step("paragraph.split", json!({})),
            step("text.insert", json!({"text": "본문"})),
        ]
    );
    assert_eq!(stopped["saved"], Value::Null);
    assert!(
        s.run("macro.record.stop", json!({})).is_err(),
        "not recording"
    );
    // Nothing is recorded once stopped.
    s.execute(Command::Insert("끝".into()))?;
    assert_eq!(s.macro_recording(), None);

    // Replaying on a blank document gives the same text and formatting.
    let mut replay = Session::new()?;
    replay.run("macro.run", json!({"steps": steps}))?;
    let mut original = Session::new()?;
    desktop_edits(&mut original)?;
    assert_eq!(replay.text(), original.text());
    for s in [&mut replay, &mut original] {
        s.move_caret(Motion::DocumentStart, false)?;
        s.move_caret(Motion::Right, false)?;
    }
    assert_eq!(
        replay.character_properties()?["bold"],
        original.character_properties()?["bold"]
    );
    assert_eq!(replay.character_properties()?["bold"], true);
    // Each step stays an ordinary undo step.
    replay.execute(Command::Undo)?;
    let all = replay.run("document.text", json!({}))?.to_string();
    assert!(all.contains("제목") && !all.contains("본문"), "{all}");
    Ok(())
}

#[test]
fn a_command_running_others_records_only_itself() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한 낱말 두 낱말".into()))?;
    s.start_macro_recording()?;
    // select.word moves the caret internally; edit.replace_all inserts many times.
    s.run("select.word", json!({}))?;
    s.run(
        "edit.replace_all",
        json!({"query": "낱말", "replacement": "단어"}),
    )?;
    let steps = s.stop_macro_recording()?;
    let ids: Vec<&str> = steps.iter().map(|s| s.command.as_str()).collect();
    assert_eq!(ids, ["select.word", "edit.replace_all"]);
    // Running a macro is not recorded step by step either.
    s.start_macro_recording()?;
    s.run_macro(&[step("text.insert", json!({"text": "!"}))], 2)?;
    assert_eq!(s.stop_macro_recording()?, vec![]);
    Ok(())
}

#[test]
fn a_failing_step_rolls_the_whole_run_back() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("처음".into()))?;
    let can_undo = s.can_undo();
    let steps = vec![
        step("text.insert", json!({"text": " 하나"})),
        step("paragraph.split", json!({})),
        step("format.paragraph", json!({"no_such_property": 1})),
    ];
    let err = s
        .run("macro.run", json!({"steps": steps, "repeat": 2}))
        .expect_err("third step fails");
    assert!(format!("{err:#}").contains("3번째 단계"), "{err:#}");
    assert_eq!(s.text(), "처음");
    assert_eq!(s.can_undo(), can_undo);
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "");
    // Invalid macros are refused before anything runs.
    for bad in [
        json!({"steps": []}),
        json!({"steps": [{"command": "file.save", "params": {"path": "x.hwp"}}]}),
        json!({"steps": [{"command": "macro.run", "params": {"name": "a"}}]}),
        json!({"steps": [{"command": "text.insert", "params": {"text": "a"}}], "repeat": 0}),
        json!({"steps": [{"command": "text.insert", "params": {"text": "a"}}], "name": "a"}),
    ] {
        assert!(s.run("macro.run", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(s.text(), "");
    Ok(())
}

#[test]
fn table_cell_moves_are_recorded_and_replayed() -> anyhow::Result<()> {
    // A clicked cell (a direct caret placement) is the macro's starting point.
    fn table_with_caret_in_first_cell() -> anyhow::Result<Session> {
        let mut s = Session::new()?;
        s.execute(Command::Table {
            rows: 2,
            columns: 2,
        })?;
        let first = s
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
            .ok_or_else(|| anyhow::anyhow!("no cell"))?
            .0;
        s.cursor = first;
        Ok(s)
    }
    let mut s = table_with_caret_in_first_cell()?;
    s.start_macro_recording()?;
    s.execute(Command::Insert("A".into()))?;
    s.move_cell(false)?;
    s.execute(Command::Insert("B".into()))?;
    let steps = s.stop_macro_recording()?;
    assert_eq!(
        steps,
        vec![
            step("text.insert", json!({"text": "A"})),
            step("caret.move_cell", json!({"backward": false})),
            step("text.insert", json!({"text": "B"})),
        ]
    );
    let mut replay = table_with_caret_in_first_cell()?;
    replay.run_macro(&steps, 1)?;
    assert_eq!(
        replay.run("document.text", json!({}))?,
        s.run("document.text", json!({}))?
    );
    Ok(())
}

#[test]
fn saved_macros_and_recording_across_documents() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(hwpcraft_engine::set_config_dir(dir.path().to_owned()));
    let mut s = Session::new()?;
    s.run("macro.record.start", json!({}))?;
    s.execute(Command::Insert("머리".into()))?;
    // Opening another document keeps the recording going.
    let path = dir.path().join("other.hwpx");
    Session::new()?.save(&path)?;
    s.open(&path)?;
    assert_eq!(s.macro_recording(), Some(1));
    s.execute(Command::Insert("말".into()))?;
    let stopped = s.run("macro.record.stop", json!({"name": " 머리말 "}))?;
    assert_eq!(stopped["saved"], "머리말");
    assert_eq!(
        s.run("macro.list", json!({}))?,
        json!({"macros": [{"name": "머리말", "steps": 1}]})
    );
    assert_eq!(
        s.run("macro.get", json!({"name": "머리말"}))?["steps"],
        json!([{"command": "text.insert", "params": {"text": "머리말"}}])
    );
    let mut other = Session::new()?;
    other.run("macro.run", json!({"name": "머리말", "repeat": 3}))?;
    assert_eq!(other.text(), "머리말머리말머리말");
    s.run(
        "macro.save",
        json!({"name": "줄", "steps": [{"command": "paragraph.split"}]}),
    )?;
    assert_eq!(hwpcraft_engine::saved_macros()?.len(), 2);
    for bad in [
        json!({"name": "", "steps": [{"command": "paragraph.split"}]}),
        json!({"name": "x".repeat(41), "steps": [{"command": "paragraph.split"}]}),
        json!({"name": "빈 것", "steps": []}),
    ] {
        assert!(s.run("macro.save", bad.clone()).is_err(), "{bad}");
    }
    s.run("macro.delete", json!({"name": "머리말"}))?;
    assert!(s.run("macro.delete", json!({"name": "머리말"})).is_err());
    assert!(s.run("macro.run", json!({"name": "머리말"})).is_err());
    assert_eq!(
        s.run("macro.list", json!({}))?,
        json!({"macros": [{"name": "줄", "steps": 1}]})
    );
    // Stopping without steps saves nothing.
    s.run("macro.record.start", json!({}))?;
    assert_eq!(
        s.run("macro.record.stop", json!({"name": "빈"}))?["saved"],
        Value::Null
    );
    Ok(())
}
