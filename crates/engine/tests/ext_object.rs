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

fn common_of(
    s: &Session,
    a: ObjectAddress,
) -> anyhow::Result<hwpcraft_doc::model::shape::CommonObjAttr> {
    match &s.core.document().sections[a.section].paragraphs[a.paragraph].controls[a.control] {
        Control::Shape(shape) => Ok(shape.common().clone()),
        Control::Picture(p) => Ok(p.common.clone()),
        Control::Table(t) => Ok(t.common.clone()),
        _ => anyhow::bail!("no common attributes"),
    }
}
fn flip_bits(s: &Session, a: ObjectAddress) -> anyhow::Result<u32> {
    match &s.core.document().sections[a.section].paragraphs[a.paragraph].controls[a.control] {
        Control::Shape(shape) => shape
            .drawing()
            .map(|d| d.shape_attr.flip)
            .ok_or_else(|| anyhow::anyhow!("no drawing")),
        _ => anyhow::bail!("not a shape"),
    }
}
fn floating(s: &mut Session, kind: &str, x: f64, y: f64) -> anyhow::Result<ObjectAddress> {
    address(&s.run(
        "shape.insert",
        json!({"kind":kind,"width_mm":30.0,"height_mm":20.0,"x_mm":x,"y_mm":y}),
    )?)
}

#[test]
fn group_ungroup_z_order_lock_and_flip_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("개체".into()))?;
    let rect = floating(&mut s, "rectangle", 20.0, 20.0)?;
    let ellipse = floating(&mut s, "ellipse", 30.0, 30.0)?;
    let line = floating(&mut s, "line", 90.0, 20.0)?;
    let z = |s: &Session, a| common_of(s, a).map(|c| c.z_order);
    // Creation order is the paint order: rect < ellipse < line.
    assert!(z(&s, rect)? < z(&s, ellipse)? && z(&s, ellipse)? < z(&s, line)?);

    // 앞뒤 순서: 맨 앞으로 lifts the rectangle above the others, 뒤로 swaps one step.
    assert_eq!(
        s.run("object.z_order", json!({"address":rect,"mode":"front"}))?["moved"],
        true
    );
    assert!(z(&s, rect)? > z(&s, line)? && z(&s, line)? > z(&s, ellipse)?);
    assert_eq!(
        s.run("object.z_order", json!({"address":rect,"mode":"front"}))?["moved"],
        false,
        "already in front: no undo step"
    );
    assert_eq!(
        s.run("object.z_order", json!({"address":rect,"mode":"backward"}))?["moved"],
        true
    );
    assert!(z(&s, line)? > z(&s, rect)? && z(&s, rect)? > z(&s, ellipse)?);
    s.run("object.z_order", json!({"address":rect,"mode":"back"}))?;
    assert!(z(&s, rect)? < z(&s, ellipse)?);
    s.run("object.z_order", json!({"address":rect,"mode":"forward"}))?;
    assert!(z(&s, rect)? > z(&s, ellipse)? && z(&s, rect)? < z(&s, line)?);

    // 대칭 toggles the RHWP flip bits (0x01 horizontal, 0x02 vertical).
    let before = flip_bits(&s, rect)?;
    s.run("object.flip", json!({"address":rect,"axis":"horizontal"}))?;
    assert_eq!(flip_bits(&s, rect)? & 0x3, (before & 0x3) ^ 0x1);
    s.run("object.flip", json!({"address":rect,"axis":"vertical"}))?;
    assert_eq!(flip_bits(&s, rect)? & 0x3, (before & 0x3) ^ 0x3);
    s.execute(Command::Undo)?;
    assert_eq!(flip_bits(&s, rect)? & 0x3, (before & 0x3) ^ 0x1);
    s.execute(Command::Redo)?;

    // 개체 보호: a locked object refuses move/resize/delete/z-order/group until unlocked.
    assert_eq!(
        s.run("object.lock", json!({"address":line}))?["locked"],
        true
    );
    assert!(s.object_locked(line));
    let generation = s.generation;
    for (id, params) in [
        ("object.delete", json!(line)),
        (
            "object.properties",
            json!({"address":line,"props":{"horzOffset":100}}),
        ),
        ("object.z_order", json!({"address":line,"mode":"back"})),
        ("object.flip", json!({"address":line,"axis":"horizontal"})),
        ("object.group", json!({"targets":[line, rect]})),
        (
            "object.line_endpoint",
            json!({"address":line,"start_mm":[1.0,1.0],"end_mm":[5.0,5.0]}),
        ),
    ] {
        assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(s.generation, generation, "{id} {params}");
    }
    // Reading is still allowed.
    assert_eq!(s.object_properties(line)?["kind"], "shape");
    assert_eq!(
        s.run("object.lock", json!({"address":line,"locked":true}))?["locked"],
        true,
        "setting the same state is a no-op"
    );
    assert_eq!(s.generation, generation);

    // 묶기 two shapes (the locked line stays out); the group is one object.
    let group = address(&s.run("object.group", json!({"targets":[rect, ellipse]}))?)?;
    assert!(matches!(shape_at(&s, group)?, ShapeObject::Group(g) if g.children.len() == 2));
    assert_eq!(kinds(&s).iter().filter(|k| *k == "group").count(), 1);
    assert_eq!(kinds(&s).iter().filter(|k| *k == "shape").count(), 1);
    let gc = common_of(&s, group)?;
    assert_eq!(
        (gc.horizontal_offset as i32, gc.vertical_offset as i32),
        (hu(20.0), hu(20.0)),
        "group frame starts at the top-left child"
    );
    assert_eq!(
        (gc.width as i32, gc.height as i32),
        (hu(40.0), hu(30.0)),
        "group frame spans both children"
    );
    s.execute(Command::Undo)?;
    assert_eq!(kinds(&s).iter().filter(|k| *k == "shape").count(), 3);
    s.execute(Command::Redo)?;
    assert_eq!(kinds(&s).iter().filter(|k| *k == "group").count(), 1);
    // Grouping removed two controls, so the line's control index moved up.
    let line = first(&s, "shape")?;
    assert!(s.object_locked(line));
    // RHWP gives the new group the top z-order; the relation must survive saving.
    let group_above = z(&s, group)? > z(&s, line)?;

    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let g = first(&r, "group")?;
        assert!(
            matches!(shape_at(&r, g)?, ShapeObject::Group(g) if g.children.len() == 2),
            "{ext}"
        );
        let l = first(&r, "shape")?;
        assert!(r.object_locked(l), "{ext}: lock bit survives saving");
        assert_eq!(
            z(&r, g)? > z(&r, l)?,
            group_above,
            "{ext}: order survives saving"
        );
    }
    // 풀기 restores two shapes; unlock all frees the line.
    s.run("object.ungroup", json!({"address":group}))?;
    assert_eq!(kinds(&s).iter().filter(|k| *k == "shape").count(), 3);
    assert!(!kinds(&s).iter().any(|k| k == "group"));
    let line = s
        .objects()
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| serde_json::from_value::<ObjectAddress>(o["address"].clone()).ok())
        .find(|a| s.object_locked(*a))
        .ok_or_else(|| anyhow::anyhow!("locked line lost after ungrouping"))?;
    assert_eq!(s.run("object.unlock_all", json!({}))?["unlocked"], 1);
    assert!(!s.object_locked(line));
    assert_eq!(s.run("object.unlock_all", json!({}))?["unlocked"], 0);
    s.run("object.delete", json!(line))?;

    let generation = s.generation;
    let rect = first(&s, "shape")?;
    for (id, params) in [
        ("object.group", json!({"targets":[rect]})),
        ("object.group", json!({"targets":[rect, rect]})),
        (
            "object.group",
            json!({"targets":[rect, {"section":0,"paragraph":0,"control":99}]}),
        ),
        ("object.ungroup", json!({"address":rect})),
        ("object.z_order", json!({"address":rect,"mode":"up"})),
        ("object.flip", json!({"address":rect,"axis":"diagonal"})),
        ("object.lock", json!({"address":rect,"locked":"yes"})),
        ("object.unlock_all", json!({"all":true})),
    ] {
        assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(s.generation, generation, "{id} {params}");
    }
    Ok(())
}

