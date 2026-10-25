use hwpcraft_doc::model::{control::Control, shape::ShapeObject};
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn images(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let img = image::RgbImage::from_fn(40, 20, |x, y| image::Rgb([x as u8 * 6, y as u8 * 12, 90]));
    let mut out = Vec::new();
    for (name, format) in [
        ("a.png", image::ImageFormat::Png),
        ("b.jpg", image::ImageFormat::Jpeg),
        ("c.bmp", image::ImageFormat::Bmp),
        ("d.gif", image::ImageFormat::Gif),
    ] {
        let path = dir.join(name);
        img.save_with_format(&path, format)?;
        out.push(path);
    }
    Ok(out)
}
fn address(v: &Value) -> anyhow::Result<ObjectAddress> {
    Ok(serde_json::from_value(v["address"].clone())?)
}
fn kinds(s: &Session) -> Vec<String> {
    s.objects()
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| o["kind"].as_str().unwrap_or("").to_owned())
        .collect()
}
fn reopen(s: &mut Session, ext: &str) -> anyhow::Result<Session> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(format!("objects.{ext}"));
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    Ok(r)
}
fn first(s: &Session, kind: &str) -> anyhow::Result<ObjectAddress> {
    let list = s.objects();
    let item = list
        .as_array()
        .and_then(|v| v.iter().find(|o| o["kind"] == kind))
        .ok_or_else(|| anyhow::anyhow!("no {kind}: {list}"))?;
    address(item)
}

#[test]
fn pictures_in_four_formats_with_properties_roundtrip() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("그림 앞".into()))?;
    for (i, path) in images(dir.path())?.iter().enumerate() {
        let v = s.run(
            "picture.insert",
            json!({"path":path,"width_mm":30.0 + i as f64,"treat_as_char":i % 2 == 0}),
        )?;
        let a = address(&v)?;
        let p = s.run("object.properties.get", json!({"address":a}))?;
        assert_eq!(p["kind"], "picture");
        assert_eq!(p["props"]["treatAsChar"], i % 2 == 0, "{path:?}");
        let w = p["props"]["width"].as_f64().unwrap_or(0.0);
        assert!((w - (30.0 + i as f64) * 7200.0 / 25.4).abs() < 2.0, "{w}");
    }
    assert_eq!(kinds(&s).iter().filter(|k| *k == "picture").count(), 4);
    let a = first(&s, "picture")?;
    let before = s.run("object.properties.get", json!({"address":a}))?;
    s.run(
        "object.properties",
        json!({"address":a,"props":{"treatAsChar":false,"textWrap":"TopAndBottom","horzRelTo":"Column",
            "horzAlign":"Center","rotationAngle":30,"cropLeft":300,"paddingLeft":283,
            "outerMarginTop":567,"effect":"GrayScale","brightness":20,"hasCaption":true,
            "captionDirection":"Top","description":"테스트 그림"}}),
    )?;
    let p = s.run("object.properties.get", json!({"address":a}))?["props"].clone();
    assert_eq!(p["textWrap"], "TopAndBottom");
    assert_eq!(p["horzAlign"], "Center");
    assert_eq!(p["rotationAngle"], 30);
    assert_eq!(p["cropLeft"], 300);
    assert_eq!(p["paddingLeft"], 283);
    assert_eq!(p["outerMarginTop"], 567);
    assert_eq!(p["effect"], "GrayScale");
    assert_eq!(p["hasCaption"], true);
    assert_eq!(p["captionDirection"], "Top");
    // Caption details alone keep the caption.
    s.run(
        "object.properties",
        json!({"address":a,"props":{"captionDirection":"Bottom"}}),
    )?;
    let p = s.run("object.properties.get", json!({"address":a}))?["props"].clone();
    assert_eq!(
        (p["hasCaption"].clone(), p["captionDirection"].clone()),
        (json!(true), json!("Bottom"))
    );
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("object.properties.get", json!({"address":a}))?,
        before
    );
    s.execute(Command::Redo)?;
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(
            kinds(&r).iter().filter(|k| *k == "picture").count(),
            4,
            "{ext}"
        );
        let a = first(&r, "picture")?;
        let p = r.object_properties(a)?["props"].clone();
        assert_eq!(p["rotationAngle"], 30, "{ext}");
        assert_eq!(p["effect"], "GrayScale", "{ext}");
        assert_eq!(p["hasCaption"], true, "{ext}");
        assert_eq!(p["cropLeft"], 300, "{ext}");
    }
    s.run("object.delete", json!(a))?;
    assert_eq!(kinds(&s).iter().filter(|k| *k == "picture").count(), 3);
    Ok(())
}

