//! Commands that only need the section laid out again leave note numbers alone. An
//! endnote numbered on from an earlier section (as Hangul stores it) keeps its
//! number when a picture number or the footnote shape changes in that section.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::json;

fn endnote_numbers(s: &Session) -> Vec<u16> {
    s.core
        .document()
        .sections
        .iter()
        .flat_map(|sec| &sec.paragraphs)
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::Endnote(n) => Some(n.number),
            _ => None,
        })
        .collect()
}

#[test]
fn an_endnote_numbered_on_keeps_its_number() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문"}))?;
    s.run("insert.endnote", json!({"text": "미주"}))?;
    // As in a file whose earlier section already holds four endnotes.
    for p in &mut s.core.document_mut().sections[0].paragraphs {
        for c in &mut p.controls {
            if let Control::Endnote(n) = c {
                n.number = 5;
            }
        }
    }
    s.run("insert.auto_number", json!({"kind": "picture"}))?;
    assert_eq!(endnote_numbers(&s), [5], "picture number");
    s.run(
        "note.shape",
        json!({"kind": "footnote", "props": {"separatorLength": 5669}}),
    )?;
    assert_eq!(endnote_numbers(&s), [5], "footnote shape");
    Ok(())
}
