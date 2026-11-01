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

const HU_PER_MM: f64 = 7200.0 / 25.4;
fn hu(mm: f64) -> i64 {
    (mm * HU_PER_MM).round() as i64
}
fn px(mm: f64) -> f64 {
    mm * 96.0 / 25.4
}

/// Outlined rectangles drawn on `page` (not the white page itself): x, y, width, height
/// in page pixels.
fn rectangles(s: &Session, page: u32) -> anyhow::Result<Vec<[f64; 4]>> {
    let svg = s.render_page_svg(page)?;
    Ok(svg
        .split("<rect ")
        .skip(1)
        .filter_map(|t| {
            let t = format!(" {}", t.split('>').next()?);
            if !t.contains("stroke=") {
                return None;
            }
            let attr = |name: &str| -> Option<f64> {
                let i = t.find(&format!(" {name}=\""))? + name.len() + 3;
                let j = t[i..].find('"')?;
                t[i..i + j].parse().ok()
            };
            Some([attr("x")?, attr("y")?, attr("width")?, attr("height")?])
        })
        .collect())
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0
}

fn object(s: &mut Session, kind: &str) -> anyhow::Result<Value> {
    objects(s)?
        .into_iter()
        .find(|o| o["kind"] == kind)
        .ok_or_else(|| anyhow::anyhow!("no {kind}"))
}

#[test]
fn master_objects_move_and_resize_through_the_body_object_commands() -> anyhow::Result<()> {
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
        json!({"applyTo": "both", "kind": "rectangle", "x_mm": 20.0, "y_mm": 30.0,
            "width_mm": 50.0, "height_mm": 10.0, "wrap": "front"}),
    )?;
    let rect = object(&mut s, "shape")?;
    assert_eq!(
        rect["address"],
        json!({"section": 0, "paragraph": 0, "control": 1, "master": "both"})
    );
    let got = s.run("object.properties.get", json!({"address": rect["address"]}))?;
    assert_eq!(got["kind"], "shape");
    assert_eq!(got["props"]["horzOffset"], hu(20.0));

    // 크기·자리: object.properties, one undo step, drawn on every page.
    let generation = s.generation;
    s.run(
        "object.properties",
        json!({"address": rect["address"], "props": {
            "width": hu(70.0), "height": hu(15.0),
            "horzOffset": hu(40.0), "vertOffset": hu(60.0)}}),
    )?;
    assert_eq!(s.generation, generation + 1);
    let moved = object(&mut s, "shape")?;
    assert_eq!(
        [
            &moved["x_mm"],
            &moved["y_mm"],
            &moved["width_mm"],
            &moved["height_mm"]
        ],
        [&json!(40.0), &json!(60.0), &json!(70.0), &json!(15.0)]
    );
    let expected = [px(40.0), px(60.0), px(70.0), px(15.0)];
    for page in 0..2 {
        let drawn = rectangles(&s, page)?;
        assert!(
            drawn
                .iter()
                .any(|r| r.iter().zip(expected).all(|(a, b)| near(*a, b))),
            "page {page}: {drawn:?}"
        );
    }
    // 옮기기: object.move_by and object.size on the picture.
    let picture = object(&mut s, "picture")?;
    s.run(
        "object.move_by",
        json!({"addresses": [picture["address"]], "dx_mm": -10.0, "dy_mm": 5.0}),
    )?;
    s.run(
        "object.size",
        json!({"address": picture["address"], "width_mm": 20.0, "keep_ratio": true}),
    )?;
    let picture = object(&mut s, "picture")?;
    assert_eq!(
        (&picture["x_mm"], &picture["y_mm"]),
        (&json!(140.0), &json!(15.0))
    );
    assert_eq!(picture["width_mm"], 20.0);
    assert_eq!(picture["height_mm"], 10.0);
    let x = px(140.0).round() as i64;
    for page in 0..2 {
        let drawn = images(&s, page)?;
        assert!(
            drawn.iter().any(|v| (v - x).abs() <= 1),
            "page {page}: {drawn:?}"
        );
    }
    assert_eq!(s.run("document.text", json!({}))?, body);
    let list = objects(&mut s)?;
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("moved.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(objects(&mut r)?, list, "{ext}");
        assert!(
            rectangles(&r, 1)?
                .iter()
                .any(|d| d.iter().zip(expected).all(|(a, b)| near(*a, b))),
            "{ext}: drawn on page 2"
        );
        // The reopened master page is edited the same way.
        let rect = object(&mut r, "shape")?;
        r.run(
            "object.move_by",
            json!({"addresses": [rect["address"]], "dx_mm": 5.0, "dy_mm": 0.0}),
        )?;
        assert_eq!(object(&mut r, "shape")?["x_mm"], 45.0, "{ext}");
    }
    // Each command was one undo step.
    s.execute(Command::Undo)?;
    assert_eq!(object(&mut s, "picture")?["width_mm"], 30.0);
    assert_eq!(object(&mut s, "picture")?["x_mm"], 140.0);
    s.execute(Command::Undo)?;
    assert_eq!(object(&mut s, "picture")?["x_mm"], 150.0);
    s.execute(Command::Undo)?;
    assert_eq!(object(&mut s, "shape")?["x_mm"], 20.0);
    assert_eq!(object(&mut s, "shape")?["width_mm"], 50.0);
    Ok(())
}