#[test]
fn picture_rejects_bad_files_and_props_without_changes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let text = dir.path().join("note.txt");
    std::fs::write(&text, "not an image")?;
    let tiff = dir.path().join("x.tiff");
    std::fs::write(&tiff, b"II*\0\x08\0\0\0")?;
    let mut s = Session::new()?;
    let images = images(dir.path())?;
    s.run("picture.insert", json!({"path":images[0]}))?;
    let a = first(&s, "picture")?;
    let generation = s.generation;
    for (id, params) in [
        ("picture.insert", json!({"path":text})),
        ("picture.insert", json!({"path":tiff})),
        (
            "picture.insert",
            json!({"path":dir.path().join("missing.png")}),
        ),
        ("picture.insert", json!({"path":images[0],"width_mm":0.0})),
        ("picture.insert", json!({"path":images[0],"bogus":1})),
        (
            "object.properties",
            json!({"address":a,"props":{"rotationAngle":999}}),
        ),
        (
            "object.properties",
            json!({"address":a,"props":{"effect":"Sepia"}}),
        ),
        (
            "object.properties",
            json!({"address":a,"props":{"fillBgColor":1}}),
        ),
        (
            "object.properties",
            json!({"address":a,"props":{"description":"a\nb"}}),
        ),
        (
            "object.properties",
            json!({"address":{"section":0,"paragraph":0,"control":99},"props":{}}),
        ),
    ] {
        assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(s.generation, generation, "{id} {params}");
    }
    // A picture can also go into a table cell (RHWP floats it beside the table).
    s.run("table.create", json!({"rows":1,"columns":1}))?;
    s.run("picture.insert", json!({"path":images[1]}))?;
    assert_eq!(kinds(&s).iter().filter(|k| *k == "picture").count(), 2);
    Ok(())
}

