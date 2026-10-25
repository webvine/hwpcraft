use hwpcraft_engine::{Command, Session, hwp_parameter_schema};
use serde_json::json;

#[test]
fn native_character_fields_preserve_other_language_slots_and_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한글 Latin 😀".into()))?;
    s.select_all()?;
    let before = s.character_properties()?;
    s.execute_hwp_action("CharShape",json!({}),Some(json!({"Height":1600,"Bold":1,"TextColor":0x563412,"FaceNameLatin":"Arial","RatioHangul":120,"SpacingHangul":-5,"SizeHanja":95,"OffsetJapanese":10,"UnderlineType":3})))?;
    let p = s.character_properties()?;
    assert_eq!(p["fontSize"], 1600);
    assert_eq!(p["bold"], true);
    assert_eq!(p["textColor"], "#123456");
    assert_eq!(p["fontFamilies"][1], "Arial");
    assert_eq!(p["fontFamilies"][0], before["fontFamilies"][0]);
    assert_eq!(p["ratios"][0], 120);
    assert_eq!(p["ratios"][1], before["ratios"][1]);
    assert_eq!(p["spacings"][0], -5);
    assert_eq!(p["relativeSizes"][2], 95);
    assert_eq!(p["charOffsets"][3], 10);
    assert_eq!(p["underlineType"], "Top");
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("native.{extension}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let p = r.character_properties()?;
        assert_eq!(p["fontSize"], 1600);
        assert_eq!(p["textColor"], "#123456");
        assert_eq!(p["fontFamilies"][1], "Arial");
    }
    s.execute(Command::Undo)?;
    assert_eq!(s.character_properties()?["fontSize"], before["fontSize"]);
    Ok(())
}

#[test]
fn native_para_units_match_stored_values_and_table_arrays_are_converted() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("문단".into()))?;
    s.execute_hwp_action("ParagraphShape",json!({}),Some(json!({"LeftMargin":1440,"RightMargin":720,"Indentation":-720,"PrevSpacing":1440,"NextSpacing":720,"LineSpacingType":1,"LineSpacing":2400,"AlignType":3,"KeepWithNext":1})))?;
    let p = s.paragraph_format()?;
    for (key, value) in [
        ("marginLeft", 1440),
        ("marginRight", 720),
        ("indent", -720),
        ("spacingBefore", 1440),
        ("spacingAfter", 720),
        ("lineSpacing", 2400),
    ] {
        assert_eq!(p[key], value, "{key}");
    }
    assert_eq!(p["alignment"], "center");
    assert_eq!(p["lineSpacingType"], "Fixed");
    assert_eq!(p["keepWithNext"], true);
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("para.{extension}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(r.paragraph_format()?["spacingBefore"], 1440);
    }
    let p = s.prepare_hwp_action(
        "TableCreate",
        json!({}),
        Some(json!({"Rows":2,"Cols":2,"ColWidth":[7200,3600]})),
    )?;
    assert_eq!(p.params["column_widths_mm"], json!([25.4, 12.7]));
    s.execute_hwp_action(
        "TableCreate",
        json!({}),
        Some(json!({"Rows":2,"Cols":2,"ColWidth":[7200,3600]})),
    )?;
    let info = s.run("table.info", json!({}))?;
    assert_eq!(info["cells"].as_array().unwrap().len(), 4);
    Ok(())
}

#[test]
fn malformed_native_parameters_are_rejected_without_any_edit() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("보존".into()))?;
    s.select_all()?;
    let generation = s.generation;
    let before = s.character_properties()?;
    for (action, fields) in [
        ("CharShape", json!({"Bold":2})),
        ("CharShape", json!({"Height":1600,"NotAField":1})),
        ("CharShape", json!({"RatioLatin":1})),
        ("CharShape", json!({"TextColor":-1})),
        ("CharShape", json!({"UnderlineType":2})),
        ("CharShape", json!({"FaceNameLatin":""})),
        ("ParagraphShape", json!({"LeftMargin":201})),
        ("ParagraphShape", json!({"AlignType":99})),
        ("TableCreate", json!({"Rows":-1})),
        ("TableCreate", json!({"Rows":200,"Cols":64})),
        ("TableCreate", json!({"Cols":2,"ColWidth":[7200]})),
        ("TableCreate", json!({"CellInfo":[1]})),
    ] {
        assert!(
            s.execute_hwp_action(action, json!({}), Some(fields.clone()))
                .is_err(),
            "{action} {fields}"
        );
        assert_eq!(s.generation, generation);
        assert_eq!(s.text(), "보존");
        assert_eq!(s.character_properties()?, before);
    }
    assert!(
        s.prepare_hwp_action("CharShape", json!({"bold":true}), Some(json!({"Bold":1})))
            .is_err()
    );
    assert!(
        s.prepare_hwp_action("MoveRight", json!({}), Some(json!({})))
            .is_err()
    );
    assert_eq!(
        hwp_parameter_schema("CharShape").unwrap()["complete"],
        false
    );
    Ok(())
}

#[test]
fn changing_one_native_language_preserves_mixed_other_language_runs() -> anyhow::Result<()> {
    use hwpcraft_engine::Cursor;
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나".into()))?;
    s.select(
        Cursor {
            offset: 0,
            ..Default::default()
        },
        false,
    )?;
    s.select(
        Cursor {
            offset: 1,
            ..Default::default()
        },
        true,
    )?;
    s.run(
        "format.character",
        json!({"fontFamily":"Arial","ratios":[80,100,100,100,100,100,100]}),
    )?;
    s.select(
        Cursor {
            offset: 1,
            ..Default::default()
        },
        false,
    )?;
    s.select(
        Cursor {
            offset: 2,
            ..Default::default()
        },
        true,
    )?;
    s.run(
        "format.character",
        json!({"fontFamily":"Times New Roman","ratios":[90,100,100,100,100,100,100]}),
    )?;
    let mut before = Vec::new();
    for offset in 0..2 {
        s.select(
            Cursor {
                offset,
                ..Default::default()
            },
            false,
        )?;
        before.push(s.character_properties()?);
    }
    s.select_all()?;
    s.run(
        "hwp.parameters.apply",
        json!({"id":"CharShape","parameterSet":{"FaceNameLatin":"Arial","RatioLatin":120}}),
    )?;
    for (offset, expected) in before.iter().enumerate() {
        s.select(
            Cursor {
                offset,
                ..Default::default()
            },
            false,
        )?;
        let actual = s.character_properties()?;
        assert_eq!(actual["fontFamilies"][0], expected["fontFamilies"][0]);
        assert_eq!(actual["ratios"][0], expected["ratios"][0]);
        assert_eq!(actual["fontFamilies"][1], "Arial");
        assert_eq!(actual["ratios"][1], 120);
    }
    s.execute(Command::Undo)?;
    for (offset, expected) in before.iter().enumerate() {
        s.select(
            Cursor {
                offset,
                ..Default::default()
            },
            false,
        )?;
        assert_eq!(s.character_properties()?, *expected);
    }
    assert!(
        s.run(
            "hwp.parameters.apply",
            json!({"id":"FileOpen","parameterSet":{"filename":"unused"}})
        )
        .is_err()
    );
    Ok(())
}
