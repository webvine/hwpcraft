//! shape.insert takes insert.shape's `text` as well as `paragraphs`, so the two
//! sibling commands accept the same text box content.
use hwpcraft_engine::Session;
use serde_json::json;

#[test]
fn shape_insert_text_becomes_text_box_paragraphs() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let r = s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 60, "height_mm": 25, "text": "첫 줄\n둘째 줄"}),
    )?;
    let got = s.run("textbox.paragraphs.get", json!({"address": r["address"]}))?;
    assert_eq!(got["paragraphs"], json!(["첫 줄", "둘째 줄"]), "{got}");
    Ok(())
}

#[test]
fn shape_insert_text_keeps_the_existing_rules() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    // An empty text is no text, as in insert.shape's example.
    s.run(
        "shape.insert",
        json!({"kind": "rectangle", "width_mm": 40, "height_mm": 20, "text": ""}),
    )?;
    assert!(
        s.run(
            "shape.insert",
            json!({"kind": "rectangle", "width_mm": 40, "height_mm": 20, "text": "글"}),
        )
        .is_err(),
        "text only goes into a text box"
    );
    assert!(
        s.run(
            "shape.insert",
            json!({"kind": "textbox", "width_mm": 40, "height_mm": 20, "text": "가", "paragraphs": ["나"]}),
        )
        .is_err(),
        "text and paragraphs together are ambiguous"
    );
    Ok(())
}
