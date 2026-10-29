//! Atomic batch rollback keeps the internal (rich) clipboard exactly as it was at
//! the checkpoint (engine performance, table-object area).
use hwpcraft_doc::Cursor;
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn at(offset: usize) -> Cursor {
    Cursor {
        offset,
        ..Default::default()
    }
}
fn copy(s: &mut Session, start: usize, end: usize) -> anyhow::Result<()> {
    s.select(at(start), false)?;
    s.select(at(end), true)?;
    s.copy()?;
    Ok(())
}
fn clipboard(s: &mut Session) -> anyhow::Result<String> {
    Ok(s.run("edit.clipboard.get", json!({}))?.to_string())
}

#[test]
fn rollback_without_a_new_copy_keeps_the_clipboard_and_pastes_it() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라마바".into()))?;
    copy(&mut s, 0, 2)?;
    let before_text = s.text().to_owned();
    let before_clip = clipboard(&mut s)?;
    let cp = s.full_checkpoint()?;
    s.select(at(6), false)?;
    s.execute(Command::Insert("추가".into()))?;
    s.rollback_full(cp)?;
    assert_eq!(s.text(), before_text);
    assert_eq!(clipboard(&mut s)?, before_clip);
    s.select(at(6), false)?;
    s.run("edit.paste", json!({}))?;
    assert_eq!(s.text(), "가나다라마바가나");
    Ok(())
}

#[test]
fn rollback_after_a_new_copy_restores_the_checkpoint_clipboard() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라마바".into()))?;
    copy(&mut s, 0, 2)?;
    let before_clip = clipboard(&mut s)?;
    let cp = s.full_checkpoint()?;
    copy(&mut s, 2, 4)?;
    assert_ne!(clipboard(&mut s)?, before_clip);
    s.rollback_full(cp)?;
    assert_eq!(clipboard(&mut s)?, before_clip);
    s.select(at(6), false)?;
    s.run("edit.paste", json!({}))?;
    assert_eq!(s.text(), "가나다라마바가나");

    // No clipboard at the checkpoint: a copy inside the batch is discarded.
    let mut empty = Session::new()?;
    empty.execute(Command::Insert("가나다".into()))?;
    let none = clipboard(&mut empty)?;
    let cp = empty.full_checkpoint()?;
    copy(&mut empty, 0, 1)?;
    empty.rollback_full(cp)?;
    assert_eq!(clipboard(&mut empty)?, none);
    Ok(())
}
