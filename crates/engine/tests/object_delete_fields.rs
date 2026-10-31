//! Deleting an object keeps a later field of the same paragraph: RHWP's delete paths take
//! the control out without moving the field ranges' control indexes (patch 0049).
use hwpcraft_doc::Cursor;
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{ObjectAddress, Session};
use serde_json::{Value, json};

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph,
        offset,
        cell: None,
    }
}

/// Field ranges that do not point at a field or hyperlink control.
fn broken_ranges(s: &Session) -> Vec<String> {
    let mut out = Vec::new();
    for (pi, p) in s.core.document().sections[0].paragraphs.iter().enumerate() {
        for fr in &p.field_ranges {
            if !matches!(
                p.controls.get(fr.control_idx),
                Some(Control::Field(_) | Control::Hyperlink(_))
            ) {
                out.push(format!("p{pi} control {}", fr.control_idx));
            }
        }
    }
    out
}

/// The document's fields without their stream positions, which a removed control moves.
fn fields(s: &mut Session) -> anyhow::Result<Value> {
    let mut list = s.run("field.list", json!({}))?;
    for field in list.as_array_mut().into_iter().flatten() {
        if let Some(field) = field.as_object_mut() {
            field.remove("startPos");
            field.remove("endPos");
        }
    }
    Ok(list)
}

fn reopen(s: &mut Session, name: &str) -> anyhow::Result<Session> {
    let dir = std::env::temp_dir().join(format!("hwpcraft-object-delete-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    Ok(back)
}

/// "앞 [object] 이름: [field 홍길동] 끝" with an object of `kind` inserted by `insert`.
fn object_then_field(
    kind: &str,
    insert: impl Fn(&mut Session) -> anyhow::Result<Value>,
) -> anyhow::Result<(Session, ObjectAddress)> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 이름: "}))?;
    s.run("insert.field", json!({"guide": "이름", "name": "name1"}))?;
    let id = s.run("field.list", json!({}))?[0]["fieldId"].clone();
    s.run("field.set", json!({"field_id": id, "value": "홍길동"}))?;
    s.run("text.insert", json!({"text": " 끝"}))?;
    s.select(at(0, 1), false)?;
    insert(&mut s)?;
    let objects = s.objects();
    let item = objects
        .as_array()
        .and_then(|v| v.iter().find(|o| o["kind"] == kind))
        .ok_or_else(|| anyhow::anyhow!("no {kind}: {objects}"))?;
    let mut address: ObjectAddress = serde_json::from_value(item["address"].clone())?;
    // A new table gets a paragraph of its own (and may split the field's paragraph): treat
    // it as a character and drag it in front of the field's paragraph.
    let host = s.core.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.iter().any(|c| matches!(c, Control::Field(_))))
        .unwrap_or(0);
    if address.paragraph != host {
        s.run(
            "object.properties",
            json!({"address": address, "props": {"treatAsChar": true}}),
        )?;
        let v = s.run(
            "object.move_inline",
            json!({"address": address, "to": at(host, 0)}),
        )?;
        address = serde_json::from_value(v["address"].clone())?;
    }
    let p = &s.core.document().sections[0].paragraphs[address.paragraph];
    let field = p
        .controls
        .iter()
        .position(|c| matches!(c, Control::Field(_)));
    assert!(
        field.is_some_and(|f| f > address.control),
        "the object comes before the field: {address:?} {field:?}"
    );
    Ok((s, address))
}

fn check(
    name: &str,
    kind: &str,
    insert: impl Fn(&mut Session) -> anyhow::Result<Value>,
) -> anyhow::Result<()> {
    let (mut s, address) = object_then_field(kind, insert)?;
    let before = fields(&mut s)?;
    assert_eq!(before.as_array().map(Vec::len), Some(1), "{name}: {before}");
    s.run("object.delete", json!(address))?;
    assert_eq!(broken_ranges(&s), Vec::<String>::new(), "{name}");
    assert_eq!(fields(&mut s)?, before, "{name}");
    for ext in ["hwp", "hwpx"] {
        let mut back = reopen(&mut s, &format!("{name}.{ext}"))?;
        assert_eq!(fields(&mut back)?, before, "{name}.{ext}");
    }
    Ok(())
}

#[test]
fn deleting_an_equation_keeps_a_later_field() -> anyhow::Result<()> {
    check("equation", "equation", |s| {
        s.run("insert.equation", json!({"script": "x", "size_pt": 12}))
    })
}

#[test]
fn deleting_a_shape_or_text_box_keeps_a_later_field() -> anyhow::Result<()> {
    check("rectangle", "shape", |s| {
        s.run(
            "shape.insert",
            json!({"kind": "rectangle", "width_mm": 20.0, "height_mm": 10.0}),
        )
    })?;
    check("textbox", "textbox", |s| {
        s.run(
            "shape.insert",
            json!({"kind": "textbox", "width_mm": 30.0, "height_mm": 10.0,
                "paragraphs": ["글상자"]}),
        )
    })
}

#[test]
fn deleting_a_picture_keeps_a_later_field() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("a.png");
    image::RgbImage::from_fn(40, 20, |x, y| image::Rgb([x as u8 * 6, y as u8 * 12, 90]))
        .save(&path)?;
    check("picture", "picture", |s| {
        s.run(
            "picture.insert",
            json!({"path": path, "width_mm": 20.0, "treat_as_char": true}),
        )
    })
}

#[test]
fn deleting_a_table_keeps_a_later_field() -> anyhow::Result<()> {
    check("table", "table", |s| {
        s.run("table.create", json!({"rows": 1, "columns": 2}))
    })
}

#[test]
fn deleting_a_footnote_keeps_a_later_field() -> anyhow::Result<()> {
    check("footnote", "footnote", |s| {
        s.run("insert.footnote", json!({"text": "주"}))
    })
}