#[test]
fn shapes_of_every_kind_with_line_fill_rotation_and_textbox_paragraphs() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("도형".into()))?;
    let mut addresses = Vec::new();
    for kind in [
        "line",
        "rectangle",
        "ellipse",
        "arc",
        "polygon",
        "curve",
        "textbox",
    ] {
        let mut p = json!({"kind":kind,"width_mm":40.0,"height_mm":20.0});
        if kind == "polygon" {
            p["points_mm"] = json!([[0.0, 20.0], [20.0, 0.0], [40.0, 20.0], [20.0, 10.0]]);
        }
        if kind == "textbox" {
            p["paragraphs"] = json!(["첫 문단", "", "셋째 문단 🦀"]);
        }
        addresses.push((kind, address(&s.run("shape.insert", p)?)?));
    }
    let shape_kind = |s: &Session, a: ObjectAddress| match &s.core.document().sections[a.section]
        .paragraphs[a.paragraph]
        .controls[a.control]
    {
        Control::Shape(shape) => match shape.as_ref() {
            ShapeObject::Line(_) => "line",
            ShapeObject::Rectangle(r) if r.drawing.text_box.is_some() => "textbox",
            ShapeObject::Rectangle(_) => "rectangle",
            ShapeObject::Ellipse(_) => "ellipse",
            ShapeObject::Arc(_) => "arc",
            ShapeObject::Polygon(_) => "polygon",
            ShapeObject::Curve(_) => "curve",
            _ => "other",
        },
        _ => "not a shape",
    };
    for (kind, a) in &addresses {
        assert_eq!(shape_kind(&s, *a), *kind);
    }
    let (_, rect) = addresses[1];
    s.run(
        "object.properties",
        json!({"address":rect,"props":{"borderColor":0x0000ff,"borderWidth":57,"lineType":2,
            "fillType":"solid","fillBgColor":0x00ffff,"rotationAngle":45,"roundRate":20,
            "textWrap":"Square","hasCaption":true}}),
    )?;
    let p = s.object_properties(rect)?["props"].clone();
    assert_eq!(p["borderColor"], 0x0000ff);
    assert_eq!(p["lineType"], 2);
    assert_eq!(p["fillType"], "solid");
    assert_eq!(p["fillBgColor"], 0x00ffff);
    assert_eq!(p["rotationAngle"], 45);
    assert_eq!(p["roundRate"], 20);
    assert_eq!(p["textWrap"], "Square");
    let (_, tb) = addresses[6];
    assert_eq!(
        s.run("textbox.paragraphs.get", json!({"address":tb}))?["paragraphs"],
        json!(["첫 문단", "", "셋째 문단 🦀"])
    );
    s.run(
        "textbox.paragraphs",
        json!({"address":tb,"paragraphs":["하나","둘"]}),
    )?;
    assert_eq!(s.textbox_paragraphs(tb)?, ["하나", "둘"]);
    s.execute(Command::Undo)?;
    assert_eq!(s.textbox_paragraphs(tb)?.len(), 3);
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        for (kind, a) in &addresses {
            assert_eq!(shape_kind(&r, *a), *kind, "{ext}");
        }
        assert_eq!(r.textbox_paragraphs(tb)?, ["하나", "둘"], "{ext}");
        assert_eq!(
            r.object_properties(rect)?["props"]["fillBgColor"],
            0x00ffff,
            "{ext}"
        );
    }
    let generation = s.generation;
    for p in [
        json!({"kind":"hexagon","width_mm":10.0,"height_mm":10.0}),
        json!({"kind":"rectangle","width_mm":0.0,"height_mm":10.0}),
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0,"paragraphs":["x"]}),
        json!({"kind":"polygon","width_mm":10.0,"height_mm":10.0,"points_mm":[[0.0,0.0],[50.0,0.0],[0.0,5.0]]}),
        json!({"kind":"polygon","width_mm":10.0,"height_mm":10.0,"points_mm":[[0.0,0.0]]}),
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0,"text_wrap":"Sideways"}),
    ] {
        assert!(s.run("shape.insert", p.clone()).is_err(), "{p}");
        assert_eq!(s.generation, generation);
    }
    assert!(
        s.run("textbox.paragraphs", json!({"address":tb,"paragraphs":[]}))
            .is_err()
    );
    assert!(
        s.run(
            "textbox.paragraphs",
            json!({"address":tb,"paragraphs":["a\tb"]})
        )
        .is_err()
    );
    assert!(
        s.run(
            "object.properties",
            json!({"address":rect,"props":{"cropLeft":1}})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn notes_with_several_paragraphs_keep_numbers() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.insert_note(false, "첫 각주")?;
    s.insert_note(true, "미주")?;
    let foot = first(&s, "footnote")?;
    let end = first(&s, "endnote")?;
    assert_eq!(s.note_paragraphs(foot)?, ["첫 각주"]);
    s.run(
        "note.paragraphs",
        json!({"address":foot,"paragraphs":["하나","둘","셋 ✓"]}),
    )?;
    s.run(
        "note.paragraphs",
        json!({"address":end,"paragraphs":["미주 1","미주 2"]}),
    )?;
    assert_eq!(s.note_paragraphs(foot)?, ["하나", "둘", "셋 ✓"]);
    assert_eq!(s.note_paragraphs(end)?, ["미주 1", "미주 2"]);
    // Shrinking back to one paragraph merges and keeps the automatic number.
    s.run(
        "note.paragraphs",
        json!({"address":foot,"paragraphs":["다시 하나"]}),
    )?;
    assert_eq!(s.note_text(foot)?, "다시 하나");
    s.execute(Command::Undo)?;
    assert_eq!(s.note_paragraphs(foot)?.len(), 3);
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(
            r.note_paragraphs(first(&r, "footnote")?)?,
            ["하나", "둘", "셋 ✓"],
            "{ext}"
        );
        assert_eq!(
            r.note_paragraphs(first(&r, "endnote")?)?,
            ["미주 1", "미주 2"],
            "{ext}"
        );
    }
    let generation = s.generation;
    assert!(
        s.run(
            "note.paragraphs",
            json!({"address":foot,"paragraphs":["줄\n바꿈"]})
        )
        .is_err()
    );
    assert!(
        s.run("note.paragraphs", json!({"address":foot,"paragraphs":[]}))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    // One undo step inserts a note with several paragraphs.
    let v = s.run(
        "note.insert",
        json!({"endnote":false,"paragraphs":["새 각주","둘째 줄"]}),
    )?;
    let a: ObjectAddress = serde_json::from_value(v["address"].clone())?;
    assert_eq!(s.note_paragraphs(a)?, ["새 각주", "둘째 줄"]);
    let notes = kinds(&s).len();
    s.execute(Command::Undo)?;
    assert_eq!(kinds(&s).len(), notes - 1);
    Ok(())
}

#[test]
fn equation_and_table_properties_through_object_properties() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("insert.equation", json!({"script":"x^2","size_pt":12.0}))?;
    let eq = first(&s, "equation")?;
    s.run(
        "object.properties",
        json!({"address":eq,"props":{"script":"sqrt {x+1}","fontSize":1600,"color":255}}),
    )?;
    let p = s.object_properties(eq)?["props"].clone();
    assert_eq!(p["script"], "sqrt {x+1}");
    assert_eq!(p["fontSize"], 1600);
    assert_eq!(p["color"], 255);
    let generation = s.generation;
    assert!(
        s.run(
            "object.properties",
            json!({"address":eq,"props":{"script":"{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{x"}})
        )
        .is_err()
    );
    assert!(
        s.run(
            "object.properties",
            json!({"address":eq,"props":{"hasCaption":true}})
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    s.execute(Command::Insert(" ".into()))?;
    let mut c = s.cursor;
    c.offset = s.len();
    s.select(c, false)?;
    s.execute(Command::Split)?;
    s.run("table.create", json!({"rows":2,"columns":2}))?;
    let t = first(&s, "table")?;
    assert_eq!(s.object_properties(t)?["kind"], "table");
    s.run(
        "object.properties",
        json!({"address":t,"props":{"repeatHeader":true}}),
    )?;
    assert_eq!(s.object_properties(t)?["props"]["repeatHeader"], true);
    s.run("object.delete", json!(t))?;
    assert!(!kinds(&s).contains(&"table".to_owned()));
    Ok(())
}

fn shape_at(s: &Session, a: ObjectAddress) -> anyhow::Result<ShapeObject> {
    match &s.core.document().sections[a.section].paragraphs[a.paragraph].controls[a.control] {
        Control::Shape(shape) => Ok(shape.as_ref().clone()),
        _ => anyhow::bail!("not a shape"),
    }
}
fn hu(mm: f64) -> i32 {
    (mm * 7200.0 / 25.4).round() as i32
}

#[test]
fn polygon_vertices_close_and_line_spans_its_box() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("좌표".into()))?;
    let poly = address(&s.run(
        "shape.insert",
        json!({"kind":"polygon","width_mm":30.0,"height_mm":25.0,
            "points_mm":[[0.0,25.0],[15.0,0.0],[30.0,25.0],[15.0,15.0]]}),
    )?)?;
    let triangle = address(&s.run(
        "shape.insert",
        json!({"kind":"polygon","width_mm":20.0,"height_mm":10.0}),
    )?)?;
    let line = address(&s.run(
        "shape.insert",
        json!({"kind":"line","width_mm":40.0,"height_mm":20.0,"x_mm":20.0,"y_mm":30.0}),
    )?)?;
    // The old single-line insert path closes its default triangle too.
    s.run(
        "insert.shape",
        json!({"kind":"polygon","width_mm":20.0,"height_mm":10.0,"text":""}),
    )?;
    let legacy = s
        .objects()
        .as_array()
        .and_then(|v| v.last().cloned())
        .ok_or_else(|| anyhow::anyhow!("no legacy polygon"))?;
    let legacy = address(&legacy)?;
    let expected_poly: Vec<(i32, i32)> = [
        (0.0, 25.0),
        (15.0, 0.0),
        (30.0, 25.0),
        (15.0, 15.0),
        (0.0, 25.0),
    ]
    .iter()
    .map(|(x, y)| (hu(*x), hu(*y)))
    .collect();
    let expected_triangle = vec![
        (hu(20.0) / 2, 0),
        (hu(20.0), hu(10.0)),
        (0, hu(10.0)),
        (hu(20.0) / 2, 0),
    ];
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let points = |a| -> anyhow::Result<Vec<(i32, i32)>> {
            match shape_at(s, a)? {
                ShapeObject::Polygon(p) => Ok(p.points.iter().map(|p| (p.x, p.y)).collect()),
                _ => anyhow::bail!("{label}: not a polygon"),
            }
        };
        assert_eq!(points(poly)?, expected_poly, "{label}");
        assert_eq!(points(triangle)?, expected_triangle, "{label}");
        assert_eq!(points(legacy)?, expected_triangle, "{label} legacy");
        let ShapeObject::Line(l) = shape_at(s, line)? else {
            anyhow::bail!("{label}: not a line");
        };
        assert_eq!(
            (l.end.x - l.start.x).abs(),
            hu(40.0),
            "{label}: {:?} {:?}",
            l.start,
            l.end
        );
        assert_eq!(
            (l.end.y - l.start.y).abs(),
            hu(20.0),
            "{label}: {:?} {:?}",
            l.start,
            l.end
        );
        let c = &l.common;
        assert_eq!(
            (c.width as i32, c.height as i32),
            (hu(40.0), hu(20.0)),
            "{label}"
        );
        assert_eq!(
            (c.horizontal_offset as i32, c.vertical_offset as i32),
            (hu(20.0), hu(30.0)),
            "{label}"
        );
        Ok(())
    };
    check(&s, "memory")?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        check(&r, ext)?;
    }
    Ok(())
}