#[test]
fn bad_master_object_addresses_change_nothing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.run(
        "page.master.object.insert",
        json!({"applyTo": "both", "kind": "rectangle", "x_mm": 20.0, "y_mm": 30.0,
            "width_mm": 50.0, "height_mm": 10.0}),
    )?;
    let rect = object(&mut s, "shape")?["address"].clone();
    let generation = s.generation;
    let list = objects(&mut s)?;
    let with = |key: &str, value: Value| {
        let mut a = rect.clone();
        a[key] = value;
        a
    };
    for (id, params) in [
        // No such master page, kind or object.
        (
            "object.properties",
            json!({"address": with("master", json!("odd")), "props": {"width": 9000}}),
        ),
        (
            "object.properties",
            json!({"address": with("master", json!("first")), "props": {"width": 9000}}),
        ),
        (
            "object.properties",
            json!({"address": with("control", json!(5)), "props": {"width": 9000}}),
        ),
        (
            "object.properties",
            json!({"address": with("section", json!(3)), "props": {"width": 9000}}),
        ),
        // Body and master objects together.
        (
            "object.move_by",
            json!({"addresses": [rect, {"section": 0, "paragraph": 0, "control": 0}], "dx_mm": 1.0, "dy_mm": 1.0}),
        ),
        // Commands that do not take master addresses.
        (
            "object.delete",
            json!({"section": 0, "paragraph": 0, "control": 1, "master": "both"}),
        ),
        ("chart.data.get", json!({"address": rect})),
        // The body command's own checks still apply.
        (
            "object.properties",
            json!({"address": rect, "props": {"rotationAngle": "x"}}),
        ),
    ] {
        assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
    }
    assert_eq!(s.generation, generation);
    assert_eq!(objects(&mut s)?, list);
    Ok(())
}

#[test]
fn master_page_text_is_edited_by_paragraph_through_context_v2() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽".into()))?;
    s.run("insert.page_break", json!({}))?;
    s.execute(Command::Insert("둘째 쪽".into()))?;
    s.run(
        "page.master.set",
        json!({"applyTo": "both", "text": "대외비\n둘째 줄"}),
    )?;
    // Master pages are listed only when asked for.
    let all = s.run("context.v2.list", json!({"limit": 1000}))?;
    assert!(!all.to_string().contains("master_page"));
    let list = s.run("context.v2.list", json!({"include": ["master_page"]}))?;
    let rows = list["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 2, "{list}");
    assert_eq!(rows[0]["kind"], "master_page");
    assert_eq!(rows[0]["text"], "대외비");
    let address = rows[0]["address"].clone();
    assert_eq!(address["path"][0]["kind"], "master_page");
    let generation = s.generation;
    s.run(
        "context.v2.replace",
        json!({"address": address, "start": 0, "end": 3, "text": "사내 한정",
            "expected_generation": generation}),
    )?;
    assert_eq!(s.generation, generation + 1);
    assert_eq!(
        s.run("page.master.get", json!({"applyTo": "both"}))?["text"],
        "사내 한정\n둘째 줄"
    );
    for page in 0..2 {
        assert!(s.render_page_svg(page)?.contains(">한<"), "page {page}");
    }
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("text.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(
            r.run("page.master.get", json!({"applyTo": "both"}))?["text"],
            "사내 한정\n둘째 줄",
            "{ext}"
        );
        assert!(r.render_page_svg(1)?.contains(">한<"), "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("page.master.get", json!({"applyTo": "both"}))?["text"],
        "대외비\n둘째 줄"
    );
    Ok(())
}

/// RHWP's exam sample (20 pages, 3 sections): the odd and even master pages hold a
/// paper-relative 1×3 table and a shape; the first page shows no master page.
fn exam_samples() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../rhwp/samples");
    ["exam_kor.hwp", "hwpx/exam_kor.hwpx"]
        .iter()
        .map(|name| root.join(name))
        .filter(|path| path.exists())
        .collect()
}

/// The `context.v2.list` row of a master-page paragraph (or one nested in it) that
/// `pick` chooses.
fn master_context(s: &mut Session, pick: impl Fn(&Value) -> bool) -> anyhow::Result<Value> {
    let list = s.run(
        "context.v2.list",
        json!({"include": ["master_page", "table_cell", "textbox"], "limit": 1000}),
    )?;
    list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["address"]["path"][0]["kind"] == "master_page" && pick(row))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no such master context"))
}

