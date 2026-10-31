//! Captions of pictures and drawing objects: the words after "그림 N" can be
//! set and read back, survive HWP/HWPX saves, and an inline picture's caption
//! is part of its line, so the next line does not cover it.
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

fn picture(s: &mut Session, dir: &std::path::Path) -> anyhow::Result<ObjectAddress> {
    let png = dir.join("p.png");
    image::RgbImage::from_pixel(48, 30, image::Rgb([68, 114, 196])).save(&png)?;
    s.execute(Command::Insert("그림 앞 문단\n".into()))?;
    let made = s.run(
        "picture.insert",
        json!({"path": png, "width_mm": 60.0, "treat_as_char": true}),
    )?;
    let a: ObjectAddress = serde_json::from_value(made["address"].clone())?;
    s.run(
        "object.properties",
        json!({"address": a, "props": {"hasCaption": true}}),
    )?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.execute(Command::Insert("\n그림 뒤 문단".into()))?;
    Ok(a)
}

/// Baseline y of the first drawn line (text runs grouped by y) containing `needle`.
fn text_y(s: &Session, needle: &str) -> anyhow::Result<f64> {
    let svg = s.render_page_svg(0)?;
    let mut rows: Vec<(f64, String)> = Vec::new();
    for part in svg.split("<text").skip(1) {
        let Some(y) = part
            .split(" y=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .and_then(|v| v.parse::<f64>().ok())
        else {
            continue;
        };
        let content = part
            .split_once('>')
            .and_then(|(_, rest)| rest.split_once("</text>"))
            .map_or("", |(text, _)| text);
        match rows.iter_mut().find(|(ry, _)| (*ry - y).abs() < 0.5) {
            Some((_, row)) => row.push_str(content),
            None => rows.push((y, content.to_owned())),
        }
    }
    rows.iter()
        .find(|(_, row)| row.replace(' ', "").contains(&needle.replace(' ', "")))
        .map(|(y, _)| *y)
        .ok_or_else(|| anyhow::anyhow!("{needle} not drawn"))
}

#[test]
fn picture_caption_text_round_trips_and_keeps_its_line() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    let a = picture(&mut s, dir.path())?;
    s.run(
        "object.caption_text",
        json!({"address": a, "text": "분기별 매출 추이"}),
    )?;
    let got = s.run("object.caption_text.get", json!({"address": a}))?;
    assert_eq!(got["text"], "분기별 매출 추이");
    // The caption sits below the picture and the next paragraph below the caption.
    let caption = text_y(&s, "분기별")?;
    let next = text_y(&s, "그림 뒤")?;
    assert!(
        next > caption + 10.0,
        "next line {next} covers the caption at {caption}"
    );
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("c.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let got = r.run("object.caption_text.get", json!({"address": a}))?;
        assert_eq!(got["text"], "분기별 매출 추이", "{ext}");
        let (caption, next) = (text_y(&r, "분기별")?, text_y(&r, "그림 뒤")?);
        assert!(
            next > caption + 10.0,
            "{ext}: next line {next} covers the caption at {caption}"
        );
    }
    s.execute(Command::Undo)?;
    assert_ne!(
        s.run("object.caption_text.get", json!({"address": a}))?["text"],
        "분기별 매출 추이"
    );
    Ok(())
}

#[test]
fn drawing_object_caption_and_refusals() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    let pic = picture(&mut s, dir.path())?;
    s.execute(Command::Insert("\n".into()))?;
    let made = s.run(
        "shape.insert",
        json!({"kind": "rectangle", "width_mm": 40.0, "height_mm": 20.0}),
    )?;
    let rect: ObjectAddress = serde_json::from_value(made["address"].clone())?;
    let caption = s.run(
        "object.properties",
        json!({"address": rect, "props": {"hasCaption": true}}),
    );
    if caption.is_ok() {
        s.run(
            "object.caption_text",
            json!({"address": rect, "text": "구성도"}),
        )?;
        assert_eq!(
            s.run("object.caption_text.get", json!({"address": rect}))?["text"],
            "구성도"
        );
    }
    let generation = s.generation;
    let bad: [Value; 3] = [
        json!({"address": pic, "text": "줄\n바꿈"}),
        json!({"address": pic, "text": "가".repeat(1001)}),
        json!({"address": {"section": 0, "paragraph": 0, "control": 0}, "text": "글자"}),
    ];
    for params in bad {
        assert!(
            s.run("object.caption_text", params.clone()).is_err(),
            "{params}"
        );
    }
    assert_eq!(s.generation, generation);
    Ok(())
}
