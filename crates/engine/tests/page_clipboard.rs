//! Document-owned page clipboard and full-scope rollback use actual paste/IO.
use anyhow::Result;
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn texts(s: &Session) -> Vec<String> {
    s.core
        .document()
        .sections
        .iter()
        .flat_map(|section| section.paragraphs.iter().map(|p| p.text.clone()))
        .collect()
}

#[test]
fn unrelated_sessions_cannot_paste_each_others_pages() -> Result<()> {
    let mut first = Session::new()?;
    first.execute(Command::Insert("first-private-fixture".into()))?;
    first.run("page.copy", json!({"page":1}))?;
    let mut second = Session::new()?;
    second.execute(Command::Insert("second-own-fixture".into()))?;
    let before = (texts(&second), second.generation, second.cursor);
    assert!(second.run("page.paste", json!({"page":1})).is_err());
    assert_eq!((texts(&second), second.generation, second.cursor), before);
    second.run("page.copy", json!({"page":1}))?;
    first.run("page.paste", json!({"page":1}))?;
    assert_eq!(
        texts(&first)
            .iter()
            .filter(|t| t.contains("first-private-fixture"))
            .count(),
        2
    );
    assert!(
        !texts(&first)
            .iter()
            .any(|t| t.contains("second-own-fixture"))
    );
    Ok(())
}

#[test]
fn full_rollback_restores_prior_page_copy_and_paste_roundtrips() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("original copied page".into()))?;
    s.run("page.copy", json!({"page":1}))?;
    s.replace_paragraph_text("baseline document")?;
    let checkpoint = s.full_checkpoint()?;
    s.run("page.copy", json!({"page":1}))?;
    s.execute(Command::Insert("discarded edit".into()))?;
    s.rollback_full(checkpoint)?;
    assert_eq!(s.text(), "baseline document");
    s.run("page.paste", json!({"page":1}))?;
    assert_eq!(texts(&s), ["baseline document", "original copied page"]);
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("page-clipboard.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(texts(&reopened), texts(&s));
        assert!(reopened.run("page.paste", json!({"page":1})).is_err());
    }
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["baseline document"]);
    s.execute(Command::Redo)?;
    assert_eq!(texts(&s), ["baseline document", "original copied page"]);
    Ok(())
}

#[test]
fn empty_copy_state_is_restored_on_rollback_and_success_keeps_new_copy() -> Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("copy fixture".into()))?;
    let checkpoint = s.full_checkpoint()?;
    s.run("page.copy", json!({"page":1}))?;
    s.rollback_full(checkpoint)?;
    assert!(s.run("page.paste", json!({"page":1})).is_err());
    let checkpoint = s.full_checkpoint()?;
    s.run("page.copy", json!({"page":1}))?;
    s.release_full(checkpoint)?;
    s.run("page.paste", json!({"page":1}))?;
    assert_eq!(texts(&s), ["copy fixture", "copy fixture"]);
    Ok(())
}
