//! 양식 개체 속성: form.properties.get reads and form.properties changes what Hangul's
//! 양식 개체 속성 dialog edits, per kind, kept through HWP and HWPX, one undo step,
//! refused without changes; form.insert takes the same properties.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn props(s: &mut Session, at: &Value) -> anyhow::Result<Value> {
    s.run("form.properties.get", json!({"at": at}))
}

fn first_at(s: &mut Session) -> anyhow::Result<Value> {
    Ok(s.run("form.list", json!({}))?[0]["at"].clone())
}

/// The properties of each kind changed to something other than the default.
fn changes(kind: &str) -> Value {
    let mut v = json!({
        "name": format!("{kind}_속성"),
        "enabled": false,
        "printable": false,
        "frame": false,
        "width_mm": 41.5,
        "height_mm": 9.25,
        "fore_color": "#1f4e79",
        "back_color": "#fff2cc",
        "font_face": "맑은 고딕",
        "font_size": 12.5,
    });
    match kind {
        "check_box" => {
            v["caption"] = json!("동의합니다");
            v["checked"] = json!(true);
        }
        "radio_button" => {
            v["caption"] = json!("예");
            v["checked"] = json!(true);
            v["group"] = json!("응답");
        }
        "push_button" => v["caption"] = json!("제출"),
        "combo_box" => {
            v["items"] = json!(["봄", "여름", "가을"]);
            v["text"] = json!("여름");
            v["edit_enable"] = json!(false);
        }
        _ => {
            v["text"] = json!("홍길동");
            v["read_only"] = json!(true);
            v["multi_line"] = json!(true);
            v["number_only"] = json!(true);
            v["max_length"] = json!(20);
            v["password_char"] = json!("*");
        }
    }
    v
}

const KINDS: [&str; 5] = [
    "check_box",
    "edit",
    "combo_box",
    "radio_button",
    "push_button",
];

#[test]
fn every_property_of_every_kind_survives_hwp_and_hwpx_and_undoes_in_one_step() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    for kind in KINDS {
        let mut s = Session::new()?;
        s.run("form.insert", json!({"kind": kind}))?;
        let at = first_at(&mut s)?;
        let before = props(&mut s, &at)?;
        let svg_before = s.render_page_svg(0)?;
        let mut params = changes(kind);
        params["at"] = at.clone();
        let answered = s.run("form.properties", params.clone())?;
        let after = props(&mut s, &at)?;
        assert_eq!(answered, after, "{kind}: answers the new properties");
        for (key, want) in changes(kind).as_object().into_iter().flatten() {
            assert_eq!(&after[key], want, "{kind}.{key}: {after}");
        }
        // The look changes on the page: font, colours, size and the frame.
        let svg = s.render_page_svg(0)?;
        assert_ne!(svg, svg_before, "{kind}");
        assert!(svg.contains("맑은 고딕"), "{kind}: font drawn");
        // RHWP paints the background of boxes and buttons; a check box or radio
        // button keeps its colour in the file only.
        if !matches!(kind, "check_box" | "radio_button") {
            assert!(
                svg.to_lowercase().contains("#fff2cc"),
                "{kind}: back colour"
            );
        }
        for ext in ["hwpx", "hwp"] {
            let path = dir.path().join(format!("{kind}.{ext}"));
            let combo_hwp = ext == "hwp" && kind == "combo_box";
            if combo_hwp {
                // HWP has no field for combo box items (Hangul keeps them in the
                // document script): refused unless asked to save without them.
                assert!(s.save(&path).is_err());
                assert_eq!(s.save_dropping_combo_items(&path, false)?, 1);
            } else {
                s.save(&path)?;
            }
            let mut r = Session::new()?;
            r.open(&path)?;
            let r_at = first_at(&mut r)?;
            let mut reopened = props(&mut r, &r_at)?;
            let mut want = after.clone();
            if combo_hwp {
                assert_eq!(reopened["items"], json!([]));
                reopened["items"] = want["items"].clone();
            }
            want["at"] = reopened["at"].clone();
            assert_eq!(reopened, want, "{kind}.{ext}");
        }
        s.execute(Command::Undo)?;
        assert_eq!(props(&mut s, &at)?, before, "{kind}: one undo step");
        s.execute(Command::Redo)?;
        let mut redone = after.clone();
        if kind == "combo_box" {
            // Saving without the items took them out of the document as well.
            redone["items"] = json!([]);
        }
        assert_eq!(props(&mut s, &at)?, redone, "{kind}: redo");
    }
    Ok(())
}

