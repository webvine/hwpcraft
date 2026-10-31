//! 개체 보호 (object.lock) survives an HWP and an HWPX save for tables, pictures, shapes and
//! text boxes.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::{Value, json};

fn reopen(s: &mut Session, name: &str) -> anyhow::Result<Session> {
    let dir = std::env::temp_dir().join(format!("hwpcraft-object-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    Ok(back)
}

/// Lock state of every table, picture and shape, in document order.
fn locks(s: &Session) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for sec in &s.core.document().sections {
        for p in &sec.paragraphs {
            for c in &p.controls {
                let (kind, locked) = match c {
                    Control::Table(t) => ("table", t.common.locked),
                    Control::Picture(p) => ("picture", p.common.locked),
                    Control::Shape(s) => ("shape", s.common().locked),
                    _ => continue,
                };
                out.push((kind.to_string(), locked));
            }
        }
    }
    out
}

fn address(s: &Session, kind: &str) -> anyhow::Result<ObjectAddress> {
    let objects = s.objects();
    let item = objects
        .as_array()
        .and_then(|v| v.iter().find(|o| o["kind"] == kind))
        .ok_or_else(|| anyhow::anyhow!("no {kind}: {objects}"))?;
    Ok(serde_json::from_value(item["address"].clone())?)
}

fn check(kind: &str, insert: impl Fn(&mut Session) -> anyhow::Result<Value>) -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞"}))?;
    insert(&mut s)?;
    let a = address(&s, kind)?;
    s.run("object.lock", json!({"address": a, "locked": true}))?;
    let locked = locks(&s);
    assert!(locked.iter().all(|l| l.1), "{kind}: {locked:?}");
    for ext in ["hwp", "hwpx"] {
        let back = reopen(&mut s, &format!("{kind}.{ext}"))?;
        assert_eq!(locks(&back), locked, "{kind}.{ext}");
    }
    // Unlocking is saved too.
    s.run("object.lock", json!({"address": a, "locked": false}))?;
    let unlocked = locks(&s);
    for ext in ["hwp", "hwpx"] {
        let back = reopen(&mut s, &format!("{kind}-off.{ext}"))?;
        assert_eq!(locks(&back), unlocked, "{kind}-off.{ext}");
    }
    Ok(())
}

#[test]
fn a_locked_table_stays_locked_after_a_save() -> anyhow::Result<()> {
    check("table", |s| {
        s.run("table.create", json!({"rows": 1, "columns": 2}))
    })
}

#[test]
fn a_locked_picture_shape_or_text_box_stays_locked_after_a_save() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a.png");
    image::RgbImage::from_fn(40, 20, |x, y| image::Rgb([x as u8 * 6, y as u8 * 12, 90]))
        .save(&path)?;
    check("picture", |s| {
        s.run("picture.insert", json!({"path": path, "width_mm": 20.0}))
    })?;
    check("shape", |s| {
        s.run(
            "shape.insert",
            json!({"kind": "rectangle", "width_mm": 20.0, "height_mm": 10.0}),
        )
    })?;
    check("textbox", |s| {
        s.run(
            "shape.insert",
            json!({"kind": "textbox", "width_mm": 30.0, "height_mm": 10.0,
                "paragraphs": ["글상자"]}),
        )
    })
}
