//! Owner: core area. Hyperlink listing/editing/removal via RHWP's hyperlink API.
use hwpcraft_engine::{Command, HyperlinkEdit, Session};
use serde_json::json;

fn linked_session() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞 링크 뒤".into()))?;
    s.anchor = Some(2);
    s.cursor.offset = 4;
    s.insert_hyperlink("https://example.com/a", None)?;
    Ok(s)
}

#[test]
fn list_update_and_remove_are_single_undo_steps() -> anyhow::Result<()> {
    let mut s = linked_session()?;
    let links = s.hyperlinks()?;
    assert_eq!(links.len(), 1);
    assert_eq!((links[0].start, links[0].end), (2, 4));
    assert_eq!(links[0].text, "링크");
    let id = links[0].field_id;
    s.cursor.offset = 3;
    assert_eq!(s.hyperlink_at_caret()?.map(|l| l.field_id), Some(id));
    s.cursor.offset = 0;
    assert!(s.hyperlink_at_caret()?.is_none());

    // Address and text in one step.
    s.update_hyperlink(HyperlinkEdit {
        field_id: id,
        uri: Some("https://example.org/b".into()),
        text: Some("고친 링크".into()),
    })?;
    let links = s.hyperlinks()?;
    assert_eq!(links[0].uri, "https://example.org/b");
    assert_eq!(links[0].text, "고친 링크");
    assert_eq!(s.text(), "앞 고친 링크 뒤");
    assert_eq!(s.cursor.offset, 2 + "고친 링크".chars().count());
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "앞 링크 뒤");
    assert_eq!(s.hyperlinks()?[0].uri, "https://example.com/a");
    s.execute(Command::Redo)?;
    assert_eq!(s.hyperlinks()?[0].text, "고친 링크");

    // Remove keeps the text.
    s.remove_hyperlink(id, true)?;
    assert!(s.hyperlinks()?.is_empty());
    assert_eq!(s.text(), "앞 고친 링크 뒤");
    s.execute(Command::Undo)?;
    assert_eq!(s.hyperlinks()?.len(), 1);
    Ok(())
}

#[test]
fn invalid_edits_leave_document_and_history_intact() -> anyhow::Result<()> {
    let mut s = linked_session()?;
    let id = s.hyperlinks()?[0].field_id;
    let before = (s.text().to_owned(), s.generation, s.can_undo());
    for edit in [
        HyperlinkEdit {
            field_id: id,
            uri: None,
            text: None,
        },
        HyperlinkEdit {
            field_id: id,
            uri: Some("file:///etc/passwd".into()),
            text: None,
        },
        HyperlinkEdit {
            field_id: id,
            uri: None,
            text: Some("두\n줄".into()),
        },
        HyperlinkEdit {
            field_id: id,
            uri: None,
            text: Some("   ".into()),
        },
        HyperlinkEdit {
            field_id: id + 999,
            uri: Some("https://example.com".into()),
            text: None,
        },
    ] {
        assert!(s.update_hyperlink(edit).is_err());
    }
    assert!(s.remove_hyperlink(id + 999, false).is_err());
    assert_eq!(
        (s.text().to_owned(), s.generation, s.can_undo()),
        before,
        "failed edits must not change the document or add history"
    );
    // The same uri is a no-op success, not an error.
    s.update_hyperlink(HyperlinkEdit {
        field_id: id,
        uri: Some("https://example.com/a".into()),
        text: None,
    })?;
    assert_eq!(s.hyperlinks()?[0].uri, "https://example.com/a");
    Ok(())
}

#[test]
fn protocol_commands_cover_list_update_remove_and_roundtrip() -> anyhow::Result<()> {
    let mut s = linked_session()?;
    let list = s.run("hyperlink.list", json!({}))?;
    let id = list[0]["fieldId"].as_u64().unwrap();
    assert!(s.run("hyperlink.list", json!({"x":1})).is_err());
    assert!(
        s.run("hyperlink.update", json!({"field_id": id, "bogus": 1}))
            .is_err()
    );
    s.run(
        "hyperlink.update",
        json!({"field_id": id, "uri": "https://example.net/c"}),
    )?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("links.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let links = r.hyperlinks()?;
        assert_eq!(links.len(), 1, "{ext}");
        assert_eq!(links[0].uri, "https://example.net/c", "{ext}");
        assert_eq!(links[0].text, "링크", "{ext}");
    }
    s.run("hyperlink.remove", json!({"field_id": id}))?;
    assert!(
        s.run("hyperlink.list", json!({}))?
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.text(), "앞 링크 뒤");
    Ok(())
}
