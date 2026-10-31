//! A command built from other commands is one undo step, and a failure inside it
//! leaves no partial step in the history (nested `transaction`).
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn state(s: &mut Session) -> anyhow::Result<(String, usize, u32)> {
    Ok((
        s.run("document.text", json!({}))?.to_string(),
        s.core.document().sections.len(),
        s.page_count()?,
    ))
}

#[test]
fn breaks_built_on_split_are_one_undo_step() -> anyhow::Result<()> {
    for (command, params) in [
        (
            "page.column_def_break",
            json!({"count": 2, "spacing_mm": 8.0}),
        ),
        ("page.section_break", json!({})),
    ] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("제목 줄과 본문 줄".into()))?;
        s.cursor.offset = 3;
        let before = state(&mut s)?;
        s.run(command, params)?;
        let after = state(&mut s)?;
        assert_ne!(before, after, "{command} changed nothing");
        s.execute(Command::Undo)?;
        assert_eq!(state(&mut s)?, before, "{command}: one undo restores it");
        s.execute(Command::Redo)?;
        assert_eq!(state(&mut s)?, after, "{command}: one redo repeats it");
        s.execute(Command::Undo)?;
        // The step before the break is the typing, not a half-done break.
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "", "{command}");
    }
    Ok(())
}
