//! 바탕쪽 그림·도형: objects placed on the paper of every page a master page covers,
//! kept through master-page text edits and both file formats, one undo step each.
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn png(dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let path = dir.join("logo.png");
    image::RgbImage::from_pixel(60, 30, image::Rgb([68, 114, 196])).save(&path)?;
    Ok(path)
}

fn objects(s: &mut Session) -> anyhow::Result<Vec<Value>> {
    Ok(s.run("page.master.object.list", json!({}))?["objects"]
        .as_array()
        .cloned()
        .unwrap_or_default())
}

/// Pictures drawn on `page`, as their x position in page pixels.
fn images(s: &Session, page: u32) -> anyhow::Result<Vec<i64>> {
    let svg = s.render_page_svg(page)?;
    Ok(svg
        .split("<image")
        .skip(1)
        .filter_map(|t| {
            let i = t.find(" x=\"")? + 4;
            let j = t[i..].find('"')?;
            t[i..i + j].parse::<f64>().ok().map(|v| v.round() as i64)
        })
        .collect())
}

#[test]
fn master_page_objects_are_placed_kept_and_removed() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let logo = png(dir.path())?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽 본문".into()))?;
    s.run("insert.page_break", json!({}))?;
    s.execute(Command::Insert("둘째 쪽 본문".into()))?;
    let body = s.run("document.text", json!({}))?;
    s.run(
        "page.master.object.insert",
        json!({"applyTo": "both", "kind": "picture", "path": logo,
            "x_mm": 150.0, "y_mm": 10.0, "width_mm": 30.0}),
    )?;
    s.run(
        "page.master.object.insert",
        json!({"applyTo": "both", "kind": "rectangle", "x_mm": 20.0, "y_mm": 280.0,
            "width_mm": 170.0, "height_mm": 4.0, "wrap": "front"}),
    )?;
    let list = objects(&mut s)?;
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(
        (
            list[0]["kind"].clone(),
            list[0]["x_mm"].clone(),
            list[0]["y_mm"].clone()
        ),
        (json!("picture"), json!(150.0), json!(10.0))
    );
    assert_eq!(list[0]["width_mm"], 30.0);
    assert_eq!(list[0]["wrap"], "behind");
    assert_eq!(
        (list[1]["kind"].clone(), list[1]["wrap"].clone()),
        (json!("shape"), json!("front"))
    );
    // The body is as it was; the picture is drawn at 150 mm on every page.
    assert_eq!(s.run("document.text", json!({}))?, body);
    assert_eq!(s.page_count()?, 2);
    let x = (150.0_f64 * 96.0 / 25.4).round() as i64;
    for page in 0..2 {
        let drawn = images(&s, page)?;
        assert!(
            drawn.iter().any(|v| (v - x).abs() <= 1),
            "page {page}: {drawn:?}"
        );
    }
    // A text edit of the master page keeps its objects.
    s.run(
        "page.master.set",
        json!({"applyTo": "both", "text": "대외비"}),
    )?;
    assert_eq!(objects(&mut s)?, list);
    assert_eq!(
        s.run("page.master.get", json!({"applyTo": "both"}))?["text"],
        "대외비"
    );
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("master.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(objects(&mut r)?, list, "{ext}");
        assert!(
            images(&r, 1)?.iter().any(|v| (v - x).abs() <= 1),
            "{ext}: drawn on page 2"
        );
    }
    // One undo step each: the text edit, then the rectangle.
    s.execute(Command::Undo)?;
    assert_eq!(objects(&mut s)?.len(), 2);
    s.execute(Command::Undo)?;
    assert_eq!(objects(&mut s)?.len(), 1);
    s.execute(Command::Redo)?;
    s.run(
        "page.master.object.delete",
        json!({"applyTo": "both", "index": 0}),
    )?;
    let left = objects(&mut s)?;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["kind"], "shape");
    assert!(images(&s, 0)?.is_empty(), "the picture is gone");
    assert_eq!(s.run("document.text", json!({}))?, body);
    Ok(())
}

#[test]
fn bad_master_object_requests_change_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let logo = png(dir.path())?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let generation = s.generation;
    for params in [
        json!({"applyTo": "first", "kind": "rectangle", "x_mm": 1, "y_mm": 1, "width_mm": 10}),
        json!({"applyTo": "both", "kind": "star", "x_mm": 1, "y_mm": 1, "width_mm": 10}),
        json!({"applyTo": "both", "kind": "picture", "x_mm": 1, "y_mm": 1, "width_mm": 10}),
        json!({"applyTo": "both", "kind": "rectangle", "path": logo, "x_mm": 1, "y_mm": 1, "width_mm": 10}),
        json!({"applyTo": "both", "kind": "picture", "path": logo, "x_mm": 1, "y_mm": 1, "width_mm": 10, "height_mm": 5}),
        json!({"applyTo": "both", "kind": "rectangle", "x_mm": -1, "y_mm": 1, "width_mm": 10}),
        json!({"applyTo": "both", "kind": "rectangle", "x_mm": 1, "y_mm": 1, "width_mm": 10, "wrap": "square"}),
        json!({"applyTo": "both", "kind": "picture", "path": dir.path().join("missing.png"), "x_mm": 1, "y_mm": 1, "width_mm": 10}),
    ] {
        assert!(
            s.run("page.master.object.insert", params.clone()).is_err(),
            "{params}"
        );
    }
    assert!(
        s.run(
            "page.master.object.delete",
            json!({"applyTo": "both", "index": 0})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    assert!(objects(&mut s)?.is_empty());
    Ok(())
}
