//! Owner: format area. Applying a style moves the paragraph head (outline number, bullet,
//! paragraph number) to what the new style gives, also when the paragraph has direct
//! paragraph formatting such as an alignment. Before, a paragraph with any direct
//! formatting kept its whole paragraph shape: a heading given 개요 1 and then centred
//! passed its outline number to every following paragraph (and to the cells of a table
//! inserted there) even after 바탕글 was applied, and a centred paragraph given 개요 1
//! got no number. The MCP report recipe (docs/mcp-commands.md) hits the first case.
use hwpcraft_doc::model::style::{Alignment, HeadType};
use hwpcraft_engine::Session;
use serde_json::json;

/// Head type, level and alignment of the body paragraph whose text is `text`.
fn shape(s: &Session, text: &str) -> (HeadType, u8, Alignment) {
    let doc = s.core.document();
    let para = doc.sections[0]
        .paragraphs
        .iter()
        .find(|p| p.text == text)
        .unwrap_or_else(|| panic!("no paragraph {text:?}"));
    let ps = &doc.doc_info.para_shapes[para.para_shape_id as usize];
    (ps.head_type, ps.para_level, ps.alignment)
}

fn outline_style(s: &mut Session) -> anyhow::Result<u64> {
    let list = s.run("format.style.list", json!({}))?;
    let items = list
        .as_array()
        .or_else(|| list["items"].as_array())
        .unwrap();
    Ok(items
        .iter()
        .find(|i| i["name"] == "개요 1")
        .and_then(|i| i["id"].as_u64())
        .expect("개요 1 style"))
}

#[test]
fn body_style_after_a_centred_heading_drops_the_outline_number() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let outline = outline_style(&mut s)?;
    s.run("text.insert", json!({"text": "제목"}))?;
    s.run("style.apply", json!({"id": outline}))?;
    s.run("format.paragraph", json!({"alignment": "center"}))?;
    assert_eq!(shape(&s, "제목").0, HeadType::Outline);
    s.run("text.insert", json!({"text": "\n"}))?;
    s.run("style.apply", json!({"id": 0}))?;
    s.run("text.insert", json!({"text": "본문"}))?;
    let (head, _, _) = shape(&s, "본문");
    assert_eq!(head, HeadType::None, "바탕글 has no outline number");
    // The heading keeps its number and its own alignment.
    assert_eq!(shape(&s, "제목"), (HeadType::Outline, 0, Alignment::Center));
    // A table inserted after it has no numbered cells.
    s.run("text.insert", json!({"text": "\n"}))?;
    let t = s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    let address = &t["table"];
    let doc = s.core.document();
    let para = &doc.sections[0].paragraphs[address["paragraph"].as_u64().unwrap() as usize];
    let control = address["control"].as_u64().unwrap() as usize;
    let hwpcraft_doc::model::control::Control::Table(table) = &para.controls[control] else {
        panic!("not a table");
    };
    for cell in &table.cells {
        for p in &cell.paragraphs {
            let ps = &doc.doc_info.para_shapes[p.para_shape_id as usize];
            assert_eq!(ps.head_type, HeadType::None, "table cell numbered");
        }
    }
    Ok(())
}

#[test]
fn outline_style_on_a_centred_paragraph_numbers_it_and_keeps_the_alignment() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let outline = outline_style(&mut s)?;
    s.run("text.insert", json!({"text": "가운데"}))?;
    s.run("format.paragraph", json!({"alignment": "center"}))?;
    s.run("style.apply", json!({"id": outline}))?;
    assert_eq!(
        shape(&s, "가운데"),
        (HeadType::Outline, 0, Alignment::Center)
    );
    // Undo puts the old paragraph shape back.
    s.run("edit.undo", json!({}))?;
    assert_eq!(shape(&s, "가운데"), (HeadType::None, 0, Alignment::Center));
    Ok(())
}

#[test]
fn a_bullet_given_directly_survives_a_style_change() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "항목"}))?;
    s.run("format.bullet.toggle", json!({}))?;
    assert_eq!(shape(&s, "항목").0, HeadType::Bullet);
    // 본문 has no head of its own; the bullet was the paragraph's own formatting.
    s.run("style.apply", json!({"id": 1}))?;
    assert_eq!(shape(&s, "항목").0, HeadType::Bullet);
    Ok(())
}

#[test]
fn the_report_recipe_without_direct_formatting_still_works() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("format.style.set.apply", json!({"name": "보고서"}))?;
    let outline = outline_style(&mut s)?;
    s.run("text.insert", json!({"text": "장 제목"}))?;
    s.run("style.apply", json!({"id": outline}))?;
    s.run("text.insert", json!({"text": "\n"}))?;
    s.run("style.apply", json!({"id": 0}))?;
    s.run("text.insert", json!({"text": "본문 글"}))?;
    assert_eq!(shape(&s, "장 제목").0, HeadType::Outline);
    assert_eq!(shape(&s, "본문 글").0, HeadType::None);
    Ok(())
}

#[test]
fn a_cell_paragraph_follows_the_same_rule() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let outline = outline_style(&mut s)?;
    let t = s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
    let table = &t["table"];
    let cell = json!({"section": table["section"], "paragraph": table["paragraph"], "offset": 0,
        "cell": {"control": table["control"], "index": 0, "paragraph": 0}});
    s.run("selection.set", json!({"cursor": cell}))?;
    s.run("text.insert", json!({"text": "셀 제목"}))?;
    s.run("style.apply", json!({"id": outline}))?;
    s.run("format.paragraph", json!({"alignment": "center"}))?;
    s.run("text.insert", json!({"text": "\n"}))?;
    s.run("style.apply", json!({"id": 0}))?;
    s.run("text.insert", json!({"text": "셀 본문"}))?;
    let doc = s.core.document();
    let heads: Vec<(String, HeadType)> = doc.sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| p.controls.iter())
        .filter_map(|c| match c {
            hwpcraft_doc::model::control::Control::Table(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| t.cells.iter())
        .flat_map(|c| c.paragraphs.iter())
        .map(|p| {
            (
                p.text.clone(),
                doc.doc_info.para_shapes[p.para_shape_id as usize].head_type,
            )
        })
        .collect();
    assert_eq!(
        heads,
        vec![
            ("셀 제목".to_owned(), HeadType::Outline),
            ("셀 본문".to_owned(), HeadType::None)
        ]
    );
    Ok(())
}