#[test]
fn equation_insert_with_color_baseline_and_font() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("식: ".into()))?;
    let v = s.run(
        "equation.insert",
        json!({"script":"sum_{i=1} ^{n} i^2","size_pt":14.0,"color":0x0000ff,
            "baseline":10,"font_name":"HYhwpEQ"}),
    )?;
    let a = address(&v)?;
    let p = s.object_properties(a)?["props"].clone();
    assert_eq!(p["script"], "sum_{i=1} ^{n} i^2");
    assert_eq!(p["fontSize"], 1400);
    assert_eq!(p["color"], 0x0000ff);
    assert_eq!(p["baseline"], 10);
    assert_eq!(p["fontName"], "HYhwpEQ");
    // The old equation.set path keeps the color it does not edit.
    s.run(
        "equation.set",
        json!({"address":a,"options":{"script":"x^3","size_pt":12.0}}),
    )?;
    assert_eq!(s.object_properties(a)?["props"]["color"], 0x0000ff);
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert!(!kinds(&s).contains(&"equation".to_owned()));
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let p = r.object_properties(first(&r, "equation")?)?["props"].clone();
        assert_eq!(p["color"], 0x0000ff, "{ext}");
        assert_eq!(p["fontSize"], 1400, "{ext}");
    }
    let generation = s.generation;
    for bad in [
        json!({"script":"","size_pt":12.0}),
        json!({"script":"x","size_pt":200.0}),
        json!({"script":"x","size_pt":12.0,"color":0x1000000}),
        json!({"script":"x","size_pt":12.0,"baseline":500}),
        json!({"script":"x","size_pt":12.0,"font_name":"a\nb"}),
        json!({"script":"x","size_pt":12.0,"bogus":1}),
    ] {
        assert!(s.run("equation.insert", bad.clone()).is_err(), "{bad}");
        assert_eq!(s.generation, generation, "{bad}");
    }
    Ok(())
}

