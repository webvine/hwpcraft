//! picture.insert places a picture like Hangul's 그림 넣기: as a character by
//! default (0.19), or, asked to float, anchored at the caret paragraph with text
//! flowing beside it — never at the paper's corner.
use hwpcraft_engine::{Command, Cursor, ObjectAddress, Session};
use serde_json::{Value, json};

fn png(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let path = dir.join("p.png");
    image::RgbImage::from_pixel(60, 40, image::Rgb([68, 114, 196])).save(&path)?;
    Ok(path)
}

fn props(s: &mut Session, a: ObjectAddress) -> anyhow::Result<Value> {
    Ok(s.run("object.properties.get", json!({"address": a}))?["props"].clone())
}

/// (x, y, width, height) of the first image drawn on page 1.
fn image_box(svg: &str) -> Option<(f64, f64, f64, f64)> {
    let tag = &svg[svg.find("<image")?..];
    let tag = &tag[..tag.find('>')?];
    let attr = |k: &str| -> Option<f64> {
        tag.split(&format!(" {k}=\""))
            .nth(1)?
            .split('"')
            .next()?
            .parse()
            .ok()
    };
    Some((attr("x")?, attr("y")?, attr("width")?, attr("height")?))
}

/// Left edges of the text glyphs drawn between `top` and `bottom`.
fn text_xs_between(svg: &str, top: f64, bottom: f64) -> Vec<f64> {
    svg.split("<text")
        .skip(1)
        .filter_map(|part| {
            let attr = |k: &str| -> Option<f64> {
                part.split(&format!(" {k}=\""))
                    .nth(1)?
                    .split('"')
                    .next()?
                    .parse()
                    .ok()
            };
            let (x, y) = (attr("x")?, attr("y")?);
            (y > top && y < bottom).then_some(x)
        })
        .collect()
}

#[test]
fn default_is_a_character_and_floating_pictures_anchor_at_the_caret() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = png(dir.path())?;
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "첫 문단입니다.
둘째 문단입니다.
"
        .into(),
    ))?;
    let made = s.run("picture.insert", json!({"path": path, "width_mm": 30.0}))?;
    let inline: ObjectAddress = serde_json::from_value(made["address"].clone())?;
    assert_eq!(props(&mut s, inline)?["treatAsChar"], true);

    // A floating picture put at the start of a paragraph of text.
    let long = "그림 옆으로 흐르는 본문입니다. ".repeat(12);
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.execute(Command::Insert(format!(
        "
{long}"
    )))?;
    let last = s.core.document().sections[0].paragraphs.len() - 1;
    s.cursor = Cursor {
        section: 0,
        paragraph: last,
        offset: 0,
        cell: None,
    };
    let made = s.run(
        "picture.insert",
        json!({"path": path, "width_mm": 40.0, "treat_as_char": false}),
    )?;
    let floating: ObjectAddress = serde_json::from_value(made["address"].clone())?;
    let p = props(&mut s, floating)?;
    assert_eq!(p["treatAsChar"], false, "{p}");
    assert_eq!(p["vertRelTo"], "Para", "{p}");
    assert_eq!(p["horzRelTo"], "Column", "{p}");
    assert_eq!(p["textWrap"], "Square", "{p}");

    // Drawn in the body below the earlier paragraphs, not at the paper's corner,
    // and the text beside it starts right of it (side-flow band).
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let svg = s.render_page_svg(0)?;
        let (x, y, w, h) = svg
            .match_indices("<image")
            .filter_map(|(i, _)| image_box(&svg[i..]))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or_else(|| anyhow::anyhow!("{label}: floating picture not drawn"))?;
        assert!(
            y > 120.0 && x > 50.0,
            "{label}: picture at ({x}, {y}) is not in the body"
        );
        let beside = text_xs_between(&svg, y + 2.0, y + h - 2.0);
        assert!(!beside.is_empty(), "{label}: no text beside the picture");
        assert!(
            beside.iter().all(|tx| *tx >= x + w - 1.0),
            "{label}: text beside the picture overlaps it ({x}+{w}): {beside:?}"
        );
        Ok(())
    };
    check(&s, "live")?;
    for ext in ["hwp", "hwpx"] {
        let file = dir.path().join(format!("placed.{ext}"));
        s.save(&file)?;
        let mut r = Session::new()?;
        r.open(&file)?;
        let q = props(&mut r, floating)?;
        for key in [
            "treatAsChar",
            "vertRelTo",
            "horzRelTo",
            "textWrap",
            "vertOffset",
            "horzOffset",
        ] {
            assert_eq!(q[key], p[key], "{ext}: {key}");
        }
        check(&r, ext)?;
    }
    Ok(())
}

#[test]
fn pictures_still_go_into_table_cells() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = png(dir.path())?;
    for treat_as_char in [true, false] {
        let mut s = Session::new()?;
        s.run("table.create", json!({"rows": 1, "columns": 1}))?;
        let made = s.run(
            "picture.insert",
            json!({"path": path, "width_mm": 20.0, "treat_as_char": treat_as_char}),
        )?;
        assert!(made.get("address").is_some(), "{treat_as_char}: {made}");
    }
    Ok(())
}

/// A picture put in a table cell stays at that cell: RHWP anchors it to the paper
/// at the cell's top-left corner, and HwpCraft no longer re-anchors it to the
/// host paragraph (which put every cell's picture at the table's first cell, or
/// beside the table as a character).
#[test]
fn a_picture_in_a_cell_is_drawn_at_that_cell() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = png(dir.path())?;
    for treat_as_char in [true, false] {
        let mut boxes = Vec::new();
        for (row, col) in [(0, 0), (1, 1)] {
            let mut s = Session::new()?;
            let table = s.run("insert.table", json!({"rows": 2, "columns": 2}))?["table"].clone();
            s.run(
                "table.edit",
                json!({"action": "fit_page", "table": table, "row": row, "col": col}),
            )?;
            s.run(
                "picture.insert",
                json!({"path": path, "width_mm": 10.0, "treat_as_char": treat_as_char}),
            )?;
            let svg = s.render_page_svg(0)?;
            boxes.push(image_box(&svg).ok_or_else(|| anyhow::anyhow!("no image"))?);
        }
        let ((x0, y0, ..), (x1, y1, ..)) = (boxes[0], boxes[1]);
        assert!(x1 > x0 + 100.0 && y1 > y0, "{treat_as_char}: {boxes:?}");
    }
    Ok(())
}

/// Typing a long run into the empty paragraph right after inserting a floating
/// picture flows the text beside it (RHWP 0033 re-projects the band).
#[test]
fn text_typed_after_a_floating_picture_flows_beside_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = png(dir.path())?;
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "첫 문단
"
        .into(),
    ))?;
    s.run(
        "picture.insert",
        json!({"path": path, "width_mm": 40.0, "treat_as_char": false}),
    )?;
    s.execute(Command::Insert(
        "그림 옆으로 흐르는 본문입니다. ".repeat(12),
    ))?;
    let svg = s.render_page_svg(0)?;
    let (x, y, w, h) = svg
        .match_indices("<image")
        .find_map(|(i, _)| image_box(&svg[i..]))
        .ok_or_else(|| anyhow::anyhow!("picture not drawn"))?;
    let beside = text_xs_between(&svg, y + 2.0, y + h - 2.0);
    assert!(!beside.is_empty(), "no text beside the picture");
    assert!(
        beside.iter().all(|tx| *tx >= x + w - 1.0),
        "text typed after the picture overlaps it ({x}+{w}): {beside:?}"
    );
    Ok(())
}