#[test]
fn connectors_attach_to_shapes_and_follow_them() -> anyhow::Result<()> {
    use hwpcraft_doc::model::shape::LinkLineType;
    let mut s = Session::new()?;
    s.execute(Command::Insert("연결".into()))?;
    let a = floating(&mut s, "rectangle", 20.0, 20.0)?;
    let b = floating(&mut s, "ellipse", 80.0, 60.0)?;
    let conn = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_stroke_arrow","width_mm":1.0,"height_mm":1.0,
            "start":{"address":a,"point":1},"end":{"address":b,"point":3}}),
    )?)?;
    let line_of = |s: &Session, c| -> anyhow::Result<hwpcraft_doc::model::shape::LineShape> {
        match shape_at(s, c)? {
            ShapeObject::Line(l) => Ok(l),
            _ => anyhow::bail!("not a line"),
        }
    };
    let l = line_of(&s, conn)?;
    let c = l
        .connector
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no connector data"))?;
    assert_eq!(c.link_type, LinkLineType::StrokeOneWay);
    assert_ne!(c.start_subject_id, 0);
    assert_ne!(c.start_subject_id, c.end_subject_id);
    assert_eq!((c.start_subject_index, c.end_subject_index), (1, 3));
    assert!(
        !c.control_points.is_empty(),
        "elbow connectors carry control points"
    );
    // Right edge of A (x 50 mm, y 30 mm) to left edge of B (x 80 mm, y 70 mm),
    // in RHWP's integer HWPUNIT arithmetic.
    let (ax, ay) = (hu(20.0) + hu(30.0), hu(20.0) + hu(20.0) / 2);
    let (bx, by) = (hu(80.0), hu(60.0) + hu(20.0) / 2);
    assert_eq!(
        (
            l.common.horizontal_offset as i32,
            l.common.vertical_offset as i32
        ),
        (ax, ay)
    );
    assert_eq!(
        (l.common.width as i32, l.common.height as i32),
        (bx - ax, by - ay)
    );
    let props = s.object_properties(conn)?["props"].clone();
    assert_eq!(props["arrowEnd"], 1);
    assert_eq!(props["connectorType"], 4);

    // Moving B re-routes the connector (one undo step with the move).
    let bp = s.object_properties(b)?["props"].clone();
    s.run(
        "object.properties",
        json!({"address":b,"props":{"horzOffset":bp["horzOffset"].as_i64().unwrap_or(0) + hu(10.0) as i64}}),
    )?;
    assert_eq!(line_of(&s, conn)?.common.width as i32, bx - ax + hu(10.0));
    s.execute(Command::Undo)?;
    assert_eq!(line_of(&s, conn)?.common.width as i32, bx - ax);
    s.execute(Command::Redo)?;

    // Both-way: RHWP creates one-way, HwpCraft widens the link type and arrows.
    let both = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight_both","width_mm":1.0,"height_mm":1.0,
            "start":{"address":a,"point":2},"end":{"address":b,"point":0}}),
    )?)?;
    let l = line_of(&s, both)?;
    assert_eq!(
        l.connector.as_ref().map(|c| c.link_type),
        Some(LinkLineType::StraightBoth)
    );
    let props = s.object_properties(both)?["props"].clone();
    assert_eq!(
        (props["arrowStart"].clone(), props["arrowEnd"].clone()),
        (json!(1), json!(1))
    );
    // A free connector keeps the drawn direction (bottom-left to top-right).
    let free = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight","width_mm":40.0,"height_mm":20.0,
            "x_mm":100.0,"y_mm":100.0,"flip_y":true}),
    )?)?;
    let l = line_of(&s, free)?;
    assert_eq!(l.connector.as_ref().map(|c| c.start_subject_id), Some(0));
    assert!(
        l.start.y > l.end.y,
        "flip_y starts at the bottom edge: {:?} {:?}",
        l.start,
        l.end
    );
    assert_eq!(kinds(&s).iter().filter(|k| *k == "shape").count(), 5);

    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let l = line_of(&r, conn)?;
        let c = l
            .connector
            .clone()
            .ok_or_else(|| anyhow::anyhow!("{ext}: connector lost"))?;
        assert_eq!(c.link_type, LinkLineType::StrokeOneWay, "{ext}");
        assert_eq!(
            (c.start_subject_index, c.end_subject_index),
            (1, 3),
            "{ext}"
        );
        assert_eq!(l.common.width as i32, bx - ax + hu(10.0), "{ext}");
        assert_eq!(
            line_of(&r, both)?.connector.as_ref().map(|c| c.link_type),
            Some(LinkLineType::StraightBoth),
            "{ext}"
        );
    }
    let generation = s.generation;
    for p in [
        json!({"kind":"connector_stroke","width_mm":1.0,"height_mm":1.0,"start":{"address":a,"point":1}}),
        json!({"kind":"connector_stroke","width_mm":1.0,"height_mm":1.0,"start":{"address":a,"point":4},"end":{"address":b,"point":0}}),
        json!({"kind":"connector_stroke","width_mm":1.0,"height_mm":1.0,"start":{"address":a,"point":1},"end":{"address":a,"point":3}}),
        json!({"kind":"connector_stroke","width_mm":1.0,"height_mm":1.0,"start":{"address":a,"point":1},"end":{"address":conn,"point":3}}),
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0,"start":{"address":a,"point":1},"end":{"address":b,"point":3}}),
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0,"flip_x":true}),
        json!({"kind":"connector_straight","width_mm":10.0,"height_mm":10.0,"treat_as_char":true}),
    ] {
        assert!(s.run("shape.insert", p.clone()).is_err(), "{p}");
        assert_eq!(s.generation, generation, "{p}");
    }
    Ok(())
}