#[test]
fn get_answers_what_set_takes() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for kind in KINDS {
        s.run("form.insert", json!({"kind": kind}))?;
    }
    let forms = s.run("form.list", json!({}))?;
    for f in forms.as_array().into_iter().flatten() {
        let got = props(&mut s, &f["at"])?;
        let generation = s.generation;
        // Passing the answer back unchanged is accepted and changes nothing visible.
        s.run("form.properties", got.clone())?;
        assert_eq!(props(&mut s, &f["at"])?, got);
        assert_eq!(s.generation, generation + 1);
    }
    Ok(())
}

#[test]
fn insert_takes_the_properties_in_one_step() -> anyhow::Result<()> {
    for kind in KINDS {
        let mut s = Session::new()?;
        let mut params = changes(kind);
        params["kind"] = json!(kind);
        let generation = s.generation;
        let made = s.run("form.insert", params)?;
        assert_eq!(s.generation, generation + 1, "{kind}: one step");
        let got = props(&mut s, &made["at"])?;
        for (key, want) in changes(kind).as_object().into_iter().flatten() {
            assert_eq!(&got[key], want, "{kind}.{key}: {got}");
        }
        s.execute(Command::Undo)?;
        assert_eq!(s.run("form.list", json!({}))?, json!([]), "{kind}");
    }
    Ok(())
}

#[test]
fn checking_a_radio_button_clears_the_rest_of_its_group() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for (caption, group, checked) in [
        ("남", "성별", true),
        ("여", "성별", false),
        ("예", "응답", true),
    ] {
        s.run(
            "form.insert",
            json!({"kind": "radio_button", "caption": caption, "group": group, "checked": checked}),
        )?;
    }
    let values = |s: &mut Session| -> anyhow::Result<Vec<i64>> {
        Ok(s.run("form.list", json!({}))?
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| f["value"].as_i64().unwrap_or(-1))
            .collect())
    };
    assert_eq!(values(&mut s)?, [1, 0, 1]);
    let forms = s.run("form.list", json!({}))?;
    s.run(
        "form.properties",
        json!({"at": forms[1]["at"], "checked": true}),
    )?;
    assert_eq!(values(&mut s)?, [0, 1, 1], "only the 성별 group changes");
    // Moving a checked button into a group clears that group's checked one.
    s.run(
        "form.properties",
        json!({"at": forms[2]["at"], "group": "성별"}),
    )?;
    assert_eq!(values(&mut s)?, [0, 0, 1]);
    s.execute(Command::Undo)?;
    assert_eq!(values(&mut s)?, [0, 1, 1]);
    Ok(())
}

