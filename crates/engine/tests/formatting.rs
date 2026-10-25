use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

fn reopen(s: &mut Session, ext: &str) -> anyhow::Result<Session> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(format!("format.{ext}"));
    s.save(&path)?;
    let mut result = Session::new()?;
    result.open(&path)?;
    Ok(result)
}
#[test]
fn named_fonts_are_applied_to_each_language_and_survive_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한글 English 😀".into()))?;
    let old = s.character_properties()?;
    s.run("format.character", json!({"fontFamily":"Arial"}))?;
    let p = s.character_properties()?;
    assert_eq!(p["fontFamily"], "Arial");
    for f in p["fontFamilies"].as_array().unwrap() {
        assert_eq!(f, "Arial");
    }
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(r.character_properties()?["fontFamily"], "Arial", "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_eq!(s.character_properties()?["fontFamily"], old["fontFamily"]);
    Ok(())
}
#[test]
fn language_metrics_and_effects_survive_save_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한글 English".into()))?;
    s.run("format.character",json!({"ratios":[90,100,100,100,100,100,100],"spacings":[-5,2,0,0,0,0,0],"relativeSizes":[100,95,100,100,100,100,100],"charOffsets":[0,10,0,0,0,0,0],"strikethrough":true,"textColor":"#123456","shadeColor":"#fffacd"}))?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let p = r.character_properties()?;
        assert_eq!(p["ratios"][0], 90, "{ext}");
        assert_eq!(p["spacings"][0], -5);
        assert_eq!(p["strikethrough"], true);
        assert_eq!(p["textColor"], "#123456");
    }
    s.execute(Command::Undo)?;
    assert_eq!(s.character_properties()?["ratios"][0], 100);
    Ok(())
}
#[test]
fn paragraph_read_write_units_and_protection_are_preserved() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("문단 여백과 첫 줄 들여쓰기".into()))?;
    let desired = json!({"marginLeft":2400,"marginRight":1200,"indent":-600,"spacingBefore":600,"spacingAfter":900,"lineSpacingType":"Fixed","lineSpacing":1800,"keepWithNext":true,"keepLines":true});
    s.run("format.paragraph", desired.clone())?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let p = r.paragraph_format()?;
        for (k, v) in desired.as_object().unwrap() {
            assert_eq!(&p[k], v, "{ext}: {k}");
        }
    }
    let native = s.paragraph_properties()?;
    assert!((native["marginLeft"].as_f64().unwrap() - 16.0).abs() < 0.1);
    assert_eq!(s.paragraph_format()?["marginLeft"], 2400);
    Ok(())
}
#[test]
fn formatting_selection_and_table_cells_use_same_engine() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가😀나".into()))?;
    s.cursor.offset = 2;
    s.anchor = Some(1);
    s.run(
        "format.character",
        json!({"fontFamily":"Arial","bold":true}),
    )?;
    assert_eq!(s.selected_text(), "😀");
    assert_eq!(s.character_properties()?["fontFamily"], "Arial");
    s.select(Cursor::default(), false)?;
    assert_ne!(s.character_properties()?["fontFamily"], "Arial");
    s.execute(Command::Table {
        rows: 1,
        columns: 1,
    })?;
    let cursor = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .unwrap()
        .0;
    s.select(cursor, false)?;
    s.execute(Command::Insert("표 셀".into()))?;
    s.run(
        "format.character",
        json!({"fontFamily":"Malgun Gothic","strikethrough":true}),
    )?;
    s.run("format.paragraph", json!({"indent":600,"spacingAfter":300}))?;
    // RHWP resolves the English alias to the document's Korean font name.
    assert_eq!(s.character_properties()?["fontFamily"], "맑은 고딕");
    assert_eq!(s.paragraph_format()?["indent"], 600);
    Ok(())
}
#[test]
fn malformed_formatting_does_not_mutate_document_or_history() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let before = s.info();
    let generation = s.generation;
    for params in [
        json!({"fontSize":-1}),
        json!({"fontSize":1e30}),
        json!({"ratios":[100]}),
        json!({"spacings":[999,0,0,0,0,0,0]}),
        json!({"bold":"yes"}),
        json!({"unknown":1}),
        json!({"textColor":"red"}),
        json!({"fontFamily":""}),
        json!({"subscript":true,"superscript":true}),
    ] {
        assert!(s.run("format.character", params).is_err());
    }
    assert!(
        s.run("format.paragraph", json!({"marginLeft":1e20}))
            .is_err()
    );
    assert_eq!(s.info(), before);
    assert_eq!(s.generation, generation);
    assert!(!s.can_undo());
    Ok(())
}

#[test]
fn separate_language_fonts_and_float_arrays_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한글 English 漢字".into()))?;
    let fonts = json!([
        "Arial",
        "Times New Roman",
        "Courier New",
        "Arial",
        "Times New Roman",
        "Courier New",
        "Arial"
    ]);
    s.run(
        "format.character",
        json!({"fontFamilies":fonts,"ratios":[90.0,100.0,100.0,100.0,100.0,100.0,100.0]}),
    )?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let before = r.generation;
        let p = r.run("format.character.get", json!({}))?;
        assert_eq!(p["fontFamilies"], fonts, "{ext}");
        assert_eq!(p["ratios"][0], 90);
        let _ = r.run("format.paragraph.get", json!({}))?;
        assert_eq!(r.generation, before);
        assert!(!r.can_undo());
    }
    Ok(())
}

#[test]
fn changing_script_position_clears_the_opposite_flag() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("x2".into()))?;
    s.cursor.offset = 2;
    s.anchor = Some(1);
    s.run("format.character", json!({"superscript":true}))?;
    s.run("format.character", json!({"subscript":true}))?;
    let p = s.character_properties()?;
    assert_eq!(p["subscript"], true);
    assert_eq!(p["superscript"], false);
    s.execute(Command::Undo)?;
    let p = s.character_properties()?;
    assert_eq!(p["superscript"], true);
    assert_eq!(p["subscript"], false);
    Ok(())
}
