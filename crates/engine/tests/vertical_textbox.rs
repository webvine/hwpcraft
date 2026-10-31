//! Owner: shell/page area. Typed editing (context.v2) of a vertical text box: replace,
//! split and character format keep the box vertical through HWP and HWPX.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::shape::ShapeObject;
use hwpcraft_engine::Session;
use serde_json::{Value, json};

/// Text box paragraphs and whether the box is vertical, for the first shape in the body.
fn textbox(s: &Session) -> anyhow::Result<(Vec<String>, bool)> {
    for para in &s.core.document().sections[0].paragraphs {
        for c in &para.controls {
            if let Control::Shape(shape) = c
                && let ShapeObject::Rectangle(_) = shape.as_ref()
                && let Some(tb) = shape.drawing().and_then(|d| d.text_box.as_ref())
            {
                return Ok((
                    tb.paragraphs.iter().map(|p| p.text.clone()).collect(),
                    tb.list_attr & 7 == 1,
                ));
            }
        }
    }
    anyhow::bail!("no text box")
}

/// The typed address of the text box's first paragraph.
fn address(s: &mut Session) -> anyhow::Result<Value> {
    let list = s.run(
        "context.v2.list",
        json!({"limit": 100, "include": ["textbox"]}),
    )?;
    list["items"]
        .as_array()
        .and_then(|items| items.first())
        .map(|item| item["address"].clone())
        .ok_or_else(|| anyhow::anyhow!("text box context: {list}"))
}

#[test]
fn a_vertical_text_box_is_edited_through_typed_contexts() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "shape.insert",
        json!({"kind":"textbox","width_mm":30,"height_mm":60,"direction":"vertical",
               "paragraphs":["세로 글상자"]}),
    )?;
    assert_eq!(textbox(&s)?, (vec!["세로 글상자".to_owned()], true));
    let at = address(&mut s)?;
    let info = s.run("context.v2.get", json!({"address": at}))?;
    assert_eq!(info["capabilities"]["replace"], true, "{info}");
    let generation = s.generation;
    s.run(
        "context.v2.replace",
        json!({"address": at, "start": 0, "end": 2, "text": "가로", "expected_generation": generation}),
    )?;
    let generation = s.generation;
    s.run(
        "context.v2.split",
        json!({"address": at, "offset": 2, "expected_generation": generation}),
    )?;
    let generation = s.generation;
    s.run(
        "context.v2.character",
        json!({"address": at, "format": {"bold": true}, "expected_generation": generation}),
    )?;
    let check = |s: &Session| -> anyhow::Result<()> {
        assert_eq!(
            textbox(s)?,
            (vec!["가로".to_owned(), " 글상자".to_owned()], true)
        );
        let svg = s
            .core
            .render_page_svg_native(0)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        for glyph in [">가</text>", ">로</text>", ">글</text>"] {
            assert!(svg.contains(glyph), "drawn: {glyph}");
        }
        Ok(())
    };
    check(&s)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("vertical.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        check(&r)?;
    }
    Ok(())
}
