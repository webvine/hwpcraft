//! Picture effects (그림자·네온·부드러운 가장자리·반사): set through the command,
//! drawn deterministically, kept through an HWPX save and undo, refused for HWP.
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::{Value, json};

fn picture(s: &mut Session, dir: &std::path::Path) -> anyhow::Result<ObjectAddress> {
    let path = dir.join("p.png");
    image::RgbImage::from_fn(60, 40, |x, y| image::Rgb([x as u8 * 4, y as u8 * 6, 120]))
        .save_with_format(&path, image::ImageFormat::Png)?;
    let v = s.run("picture.insert", json!({"path": path, "width_mm": 40.0}))?;
    Ok(serde_json::from_value(v["address"].clone())?)
}

fn effects(s: &mut Session, a: ObjectAddress) -> anyhow::Result<Value> {
    Ok(s.run("picture.effects.get", json!({"address": a}))?["effects"].clone())
}

fn page_svg(s: &Session) -> anyhow::Result<String> {
    s.render_page_svg(0)
}

#[test]
fn each_effect_draws_round_trips_through_hwpx_and_undoes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cases = [
        (
            "shadow",
            json!({"transparency": 60, "blur_mm": 1.5, "distance_mm": 2.5, "direction": 45, "color": "#203040"}),
            "feOffset",
        ),
        (
            "shadow",
            json!({"style": "inside", "transparency": 30, "blur_mm": 1, "distance_mm": 1, "direction": 135}),
            "tableValues=\"1 0\"",
        ),
        (
            "glow",
            json!({"transparency": 40, "size_mm": 2, "color": "#FFC000"}),
            "dilate",
        ),
        ("soft_edge", json!({"size_mm": 1.5}), "erode"),
        (
            "reflection",
            json!({"transparency_start": 40, "transparency_end": 100, "size": 60, "distance_mm": 1, "blur_mm": 0.5}),
            "scale(1 -1)",
        ),
    ];
    for (effect, value, marker) in cases {
        let mut s = Session::new()?;
        let a = picture(&mut s, dir.path())?;
        let plain = page_svg(&s)?;
        let set = s.run("picture.effects", json!({"address": a, effect: value}))?;
        assert!(set["effects"][effect].is_object(), "{effect}: {set}");
        let drawn = page_svg(&s)?;
        assert!(drawn.contains(marker), "{effect}: drawing lacks {marker}");
        assert_eq!(drawn, page_svg(&s)?, "{effect}: drawing is deterministic");
        let view = effects(&mut s, a)?;

        let path = dir.path().join(format!("{effect}.hwpx"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let b = serde_json::from_value::<ObjectAddress>(
            reopened
                .objects()
                .as_array()
                .and_then(|list| list.iter().find(|o| o["kind"] == "picture"))
                .map(|o| o["address"].clone())
                .unwrap_or_default(),
        )?;
        assert_eq!(
            effects(&mut reopened, b)?,
            view,
            "{effect}: HWPX round trip"
        );
        assert!(
            page_svg(&reopened)?.contains(marker),
            "{effect}: reopened drawing"
        );

        s.execute(hwpcraft_engine::Command::Undo)?;
        assert!(effects(&mut s, a)?[effect].is_null(), "{effect}: undo");
        assert_eq!(page_svg(&s)?, plain, "{effect}: undo restores the drawing");
        s.execute(hwpcraft_engine::Command::Redo)?;
        assert_eq!(effects(&mut s, a)?, view, "{effect}: redo");
    }
    Ok(())
}

#[test]
fn effects_combine_remove_and_clear() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    let a = picture(&mut s, dir.path())?;
    s.run(
        "picture.effects",
        json!({"address": a, "shadow": {}, "glow": {}, "soft_edge": {}, "reflection": {}}),
    )?;
    let all = effects(&mut s, a)?;
    for key in ["shadow", "glow", "soft_edge", "reflection"] {
        assert!(all[key].is_object(), "{key}: {all}");
    }
    s.run("picture.effects", json!({"address": a, "glow": null}))?;
    let without_glow = effects(&mut s, a)?;
    assert!(without_glow["glow"].is_null() && without_glow["shadow"] == all["shadow"]);
    s.run("picture.effects", json!({"address": a, "clear": true}))?;
    let cleared = effects(&mut s, a)?;
    assert!(
        ["shadow", "glow", "soft_edge", "reflection"]
            .iter()
            .all(|k| cleared[*k].is_null())
    );
    Ok(())
}

#[test]
fn bad_requests_change_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    let a = picture(&mut s, dir.path())?;
    let generation = s.generation;
    for params in [
        json!({"address": a, "shadow": {"transparency": 120}}),
        json!({"address": a, "glow": {"color": "orange"}}),
        json!({"address": a, "reflection": {"size": 0}}),
        json!({"address": a, "sparkle": {}}),
        json!({"address": a, "shadow": {"bogus": 1}}),
        json!({"shadow": {}}),
    ] {
        assert!(
            s.run("picture.effects", params.clone()).is_err(),
            "{params}"
        );
    }
    assert_eq!(s.generation, generation);
    s.execute(hwpcraft_engine::Command::Insert("글".into()))?;
    let text = ObjectAddress {
        section: 0,
        paragraph: 0,
        control: 0,
    };
    assert!(
        s.run("picture.effects", json!({"address": text, "glow": {}}))
            .is_err()
    );
    Ok(())
}

