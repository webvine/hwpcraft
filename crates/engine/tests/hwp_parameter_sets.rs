//! Owner: format area. Public ParameterSets for InsertText, PageSetup (SecDef.PageDef)
//! and the 찾기 / 바꾸기 actions (FindReplace), with field names, units and value
//! lists from Hancom's published ParameterSet table. Unsupported fields fail before
//! any edit.
use hwpcraft_doc::model::page::BindingMethod;
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn action(s: &mut Session, id: &str, set: Value) -> anyhow::Result<Value> {
    s.execute_hwp_action(id, json!({}), Some(set))
}

#[test]
fn insert_text_takes_the_text_item() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    action(&mut s, "InsertText", json!({"Text": "한글 자동화"}))?;
    assert_eq!(s.text(), "한글 자동화");
    assert!(action(&mut s, "InsertText", json!({"Text": 3})).is_err());
    assert!(action(&mut s, "InsertText", json!({"Txt": "x"})).is_err());
    assert!(action(&mut s, "InsertText", json!({})).is_err());
    assert_eq!(s.text(), "한글 자동화");
    Ok(())
}

#[test]
fn page_setup_takes_hwpunit_page_def_and_apply_to() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    // The published example: margins in HWPUNIT (1 mm = 283.465), 맞쪽 편집, 넓게.
    action(
        &mut s,
        "PageSetup",
        json!({"ApplyTo": 2, "PageDef": {"PaperWidth": 59528, "PaperHeight": 84188,
            "TopMargin": 5668, "BottomMargin": 5668, "LeftMargin": 2834, "RightMargin": 2834,
            "HeaderLen": 1417, "FooterLen": 1417, "GutterLen": 0, "GutterType": 1, "Landscape": 1}}),
    )?;
    let page = s.core.document().sections[0].section_def.page_def.clone();
    assert_eq!(
        (page.width, page.height),
        (59528, 84188),
        "paper in HWPUNIT"
    );
    assert_eq!(
        (
            page.margin_top,
            page.margin_bottom,
            page.margin_left,
            page.margin_right
        ),
        (5668, 5668, 2834, 2834)
    );
    assert_eq!(
        (page.margin_header, page.margin_footer, page.margin_gutter),
        (1417, 1417, 0)
    );
    assert!(page.landscape);
    assert_eq!(page.binding, BindingMethod::DuplexSided);

    // ApplyTo 3 (문서 전체) reaches every section; each keeps what is not given.
    s.run("text.insert", json!({"text": "둘째 구역"}))?;
    s.run("page.section_break", json!({}))?;
    assert_eq!(s.core.document().sections.len(), 2);
    let second = s.cursor.section;
    s.run("page.setup", json!({"top_mm": 30.0}))?;
    action(
        &mut s,
        "PageSetup",
        json!({"ApplyTo": 3, "PageDef": {"LeftMargin": 4252}}),
    )?;
    let sections = &s.core.document().sections;
    assert!(
        sections
            .iter()
            .all(|x| x.section_def.page_def.margin_left == 4252)
    );
    assert_eq!(sections[0].section_def.page_def.margin_top, 5668);
    assert_eq!(
        sections[second].section_def.page_def.margin_top,
        (30.0f64 * 7200.0 / 25.4).round() as u32
    );
    // One undo step takes the whole-document change back.
    s.execute(hwpcraft_engine::Command::Undo)?;
    assert!(
        s.core
            .document()
            .sections
            .iter()
            .all(|x| x.section_def.page_def.margin_left == 2834)
    );

    // Not supported: refused before any edit.
    let before = format!("{:?}", s.core.document().sections[0].section_def.page_def);
    for bad in [
        json!({"ApplyTo": 1, "PageDef": {"LeftMargin": 1000}}),
        json!({"ApplyTo": 4, "PageDef": {"LeftMargin": 1000}}),
        json!({"PageDef": {"LeftMargin": -1}}),
        json!({"PageDef": {"Margin": 1000}}),
        json!({"PageDef": {"GutterType": 3}}),
        json!({"PageDef": 5}),
        json!({"Paper": 1}),
    ] {
        assert!(action(&mut s, "PageSetup", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(
        format!("{:?}", s.core.document().sections[0].section_def.page_def),
        before
    );
    let schema = s.run("hwp.parameters.schema", json!({"id": "PageSetup"}))?;
    assert!(
        schema["fields"]
            .as_array()
            .is_some_and(|f| f.contains(&json!("PageDef.LeftMargin")))
    );
    Ok(())
}

#[test]
fn find_replace_sets_drive_find_and_replace() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "사과 Apple apple 사과"}))?;
    // A GetDefault-like set: off switches HwpCraft lacks are accepted.
    let r = action(
        &mut s,
        "AllReplace",
        json!({"FindString": "apple", "ReplaceString": "배", "Direction": 2, "MatchCase": 1,
            "IgnoreMessage": 1, "ReplaceMode": 1, "AllWordForms": 0, "AutoSpell": 0,
            "FindJaso": 0, "FindStyle": "", "ReplaceStyle": "", "FindParaShape": {}}),
    )?;
    assert_eq!(r["replaced"], 1, "{r}");
    assert_eq!(s.text(), "사과 Apple 배 사과");
    // MatchCase off finds both cases.
    let r = action(
        &mut s,
        "AllReplace",
        json!({"FindString": "APPLE", "ReplaceString": "감"}),
    )?;
    assert_eq!(r["replaced"], 1, "{r}");
    assert_eq!(s.text(), "사과 감 배 사과");
    // A switch HwpCraft lacks, turned on, is refused before any edit.
    for bad in [
        json!({"FindString": "사과", "ReplaceString": "x", "AllWordForms": 1}),
        json!({"FindString": "사과", "ReplaceString": "x", "FindStyle": "개요 1"}),
        json!({"FindString": "사과", "ReplaceString": "x", "ReplaceCharShape": {"Bold": 1}}),
        json!({"FindString": "사과", "ReplaceString": "x", "Direction": 3}),
        json!({"FindString": "사과", "ReplaceString": "x", "Unknown": 1}),
        json!({"ReplaceString": "x"}),
    ] {
        assert!(action(&mut s, "AllReplace", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(s.text(), "사과 감 배 사과");

    // FindCharShape: only the bold 사과.
    s.run("select.text", json!({"text": "사과", "occurrence": 2}))?;
    s.run("format.character", json!({"bold": true}))?;
    let r = action(
        &mut s,
        "AllReplace",
        json!({"FindString": "사과", "ReplaceString": "귤", "FindCharShape": {"Bold": 1}}),
    )?;
    assert_eq!(r["replaced"], 1, "{r}");
    assert_eq!(s.text(), "사과 감 배 귤");

    // RepeatFind selects the next match; FindAll lists them.
    s.run("caret.move", json!({"motion": "document_start"}))?;
    action(
        &mut s,
        "RepeatFind",
        json!({"FindString": "배", "Direction": 0}),
    )?;
    assert_eq!(s.selected_text(), "배");
    let all = action(&mut s, "FindAll", json!({"FindString": "사과"}))?;
    assert_eq!(all["total"], 1, "{all}");
    assert!(
        action(
            &mut s,
            "FindAll",
            json!({"FindString": "사과", "WholeWordOnly": 1})
        )
        .is_err()
    );
    // ExecReplace replaces the selected match.
    action(
        &mut s,
        "ExecReplace",
        json!({"FindString": "배", "ReplaceString": "밤"}),
    )?;
    assert_eq!(s.text(), "사과 감 밤 귤");
    Ok(())
}