#[test]
fn line_endpoints_move_in_paper_coordinates() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("직선".into()))?;
    let line = address(&s.run(
        "shape.insert",
        json!({"kind":"line","width_mm":40.0,"height_mm":20.0,"x_mm":20.0,"y_mm":30.0}),
    )?)?;
    s.run(
        "object.line_endpoint",
        json!({"address":line,"start_mm":[70.0,20.0],"end_mm":[10.0,10.0]}),
    )?;
    let check = |s: &Session, label: &str| -> anyhow::Result<()> {
        let ShapeObject::Line(l) = shape_at(s, line)? else {
            anyhow::bail!("{label}: not a line");
        };
        assert_eq!(
            (
                l.common.horizontal_offset as i32,
                l.common.vertical_offset as i32
            ),
            (hu(10.0), hu(10.0)),
            "{label}"
        );
        let (w, h) = (hu(70.0) - hu(10.0), hu(20.0) - hu(10.0));
        assert_eq!(
            (l.common.width as i32, l.common.height as i32),
            (w, h),
            "{label}"
        );
        assert_eq!(
            (l.start.x, l.start.y),
            (w, h),
            "{label}: start is the right end"
        );
        assert_eq!((l.end.x, l.end.y), (0, 0), "{label}");
        Ok(())
    };
    check(&s, "memory")?;
    s.execute(Command::Undo)?;
    let ShapeObject::Line(l) = shape_at(&s, line)? else {
        anyhow::bail!("not a line");
    };
    assert_eq!(l.common.width as i32, hu(40.0));
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        check(&r, ext)?;
    }
    let rect = floating(&mut s, "rectangle", 20.0, 60.0)?;
    let conn = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight","width_mm":10.0,"height_mm":10.0,"x_mm":20.0,"y_mm":90.0}),
    )?)?;
    let generation = s.generation;
    for (a, start, end) in [
        (rect, [1.0, 1.0], [5.0, 5.0]),
        (conn, [1.0, 1.0], [5.0, 5.0]),
        (line, [1.0, 1.0], [1.0, 1.0]),
        (line, [-1.0, 1.0], [5.0, 5.0]),
        (line, [1.0, 1.0], [5.0, 1001.0]),
    ] {
        assert!(
            s.run(
                "object.line_endpoint",
                json!({"address":a,"start_mm":start,"end_mm":end})
            )
            .is_err()
        );
        assert_eq!(s.generation, generation);
    }
    Ok(())
}

#[test]
fn click_here_fields_in_body_and_cell_with_values() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("이름: ".into()))?;
    let v = s.run(
        "insert.field",
        json!({"guide":"이름을 입력하세요","name":"name1"}),
    )?;
    let id = v["field_id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("{v}"))? as u32;
    let list = s.run("field.list", json!({}))?;
    let fields = list.as_array().ok_or_else(|| anyhow::anyhow!("{list}"))?;
    assert_eq!(fields.len(), 1, "{list}");
    assert_eq!(fields[0]["name"], "name1");
    assert_eq!(fields[0]["guide"], "이름을 입력하세요");
    assert_eq!(fields[0]["fieldId"], id);
    s.run("field.set", json!({"field_id":id,"value":"홍길동"}))?;
    assert_eq!(s.run("field.list", json!({}))?[0]["value"], "홍길동");
    assert!(s.text().contains("홍길동"), "{}", s.text());
    s.execute(Command::Undo)?;
    assert_eq!(s.run("field.list", json!({}))?[0]["value"], "");
    s.execute(Command::Redo)?;

    // A second field inside a table cell, set by name.
    let mut c = s.cursor;
    c.offset = s.len();
    s.select(c, false)?;
    s.execute(Command::Split)?;
    s.run("table.create", json!({"rows":1,"columns":2}))?;
    let cell = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("no cell"))?
        .0;
    s.select(cell, false)?;
    s.run(
        "insert.field",
        json!({"guide":"부서","name":"dept","memo":"소속 부서","editable":true}),
    )?;
    s.run("field.set", json!({"name":"dept","value":"총무부"}))?;
    let list = s.run("field.list", json!({}))?;
    assert_eq!(list.as_array().map(Vec::len), Some(2), "{list}");
    assert!(
        list.as_array()
            .into_iter()
            .flatten()
            .any(|f| f["name"] == "dept" && f["value"] == "총무부"),
        "{list}"
    );
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let list = r.run("field.list", json!({}))?;
        let names: Vec<(String, String)> = list
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                (
                    f["name"].as_str().unwrap_or("").to_owned(),
                    f["value"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        assert!(
            names.contains(&("name1".into(), "홍길동".into())),
            "{ext}: {list}"
        );
        assert!(
            names.contains(&("dept".into(), "총무부".into())),
            "{ext}: {list}"
        );
    }
    let generation = s.generation;
    for (id, params) in [
        ("insert.field", json!({"guide":""})),
        ("insert.field", json!({"guide":"a","name":"x\ny"})),
        ("insert.field", json!({"guide":"a","bogus":1})),
        ("field.set", json!({"field_id":9999,"value":"x"})),
        (
            "field.set",
            json!({"field_id":id,"name":"name1","value":"x"}),
        ),
        ("field.set", json!({"value":"x"})),
        ("field.set", json!({"name":"nobody","value":"x"})),
        ("field.set", json!({"field_id":id,"value":"a\u{7}b"})),
        ("field.list", json!({"x":1})),
    ] {
        assert!(s.run(id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(s.generation, generation, "{id} {params}");
    }
    Ok(())
}

#[test]
fn form_objects_list_get_and_set_values() -> anyhow::Result<()> {
    let path = rhwp_sample("form-01.hwp");
    assert!(path.exists(), "RHWP form sample missing: {path:?}");
    let mut s = Session::new()?;
    s.open(&path)?;
    let forms = s.run("form.list", json!({}))?;
    let list = forms.as_array().ok_or_else(|| anyhow::anyhow!("{forms}"))?;
    assert!(!list.is_empty(), "{forms}");
    assert!(kinds(&s).iter().any(|k| k == "form"));
    // Pick a body-level form object and edit according to its kind.
    let body = list
        .iter()
        .find(|f| f["at"].get("cell").is_none())
        .ok_or_else(|| anyhow::anyhow!("no body form: {forms}"))?;
    let at = body["at"].clone();
    let a: ObjectAddress = serde_json::from_value(at.clone())?;
    let info = s.run("form.get", json!({"address":a}))?;
    assert_eq!(info["formType"], body["type"]);
    assert_eq!(info["name"], body["name"]);
    let kind = body["type"].as_str().unwrap_or("");
    let (key, value, read): (&str, Value, fn(&Value) -> Value) = match kind {
        "Edit" | "ComboBox" => ("text", json!("HwpCraft 입력"), |f| f["text"].clone()),
        "CheckBox" | "RadioButton" => {
            let flipped = 1 - body["value"].as_i64().unwrap_or(0).clamp(0, 1);
            ("value", json!(flipped), |f| f["value"].clone())
        }
        _ => ("caption", json!("눌러 주세요"), |f| {
            f["caption"].clone()
        }),
    };
    let original = read(body);
    s.run("form.set", json!({"at":at,key:value}))?;
    let find = |s: &mut Session| -> anyhow::Result<Value> {
        let forms = s.run("form.list", json!({}))?;
        forms
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["at"] == at)
            .map(read)
            .ok_or_else(|| anyhow::anyhow!("form vanished: {forms}"))
    };
    assert_eq!(find(&mut s)?, value);
    s.execute(Command::Undo)?;
    assert_eq!(find(&mut s)?, original);
    s.execute(Command::Redo)?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        assert_eq!(find(&mut r)?, value, "{ext}");
    }
    let generation = s.generation;
    let wrong_key = if key == "text" { "value" } else { "text" };
    for params in [
        json!({"at":at}),
        json!({"at":at,wrong_key:json!(if wrong_key == "value" {json!(1)} else {json!("x")})}),
        json!({"at":at,"value":2}),
        json!({"at":{"section":0,"paragraph":0,"control":99},"text":"x"}),
        json!({"at":at,"text":"a\tb"}),
    ] {
        assert!(s.run("form.set", params.clone()).is_err(), "{params}");
        assert_eq!(s.generation, generation, "{params}");
    }
    assert!(
        s.run(
            "form.get",
            json!({"address":{"section":0,"paragraph":0,"control":0}})
        )
        .is_err()
    );
    assert!(
        Session::new()?
            .run("form.list", json!({}))?
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    Ok(())
}

// ---- 0.14 table-object area: alignment, size, styles, navigation, text boxes, pictures, numbers, notes ----

fn props(s: &mut Session, a: ObjectAddress) -> anyhow::Result<Value> {
    Ok(s.run("object.properties.get", json!({"address":a}))?["props"].clone())
}
fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> anyhow::Result<ObjectAddress> {
    address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":w,"height_mm":h,"x_mm":x,"y_mm":y}),
    )?)
}