#[test]
fn hwp_save_refuses_effects_it_cannot_write_and_keeps_the_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    let a = picture(&mut s, dir.path())?;
    s.run(
        "picture.effects",
        json!({"address": a, "glow": {}, "shadow": {}}),
    )?;
    let path = dir.path().join("out.hwp");
    std::fs::write(&path, b"existing")?;
    assert_eq!(s.picture_effects_blocking_save(&path), 2);
    assert_eq!(
        s.picture_effects_blocking_save(&dir.path().join("out.hwpx")),
        0
    );
    let error = s.save(&path).expect_err("HWP cannot hold the effects yet");
    assert!(error.to_string().contains("dropPictureEffects"), "{error}");
    assert_eq!(std::fs::read(&path)?, b"existing");

    // A drop that cannot be written puts the effects back.
    let folder = dir.path().join("folder.hwp");
    std::fs::create_dir(&folder)?;
    assert!(s.save_dropping_picture_effects(&folder, false).is_err());
    assert!(effects(&mut s, a)?["glow"].is_object());

    assert_eq!(s.save_dropping_picture_effects(&path, false)?, 2);
    assert!(
        effects(&mut s, a)?["glow"].is_null(),
        "dropped from the document too"
    );
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    assert_eq!(reopened.picture_effects_blocking_save(&path), 0);
    s.execute(hwpcraft_engine::Command::Undo)?;
    assert!(
        effects(&mut s, a)?["glow"].is_object(),
        "dropping is one undo step"
    );
    // Nothing to drop: a plain save, also to HWPX.
    assert_eq!(
        s.save_dropping_picture_effects(&dir.path().join("keep.hwpx"), false)?,
        0
    );
    assert!(effects(&mut s, a)?["glow"].is_object());
    Ok(())
}

/// Every picture shadow in the document, wherever the picture sits.
fn shadows(s: &Session) -> Vec<String> {
    use hwpcraft_doc::model::control::Control;
    use hwpcraft_doc::model::paragraph::Paragraph;
    use hwpcraft_doc::model::shape::ShapeObject;
    fn shape(s: &ShapeObject, out: &mut Vec<String>) {
        match s {
            ShapeObject::Picture(p) => {
                out.extend(p.effects.shadow.as_ref().map(|x| format!("{x:?}")))
            }
            ShapeObject::Group(g) => g.children.iter().for_each(|c| shape(c, out)),
            other => {
                if let Some(t) = other.drawing().and_then(|d| d.text_box.as_ref()) {
                    paragraphs(&t.paragraphs, out);
                }
            }
        }
    }
    fn paragraphs(list: &[Paragraph], out: &mut Vec<String>) {
        for c in list.iter().flat_map(|p| &p.controls) {
            match c {
                Control::Picture(p) => {
                    out.extend(p.effects.shadow.as_ref().map(|x| format!("{x:?}")))
                }
                Control::Table(t) => t
                    .cells
                    .iter()
                    .for_each(|cell| paragraphs(&cell.paragraphs, out)),
                Control::Shape(sh) => shape(sh, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for section in &s.core.document().sections {
        paragraphs(&section.paragraphs, &mut out);
    }
    out
}

/// The shadow saved by Hangul in RHWP's `aift.hwpx` sample survives an HWPX save
/// attribute for attribute, and is drawn.
#[test]
fn hangul_saved_shadow_round_trips_and_draws() -> anyhow::Result<()> {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/hwpx/aift.hwpx");
    assert!(sample.exists(), "RHWP sample missing: {sample:?}");
    let mut s = Session::new()?;
    s.open(&sample)?;
    let before = shadows(&s);
    assert_eq!(before.len(), 1, "one Hangul shadow in the sample");
    assert!(
        before[0].contains("\"0.8\"") && before[0].contains("\"45\""),
        "{}",
        before[0]
    );
    let drawn = (0..s.page_count()?)
        .filter(|p| {
            s.render_page_svg(*p)
                .is_ok_and(|svg| svg.contains("feOffset"))
        })
        .count();
    assert_eq!(drawn, 1, "the shadow is drawn on one page");
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("aift.hwpx");
    s.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    assert_eq!(shadows(&reopened), before);

    let hwp = dir.path().join("aift.hwp");
    assert!(
        s.save(&hwp).is_err(),
        "the Hangul shadow is not dropped silently"
    );
    assert_eq!(s.save_dropping_picture_effects(&hwp, false)?, 1);
    let mut converted = Session::new()?;
    converted.open(&hwp)?;
    assert_eq!(converted.page_count()?, s.page_count()?);
    Ok(())
}
