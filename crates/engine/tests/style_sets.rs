//! 스타일마당: a set restyles the document in one undo step and survives saves; user
//! sets are saved from a document and removed again.
use hwpcraft_engine::{Session, set_config_dir};
use serde_json::{Value, json};

fn style(s: &mut Session, name: &str) -> anyhow::Result<Value> {
    let exported = s.run("format.style.export", json!({}))?;
    exported["styles"]
        .as_array()
        .and_then(|a| a.iter().find(|st| st["name"] == name).cloned())
        .ok_or_else(|| anyhow::anyhow!("no style {name}"))
}

#[test]
fn a_style_set_applies_in_one_step_and_user_sets_come_and_go() -> anyhow::Result<()> {
    let config = tempfile::tempdir()?;
    assert!(set_config_dir(config.path().to_path_buf()));
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "제목\n본문"}))?;
    let listed = s.run("format.style.set.list", json!({}))?;
    let names: Vec<_> = listed["sets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["name"].as_str())
        .collect();
    assert_eq!(names, ["보고서", "공문", "간결"], "{listed}");

    let before = s.run("format.style.export", json!({}))?;
    let fonts = style(&mut s, "개요 1")?["character"]["fontFamilies"].clone();
    s.run("format.style.set.apply", json!({"name": "보고서"}))?;
    let h1 = style(&mut s, "개요 1")?;
    assert_eq!(h1["character"]["fontSize"], 1600, "{h1}");
    assert_eq!(h1["character"]["bold"], true, "{h1}");
    assert_eq!(h1["paragraph"]["spacingBefore"], 2400, "{h1}");
    assert_eq!(h1["paragraph"]["keepWithNext"], true, "{h1}");
    // The set keeps the document's fonts.
    assert_eq!(h1["character"]["fontFamilies"], fonts);
    assert_eq!(style(&mut s, "본문")?["paragraph"]["lineSpacing"], 170);

    let dir = tempfile::tempdir()?;
    for name in ["set.hwp", "set.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        let h1 = style(&mut back, "개요 1")?;
        assert_eq!(h1["character"]["fontSize"], 1600, "{name}: {h1}");
        assert_eq!(h1["paragraph"]["spacingBefore"], 2400, "{name}: {h1}");
    }

    // A user set from this document, applied to a fresh one.
    s.run("format.style.set.save", json!({"name": "우리 팀"}))?;
    s.run("edit.undo", json!({}))?;
    assert_eq!(s.run("format.style.export", json!({}))?, before);
    let mut fresh = Session::new()?;
    fresh.run("format.style.set.apply", json!({"name": "우리 팀"}))?;
    assert_eq!(style(&mut fresh, "개요 1")?["character"]["fontSize"], 1600);

    // Built-in sets are fixed; bad names and unknown sets fail without effect.
    assert!(
        s.run("format.style.set.save", json!({"name": "보고서"}))
            .is_err()
    );
    assert!(
        s.run("format.style.set.delete", json!({"name": "간결"}))
            .is_err()
    );
    assert!(
        s.run("format.style.set.save", json!({"name": "a/b"}))
            .is_err()
    );
    assert!(
        s.run("format.style.set.apply", json!({"name": "없음"}))
            .is_err()
    );
    assert_eq!(s.run("format.style.export", json!({}))?, before);
    let left = s.run("format.style.set.delete", json!({"name": "우리 팀"}))?;
    assert_eq!(left["sets"].as_array().map(Vec::len), Some(3), "{left}");
    assert!(
        s.run("format.style.set.delete", json!({"name": "우리 팀"}))
            .is_err()
    );
    Ok(())
}
