//! Owner: object area. A picture that grows (개체 크기, 그림 바꾸기) pushes the text after
//! it down at once, whether it sits in the line as a character or floats with 위아래
//! wrapping, and the same place comes back after saving and opening again.
use hwpcraft_engine::{Command, Cursor, ObjectAddress, Session};
use serde_json::{Value, json};

fn png(width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([20, 160, 40]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// "본문" with a 30 mm wide picture (60×30 px), then a paragraph "다음 문단".
fn picture_then_text(
    dir: &std::path::Path,
    treat_as_char: bool,
) -> anyhow::Result<(Session, ObjectAddress)> {
    let path = dir.join("old.png");
    std::fs::write(&path, png(60, 30)?)?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let v = s.run(
        "picture.insert",
        json!({"path": path, "width_mm": 30.0, "treat_as_char": treat_as_char}),
    )?;
    let a: ObjectAddress = serde_json::from_value(v["address"].clone())?;
    if !treat_as_char {
        s.set_object_properties(a, &json!({"textWrap": "TopAndBottom"}))?;
    }
    s.execute(Command::Split)?;
    s.execute(Command::Insert("다음 문단".into()))?;
    Ok((s, a))
}

fn picture_bottom(s: &Session) -> anyhow::Result<f64> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let c = layout["controls"]
        .as_array()
        .and_then(|l| l.iter().find(|c| c["type"] == "image"))
        .ok_or_else(|| anyhow::anyhow!("no picture"))?;
    Ok(c["y"].as_f64().unwrap_or(0.0) + c["h"].as_f64().unwrap_or(0.0))
}

fn next_top(s: &Session, a: ObjectAddress) -> anyhow::Result<f64> {
    let next = Cursor {
        section: 0,
        paragraph: a.paragraph + 1,
        offset: 0,
        ..Default::default()
    };
    Ok(s.caret_at(next)?["y"].as_f64().unwrap_or(0.0))
}

/// The text after the picture starts below it, now and after saving and opening again.
fn check_clear(s: &mut Session, a: ObjectAddress, dir: &std::path::Path) -> anyhow::Result<()> {
    let (bottom, top) = (picture_bottom(s)?, next_top(s, a)?);
    assert!(
        top >= bottom,
        "the next paragraph ({top}) overlaps the picture ({bottom})"
    );
    for ext in ["hwp", "hwpx"] {
        let saved = dir.join(format!("grown.{ext}"));
        s.save(&saved)?;
        let mut back = Session::new()?;
        back.open(&saved)?;
        let again = next_top(&back, a)?;
        assert!(
            (again - top).abs() < 1.0,
            "{ext}: {again} after opening, {top} before"
        );
    }
    Ok(())
}

#[test]
fn an_inline_picture_made_taller_pushes_the_next_paragraph_down() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, a) = picture_then_text(dir.path(), true)?;
    let before = next_top(&s, a)?;
    s.run("object.size", json!({"address": a, "height_mm": 60.0}))?;
    let after = next_top(&s, a)?;
    assert!(after > before + 100.0, "{before} -> {after}");
    check_clear(&mut s, a, dir.path())?;
    // Undone, the paragraph comes back up.
    s.execute(Command::Undo)?;
    assert!((next_top(&s, a)? - before).abs() < 1.0);
    Ok(())
}

#[test]
fn a_floating_picture_made_taller_pushes_the_next_paragraph_down() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, a) = picture_then_text(dir.path(), false)?;
    assert!(next_top(&s, a)? >= picture_bottom(&s)?);
    s.run("object.size", json!({"address": a, "height_mm": 60.0}))?;
    check_clear(&mut s, a, dir.path())
}

#[test]
fn a_floating_picture_replaced_by_a_taller_one_pushes_the_next_paragraph_down() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let (mut s, a) = picture_then_text(dir.path(), false)?;
    let tall = dir.path().join("tall.png");
    std::fs::write(&tall, png(30, 60)?)?;
    s.run("picture.replace", json!({"address": a, "path": tall}))?;
    check_clear(&mut s, a, dir.path())
}