fn rhwp_sample(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples")
        .join(rel)
}

#[test]
fn chart_values_edit_undo_and_roundtrip() -> anyhow::Result<()> {
    let path = rhwp_sample("chart/세로막대형/묶은세로막대형.hwpx");
    assert!(path.exists(), "RHWP chart sample missing: {path:?}");
    let mut s = Session::new()?;
    s.open(&path)?;
    let list = s.run("chart.list", json!({}))?;
    assert!(list.as_array().is_some_and(|v| !v.is_empty()), "{list}");
    let data = s.run("chart.data.get", json!({"index":0}))?;
    let series = data["series"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("no series: {data}"))?;
    let mut values: Vec<Vec<String>> = series
        .iter()
        .map(|s| {
            s["values"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| v.as_str().unwrap_or("").to_owned())
                .collect()
        })
        .collect();
    values[0][0] = "42.5".into();
    let edits: Vec<Value> = values.iter().map(|v| json!({"values":v})).collect();
    s.run("chart.data", json!({"index":0,"series":edits}))?;
    let after = s.run("chart.data.get", json!({"index":0}))?;
    assert_eq!(after["series"][0]["values"][0], "42.5");
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("chart.data.get", json!({"index":0}))?["series"][0]["values"][0],
        data["series"][0]["values"][0]
    );
    s.execute(Command::Redo)?;
    for ext in ["hwpx", "hwp"] {
        let mut r = reopen(&mut s, ext)?;
        let v = r.run("chart.data.get", json!({"index":0}))?;
        assert_eq!(v["series"][0]["values"][0], "42.5", "{ext}: {v}");
    }
    let generation = s.generation;
    let mut bad = values.clone();
    bad[0][0] = "열둘".into();
    for p in [
        json!({"index":99,"series":[{"values":["1"]}]}),
        json!({"index":0,"series":[]}),
        json!({"index":0,"series":bad.iter().map(|v| json!({"values":v})).collect::<Vec<_>>()}),
        json!({"index":0,"series":[{"values":["1"]}],"extra":true}),
    ] {
        assert!(s.run("chart.data", p.clone()).is_err(), "{p}");
        assert_eq!(s.generation, generation, "{p}");
    }
    Ok(())
}