#[test]
fn object_align_size_rotate_style_format_and_navigation() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("개체".into()))?;
    let r1 = rect(&mut s, 20.0, 30.0, 40.0, 20.0)?;
    let r2 = rect(&mut s, 70.0, 80.0, 30.0, 10.0)?;
    let r3 = rect(&mut s, 120.0, 50.0, 20.0, 20.0)?;
    let off = |s: &mut Session, a| -> anyhow::Result<(i64, i64, i64, i64)> {
        let p = props(s, a)?;
        Ok((
            p["horzOffset"].as_i64().unwrap_or(0),
            p["vertOffset"].as_i64().unwrap_or(0),
            p["width"].as_i64().unwrap_or(0),
            p["height"].as_i64().unwrap_or(0),
        ))
    };
    let before = (off(&mut s, r1)?, off(&mut s, r2)?, off(&mut s, r3)?);
    // Left edges meet at the leftmost object; one undo step restores both.
    let v = s.run("object.align", json!({"targets":[r1,r2],"mode":"left"}))?;
    assert_eq!(v["moved"], 1);
    assert!(
        (off(&mut s, r2)?.0 - off(&mut s, r1)?.0).abs() <= 8,
        "{:?}",
        off(&mut s, r2)?
    );
    s.execute(Command::Undo)?;
    assert_eq!((off(&mut s, r1)?, off(&mut s, r2)?), (before.0, before.1));
    // Right / bottom edges and centers.
    s.run("object.align", json!({"targets":[r1,r2,r3],"mode":"right"}))?;
    let right = |o: (i64, i64, i64, i64)| o.0 + o.2;
    assert!((right(off(&mut s, r1)?) - right(off(&mut s, r3)?)).abs() <= 8);
    assert!((right(off(&mut s, r2)?) - right(off(&mut s, r3)?)).abs() <= 8);
    s.run("object.align", json!({"targets":[r1,r2],"mode":"middle"}))?;
    let mid = |o: (i64, i64, i64, i64)| o.1 + o.3 / 2;
    assert!((mid(off(&mut s, r1)?) - mid(off(&mut s, r2)?)).abs() <= 8);
    // Same size takes the first target as the reference.
    s.run(
        "object.align",
        json!({"targets":[r1,r2,r3],"mode":"same_size"}),
    )?;
    assert_eq!(
        (off(&mut s, r2)?.2, off(&mut s, r2)?.3),
        (off(&mut s, r1)?.2, off(&mut s, r1)?.3)
    );
    assert_eq!(
        (off(&mut s, r3)?.2, off(&mut s, r3)?.3),
        (off(&mut s, r1)?.2, off(&mut s, r1)?.3)
    );
    // Equal horizontal spacing keeps the outer objects in place.
    s.run("object.align", json!({"targets":[r1,r2,r3],"mode":"top"}))?;
    let xs_before = (off(&mut s, r1)?.0, off(&mut s, r3)?.0);
    s.run(
        "object.align",
        json!({"targets":[r1,r2,r3],"mode":"space_h"}),
    )?;
    assert_eq!((off(&mut s, r1)?.0, off(&mut s, r3)?.0), xs_before);
    let gap1 = off(&mut s, r2)?.0 - right(off(&mut s, r1)?);
    let gap2 = off(&mut s, r3)?.0 - right(off(&mut s, r2)?);
    assert!((gap1 - gap2).abs() <= 8, "{gap1} {gap2}");
    // A single object aligns to the paper edge.
    s.run("object.align", json!({"targets":[r3],"mode":"right"}))?;
    let paper = (210.0 * 7200.0 / 25.4) as i64;
    assert!(
        (right(off(&mut s, r3)?) - paper).abs() <= 40,
        "{:?} vs {paper}",
        off(&mut s, r3)?
    );
    // Invalid requests leave the document alone.
    let generation0 = s.generation;
    assert!(
        s.run("object.align", json!({"targets":[r1,r2],"mode":"diagonal"}))
            .is_err()
    );
    assert!(
        s.run("object.align", json!({"targets":[r1,r1],"mode":"left"}))
            .is_err()
    );
    assert!(
        s.run("object.align", json!({"targets":[r1,r2],"mode":"space_h"}))
            .is_err()
    );
    assert!(
        s.run("object.align", json!({"targets":[],"mode":"left"}))
            .is_err()
    );
    assert_eq!(s.generation, generation0);
    s.run("object.lock", json!({"address":r1,"locked":true}))?;
    assert!(
        s.run("object.align", json!({"targets":[r1,r2],"mode":"left"}))
            .is_err()
    );
    s.run("object.lock", json!({"address":r1,"locked":false}))?;
    let inline = address(&s.run(
        "shape.insert",
        json!({"kind":"ellipse","width_mm":10.0,"height_mm":10.0,"treat_as_char":true}),
    )?)?;
    assert!(
        s.run("object.align", json!({"targets":[inline,r2],"mode":"left"}))
            .is_err()
    );
    s.execute(Command::Undo)?;

    // Size: keep_ratio derives the other side; rotation is absolute or relative.
    s.run(
        "object.size",
        json!({"address":r1,"width_mm":50.0,"keep_ratio":true}),
    )?;
    let o = off(&mut s, r1)?;
    assert!((o.2 as f64 - 50.0 * 7200.0 / 25.4).abs() < 2.0);
    assert!((o.3 as f64 - 25.0 * 7200.0 / 25.4).abs() < 4.0, "{o:?}");
    assert!(s.run("object.size", json!({"address":r1})).is_err());
    assert!(
        s.run("object.size", json!({"address":r1,"width_mm":0.0}))
            .is_err()
    );
    assert_eq!(
        s.run("object.rotate", json!({"address":r1,"by":90}))?["angle"],
        90
    );
    assert_eq!(
        s.run("object.rotate", json!({"address":r1,"by":90}))?["angle"],
        180
    );
    assert_eq!(
        s.run("object.rotate", json!({"address":r1,"angle":45}))?["angle"],
        45
    );
    assert_eq!(
        s.run("object.rotate", json!({"address":r1,"by":-90}))?["angle"],
        315
    );
    assert!(
        s.run("object.rotate", json!({"address":r1,"angle":10,"by":5}))
            .is_err()
    );
    assert_eq!(
        s.run("object.protect_size", json!({"address":r1}))?["protected"],
        true
    );
    assert_eq!(props(&mut s, r1)?["sizeProtect"], true);
    assert_eq!(
        s.run("object.protect_size", json!({"address":r1}))?["protected"],
        false
    );
    s.run(
        "object.outer_margin",
        json!({"address":r1,"preset":"normal"}),
    )?;
    assert_eq!(props(&mut s, r1)?["outerMarginLeft"], 567);
    assert_eq!(props(&mut s, r1)?["outerMarginBottom"], 567);
    s.run("object.outer_margin", json!({"address":r1,"mm":0.0}))?;
    assert_eq!(props(&mut s, r1)?["outerMarginTop"], 0);
    assert!(
        s.run("object.outer_margin", json!({"address":r1,"preset":"huge"}))
            .is_err()
    );
    assert!(
        s.run(
            "object.outer_margin",
            json!({"address":r1,"preset":"none","mm":1.0})
        )
        .is_err()
    );

    // Quick style: colors arrive as #rrggbb and land as RHWP BGR integers.
    s.run(
        "object.style",
        json!({"address":r1,"fill":"#ff0000","outline":"#0000ff","line_width_mm":1.0,"line_type":3}),
    )?;
    let p = props(&mut s, r1)?;
    assert_eq!(
        (p["fillType"].as_str(), p["fillBgColor"].as_i64()),
        (Some("solid"), Some(0x0000ff))
    );
    assert_eq!(
        (
            p["borderColor"].as_i64(),
            p["borderWidth"].as_i64(),
            p["lineType"].as_i64()
        ),
        (Some(0xff0000), Some(283), Some(3))
    );
    s.run(
        "object.style",
        json!({"address":r1,"fill":"none","outline":"none"}),
    )?;
    let p = props(&mut s, r1)?;
    assert_eq!(
        (p["fillType"].as_str(), p["lineType"].as_i64()),
        (Some("none"), Some(0))
    );
    s.run("object.style", json!({"address":r1,"outline":"#00ff00"}))?;
    assert_eq!(
        props(&mut s, r1)?["lineType"],
        1,
        "outline color turns the line back on"
    );
    assert!(s.run("object.style", json!({"address":r1})).is_err());
    assert!(
        s.run("object.style", json!({"address":r1,"fill":"red"}))
            .is_err()
    );
    assert!(
        s.run("object.style", json!({"address":r1,"line_width_mm":9.0}))
            .is_err()
    );
    // Shadow: on → nudge → none.
    let v = s.run("object.shadow", json!({"address":r1,"mode":"on","kind":4}))?;
    assert_eq!(v["shadowType"], 4);
    let v = s.run(
        "object.shadow",
        json!({"address":r1,"mode":"right","step":5}),
    )?;
    assert_eq!(
        v["shadowOffsetX"].as_i64().unwrap_or(0)
            - props(&mut s, r1)?["shadowOffsetX"].as_i64().unwrap_or(0),
        0
    );
    let x0 = v["shadowOffsetX"].as_i64().unwrap_or(0);
    let v = s.run(
        "object.shadow",
        json!({"address":r1,"mode":"left","step":5}),
    )?;
    assert_eq!(v["shadowOffsetX"].as_i64().unwrap_or(0), x0 - 5);
    s.run("object.shadow", json!({"address":r1,"mode":"reset"}))?;
    assert_eq!(props(&mut s, r1)?["shadowOffsetY"], 0);
    assert_eq!(
        s.run("object.shadow", json!({"address":r1,"mode":"none"}))?["shadowType"],
        0
    );
    assert!(
        s.run("object.shadow", json!({"address":r1,"mode":"up"}))
            .is_err()
    );
    assert!(
        s.run("object.shadow", json!({"address":r1,"mode":"on","kind":9}))
            .is_err()
    );

    // 개체 모양 복사/붙이기: look only, not size or position.
    s.run(
        "object.style",
        json!({"address":r1,"fill":"#123456","outline":"#654321","line_width_mm":0.5}),
    )?;
    let f = s.run("object.format.get", json!({"address":r1}))?["format"].clone();
    assert!(f.get("width").is_none() && f.get("horzOffset").is_none());
    let size2 = (off(&mut s, r2)?.2, off(&mut s, r2)?.3);
    s.run("object.format", json!({"address":r2,"format":f}))?;
    let p2 = props(&mut s, r2)?;
    assert_eq!(p2["fillBgColor"], props(&mut s, r1)?["fillBgColor"]);
    assert_eq!(p2["borderWidth"], props(&mut s, r1)?["borderWidth"]);
    assert_eq!((off(&mut s, r2)?.2, off(&mut s, r2)?.3), size2);
    assert!(
        s.run("object.format", json!({"address":r2,"format":"x"}))
            .is_err()
    );

    // Next / previous object in document order, wrapping like Hangul's Tab cycle.
    let n = |s: &mut Session,
             from: Option<ObjectAddress>,
             dir: &str,
             wrap: bool|
     -> anyhow::Result<Value> {
        let mut p = json!({"direction":dir,"wrap":wrap});
        if let Some(a) = from {
            p["address"] = json!(a);
        }
        s.run("object.neighbor", p)
    };
    assert_eq!(address(&n(&mut s, Some(r1), "next", true)?)?, r2);
    assert_eq!(address(&n(&mut s, Some(r3), "next", true)?)?, r1);
    assert!(n(&mut s, Some(r3), "next", false)?["address"].is_null());
    assert_eq!(address(&n(&mut s, Some(r1), "prev", true)?)?, r3);
    assert_eq!(address(&n(&mut s, None, "next", true)?)?, r1);
    assert_eq!(n(&mut s, Some(r1), "next", true)?["kind"], "shape");
    assert!(n(&mut s, Some(r1), "sideways", true).is_err());

    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let a = first(&r, "shape")?;
        let p = props(&mut r, a)?;
        assert_eq!(p["rotationAngle"], 315, "{ext}");
        assert_eq!(p["fillBgColor"], 0x563412, "{ext}");
    }
    Ok(())
}

