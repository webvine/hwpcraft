//! Owner: review-tools area. 문서 비교 명령 (crates/engine/src/review.rs).
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
fn compare_reports_changed_added_and_identical() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let original = temp.path().join("original.hwpx");
    let mut s = Session::new()?;
    s.execute(Command::Insert("계약서\n제1조 목적\n제2조 기간".into()))?;
    s.save(&original)?;

    // Identical right after saving.
    let v = s.run("review.compare", json!({"path": original}))?;
    assert_eq!(v["identical"], true);
    assert_eq!(v["total"], 0);

    // Edit the current document: change paragraph 1 and append a paragraph.
    s.select(at(1, 7), false)?;
    s.execute(Command::Insert("(안)".into()))?;
    s.select(at(2, 7), false)?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("제3조 비용".into()))?;
    let v = s.run("review.compare", json!({"path": original}))?;
    assert_eq!(v["identical"], false);
    let findings = v["findings"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<&str> = findings.iter().filter_map(|f| f["kind"].as_str()).collect();
    assert!(kinds.contains(&"textChanged"), "{kinds:?}");
    // A paragraph only in the current document is reported as added here.
    let added = findings
        .iter()
        .find(|f| f["kind"] == "paragraphRemoved")
        .cloned();
    assert!(added.is_some(), "{kinds:?}");
    let added = added.unwrap_or_default();
    assert_eq!(added["label"], "문단 추가 (현재 문서에만 있음)");
    assert_eq!(added["cursor"]["paragraph"], 3);
    let changed = findings
        .iter()
        .find(|f| f["kind"] == "textChanged")
        .cloned()
        .unwrap_or_default();
    assert_eq!(changed["section"], 0);
    assert_eq!(changed["paragraph"], 1);
    assert_eq!(changed["cursor"]["paragraph"], 1);
    assert!(v["summary"].as_array().is_some_and(|s| !s.is_empty()));
    // The comparison never edits the document.
    assert!(!s.can_undo() || s.entries().len() == 4);
    assert_eq!(s.entries()[1].1, "제1조 목적(안)");
    Ok(())
}

#[test]
fn compare_rejects_missing_file_and_bad_params() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let temp = tempfile::tempdir()?;
    let missing = temp.path().join("none.hwpx");
    let err = s
        .run("review.compare", json!({"path": missing}))
        .unwrap_err();
    assert!(err.to_string().contains("비교할 파일이 없습니다"), "{err}");
    assert!(s.run("review.compare", json!({})).is_err());
    assert!(
        s.run("review.compare", json!({"path": "a.hwpx", "bogus": true}))
            .is_err()
    );
    // A non-document file fails to open without touching the current document.
    let text = temp.path().join("note.txt");
    std::fs::write(&text, "hello")?;
    assert!(s.run("review.compare", json!({"path": text})).is_err());
    assert_eq!(s.text(), "본문");
    Ok(())
}

#[test]
fn compare_ignores_whitespace_when_asked() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let original = temp.path().join("spaces.hwpx");
    let mut s = Session::new()?;
    s.execute(Command::Insert("가 나 다".into()))?;
    s.save(&original)?;
    s.select(at(0, 1), false)?;
    s.execute(Command::Insert("  ".into()))?;
    let strict = s.run("review.compare", json!({"path": original}))?;
    assert_eq!(strict["identical"], false);
    let loose = s.run(
        "review.compare",
        json!({"path": original, "ignore_whitespace": true}),
    )?;
    assert_eq!(loose["identical"], true);
    Ok(())
}
