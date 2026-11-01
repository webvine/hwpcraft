//! A rectangle or text box treated as a character in the middle of a paragraph raises
//! its line like an inline picture does, so the following lines are not drawn under it.
use hwpcraft_engine::Session;
use serde_json::{Value, json};

/// Laid-out (y, h) of the only control on page 1.
fn object_box(s: &Session) -> anyhow::Result<(f64, f64)> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let c = &layout["controls"][0];
    Ok((
        c["y"].as_f64().unwrap_or(f64::NAN),
        c["h"].as_f64().unwrap_or(f64::NAN),
    ))
}

/// Baselines (y) of the text drawn on page 1.
fn baselines(s: &Session) -> anyhow::Result<Vec<f64>> {
    let svg = s.render_page_svg(0)?;
    let mut out = Vec::new();
    for tag in svg.split("<text").skip(1) {
        let head = tag.split('>').next().unwrap_or("");
        if let Some(y) = head
            .split(" y=\"")
            .nth(1)
            .and_then(|v| v.split('"').next())
            .and_then(|v| v.parse::<f64>().ok())
        {
            out.push(y);
        }
    }
    Ok(out)
}

#[test]
fn inline_shapes_push_the_next_lines_down() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let png = dir.path().join("p.png");
    image::RgbImage::from_pixel(40, 40, image::Rgb([68, 114, 196])).save(&png)?;
    for (cmd, params) in [
        // The control group: pictures already raise their line.
        (
            "picture.insert",
            json!({"path": png, "width_mm": 40.0, "treat_as_char": true}),
        ),
        (
            "shape.insert",
            json!({"kind": "rectangle", "width_mm": 40.0, "height_mm": 27.0, "treat_as_char": true}),
        ),
        (
            "shape.insert",
            json!({"kind": "textbox", "width_mm": 40.0, "height_mm": 27.0, "treat_as_char": true,
                   "paragraphs": ["글상자"]}),
        ),
        (
            "insert.shape",
            json!({"kind": "textbox", "width_mm": 40.0, "height_mm": 27.0, "text": "글상자"}),
        ),
    ] {
        let mut s = Session::new()?;
        let text = "글자처럼 취급한 개체 주변 본문 흐름 확인용 문장입니다. ".repeat(8);
        s.run("text.insert", json!({ "text": text }))?;
        s.run(
            "selection.set",
            json!({"anchor": null, "cursor": {"cell": null, "offset": 10, "paragraph": 0, "section": 0}}),
        )?;
        s.run(cmd, params.clone())?;
        let (top, h) = object_box(&s)?;
        let lines = baselines(&s)?;
        // Distinct baselines within the object's height: its own line sits on the object's
        // bottom (one baseline, above the line's descent), and a text box draws its own text
        // inside (one more). More than that are following lines drawn under the object.
        let mut under: Vec<f64> = lines
            .iter()
            .map(|y| (y * 10.0).round() / 10.0)
            .filter(|y| *y > top + 2.0 && *y < top + h - 2.0)
            .collect();
        under.sort_by(f64::total_cmp);
        under.dedup();
        let allowed = if params["kind"] == "textbox" { 2 } else { 1 };
        assert!(
            under.len() <= allowed,
            "{cmd} {params}: text lines drawn under the object ({top} + {h}): {under:?} of {lines:?}"
        );
    }
    Ok(())
}