#[test]
fn bad_properties_change_nothing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("form.insert", json!({"kind": "edit", "name": "이름"}))?;
    s.run("form.insert", json!({"kind": "check_box"}))?;
    let forms = s.run("form.list", json!({}))?;
    let (edit, check) = (forms[0]["at"].clone(), forms[1]["at"].clone());
    let before = (props(&mut s, &edit)?, props(&mut s, &check)?);
    let generation = s.generation;
    for (at, extra) in [
        (&check, json!({"name": "이름"})),
        (&check, json!({"name": " "})),
        (&check, json!({"text": "글"})),
        (&check, json!({"items": ["a"]})),
        (&check, json!({"group": "g"})),
        (&check, json!({"read_only": true})),
        (&check, json!({"edit_enable": true})),
        (&check, json!({"kind": "edit"})),
        (&check, json!({"width_mm": 2.0})),
        (&check, json!({"height_mm": 201.0})),
        (&check, json!({"fore_color": "red"})),
        (&check, json!({"back_color": "#12345"})),
        (&check, json!({"font_size": 0.5})),
        (&check, json!({"font_size": 5000.0})),
        (&check, json!({"font_face": ""})),
        (&check, json!({"caption": "줄\n바꿈"})),
        (&check, json!({"bogus": 1})),
        (&edit, json!({"caption": "글"})),
        (&edit, json!({"checked": true})),
        (&edit, json!({"max_length": 0})),
        (&edit, json!({"password_char": "**"})),
        // One bad key refuses the good ones beside it.
        (&edit, json!({"font_face": "굴림", "max_length": 0})),
    ] {
        let mut params = extra.clone();
        params["at"] = at.clone();
        assert!(s.run("form.properties", params).is_err(), "{extra}");
    }
    assert!(
        s.run(
            "form.properties",
            json!({"at": {"section": 0, "paragraph": 0, "control": 9}, "enabled": false})
        )
        .is_err()
    );
    assert!(s.run("form.properties", json!({"enabled": false})).is_err());
    assert_eq!(s.generation, generation);
    assert_eq!((props(&mut s, &edit)?, props(&mut s, &check)?), before);
    for params in [
        json!({"kind": "check_box", "read_only": true}),
        json!({"kind": "edit", "font_size": 0}),
        json!({"kind": "combo_box", "edit_enable": "yes"}),
    ] {
        assert!(s.run("form.insert", params.clone()).is_err(), "{params}");
    }
    assert_eq!(s.generation, generation);
    Ok(())
}

/// form.get takes the `at` form.insert answers (with "cell": null) and objects in cells.
#[test]
fn form_get_takes_the_inserted_at_and_cell_objects() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let made = s.run("form.insert", json!({"kind": "edit", "text": "본문"}))?;
    assert!(made["at"].get("cell").is_some(), "{made}");
    let info = s.run("form.get", json!({"at": made["at"]}))?;
    assert_eq!(info["text"], "본문");
    s.execute(Command::Insert("\n".into()))?;
    s.run("table.create", json!({"rows": 1, "columns": 2}))?;
    let made = s.run(
        "form.insert",
        json!({"kind": "combo_box", "items": ["가", "나"], "text": "나"}),
    )?;
    assert!(made["at"]["cell"].is_object(), "{made}");
    let info = s.run("form.get", json!({"at": made["at"]}))?;
    assert_eq!(info["formType"], "ComboBox");
    assert_eq!(info["text"], "나");
    assert_eq!(info["items"], json!(["가", "나"]));
    // Properties of an object in a cell change like those in the body.
    s.run(
        "form.properties",
        json!({"at": made["at"], "back_color": "#ddeeff", "width_mm": 30.0}),
    )?;
    let got = props(&mut s, &made["at"])?;
    assert_eq!(got["back_color"], "#ddeeff");
    assert_eq!(got["width_mm"], 30.0);
    Ok(())
}

fn items_after_reopen(path: &std::path::Path) -> anyhow::Result<Vec<Value>> {
    let mut r = Session::new()?;
    r.open(path)?;
    let forms = r.run("form.list", json!({}))?;
    let mut out = Vec::new();
    for f in forms.as_array().into_iter().flatten() {
        if f["type"] == "ComboBox" {
            out.push(props(&mut r, &f["at"])?["items"].clone());
        }
    }
    Ok(out)
}