#[test]
fn textbox_alignment_direction_margin_and_connector_type() -> anyhow::Result<()> {
    use hwpcraft_doc::model::{shape::LinkLineType, style::Alignment};
    let mut s = Session::new()?;
    s.execute(Command::Insert("글상자".into()))?;
    let tb = address(&s.run(
        "shape.insert",
        json!({"kind":"textbox","width_mm":50.0,"height_mm":30.0,"paragraphs":["가","나"]}),
    )?)?;
    let para_align = |s: &Session, a: ObjectAddress, i: usize| -> Option<Alignment> {
        let doc = s.core.document();
        let p = match &doc.sections[a.section].paragraphs[a.paragraph].controls[a.control] {
            Control::Shape(sh) => sh.drawing()?.text_box.as_ref()?.paragraphs.get(i)?,
            _ => return None,
        };
        doc.doc_info
            .para_shapes
            .get(usize::from(p.para_shape_id))
            .map(|ps| ps.alignment)
    };
    s.run(
        "textbox.align",
        json!({"address":tb,"horizontal":"center","vertical":"bottom"}),
    )?;
    assert_eq!(para_align(&mut s, tb, 0), Some(Alignment::Center));
    assert_eq!(para_align(&mut s, tb, 1), Some(Alignment::Center));
    assert_eq!(props(&mut s, tb)?["tbVerticalAlign"], "Bottom");
    s.run("textbox.align", json!({"address":tb,"horizontal":"right"}))?;
    assert_eq!(para_align(&mut s, tb, 1), Some(Alignment::Right));
    assert_eq!(props(&mut s, tb)?["tbVerticalAlign"], "Bottom");
    s.execute(Command::Undo)?;
    assert_eq!(para_align(&mut s, tb, 1), Some(Alignment::Center));
    assert!(s.run("textbox.align", json!({"address":tb})).is_err());
    assert!(
        s.run("textbox.align", json!({"address":tb,"horizontal":"middle"}))
            .is_err()
    );
    assert_eq!(s.textbox_paragraphs(tb)?, ["가", "나"]);

    // 세로 글상자 / 영문 세움 and a new vertical text box.
    let v = s.run(
        "textbox.direction",
        json!({"address":tb,"direction":"vertical"}),
    )?;
    assert_eq!(v["direction"], "vertical");
    let v = s.run(
        "textbox.direction",
        json!({"address":tb,"direction":"vertical_all"}),
    )?;
    assert_eq!(v["direction"], "vertical_all");
    s.execute(Command::Undo)?;
    assert_eq!(s.textbox_direction(tb)?, "vertical");
    assert!(
        s.run(
            "textbox.direction",
            json!({"address":tb,"direction":"diagonal"})
        )
        .is_err()
    );
    let vt = address(&s.run(
        "shape.insert",
        json!({"kind":"textbox","width_mm":20.0,"height_mm":60.0,"paragraphs":["세로"],"direction":"vertical"}),
    )?)?;
    assert_eq!(s.textbox_direction(vt)?, "vertical");
    assert!(
        s.run(
            "shape.insert",
            json!({"kind":"rectangle","width_mm":20.0,"height_mm":20.0,"direction":"vertical"}),
        )
        .is_err()
    );
    s.run("textbox.margin", json!({"address":tb,"preset":"wide"}))?;
    assert_eq!(props(&mut s, tb)?["tbMarginLeft"], 850);
    assert_eq!(props(&mut s, tb)?["tbMarginBottom"], 850);
    let rect_a = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0,"x_mm":20.0,"y_mm":120.0}),
    )?)?;
    assert!(
        s.run("textbox.margin", json!({"address":rect_a,"preset":"wide"}))
            .is_err()
    );
    assert!(
        s.run(
            "textbox.direction",
            json!({"address":rect_a,"direction":"vertical"})
        )
        .is_err()
    );
    let r = reopen(&mut s, "hwpx")?;
    let boxes: Vec<ObjectAddress> = r
        .objects()
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["kind"] == "textbox")
        .map(|o| serde_json::from_value(o["address"].clone()).unwrap())
        .collect();
    assert_eq!(boxes.len(), 2);
    assert_eq!(r.textbox_direction(boxes[0])?, "vertical");
    assert_eq!(para_align(&r, boxes[0], 0), Some(Alignment::Center));

    // Connector kind changes keep the arrow class.
    let rect_b = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0,"x_mm":90.0,"y_mm":120.0}),
    )?)?;
    let c = address(&s.run(
        "shape.insert",
        json!({"kind":"connector_straight_arrow","width_mm":50.0,"height_mm":10.0,
            "start":{"address":rect_a,"point":1},"end":{"address":rect_b,"point":3}}),
    )?)?;
    let link = |s: &Session, a: ObjectAddress| match &s.core.document().sections[a.section]
        .paragraphs[a.paragraph]
        .controls[a.control]
    {
        Control::Shape(sh) => match sh.as_ref() {
            ShapeObject::Line(l) => l.connector.as_ref().map(|c| c.link_type),
            _ => None,
        },
        _ => None,
    };
    assert_eq!(link(&s, c), Some(LinkLineType::StraightOneWay));
    s.run(
        "object.connector_type",
        json!({"address":c,"kind":"stroke"}),
    )?;
    assert_eq!(link(&s, c), Some(LinkLineType::StrokeOneWay));
    s.run("object.connector_type", json!({"address":c,"kind":"arc"}))?;
    assert_eq!(link(&s, c), Some(LinkLineType::ArcOneWay));
    s.execute(Command::Undo)?;
    assert_eq!(link(&s, c), Some(LinkLineType::StrokeOneWay));
    assert!(
        s.run(
            "object.connector_type",
            json!({"address":c,"kind":"zigzag"})
        )
        .is_err()
    );
    assert!(
        s.run(
            "object.connector_type",
            json!({"address":rect_a,"kind":"arc"})
        )
        .is_err()
    );
    let r = reopen(&mut s, "hwp")?;
    let connectors: Vec<ObjectAddress> = r
        .objects()
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["kind"] == "shape")
        .map(|o| serde_json::from_value(o["address"].clone()).unwrap())
        .filter(|a| link(&r, *a).is_some())
        .collect();
    assert_eq!(connectors.len(), 1);
    assert_eq!(link(&r, connectors[0]), Some(LinkLineType::StrokeOneWay));
    Ok(())
}

