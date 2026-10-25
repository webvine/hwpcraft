//! Owner: review-tools area. 한자 변환 명령 (crates/engine/src/hanja.rs).
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

#[test]
fn candidates_for_word_at_caret_and_explicit_text() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한자 변환".into()))?;
    s.select(at(0, 2), false)?;
    let v = s.run("tools.hanja.candidates", json!({}))?;
    assert_eq!(v["text"], "한자");
    assert_eq!(v["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(v["items"][0]["source"], "한");
    let first = v["items"][0]["candidates"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(first.iter().any(|c| c["hanja"] == "漢"), "{first:?}");
    assert_eq!(first[0]["ks_x_1001"], true);
    // A hanja source lists its reading and other hanja with the same reading.
    let v = s.run("tools.hanja.candidates", json!({"text":"漢"}))?;
    assert_eq!(v["items"][0]["reading"], "한");
    assert_eq!(v["hangul"], "한");
    assert!(
        v["items"][0]["candidates"]
            .as_array()
            .is_some_and(|c| !c.is_empty() && c.iter().all(|x| x["hanja"] != "漢"))
    );
    // Limits and bad input.
    assert!(
        s.run("tools.hanja.candidates", json!({"text": "가".repeat(65)}))
            .is_err()
    );
    assert!(s.run("tools.hanja.candidates", json!({"bogus":1})).is_err());
    s.select(at(0, 2), false)?;
    s.execute(Command::Insert(" ".into()))?;
    // Caret between two spaces: no word.
    assert!(s.run("tools.hanja.candidates", json!({})).is_err());
    Ok(())
}

#[test]
fn convert_formats_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한자 변환".into()))?;
    s.select(at(0, 2), false)?;
    let v = s.run(
        "tools.hanja.convert",
        json!({"hanja":"漢字","format":"hanja_hangul"}),
    )?;
    assert_eq!(v["replaced"], "한자");
    assert_eq!(v["text"], "漢字(한자)");
    assert_eq!(s.text(), "漢字(한자) 변환");
    assert_eq!(s.cursor.offset, 6);
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "한자 변환");
    // Other formats, applied to an explicit selection.
    for (format, expected) in [
        ("hanja", "漢字 변환"),
        ("hangul_hanja", "한자(漢字) 변환"),
        ("hanja_bracket", "漢字[한자] 변환"),
    ] {
        s.select(at(0, 0), false)?;
        s.select(at(0, 2), true)?;
        s.run(
            "tools.hanja.convert",
            json!({"hanja":"漢字","format":format}),
        )?;
        assert_eq!(s.text(), expected, "{format}");
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "한자 변환");
    }
    // Unchosen syllables may stay Hangul; non-CJK text is rejected.
    s.select(at(0, 2), false)?;
    s.run("tools.hanja.convert", json!({"hanja":"漢자"}))?;
    assert_eq!(s.text(), "漢자 변환");
    s.execute(Command::Undo)?;
    assert!(
        s.run("tools.hanja.convert", json!({"hanja":"abc"}))
            .is_err()
    );
    assert!(
        s.run(
            "tools.hanja.convert",
            json!({"hanja":"漢","format":"bogus"})
        )
        .is_err()
    );
    assert_eq!(s.text(), "한자 변환");
    Ok(())
}

#[test]
fn to_hangul_selection_word_and_document() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("老人과 敬老 比率\n確率 100%".into()))?;
    // Word at the caret.
    s.select(at(0, 1), false)?;
    // The word run includes the particle; only the hanja change.
    let v = s.run("tools.hanja.to_hangul", json!({}))?;
    assert_eq!(v["text"], "노인과");
    assert_eq!(s.entries()[0].1, "노인과 敬老 比率");
    s.execute(Command::Undo)?;
    // Explicit selection keeps non-hanja characters.
    s.select(at(0, 0), false)?;
    s.select(at(0, 6), true)?;
    s.run("tools.hanja.to_hangul", json!({"scope":"selection"}))?;
    assert_eq!(s.entries()[0].1, "노인과 경로 比率");
    s.execute(Command::Undo)?;
    // Whole document, one undo step; 두음 법칙 only at a hanja run start.
    let v = s.run("tools.hanja.to_hangul", json!({"scope":"document"}))?;
    assert_eq!(v["replaced"], 4);
    assert_eq!(s.entries()[0].1, "노인과 경로 비율");
    assert_eq!(s.entries()[1].1, "확률 100%");
    s.execute(Command::Undo)?;
    assert_eq!(s.entries()[0].1, "老人과 敬老 比率");
    assert_eq!(s.entries()[1].1, "確率 100%");
    // Nothing to convert.
    s.select(at(1, 3), false)?;
    s.select(at(1, 7), true)?;
    let v = s.run("tools.hanja.to_hangul", json!({"scope":"selection"}))?;
    assert_eq!(v["replaced"], 0);
    Ok(())
}

#[test]
fn converted_text_survives_hwpx_and_hwp_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한자".into()))?;
    s.run(
        "tools.hanja.convert",
        json!({"hanja":"漢字","format":"hanja_hangul"}),
    )?;
    let temp = tempfile::tempdir()?;
    for name in ["hanja.hwpx", "hanja.hwp"] {
        let path = temp.path().join(name);
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(reopened.entries()[0].1, "漢字(한자)", "{name}");
    }
    Ok(())
}
