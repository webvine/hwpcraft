//! Owner: object area. 그림 바꾸기 (`picture.replace`) for body and header pictures, and the
//! pictures an AI edit question carries (`picture.insert_attachment`, `attachment`), which
//! an AI run can use although it may not read files.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

fn png(width: u32, height: u32, color: [u8; 3]) -> anyhow::Result<Vec<u8>> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb(color));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// The image a picture shows: its bin data decoded, as (width, height, first pixel).
fn shown(s: &Session, a: ObjectAddress, header: bool) -> anyhow::Result<(u32, u32, [u8; 3])> {
    let doc = s.core.document();
    let pictures: Vec<&Control> = if header {
        doc.sections[a.section]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter_map(|c| match c {
                Control::Header(h) => Some(&h.paragraphs),
                _ => None,
            })
            .flatten()
            .flat_map(|p| &p.controls)
            .collect()
    } else {
        doc.sections[a.section].paragraphs[a.paragraph]
            .controls
            .iter()
            .collect()
    };
    let id = pictures
        .iter()
        .find_map(|c| match c {
            Control::Picture(p) => Some(p.image_attr.bin_data_id),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("no picture"))?;
    let bytes = s
        .core
        .get_bin_data_image_data_native(id)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let img = image::load_from_memory(&bytes)?.to_rgb8();
    Ok((img.width(), img.height(), img.get_pixel(0, 0).0))
}

fn size_mm(s: &Session, a: ObjectAddress) -> anyhow::Result<(f64, f64)> {
    let p = s.object_properties(a)?["props"].clone();
    let mm = |k: &str| p[k].as_f64().unwrap_or(0.0) * 25.4 / 7200.0;
    Ok((mm("width"), mm("height")))
}

fn body_picture(dir: &std::path::Path) -> anyhow::Result<(Session, ObjectAddress)> {
    let path = dir.join("old.png");
    std::fs::write(&path, png(60, 30, [200, 30, 30])?)?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let v = s.run("picture.insert", json!({"path": path, "width_mm": 30.0}))?;
    let a: ObjectAddress = serde_json::from_value(v["address"].clone())?;
    Ok((s, a))
}

#[test]
fn a_body_picture_takes_a_new_image_in_its_place() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, a) = body_picture(dir.path())?;
    assert_eq!(shown(&s, a, false)?, (60, 30, [200, 30, 30]));
    let new = dir.path().join("new.png");
    std::fs::write(&new, png(40, 40, [20, 160, 40])?)?;
    assert_eq!(s.object_properties(a)?["props"]["description"], "old.png");
    s.run("picture.replace", json!({"address": a, "path": new}))?;
    assert_eq!(shown(&s, a, false)?, (40, 40, [20, 160, 40]));
    // The description named the old file; now the new one.
    assert_eq!(s.object_properties(a)?["props"]["description"], "new.png");
    // Same width, height after the new (square) picture.
    let (w, h) = size_mm(&s, a)?;
    assert!(
        (w - 30.0).abs() < 0.2 && (h - 30.0).abs() < 0.2,
        "{w} x {h}"
    );
    // One undo takes it back.
    s.execute(Command::Undo)?;
    assert_eq!(shown(&s, a, false)?, (60, 30, [200, 30, 30]));
    let (w, h) = size_mm(&s, a)?;
    assert!(
        (w - 30.0).abs() < 0.2 && (h - 15.0).abs() < 0.2,
        "{w} x {h}"
    );
    // keep_aspect false keeps the frame.
    s.run(
        "picture.replace",
        json!({"address": a, "path": new, "keep_aspect": false}),
    )?;
    let (w, h) = size_mm(&s, a)?;
    assert!(
        (w - 30.0).abs() < 0.2 && (h - 15.0).abs() < 0.2,
        "{w} x {h}"
    );
    // Not a picture, a bad file, or no source: refused, nothing changed.
    let generation = s.generation;
    let text = dir.path().join("note.txt");
    std::fs::write(&text, "그림 아님")?;
    for params in [
        json!({"address": a, "path": text}),
        json!({"address": a}),
        json!({"address": a, "path": new, "attachment": 1}),
        json!({"address": {"section": 0, "paragraph": 0, "control": 9}, "path": new}),
    ] {
        assert!(
            s.run("picture.replace", params.clone()).is_err(),
            "{params}"
        );
        assert_eq!(s.generation, generation, "{params}");
    }
    Ok(())
}

/// A taller picture treated as a character pushes the following paragraph down at once
/// (not only after saving and opening again), and its crop range is the new image's.
#[test]
fn the_text_after_a_replaced_picture_moves_with_its_height() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, a) = body_picture(dir.path())?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("다음 문단".into()))?;
    let next = hwpcraft_engine::Cursor {
        section: 0,
        paragraph: a.paragraph + 1,
        offset: 0,
        ..Default::default()
    };
    let picture_bottom = |s: &Session| -> anyhow::Result<f64> {
        let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
        let c = layout["controls"]
            .as_array()
            .and_then(|l| l.iter().find(|c| c["type"] == "image"))
            .ok_or_else(|| anyhow::anyhow!("no picture"))?;
        Ok(c["y"].as_f64().unwrap_or(0.0) + c["h"].as_f64().unwrap_or(0.0))
    };
    let next_top =
        |s: &Session| -> anyhow::Result<f64> { Ok(s.caret_at(next)?["y"].as_f64().unwrap_or(0.0)) };
    assert!(next_top(&s)? >= picture_bottom(&s)?);
    let tall = dir.path().join("tall.png");
    std::fs::write(&tall, png(30, 60, [20, 160, 40])?)?;
    s.run("picture.replace", json!({"address": a, "path": tall}))?;
    let (bottom, top) = (picture_bottom(&s)?, next_top(&s)?);
    assert!(
        top >= bottom,
        "the next paragraph ({top}) overlaps the picture ({bottom})"
    );
    match s.core.document().sections[0].paragraphs[a.paragraph]
        .controls
        .get(a.control)
    {
        Some(Control::Picture(p)) => assert_eq!(p.img_dim, (30 * 75, 60 * 75)),
        _ => anyhow::bail!("no picture"),
    }
    // Saved and opened again, the same place.
    let saved = dir.path().join("tall.hwp");
    s.save(&saved)?;
    let mut back = Session::new()?;
    back.open(&saved)?;
    assert!((next_top(&back)? - top).abs() < 1.0);
    Ok(())
}