/// HWP has no field for combo box items (Hangul keeps them in the document script),
/// so a save that would lose them is refused without touching the file, and the
/// explicit save drops them from the file and the document alike.
#[test]
fn an_hwp_save_refuses_to_lose_combo_items() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.run(
        "form.insert",
        json!({"kind": "combo_box", "items": ["봄", "여름"], "text": "여름"}),
    )?;
    s.execute(Command::Insert(" ".into()))?;
    s.run("form.insert", json!({"kind": "combo_box"}))?;
    let at = s.run("form.list", json!({}))?[1]["at"].clone();
    // Items given later with form.properties count too.
    s.run("form.properties", json!({"at": at, "items": ["가", "나"]}))?;
    let hwp = dir.path().join("forms.hwp");
    std::fs::write(&hwp, b"existing")?;
    assert_eq!(s.combo_items_blocking_save(&hwp), 2);
    assert_eq!(
        s.combo_items_blocking_save(&dir.path().join("forms.hwpx")),
        0
    );
    let refused = s.save(&hwp).expect_err("refused");
    assert!(refused.to_string().contains("콤보 상자 2개"), "{refused}");
    assert_eq!(std::fs::read(&hwp)?, b"existing", "the file is untouched");
    assert!(s.dirty());
    // Page-range save of the whole document refuses the same way.
    let pages = s.page_count()?;
    assert!(
        s.run(
            "file.save_pages",
            json!({"path": hwp, "from": 1, "to": pages})
        )
        .is_err()
    );
    // HWPX keeps them.
    let hwpx = dir.path().join("forms.hwpx");
    s.save(&hwpx)?;
    assert_eq!(
        items_after_reopen(&hwpx)?,
        [json!(["봄", "여름"]), json!(["가", "나"])]
    );
    // Asked for: saved without the items, in the file and the document.
    assert_eq!(s.save_dropping_combo_items(&hwp, false)?, 2);
    assert_eq!(items_after_reopen(&hwp)?, [json!([]), json!([])]);
    let forms = s.run("form.list", json!({}))?;
    assert_eq!(forms[0]["text"], "여름", "the chosen value stays");
    assert_eq!(props(&mut s, &forms[0]["at"])?["items"], json!([]));
    assert!(!s.dirty());
    assert_eq!(s.combo_items_blocking_save(&hwp), 0);
    s.save(&hwp)?;
    Ok(())
}

/// Items the document script gives (Hangul's HWP, or HWPX with its scripts) survive an
/// HWP save and are not refused; changing them is.
#[test]
fn items_the_document_script_gives_are_kept() -> anyhow::Result<()> {
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../rhwp/samples");
    let (hwp_sample, hwpx_sample) = (
        samples.join("form-01.hwp"),
        samples.join("hwpx/form-01.hwpx"),
    );
    if !hwp_sample.exists() || !hwpx_sample.exists() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let seasons = json!(["봄", "여름", "가을", "겨울"]);
    for sample in [&hwp_sample, &hwpx_sample] {
        let mut s = Session::new()?;
        s.open(sample)?;
        let out = dir.path().join("again.hwp");
        assert_eq!(s.combo_items_blocking_save(&out), 0, "{}", sample.display());
        s.save(&out)?;
        assert_eq!(
            items_after_reopen(&out)?,
            std::slice::from_ref(&seasons),
            "{}",
            sample.display()
        );
        // A list the script does not give is refused.
        let combo = s
            .run("form.list", json!({}))?
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["type"] == "ComboBox")
            .map(|f| f["at"].clone())
            .expect("combo box");
        s.run(
            "form.properties",
            json!({"at": combo, "items": ["봄", "여름"]}),
        )?;
        assert_eq!(s.combo_items_blocking_save(&out), 1, "{}", sample.display());
        assert!(s.save(&out).is_err());
        // Renaming it leaves the script's lines behind as well.
        s.execute(Command::Undo)?;
        s.run("form.properties", json!({"at": combo, "name": "계절"}))?;
        let renamed = s.combo_items_blocking_save(&out);
        // Hangul's HWP gives the items from the script only (none in the model).
        let expected = usize::from(sample == &hwpx_sample);
        assert_eq!(renamed, expected, "{}", sample.display());
    }
    Ok(())
}
