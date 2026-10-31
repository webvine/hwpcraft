//! Owner: format area. A control put into a paragraph's text (덧말, 글자 겹치기) moves
//! the controls after it up by one, and so must every record that names a control by
//! its index: CTRL_DATA records and field ranges. Otherwise a 누름틀 after it points at
//! the new control: `field.list` comes back empty and an HWP save drops the field's
//! value (REVIEW-RHWP-EDIT-0.19 H3).
use hwpcraft_engine::Session;
use serde_json::{Value, json};

/// "기준글 [누름틀 이름 = 홍길동] 뒤", saved as HWP and opened again, so the field's
/// records are the ones read from the file.
fn opened(dir: &std::path::Path) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "기준글 "}))?;
    s.run(
        "insert.field",
        json!({"guide": "이름을 입력하세요", "name": "이름"}),
    )?;
    s.run("field.set", json!({"name": "이름", "value": "홍길동"}))?;
    s.run("caret.move", json!({"motion": "line_end"}))?;
    s.run("text.insert", json!({"text": " 뒤"}))?;
    let path = dir.join("field.hwp");
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    Ok(back)
}

fn fields(s: &mut Session) -> anyhow::Result<Vec<(String, String)>> {
    let list = s.run("field.list", json!({}))?;
    let items = list
        .as_array()
        .or_else(|| list["fields"].as_array())
        .cloned()
        .unwrap_or_default();
    Ok(items
        .iter()
        .map(|f: &Value| {
            (
                f["name"].as_str().unwrap_or_default().to_string(),
                f["value"]
                    .as_str()
                    .or_else(|| f["text"].as_str())
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect())
}

fn check(
    dir: &std::path::Path,
    edit: impl Fn(&mut Session) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut s = opened(dir)?;
    let expected = vec![("이름".to_string(), "홍길동".to_string())];
    assert_eq!(fields(&mut s)?, expected, "before the edit");
    edit(&mut s)?;
    assert_eq!(fields(&mut s)?, expected, "after the edit");
    for name in ["again.hwp", "again.hwpx"] {
        let path = dir.join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(fields(&mut back)?, expected, "{name} reopened");
        assert!(back.text().contains("홍길동"), "{name}: {}", back.text());
    }
    Ok(())
}

#[test]
fn a_ruby_before_a_field_keeps_the_field() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    check(dir.path(), |s| {
        s.run("select.text", json!({"text": "기준글"}))?;
        s.run("format.ruby", json!({"text": "덧말"}))?;
        Ok(())
    })
}

#[test]
fn a_character_overlap_before_a_field_keeps_the_field() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    check(dir.path(), |s| {
        s.run("caret.move", json!({"motion": "line_start"}))?;
        s.run("format.char_overlap", json!({"chars": "가"}))?;
        Ok(())
    })
}

#[test]
fn removing_a_ruby_or_an_overlap_before_a_field_keeps_the_field() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    check(dir.path(), |s| {
        s.run("select.text", json!({"text": "기준글"}))?;
        let made = s.run("format.ruby", json!({"text": "덧말"}))?;
        s.run("format.ruby.delete", json!({"control": made["control"]}))?;
        s.run("caret.move", json!({"motion": "line_start"}))?;
        let made = s.run("format.char_overlap", json!({"chars": "가"}))?;
        s.run(
            "format.char_overlap.delete",
            json!({"control": made["control"]}),
        )?;
        Ok(())
    })
}

#[test]
fn deleting_marks_before_a_field_keeps_the_field() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    check(dir.path(), |s| {
        s.run("caret.move", json!({"motion": "line_start"}))?;
        s.run(
            "insert.equation",
            json!({"script": "a over b", "size_pt": 12}),
        )?;
        let r = s.run("edit.delete_ctrl_marks", json!({"kinds": ["equation"]}))?;
        anyhow::ensure!(r["deleted"] == 1, "{r}");
        Ok(())
    })
}
