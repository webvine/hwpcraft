//! Hangul's 개인 정보 보안 field (`%cpr`, HWPX PRIVATE_INFO) holds text Hangul
//! encrypted with a password in a format that is not public, so HwpCraft does not
//! create or open one. It must still survive: a synthetic field with arbitrary content,
//! built through the RHWP model, keeps its kind and content through saves and edits.
use hwpcraft_doc::HyperlinkTarget;
use hwpcraft_doc::model::control::{Control, Field, FieldType};
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

const CPR: u32 = u32::from_be_bytes(*b"%cpr");
/// Arbitrary content standing in for Hangul's encrypted text: anything an XML 1.0
/// attribute can carry (HWPX cannot hold control characters), XML specials included.
const CONTENT: &str = "Zx9+/q0=가ÿ\u{7f}#;\\<&>\"'";

fn private_field(s: &Session) -> Option<(Field, String)> {
    let para = &s.core.document().sections[0].paragraphs[0];
    para.field_ranges
        .iter()
        .find_map(|r| match para.controls.get(r.control_idx) {
            Some(Control::Field(f)) if f.field_type == FieldType::PrivateInfoSecurity => Some((
                f.clone(),
                para.text
                    .chars()
                    .skip(r.start_char_idx)
                    .take(r.end_char_idx - r.start_char_idx)
                    .collect(),
            )),
            _ => None,
        })
}

#[test]
fn a_private_info_field_survives_saves_and_nearby_edits() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "연락처 ******* 끝"}))?;
    let id = s
        .core
        .insert_hyperlink_native(
            &HyperlinkTarget::body(0, 0),
            4,
            11,
            "https://example.invalid/x",
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let doc = s.core.document_mut();
    for c in &mut doc.sections[0].paragraphs[0].controls {
        if let Control::Field(f) = c
            && f.field_id == id
        {
            f.field_type = FieldType::PrivateInfoSecurity;
            f.ctrl_id = CPR;
            f.command = CONTENT.to_owned();
            f.hyperlink_format = None;
            f.raw_parameters_xml = None;
        }
    }
    doc.sections[0].raw_stream = None;
    let (_, shown) = private_field(&s).ok_or_else(|| anyhow::anyhow!("no field"))?;
    assert_eq!(shown, "*******");

    // Typing and deleting before the field, and deleting after it, leave it whole.
    s.cursor = Cursor {
        offset: 0,
        ..Default::default()
    };
    s.run("text.insert", json!({"text": "본인 "}))?;
    s.run("text.backspace", json!({}))?;
    let len = s.core.document().sections[0].paragraphs[0]
        .text
        .chars()
        .count();
    s.cursor = Cursor {
        offset: len,
        ..Default::default()
    };
    s.run("text.backspace", json!({}))?;
    let (field, shown) = private_field(&s).ok_or_else(|| anyhow::anyhow!("field lost"))?;
    assert_eq!(field.command, CONTENT);
    assert_eq!(shown, "*******");
    assert_eq!(
        s.core.document().sections[0].paragraphs[0].text,
        "본인연락처 ******* "
    );

    let dir = tempfile::tempdir()?;
    for name in ["private.hwp", "private.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let (field, shown) =
            private_field(&reopened).ok_or_else(|| anyhow::anyhow!("{name}: field lost"))?;
        assert_eq!(field.ctrl_id, CPR, "{name}");
        assert_eq!(field.command, CONTENT, "{name}");
        assert_eq!(shown, "*******", "{name}");
        // A second round keeps it as well.
        let again = dir.path().join(format!("again-{name}"));
        reopened.save(&again)?;
        let mut twice = Session::new()?;
        twice.open(&again)?;
        assert_eq!(
            private_field(&twice).map(|(f, t)| (f.command, t)),
            Some((CONTENT.to_owned(), "*******".to_owned())),
            "{name}"
        );
    }
    Ok(())
}
