//! Owner: review-tools area. 상용구 명령 (crates/engine/src/autotext.rs).
//! The store lives in one temp folder per test binary, fixed with `set_config_dir`.
use hwpcraft_engine::{Command, Cursor, Session, set_config_dir};
use serde_json::json;
use std::sync::{Mutex, OnceLock};

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

/// One store per test binary; tests use distinct names and run under a lock.
fn store() -> std::sync::MutexGuard<'static, tempfile::TempDir> {
    static DIR: OnceLock<Mutex<tempfile::TempDir>> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        assert!(set_config_dir(dir.path().to_path_buf()));
        Mutex::new(dir)
    });
    dir.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn add_list_insert_delete_roundtrip() -> anyhow::Result<()> {
    let guard = store();
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let v = s.run(
        "tools.autotext.add",
        json!({"name":"인사말","abbreviation":"인사","text":"안녕하십니까.\n감사합니다."}),
    )?;
    assert!(
        v["entries"]
            .as_array()
            .is_some_and(|e| e.iter().any(|x| x["name"] == "인사말"))
    );
    assert!(guard.path().join("autotext.json").is_file());
    // Duplicate name needs overwrite; duplicate abbreviation is rejected.
    assert!(
        s.run("tools.autotext.add", json!({"name":"인사말","text":"x"}))
            .is_err()
    );
    assert!(
        s.run(
            "tools.autotext.add",
            json!({"name":"다른","abbreviation":"인사","text":"x"})
        )
        .is_err()
    );
    s.run(
        "tools.autotext.add",
        json!({"name":"인사말","abbreviation":"인사","text":"안녕하십니까.","overwrite":true}),
    )?;
    // Insert at the caret: the text replaces nothing and keeps the caret after it.
    s.select(at(0, 2), false)?;
    let v = s.run("tools.autotext.insert", json!({"name":"인사말"}))?;
    assert_eq!(v["inserted"], "안녕하십니까.");
    assert_eq!(s.text(), "본문안녕하십니까.");
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "본문");
    // A fresh session sees the same store.
    let mut other = Session::new()?;
    let list = other.run("tools.autotext.list", json!({}))?;
    assert!(
        list["entries"]
            .as_array()
            .is_some_and(|e| e.iter().any(|x| x["name"] == "인사말"))
    );
    s.run("tools.autotext.delete", json!({"name":"인사말"}))?;
    assert!(
        s.run("tools.autotext.delete", json!({"name":"인사말"}))
            .is_err()
    );
    assert!(
        s.run("tools.autotext.insert", json!({"name":"인사말"}))
            .is_err()
    );
    drop(guard);
    Ok(())
}

#[test]
fn add_from_selection_and_expand_abbreviation() -> anyhow::Result<()> {
    let guard = store();
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "귀사의 무궁한 발전을 기원합니다. 끝".into(),
    ))?;
    s.select(at(0, 0), false)?;
    s.select(at(0, 18), true)?;
    s.run(
        "tools.autotext.add",
        json!({"name":"기원문","abbreviation":"기원"}),
    )?;
    let list = s.run("tools.autotext.list", json!({}))?;
    let entry = list["entries"]
        .as_array()
        .and_then(|e| e.iter().find(|x| x["name"] == "기원문").cloned())
        .unwrap_or_default();
    assert_eq!(entry["text"], "귀사의 무궁한 발전을 기원합니다.");
    // Alt+I: the abbreviation before the caret becomes the text, in one undo step.
    s.select(at(0, 20), false)?;
    s.execute(Command::Insert(" 기원".into()))?;
    let v = s.run("tools.autotext.expand", json!({}))?;
    assert_eq!(v["expanded"], "기원");
    assert_eq!(
        s.text(),
        "귀사의 무궁한 발전을 기원합니다. 끝 귀사의 무궁한 발전을 기원합니다."
    );
    s.execute(Command::Undo)?;
    assert_eq!(s.text(), "귀사의 무궁한 발전을 기원합니다. 끝 기원");
    // Unknown word, no word, and a selection are errors that leave the text alone.
    s.execute(Command::Insert("x".into()))?;
    assert!(s.run("tools.autotext.expand", json!({})).is_err());
    s.select(at(0, 0), false)?;
    assert!(s.run("tools.autotext.expand", json!({})).is_err());
    s.select(at(0, 3), true)?;
    assert!(s.run("tools.autotext.expand", json!({})).is_err());
    assert_eq!(s.text(), "귀사의 무궁한 발전을 기원합니다. 끝 기원x");
    s.run("tools.autotext.delete", json!({"name":"기원문"}))?;
    drop(guard);
    Ok(())
}

#[test]
fn bad_parameters_and_empty_selection_are_rejected() -> anyhow::Result<()> {
    let guard = store();
    let mut s = Session::new()?;
    assert!(
        s.run("tools.autotext.add", json!({"name":"","text":"x"}))
            .is_err()
    );
    assert!(
        s.run(
            "tools.autotext.add",
            json!({"name":"a","text":"x","bogus":1})
        )
        .is_err()
    );
    assert!(
        s.run(
            "tools.autotext.add",
            json!({"name":"a","abbreviation":"a b","text":"x"})
        )
        .is_err()
    );
    // No selection and no text.
    assert!(s.run("tools.autotext.add", json!({"name":"빈것"})).is_err());
    assert!(s.run("tools.autotext.list", json!({"x":1})).is_err());
    assert!(s.run("tools.autotext.insert", json!({})).is_err());
    drop(guard);
    Ok(())
}

#[test]
fn inserted_autotext_survives_hwpx_roundtrip() -> anyhow::Result<()> {
    let guard = store();
    let mut s = Session::new()?;
    s.run(
        "tools.autotext.add",
        json!({"name":"두줄","text":"첫째 줄\n둘째 줄"}),
    )?;
    s.run("tools.autotext.insert", json!({"name":"두줄"}))?;
    assert_eq!(s.entries().len(), 2);
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("autotext.hwpx");
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    assert_eq!(r.entries()[0].1, "첫째 줄");
    assert_eq!(r.entries()[1].1, "둘째 줄");
    s.run("tools.autotext.delete", json!({"name":"두줄"}))?;
    drop(guard);
    Ok(())
}