#[test]
fn an_ai_run_uses_the_pictures_of_its_question_not_files() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, a) = body_picture(dir.path())?;
    let file = dir.path().join("file.png");
    std::fs::write(&file, png(10, 10, [1, 2, 3])?)?;
    s.set_commands_restricted(true);
    // Files stay out of reach.
    let e = s
        .run("picture.replace", json!({"address": a, "path": file}))
        .unwrap_err();
    assert!(e.to_string().contains("attachment"), "{e}");
    assert!(s.run("picture.insert", json!({"path": file})).is_err());
    // No pictures with the question yet.
    let e = s
        .run("picture.replace", json!({"address": a, "attachment": 1}))
        .unwrap_err();
    assert!(e.to_string().contains("첨부된 그림이 없습니다"), "{e}");
    // The question's picture 2 (its file 1 being text, say).
    s.set_run_pictures(vec![(2, "로고.png".into(), png(50, 25, [9, 9, 200])?)]);
    let e = s
        .run("picture.replace", json!({"address": a, "attachment": 1}))
        .unwrap_err();
    assert!(e.to_string().contains("그림 첨부: 2"), "{e}");
    // A description someone wrote stays.
    s.run(
        "object.properties",
        json!({"address": a, "props": {"description": "회사 로고"}}),
    )?;
    s.run("picture.replace", json!({"address": a, "attachment": 2}))?;
    assert_eq!(shown(&s, a, false)?, (50, 25, [9, 9, 200]));
    assert_eq!(s.object_properties(a)?["props"]["description"], "회사 로고");
    let v = s.run(
        "picture.insert_attachment",
        json!({"attachment": 2, "width_mm": 20.0}),
    )?;
    let b: ObjectAddress = serde_json::from_value(v["address"].clone())?;
    assert_eq!(s.object_properties(b)?["props"]["treatAsChar"], true);
    // The question ended.
    s.clear_run_pictures();
    assert!(
        s.run("picture.insert_attachment", json!({"attachment": 2}))
            .is_err()
    );
    Ok(())
}

/// A document whose header holds a picture (and whose body has another one).
fn header_picture(dir: &std::path::Path) -> anyhow::Result<Session> {
    let path = dir.join("old.png");
    std::fs::write(&path, png(60, 30, [200, 30, 30])?)?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.execute(Command::Split)?;
    s.run(
        "picture.insert",
        json!({"path": path, "width_mm": 30.0, "treat_as_char": false}),
    )?;
    s.run("page.header", json!({"apply_to": "both", "template": 0}))?;
    let doc = s.core.document_mut();
    let picture = doc.sections[0].paragraphs[1].clone();
    let paragraphs = doc.sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
        .find_map(|c| match c {
            Control::Header(h) => Some(&mut h.paragraphs),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("a header"))?;
    if paragraphs.is_empty() {
        paragraphs.push(Paragraph::new_empty());
    }
    paragraphs.truncate(1);
    paragraphs.push(picture);
    let saved = dir.join("header.hwpx");
    s.save(&saved)?;
    let mut s = Session::new()?;
    s.open(&saved)?;
    Ok(s)
}

#[test]
fn a_header_picture_is_replaced_through_its_header_address() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = header_picture(dir.path())?;
    let list = s.run("page.header_footer.object.list", json!({}))?;
    let address = list["objects"]
        .as_array()
        .and_then(|o| o.iter().find(|o| o["kind"] == "picture"))
        .map(|o| o["address"].clone())
        .ok_or_else(|| anyhow::anyhow!("the header picture is listed: {list}"))?;
    let body = ObjectAddress {
        section: 0,
        paragraph: 1,
        control: 0,
    };
    let header = |s: &Session| shown(s, body, true);
    assert_eq!(header(&s)?, (60, 30, [200, 30, 30]));
    // In an AI run, from the question's picture; a file stays refused here too.
    s.set_commands_restricted(true);
    let file = dir.path().join("file.png");
    std::fs::write(&file, png(10, 10, [1, 2, 3])?)?;
    assert!(
        s.run("picture.replace", json!({"address": address, "path": file}))
            .is_err()
    );
    s.set_run_pictures(vec![(1, "새 로고.png".into(), png(40, 40, [20, 160, 40])?)]);
    s.run(
        "picture.replace",
        json!({"address": address, "attachment": 1}),
    )?;
    assert_eq!(header(&s)?, (40, 40, [20, 160, 40]));
    // The body picture keeps its image.
    assert_eq!(shown(&s, body, false)?, (60, 30, [200, 30, 30]));
    // Saved and opened again, the header shows the new picture.
    s.set_commands_restricted(false);
    let saved = dir.path().join("replaced.hwpx");
    s.save(&saved)?;
    let mut reopened = Session::new()?;
    reopened.open(&saved)?;
    assert_eq!(header(&reopened)?, (40, 40, [20, 160, 40]));
    // One undo takes it back.
    s.execute(Command::Undo)?;
    assert_eq!(header(&s)?, (60, 30, [200, 30, 30]));
    let _: Value = list;
    Ok(())
}