#[test]
fn memo_list_reads_existing_memos() -> anyhow::Result<()> {
    // RHWP #5866 sample: a Hancom memo field (HWP5). RHWP keeps its body as raw
    // records, so the list reports the memo with an empty paragraph list.
    let path = rhwp_sample("issue5866/memo_field_hwp5.hwp");
    assert!(path.exists(), "RHWP memo sample missing: {path:?}");
    let mut s = Session::new()?;
    s.open(&path)?;
    let memos = s.run("memo.list", json!({}))?;
    let list = memos.as_array().ok_or_else(|| anyhow::anyhow!("{memos}"))?;
    assert!(!list.is_empty(), "{memos}");
    assert!(
        list.iter()
            .all(|m| m["paragraphs"].is_array() && m["at"]["control"].is_u64()),
        "{memos}"
    );
    // A help-text click-here field is not a memo.
    let mut help = Session::new()?;
    help.open(&rhwp_sample("field-01-memo.hwp"))?;
    assert!(
        help.run("memo.list", json!({}))?
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(
        Session::new()?
            .run("memo.list", json!({}))?
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    Ok(())
}

#[test]
fn freehand_curve_keeps_drawn_points_open() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("자유 곡선".into()))?;
    let drawn = [
        [0.0, 10.0],
        [5.0, 2.0],
        [12.0, 8.0],
        [20.0, 0.0],
        [30.0, 6.0],
    ];
    let a = address(&s.run(
        "shape.insert",
        json!({"kind":"curve","width_mm":30.0,"height_mm":10.0,"points_mm":drawn,
            "x_mm":20.0,"y_mm":40.0}),
    )?)?;
    let expected: Vec<(i32, i32)> = drawn.iter().map(|[x, y]| (hu(*x), hu(*y))).collect();
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let ShapeObject::Curve(c) = shape_at(s, a)? else {
            anyhow::bail!("{label}: not a curve");
        };
        let got: Vec<(i32, i32)> = c.points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(
            got, expected,
            "{label}: a freehand curve must not be closed"
        );
        Ok(())
    };
    check(&s, "memory")?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        check(&r, ext)?;
    }
    let generation = s.generation;
    for p in [
        json!({"kind":"curve","width_mm":10.0,"height_mm":10.0,"points_mm":[[0.0,0.0]]}),
        json!({"kind":"curve","width_mm":10.0,"height_mm":10.0,"points_mm":[[0.0,0.0],[50.0,1.0]]}),
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0,"points_mm":[[0.0,0.0],[5.0,5.0]]}),
    ] {
        assert!(s.run("shape.insert", p.clone()).is_err(), "{p}");
        assert_eq!(s.generation, generation);
    }
    Ok(())
}
