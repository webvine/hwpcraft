//! Owner: format area. 각주 / 미주 글자 모양 (`format.footnote.character`): a range of
//! a note's text gets a character patch in one undo step and keeps it on save.
use hwpcraft_engine::Session;
use serde_json::json;

fn note_control(s: &mut Session) -> anyhow::Result<u64> {
    let caret = s.run("caret.get", json!({}))?;
    let paragraph = caret["cursor"]["paragraph"].as_u64().unwrap_or(0) as usize;
    let para = &s.core.document().sections[0].paragraphs[paragraph];
    Ok(para
        .controls
        .iter()
        .position(|c| matches!(c, hwpcraft_doc::model::control::Control::Footnote(_)))
        .expect("a footnote in the caret paragraph") as u64)
}

#[test]
fn a_footnote_range_takes_a_character_format() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문"}))?;
    s.run("insert.footnote", json!({"text": "출처 표기"}))?;
    let control = note_control(&mut s)?;
    let plain = s.run(
        "format.footnote.character.get",
        json!({"control": control, "offset": 2}),
    )?;
    assert_eq!(plain["italic"], false, "{plain}");
    // The note text starts after its number and a space ("  출처 표기").
    let out = s.run(
        "format.footnote.character",
        json!({"control": control, "start": 2, "end": 4, "format": {"italic": true, "bold": true}}),
    )?;
    assert_eq!(
        (out["italic"].clone(), out["bold"].clone()),
        (json!(true), json!(true)),
        "{out}"
    );
    let after = s.run(
        "format.footnote.character.get",
        json!({"control": control, "offset": 5}),
    )?;
    assert_eq!(after["italic"], false, "outside the range: {after}");

    let dir = tempfile::tempdir()?;
    for name in ["note.hwp", "note.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        back.run("caret.move", json!({"motion": "document_start"}))?;
        let control = note_control(&mut back)?;
        let saved = back.run(
            "format.footnote.character.get",
            json!({"control": control, "offset": 2}),
        )?;
        assert_eq!(saved["italic"], true, "{name}: {saved}");
    }
    s.run("edit.undo", json!({}))?;
    let undone = s.run(
        "format.footnote.character.get",
        json!({"control": control, "offset": 2}),
    )?;
    assert_eq!(undone["italic"], false, "one undo step: {undone}");
    // A control that is not a note is refused.
    assert!(
        s.run(
            "format.footnote.character",
            json!({"control": 99, "format": {"bold": true}})
        )
        .is_err()
    );
    Ok(())
}

/// The help examples of the four footnote format commands run as written on a new
/// document whose first paragraph holds one footnote (its control 2).
#[test]
fn the_help_examples_run_on_a_new_document_with_a_footnote() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문"}))?;
    let made = s.run("insert.footnote", json!({"text": "출처 표기"}))?;
    assert_eq!(note_control(&mut s)?, 2, "{made}");
    for id in [
        "format.footnote.paragraph",
        "format.footnote.paragraph.get",
        "format.footnote.character",
        "format.footnote.character.get",
    ] {
        let (_, example) = hwpcraft_engine::command_help(id).expect("documented");
        s.run(id, example.clone())
            .map_err(|e| anyhow::anyhow!("{id} {example}: {e}"))?;
    }
    Ok(())
}