#[test]
fn pictures_save_to_files_and_recolor() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("그림".into()))?;
    let mut pics = Vec::new();
    for path in images(dir.path())?.iter().take(2) {
        pics.push(address(
            &s.run("picture.insert", json!({"path":path,"width_mm":30.0}))?,
        )?);
    }
    let target = out.path().join("first.dat");
    let v = s.run("picture.save", json!({"address":pics[0],"path":target}))?;
    let written = PathBuf::from(v["path"].as_str().unwrap_or(""));
    assert_eq!(
        written.extension().and_then(|e| e.to_str()),
        Some("png"),
        "{written:?}"
    );
    let bytes = std::fs::read(&written)?;
    assert!(
        bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "{} bytes",
        bytes.len()
    );
    // Never overwrite silently.
    assert!(
        s.run("picture.save", json!({"address":pics[0],"path":target}))
            .is_err()
    );
    s.run(
        "picture.save",
        json!({"address":pics[0],"path":target,"overwrite":true}),
    )?;
    assert!(
        s.run(
            "picture.save",
            json!({"address":pics[0],"path":out.path().join("nope").join("x.png")})
        )
        .is_err()
    );
    let v = s.run("picture.save_all", json!({"dir":out.path(),"prefix":"pic"}))?;
    let files: Vec<PathBuf> = serde_json::from_value(v["files"].clone())?;
    assert_eq!(files.len(), 2);
    assert!(
        files[0].ends_with("pic1.png") && files[1].ends_with("pic2.jpg"),
        "{files:?}"
    );
    assert!(files.iter().all(|f| f.is_file()));
    assert!(
        s.run("picture.save_all", json!({"dir":out.path(),"prefix":"pic"}))
            .is_err()
    );
    assert!(
        s.run("picture.save_all", json!({"dir":out.path(),"prefix":"a/b"}))
            .is_err()
    );
    let shape = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0}),
    )?)?;
    assert!(
        s.run(
            "picture.save",
            json!({"address":shape,"path":out.path().join("s.png")})
        )
        .is_err()
    );
    let mut empty = Session::new()?;
    assert!(
        empty
            .run("picture.save_all", json!({"dir":out.path()}))
            .is_err()
    );

    s.run(
        "picture.recolor",
        json!({"address":pics[1],"mode":"grayscale"}),
    )?;
    assert_eq!(props(&mut s, pics[1])?["effect"], "GrayScale");
    s.run(
        "picture.recolor",
        json!({"address":pics[1],"mode":"watermark"}),
    )?;
    assert_eq!(props(&mut s, pics[1])?["effect"], "Pattern8x8");
    s.execute(Command::Undo)?;
    assert_eq!(props(&mut s, pics[1])?["effect"], "GrayScale");
    s.run(
        "picture.recolor",
        json!({"address":pics[1],"mode":"original"}),
    )?;
    assert_eq!(props(&mut s, pics[1])?["effect"], "RealPic");
    assert!(
        s.run("picture.recolor", json!({"address":pics[1],"mode":"sepia"}))
            .is_err()
    );
    assert!(
        s.run(
            "picture.recolor",
            json!({"address":shape,"mode":"grayscale"})
        )
        .is_err()
    );
    assert!(
        s.run("object.shadow", json!({"address":pics[1],"mode":"on"}))
            .is_err()
    );
    Ok(())
}

