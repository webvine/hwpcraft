//! 양식 개체 만들기: every kind inserted at the caret, kept through HWP and HWPX,
//! editable with form.set, refused cleanly, and written with the values Hangul
//! uses for a new object (RHWP's samples/hwpx/form-01.hwpx).
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn list(s: &mut Session) -> anyhow::Result<Vec<Value>> {
    Ok(s.run("form.list", json!({}))?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

/// Kind, name, caption, text and value of each form, in document order.
fn summary(forms: &[Value]) -> Vec<String> {
    forms
        .iter()
        .map(|f| {
            format!(
                "{} {} {} {} {}",
                f["type"], f["name"], f["caption"], f["text"], f["value"]
            )
        })
        .collect()
}

fn insert_all(s: &mut Session) -> anyhow::Result<()> {
    s.execute(Command::Insert("동의 여부: ".into()))?;
    s.run(
        "form.insert",
        json!({"kind": "check_box", "caption": "동의합니다", "checked": true}),
    )?;
    s.execute(Command::Insert("\n이름: ".into()))?;
    s.run("form.insert", json!({"kind": "edit", "text": "홍길동"}))?;
    s.execute(Command::Insert("\n분기: ".into()))?;
    s.run(
        "form.insert",
        json!({"kind": "combo_box", "items": ["1분기", "2분기", "3분기"], "text": "2분기"}),
    )?;
    s.execute(Command::Insert("\n성별: ".into()))?;
    s.run(
        "form.insert",
        json!({"kind": "radio_button", "caption": "남", "group": "성별", "checked": true}),
    )?;
    s.run(
        "form.insert",
        json!({"kind": "radio_button", "caption": "여", "group": "성별"}),
    )?;
    s.execute(Command::Insert("\n".into()))?;
    s.run(
        "form.insert",
        json!({"kind": "push_button", "caption": "제출"}),
    )?;
    Ok(())
}

#[test]
fn every_kind_goes_in_at_the_caret_and_survives_both_formats() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    let blank = s.render_page_svg(0)?;
    insert_all(&mut s)?;
    let forms = list(&mut s)?;
    let got = summary(&forms);
    assert_eq!(
        got,
        [
            r#""CheckBox" "CheckBox1" "동의합니다" "" 1"#,
            r#""Edit" "Edit1" "" "홍길동" 0"#,
            r#""ComboBox" "ComboBox1" "" "2분기" 0"#,
            r#""RadioButton" "RadioButton1" "남" "" 1"#,
            r#""RadioButton" "RadioButton2" "여" "" 0"#,
            r#""PushButton" "PushButton1" "제출" "" 0"#,
        ]
    );
    // The text typed before and after each object stays around it.
    let text = s.run("document.text", json!({}))?;
    assert!(text.to_string().contains("동의 여부:"), "{text}");
    let svg = s.render_page_svg(0)?;
    assert_ne!(svg, blank);
    assert_eq!(svg, s.render_page_svg(0)?, "deterministic");
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("forms.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(summary(&list(&mut r)?), got, "{ext}");
        assert_eq!(r.page_count()?, s.page_count()?, "{ext}");
    }
    // The new objects are ordinary form objects: form.set edits them.
    s.run("form.set", json!({"at": forms[0]["at"], "value": 0}))?;
    s.run("form.set", json!({"at": forms[1]["at"], "text": "김철수"}))?;
    let after = list(&mut s)?;
    assert_eq!(after[0]["value"], 0);
    assert_eq!(after[1]["text"], "김철수");
    // Each insertion is one undo step.
    for _ in 0..2 {
        s.execute(Command::Undo)?;
    }
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(list(&mut s)?.len(), 5);
    // MCP clients pass the `at` that form.list gives to form.get as well, and set a
    // check box with true/false as well as 1/0.
    let forms = list(&mut s)?;
    assert_eq!(forms[0]["type"], "CheckBox");
    let info = s.run("form.get", json!({"at": forms[0]["at"]}))?;
    assert_eq!(info["name"], forms[0]["name"]);
    s.run("form.set", json!({"at": forms[0]["at"], "value": false}))?;
    assert_eq!(list(&mut s)?[0]["value"], 0);
    s.run("form.set", json!({"at": forms[0]["at"], "value": true}))?;
    assert_eq!(list(&mut s)?[0]["value"], 1);
    Ok(())
}

#[test]
fn forms_go_into_table_cells() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("table.create", json!({"rows": 1, "columns": 2}))?;
    let made = s.run("form.insert", json!({"kind": "check_box"}))?;
    assert!(made["at"]["cell"].is_object(), "{made}");
    let forms = list(&mut s)?;
    assert_eq!(forms.len(), 1);
    assert_eq!(forms[0]["caption"], "선택 상자");
    assert_eq!(forms[0]["at"]["cell"], made["at"]["cell"]);
    Ok(())
}

#[test]
fn bad_requests_change_nothing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("form.insert", json!({"kind": "check_box", "name": "동의"}))?;
    let generation = s.generation;
    for params in [
        json!({"kind": "slider"}),
        json!({"kind": "check_box", "items": ["a"]}),
        json!({"kind": "edit", "checked": true}),
        json!({"kind": "combo_box", "items": ["a"], "text": "b"}),
        json!({"kind": "check_box", "name": "동의"}),
        json!({"kind": "check_box", "group": "g"}),
        json!({"kind": "push_button", "text": "글"}),
        json!({"kind": "edit", "caption": "글"}),
        json!({"kind": "check_box", "width_mm": 0.5}),
        json!({"kind": "check_box", "caption": "줄\n바꿈"}),
        json!({"kind": "check_box", "bogus": 1}),
    ] {
        assert!(s.run("form.insert", params.clone()).is_err(), "{params}");
    }
    assert_eq!(s.generation, generation);
    assert_eq!(list(&mut s)?.len(), 1);
    Ok(())
}

/// The property bag of a new object is the one Hangul wrote for a new object of
/// the same kind, apart from what is per document (tab order, border fill id).
#[test]
fn new_objects_use_hangul_values_from_the_sample() -> anyhow::Result<()> {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/hwpx/form-01.hwpx");
    if !sample.exists() {
        return Ok(());
    }
    let mut hangul = Session::new()?;
    hangul.open(&sample)?;
    let mut ours = Session::new()?;
    for kind in [
        "push_button",
        "check_box",
        "combo_box",
        "radio_button",
        "edit",
    ] {
        ours.run("form.insert", json!({"kind": kind}))?;
    }
    fn forms(s: &Session) -> Vec<hwpcraft_doc::model::control::FormObject> {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter_map(|c| match c {
                Control::Form(f) => Some((**f).clone()),
                _ => None,
            })
            .collect()
    }
    let (theirs, mine) = (forms(&hangul), forms(&ours));
    assert_eq!(theirs.len(), 5);
    for (h, m) in theirs.iter().zip(&mine) {
        assert_eq!(h.form_type, m.form_type);
        assert_eq!(
            (h.width, h.height),
            (m.width, m.height),
            "{:?}",
            h.form_type
        );
        assert_eq!(h.back_color, m.back_color, "{:?}", h.form_type);
        assert_eq!(h.caption, m.caption, "{:?}", h.form_type);
        let skip = ["TabOrder", "BorderType"];
        let keys = |f: &hwpcraft_doc::model::control::FormObject| {
            let mut k: Vec<(String, String)> = f
                .properties
                .iter()
                .filter(|(k, _)| !skip.contains(&k.as_str()) && !k.starts_with("listItem"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            k.sort();
            k
        };
        assert_eq!(keys(h), keys(m), "{:?}", h.form_type);
    }
    Ok(())
}