/// The first five pages that show the edited odd master page (its new "Z" and "Q").
fn marked_pages(s: &Session) -> anyhow::Result<Vec<u32>> {
    let mut pages = vec![];
    for page in 0..s.page_count()?.min(5) {
        let svg = s.render_page_svg(page)?;
        if svg.contains(">Z<") && svg.contains(">Q<") {
            pages.push(page);
        }
    }
    Ok(pages)
}

/// The empty second line of the odd master page's table (six figure spaces).
fn odd_cell(row: &Value) -> bool {
    row["kind"] == "table_cell" && row["text"] == "\u{2007}".repeat(6)
}

fn odd_table(s: &mut Session) -> anyhow::Result<Value> {
    objects(s)?
        .into_iter()
        .find(|o| o["kind"] == "table" && o["applyTo"] == "odd")
        .ok_or_else(|| anyhow::anyhow!("no odd master table"))
}

#[test]
fn master_pages_with_tables_take_text_cell_and_object_edits() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for sample in exam_samples() {
        let mut s = Session::new()?;
        s.open(&sample)?;
        let name = sample.display().to_string();
        // 표 셀 글: a cell of the odd master page's table, and the text box in another
        // cell of it.
        let cell = master_context(&mut s, odd_cell)?;
        let master = cell["address"]["path"][0]["master"].clone();
        let generation = s.generation;
        s.run(
            "context.v2.replace",
            json!({"address": cell["address"], "start": 0, "end": 6, "text": "Z확인",
                "expected_generation": generation}),
        )?;
        assert_eq!(s.generation, generation + 1, "{name}");
        let label = master_context(&mut s, |row| {
            row["kind"] == "textbox"
                && row["text"] == "홀수형"
                && row["address"]["path"][0]["master"] == master
        })?;
        s.run(
            "context.v2.replace",
            json!({"address": label["address"], "start": 0, "end": 2, "text": "Q",
                "expected_generation": s.generation}),
        )?;
        let drawn = marked_pages(&s)?;
        assert!(
            drawn.contains(&2) && drawn.iter().all(|p| p % 2 == 0 && *p > 0),
            "{name}: {drawn:?}"
        );
        // 바탕쪽 글: the odd master page holds a table and takes a whole-text edit,
        // keeping the table and the shape.
        let got = s.run("page.master.get", json!({"applyTo": "odd"}))?;
        assert_eq!(got["editable"], true, "{name}: {got}");
        let before = objects(&mut s)?;
        s.run(
            "page.master.set",
            json!({"applyTo": "odd", "text": "홀수 바탕쪽 글"}),
        )?;
        let after = objects(&mut s)?;
        assert_eq!(after.len(), before.len(), "{name}");
        // 표 자리: object.properties with the master address.
        let table = odd_table(&mut s)?;
        let props = s.run(
            "object.properties.get",
            json!({"address": table["address"]}),
        )?;
        assert_eq!(props["kind"], "table", "{name}");
        let x = props["props"]["horzOffset"].as_i64().unwrap_or(0);
        s.run(
            "object.properties",
            json!({"address": table["address"], "props": {"horzOffset": x + hu(10.0)}}),
        )?;
        let moved = odd_table(&mut s)?;
        let shift = moved["x_mm"].as_f64().unwrap_or(0.0) - table["x_mm"].as_f64().unwrap_or(0.0);
        assert!((shift - 10.0).abs() < 0.02, "{name}: {shift}");
        let list = objects(&mut s)?;
        for ext in ["hwp", "hwpx"] {
            // The HWPX sample's third section has an 임의 쪽 master page, which only
            // HWPX keeps (`refuse_unkept_hwp_save`).
            if ext == "hwp" && name.ends_with(".hwpx") {
                assert!(s.save(&dir.path().join("refused.hwp")).is_err());
                continue;
            }
            let path = dir.path().join(format!("exam.{ext}"));
            s.save(&path)?;
            let mut r = Session::new()?;
            r.open(&path)?;
            assert_eq!(objects(&mut r)?, list, "{name} → {ext}");
            assert_eq!(
                r.run("page.master.get", json!({"applyTo": "odd"}))?["text"],
                "홀수 바탕쪽 글",
                "{name} → {ext}"
            );
            let row = master_context(&mut r, |row| row["text"] == "Z확인")?;
            assert_eq!(row["kind"], "table_cell");
            master_context(&mut r, |row| row["text"] == "Q형")?;
            assert_eq!(marked_pages(&r)?, drawn, "{name} → {ext}");
        }
        // One undo step each, back to the opened document.
        s.execute(Command::Undo)?;
        assert_eq!(odd_table(&mut s)?, table, "{name}");
        s.execute(Command::Undo)?;
        assert_eq!(objects(&mut s)?, before, "{name}");
        s.execute(Command::Undo)?;
        master_context(&mut s, |row| {
            row["text"] == "홀수형" && row["address"]["path"][0]["master"] == master
        })?;
        s.execute(Command::Undo)?;
        master_context(&mut s, odd_cell)?;
        assert!(marked_pages(&s)?.is_empty(), "{name}");
    }
    Ok(())
}