#[test]
fn auto_numbers_note_conversion_note_shape_and_deletions() -> anyhow::Result<()> {
    use hwpcraft_doc::model::control::AutoNumberType;
    let mut s = Session::new()?;
    s.execute(Command::Insert("그림 ".into()))?;
    let auto = |s: &Session, a: ObjectAddress| match &s.core.document().sections[a.section]
        .paragraphs[a.paragraph]
        .controls[a.control]
    {
        Control::AutoNumber(n) => Some(n.number_type),
        _ => None,
    };
    let v = s.run("insert.auto_number", json!({"kind":"picture"}))?;
    let a = address(&v)?;
    assert_eq!(auto(&s, a), Some(AutoNumberType::Picture));
    assert_eq!(s.cursor.offset, 4);
    s.execute(Command::Insert(" / 쪽 ".into()))?;
    let b = address(&s.run("insert.auto_number", json!({"kind":"total"}))?)?;
    assert_eq!(auto(&s, b), Some(AutoNumberType::TotalPage));
    let c = address(&s.run("insert.auto_number", json!({"kind":"page"}))?)?;
    assert_eq!(auto(&s, c), Some(AutoNumberType::Page));
    let d = address(&s.run("insert.auto_number", json!({"kind":"table"}))?)?;
    assert_eq!(auto(&s, d), Some(AutoNumberType::Table));
    s.execute(Command::Undo)?;
    let numbers = s.core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .filter(|c| matches!(c, Control::AutoNumber(_)))
        .count();
    assert_eq!(numbers, 3);
    assert!(
        s.run("insert.auto_number", json!({"kind":"chapter"}))
            .is_err()
    );
    assert!(s.core.page_count() >= 1);

    // Footnote → endnote → footnote keeps the text and flips the inner number kind.
    s.execute(Command::Split)?;
    s.execute(Command::Insert("본문".into()))?;
    let note = address(&s.run(
        "note.insert",
        json!({"endnote":false,"paragraphs":["각주 내용"]}),
    )?)?;
    let inner = |s: &Session, a: ObjectAddress| -> Option<(bool, AutoNumberType, u16)> {
        let (endnote, paragraphs, number) = match &s.core.document().sections[a.section].paragraphs
            [a.paragraph]
            .controls[a.control]
        {
            Control::Footnote(n) => (false, &n.paragraphs, n.number),
            Control::Endnote(n) => (true, &n.paragraphs, n.number),
            _ => return None,
        };
        let kind = paragraphs
            .iter()
            .flat_map(|p| p.controls.iter())
            .find_map(|c| match c {
                Control::AutoNumber(n) => Some(n.number_type),
                _ => None,
            })?;
        Some((endnote, kind, number))
    };
    assert_eq!(inner(&s, note), Some((false, AutoNumberType::Footnote, 1)));
    let v = s.run("note.convert", json!({"address":note}))?;
    assert_eq!(address(&v)?, note);
    assert_eq!(inner(&s, note), Some((true, AutoNumberType::Endnote, 1)));
    assert_eq!(s.note_paragraphs(note)?, ["각주 내용"]);
    assert!(kinds(&s).contains(&"endnote".to_owned()));
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let e = first(&r, "endnote")?;
        assert_eq!(inner(&r, e).map(|x| x.0), Some(true), "{ext}");
        assert_eq!(r.note_paragraphs(e)?, ["각주 내용"], "{ext}");
    }
    s.execute(Command::Undo)?;
    assert_eq!(inner(&s, note), Some((false, AutoNumberType::Footnote, 1)));
    let shape = address(&s.run(
        "shape.insert",
        json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0}),
    )?)?;
    assert!(s.run("note.convert", json!({"address":shape})).is_err());

    // Note shape: footnotes through the model, endnotes through RHWP, same keys.
    let g = s.run("note.shape.get", json!({"kind":"footnote"}))?;
    assert_eq!(
        (g["numberFormat"].as_str(), g["startNumber"].as_u64()),
        (Some("digit"), Some(1))
    );
    let g = s.run(
        "note.shape",
        json!({"kind":"footnote","props":{"startNumber":5,"numberFormat":"circledDigit","separatorLength":5669,"separatorColor":"#ff0000","separatorLineType":3}}),
    )?;
    assert_eq!(
        (g["numberFormat"].as_str(), g["startNumber"].as_u64()),
        (Some("circledDigit"), Some(5))
    );
    assert_eq!(
        (
            g["separatorLength"].as_i64(),
            g["separatorColor"].as_str(),
            g["separatorLineType"].as_u64()
        ),
        (Some(5669), Some("#ff0000"), Some(3))
    );
    assert_eq!(inner(&s, note).map(|x| x.2), Some(5));
    s.execute(Command::Undo)?;
    assert_eq!(
        s.run("note.shape.get", json!({"kind":"footnote"}))?["startNumber"],
        1
    );
    assert_eq!(inner(&s, note).map(|x| x.2), Some(1));
    let g = s.run(
        "note.shape",
        json!({"kind":"endnote","props":{"startNumber":3,"separatorEnabled":false}}),
    )?;
    assert_eq!(
        (g["startNumber"].as_u64(), g["separatorEnabled"].as_bool()),
        (Some(3), Some(false))
    );
    assert!(
        s.run(
            "note.shape",
            json!({"kind":"footnote","props":{"startNumber":0}})
        )
        .is_err()
    );
    assert!(
        s.run(
            "note.shape",
            json!({"kind":"footnote","props":{"lineColor":"#000000"}})
        )
        .is_err()
    );
    assert!(
        s.run("note.shape", json!({"kind":"footnote","props":{}}))
            .is_err()
    );
    assert!(s.run("note.shape.get", json!({"kind":"margin"})).is_err());
    s.run(
        "note.shape",
        json!({"kind":"footnote","props":{"numberFormat":"hangulSyllable","prefixChar":"(","suffixChar":")"}}),
    )?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let g = r.run("note.shape.get", json!({"kind":"footnote"}))?;
        assert_eq!(
            (
                g["numberFormat"].as_str(),
                g["prefixChar"].as_str(),
                g["suffixChar"].as_str()
            ),
            (Some("hangulSyllable"), Some("("), Some(")")),
            "{ext}"
        );
    }

    // note.delete defaults to the caret paragraph's note; field.delete keeps or drops text per RHWP.
    s.select(
        hwpcraft_doc::Cursor {
            section: note.section,
            paragraph: note.paragraph,
            offset: 0,
            cell: None,
        },
        false,
    )?;
    let notes_before = kinds(&s).iter().filter(|k| *k == "footnote").count();
    s.run("note.delete", json!({}))?;
    assert_eq!(
        kinds(&s).iter().filter(|k| *k == "footnote").count(),
        notes_before - 1
    );
    s.execute(Command::Undo)?;
    assert_eq!(
        kinds(&s).iter().filter(|k| *k == "footnote").count(),
        notes_before
    );
    assert!(s.run("note.delete", json!({"address":shape})).is_err());
    let mut body = Session::new()?;
    assert!(body.run("note.delete", json!({})).is_err());

    body.execute(Command::Insert("앞 ".into()))?;
    body.run(
        "insert.field",
        json!({"guide":"이름을 입력하세요","name":"이름"}),
    )?;
    let fields = body.run("field.list", json!({}))?;
    let count = |v: &Value| {
        v.as_array()
            .map(Vec::len)
            .or_else(|| v["fields"].as_array().map(Vec::len))
            .unwrap_or(0)
    };
    assert_eq!(count(&fields), 1);
    let text_before = body.text().to_owned();
    body.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 0,
            offset: 3,
            cell: None,
        },
        false,
    )?;
    assert_eq!(body.run("field.delete", json!({}))?["removed"], true);
    assert_eq!(count(&body.run("field.list", json!({}))?), 0);
    assert!(body.text().len() <= text_before.len());
    body.execute(Command::Undo)?;
    assert_eq!(count(&body.run("field.list", json!({}))?), 1);
    assert_eq!(body.text(), text_before);
    body.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        },
        false,
    )?;
    assert_eq!(body.run("field.delete", json!({}))?["removed"], false);
    assert!(body.run("field.delete", json!({"field_id":9999})).is_err());
    Ok(())
}

