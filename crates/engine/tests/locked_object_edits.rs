//! 개체 보호: a locked object refuses its caption text, picture effects and text-box
//! paragraph edits, as every other object edit does, and stays as it was.
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::json;

fn address(v: &serde_json::Value) -> anyhow::Result<ObjectAddress> {
    Ok(serde_json::from_value(v["address"].clone())?)
}

#[test]
fn a_locked_object_refuses_caption_effect_and_text_box_edits() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let png = dir.path().join("p.png");
    image::RgbImage::from_pixel(48, 30, image::Rgb([68, 114, 196])).save(&png)?;
    let mut s = Session::new()?;
    let picture = address(&s.run("picture.insert", json!({"path": png, "width_mm": 40.0}))?)?;
    s.run(
        "object.properties",
        json!({"address": picture, "props": {"hasCaption": true}}),
    )?;
    let text_box = address(&s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 40.0, "height_mm": 20.0, "paragraphs": ["원래 글"]}),
    )?)?;
    for a in [picture, text_box] {
        s.run("object.lock", json!({"address": a, "locked": true}))?;
    }
    let caption = s.run("object.caption_text.get", json!({"address": picture}))?;
    let effects = s.run("picture.effects.get", json!({"address": picture}))?;
    for (command, params) in [
        (
            "object.caption_text",
            json!({"address": picture, "text": "새 캡션"}),
        ),
        ("picture.effects", json!({"address": picture, "glow": {}})),
        (
            "textbox.paragraphs",
            json!({"address": text_box, "paragraphs": ["바꾼 글"]}),
        ),
    ] {
        let error = s.run(command, params).expect_err(command);
        assert!(error.to_string().contains("보호"), "{command}: {error}");
    }
    assert_eq!(
        s.run("object.caption_text.get", json!({"address": picture}))?,
        caption
    );
    assert_eq!(
        s.run("picture.effects.get", json!({"address": picture}))?,
        effects
    );
    assert_eq!(s.textbox_paragraphs(text_box)?, ["원래 글"]);
    // Unlocked, the same edits go through.
    s.run("object.lock", json!({"address": picture, "locked": false}))?;
    s.run("picture.effects", json!({"address": picture, "glow": {}}))?;
    Ok(())
}
