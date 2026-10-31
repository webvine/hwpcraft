//! Owner: shell/page area. 문서마당 (file.template.*), in a settings folder of its own.
use hwpcraft_engine::{Command, Session, set_config_dir};
use serde_json::json;

#[test]
fn templates_are_saved_listed_used_and_deleted() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(set_config_dir(dir.path().to_path_buf()));
    let mut s = Session::new()?;
    assert_eq!(
        s.run("file.template.list", json!({}))?["templates"],
        json!([])
    );
    s.execute(Command::Insert("주간 보고 양식".into()))?;
    let own = dir.path().join("own.hwpx");
    s.save(&own)?;
    let saved = s.run("file.template.save", json!({"name": "주간 보고"}))?;
    assert!(
        saved["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("주간 보고.hwpx"))
    );
    assert_eq!(
        s.path.as_deref(),
        Some(own.as_path()),
        "the document keeps its file"
    );
    let list = s.run("file.template.list", json!({}))?;
    assert_eq!(list["templates"][0]["name"], "주간 보고");
    // A new document from the template: same content, no file yet.
    let mut t = Session::new()?;
    t.run("file.template.new", json!({"name": "주간 보고"}))?;
    assert!(t.text().contains("주간 보고 양식"));
    assert_eq!(t.path, None);
    assert!(!t.dirty());
    for bad in ["", "../x", "a/b", "a:b", ".hidden"] {
        assert!(
            s.run("file.template.save", json!({"name": bad})).is_err(),
            "{bad:?}"
        );
    }
    assert!(
        s.run("file.template.new", json!({"name": "없는 서식"}))
            .is_err()
    );
    // Unsaved changes are given up only with force, as with file.new.
    s.execute(Command::Insert(" 고침".into()))?;
    assert!(
        s.run("file.template.new", json!({"name": "주간 보고"}))
            .is_err()
    );
    assert!(s.dirty() && s.text().contains("고침"));
    s.run(
        "file.template.new",
        json!({"name": "주간 보고", "force": true}),
    )?;
    assert!(!s.dirty() && s.path.is_none());
    s.run("file.template.delete", json!({"name": "주간 보고"}))?;
    assert_eq!(
        s.run("file.template.list", json!({}))?["templates"],
        json!([])
    );
    assert!(
        s.run("file.template.delete", json!({"name": "주간 보고"}))
            .is_err()
    );
    Ok(())
}
