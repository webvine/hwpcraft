//! Owner: core area. Password-protected open/save (RHWP HWP5 EncryptVersion 4 and
//! ODF-encrypted HWPX) and HTML clipboard paste.
use hwpcraft_engine::{Command, Session};
use serde_json::json;

#[test]
fn password_save_requires_password_to_reopen() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("비밀 문서 😀".into()))?;
        assert!(!s.has_password());
        s.set_password(Some("한글암호1!".into()))?;
        assert!(s.has_password());
        assert!(s.dirty(), "setting a password asks for a save");
        let path = temp.path().join(format!("secret.{ext}"));
        s.save(&path)?;
        assert!(!s.dirty());

        let mut plain = Session::new()?;
        let error = plain.open(&path).expect_err("plain open must fail");
        assert!(hwpcraft_formats::needs_password(&error), "{ext}: {error:#}");
        let wrong = plain
            .open_with_password(&path, "틀린암호")
            .expect_err("wrong password must fail");
        assert!(hwpcraft_formats::wrong_password(&wrong), "{ext}: {wrong:#}");
        // A failed open keeps the blank session usable.
        assert_eq!(plain.text(), "");
        plain.execute(Command::Insert("a".into()))?;

        let mut opened = Session::new()?;
        opened.open_with_password(&path, "한글암호1!")?;
        assert_eq!(opened.text(), "비밀 문서 😀", "{ext}");
        assert!(
            opened.has_password(),
            "the password is kept for later saves"
        );

        // Clearing the password saves a plain file again.
        opened.set_password(None)?;
        let plain_path = temp.path().join(format!("plain.{ext}"));
        opened.save(&plain_path)?;
        let mut again = Session::new()?;
        again.open(&plain_path)?;
        assert_eq!(again.text(), "비밀 문서 😀", "{ext}");
    }
    Ok(())
}

#[test]
fn invalid_passwords_and_non_hwp_targets_are_rejected() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    assert!(s.set_password(Some(String::new())).is_err());
    assert!(s.set_password(Some("a\u{7}b".into())).is_err());
    assert!(s.set_password(Some("x".repeat(65))).is_err());
    assert!(!s.has_password());
    assert!(!s.dirty());
    s.set_password(Some("ok".into()))?;
    let temp = tempfile::tempdir()?;
    // PDF/TXT exports ignore the password instead of failing.
    s.execute(Command::Insert("본문".into()))?;
    s.save(&temp.path().join("out.txt"))?;
    assert_eq!(
        std::fs::read_to_string(temp.path().join("out.txt"))?,
        "본문"
    );
    assert!(
        hwpcraft_formats::save_with_password(&s.core, &temp.path().join("x.pdf"), "ok").is_err()
    );
    assert!(hwpcraft_formats::open_with_password(&temp.path().join("out.txt"), "").is_err());
    // Protocol form.
    assert_eq!(s.run("file.password", json!({}))?["protected"], false);
    assert_eq!(
        s.run("file.password", json!({"password": "pw"}))?["protected"],
        true
    );
    assert!(
        s.run("file.password", json!({"password": "", "x": 1}))
            .is_err()
    );
    Ok(())
}

#[test]
fn html_paste_keeps_formatting_and_is_atomic() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞".into()))?;
    s.paste_html("<p><b>굵게</b> 보통</p>")?;
    assert_eq!(s.text(), "앞굵게 보통");
    s.cursor.offset = 1;
    s.anchor = Some(2); // Inspect the first pasted character, not left-affinity insertion shape.
    assert_eq!(s.character_properties()?["bold"], true);
    s.cursor.offset = 5;
    s.anchor = Some(6);
    assert_eq!(s.character_properties()?["bold"], false);
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "앞");

    // Multi-paragraph HTML splits paragraphs.
    s.cursor.offset = 1;
    s.paste_html("<p>첫째</p><p>둘째</p>")?;
    let texts: Vec<String> = s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect();
    assert!(texts.iter().any(|t| t.contains("첫째")) && texts.iter().any(|t| t.contains("둘째")));

    // Empty or oversized HTML is refused without touching the document.
    let before = (s.generation, s.text().to_owned());
    assert!(s.paste_html("   ").is_err());
    assert!(s.paste_html(&"x".repeat(4 * 1024 * 1024 + 1)).is_err());
    assert_eq!((s.generation, s.text().to_owned()), before);

    // paste_with_html prefers HTML from other programs, text otherwise.
    let mut t = Session::new()?;
    t.paste_with_html("굵게", Some("<b>굵게</b>"))?;
    t.cursor.offset = 1;
    assert_eq!(t.character_properties()?["bold"], true);
    t.cursor.offset = t.len();
    t.paste_with_html(" 글", None)?;
    assert_eq!(t.text(), "굵게 글");
    // Our own copy wins over the HTML flavour (internal clipboard keeps objects).
    t.select_all()?;
    let copied = t.copy()?;
    assert!(t.copy_html()?.is_some_and(|h| h.contains("굵게")));
    t.cursor.offset = t.len();
    t.clear_selection();
    t.paste_with_html(&copied, Some("<i>무시</i>"))?;
    assert_eq!(t.text(), "굵게 글굵게 글");
    Ok(())
}