#[test]
fn note_shape_rejects_decoration_characters_outside_the_bmp() -> anyhow::Result<()> {
    // HWP stores note decoration characters as one UTF-16 unit; a non-BMP
    // character would be truncated to an unrelated code unit on save.
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.run(
        "note.insert",
        json!({"endnote":false,"paragraphs":["각주"]}),
    )?;
    for kind in ["footnote", "endnote"] {
        for key in ["prefixChar", "suffixChar", "userChar"] {
            let generation = s.generation;
            let before = s.run("note.shape.get", json!({"kind":kind}))?;
            let r = s.run("note.shape", json!({"kind":kind,"props":{key:"😀"}}));
            assert!(r.is_err(), "{kind} {key} accepted a non-BMP character");
            assert_eq!(s.generation, generation, "{kind} {key}");
            assert_eq!(s.run("note.shape.get", json!({"kind":kind}))?, before);
        }
    }
    s.run(
        "note.shape",
        json!({"kind":"footnote","props":{"prefixChar":"※","suffixChar":"〕"}}),
    )?;
    for ext in ["hwp", "hwpx"] {
        let mut r = reopen(&mut s, ext)?;
        let g = r.run("note.shape.get", json!({"kind":"footnote"}))?;
        assert_eq!(
            (g["prefixChar"].as_str(), g["suffixChar"].as_str()),
            (Some("※"), Some("〕")),
            "{ext}"
        );
    }
    Ok(())
}
