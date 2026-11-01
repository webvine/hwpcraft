//! A body selection runs over section boundaries, as in Hangul: Select All takes
//! the whole document, formatting and copying reach every section, and deleting
//! joins the sections the selection crossed in one undo step.
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

fn at(section: usize, paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section,
        paragraph,
        offset,
        cell: None,
    }
}

/// "가나다" in section 0 and "라마바" in section 1.
fn two_sections() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다".into()))?;
    s.run("page.section_break", json!({}))?;
    s.execute(Command::Insert("라마바".into()))?;
    let doc = s.core.document();
    assert_eq!(doc.sections.len(), 2);
    Ok(s)
}

fn texts(s: &Session) -> Vec<String> {
    s.core
        .document()
        .sections
        .iter()
        .flat_map(|sec| sec.paragraphs.iter())
        .map(|p| p.text.clone())
        .filter(|t| !t.is_empty())
        .collect()
}

#[test]
fn select_all_takes_every_section_and_formats_them() -> anyhow::Result<()> {
    let mut s = two_sections()?;
    s.select_all()?;
    let text = s.selected_text();
    assert!(
        text.contains("가나다") && text.contains("라마바"),
        "{text:?}"
    );
    s.execute(Command::Character(json!({"bold": true})))?;
    for (section, paragraph) in [(0, 0), (1, 0)] {
        s.select(at(section, paragraph, 1), false)?;
        assert_eq!(s.character_properties()?["bold"], true, "{section}");
    }
    // Copy over sections still gives the plain text.
    s.select_all()?;
    let copied = s.copy()?;
    assert!(
        copied.contains("가나다") && copied.contains("라마바"),
        "{copied:?}"
    );
    Ok(())
}

#[test]
fn deleting_over_a_section_boundary_joins_the_sections() -> anyhow::Result<()> {
    let mut s = two_sections()?;
    let last = s.core.document().sections[1].paragraphs.len() - 1;
    s.select(at(0, 0, 1), false)?;
    s.select(at(1, last, 2), true)?;
    s.execute(Command::Backspace)?;
    assert_eq!(s.core.document().sections.len(), 1);
    assert_eq!(texts(&s), ["가바"]);
    assert_eq!(s.cursor, at(0, 0, 1));
    for name in ["j.hwpx", "j.hwp"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut again = Session::new()?;
        again.open(&path)?;
        assert_eq!(again.core.document().sections.len(), 1, "{name}");
        assert_eq!(texts(&again), ["가바"], "{name}");
    }
    // One undo brings both sections back.
    s.execute(Command::Undo)?;
    assert_eq!(s.core.document().sections.len(), 2);
    assert_eq!(texts(&s), ["가나다", "라마바"]);
    Ok(())
}

#[test]
fn commands_limited_to_one_section_say_so() -> anyhow::Result<()> {
    let mut s = two_sections()?;
    s.select_all()?;
    assert!(s.run("edit.sort", json!({})).is_err());
    Ok(())
}
