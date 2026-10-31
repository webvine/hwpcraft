//! Deleting or moving a style reaches every paragraph that uses it — headers, text
//! boxes and the rest, not only the body — so a saved file never points at a style
//! that is gone.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::{Value, json};

/// Every `style_id` in the document's paragraphs, at any depth (body, cells, headers,
/// footers, notes, text boxes, captions, master pages).
fn style_ids(s: &Session) -> anyhow::Result<Vec<u64>> {
    fn collect(v: &Value, out: &mut Vec<u64>) {
        match v {
            Value::Object(map) => {
                for (k, v) in map {
                    if k == "style_id" {
                        out.extend(v.as_u64());
                    }
                    collect(v, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|v| collect(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for section in &s.core.document().sections {
        collect(&serde_json::to_value(&section.paragraphs)?, &mut out);
        for page in &section.section_def.master_pages {
            collect(&serde_json::to_value(&page.paragraphs)?, &mut out);
        }
    }
    Ok(out)
}

/// A body paragraph, a header and a text box, all in a new style "검토 문단".
fn document() -> anyhow::Result<(Session, usize)> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "본문"}))?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리말 글"}),
    )?;
    let made = s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 40.0, "height_mm": 15.0, "paragraphs": ["글상자 글"]}),
    )?;
    assert!(made.is_object(), "{made}");
    s.run("format.style.create", json!({"name": "검토 문단"}))?;
    let id = s.core.document().doc_info.styles.len() - 1;
    let style = id as u8;
    // Give the header's and the text box's paragraphs the style through the model.
    for para in &mut s.core.document_mut().sections[0].paragraphs {
        para.style_id = style;
        for control in &mut para.controls {
            match control {
                Control::Header(h) => h.paragraphs.iter_mut().for_each(|p| p.style_id = style),
                Control::Shape(shape) => {
                    if let Some(tb) = shape.drawing_mut().and_then(|d| d.text_box.as_mut()) {
                        tb.paragraphs.iter_mut().for_each(|p| p.style_id = style);
                    }
                }
                _ => {}
            }
        }
    }
    s.core.document_mut().sections[0].raw_stream = None;
    let used = style_ids(&s)?.iter().filter(|i| **i == id as u64).count();
    assert!(used >= 3, "body, header and text box: {used}");
    Ok((s, id))
}

#[test]
fn deleting_a_style_leaves_no_paragraph_pointing_past_the_list() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for name in ["deleted.hwp", "deleted.hwpx"] {
        let (mut s, id) = document()?;
        s.run("format.style.delete", json!({"id": id}))?;
        let count = s.core.document().doc_info.styles.len() as u64;
        let ids = style_ids(&s)?;
        assert!(ids.iter().all(|i| *i < count), "{name}: {ids:?} of {count}");
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        let count = back.core.document().doc_info.styles.len() as u64;
        let ids = style_ids(&back)?;
        assert!(
            ids.iter().all(|i| *i < count),
            "{name} reopened: {ids:?} of {count}"
        );
        assert!(!ids.contains(&(id as u64)) || (id as u64) < count, "{name}");
    }
    Ok(())
}

#[test]
fn moving_a_style_moves_it_in_headers_and_text_boxes_too() -> anyhow::Result<()> {
    let (mut s, id) = document()?;
    let before = style_ids(&s)?.iter().filter(|i| **i == id as u64).count();
    s.run("format.style.move", json!({"id": id, "direction": "up"}))?;
    let ids = style_ids(&s)?;
    assert_eq!(
        ids.iter().filter(|i| **i == id as u64 - 1).count(),
        before,
        "{ids:?}"
    );
    Ok(())
}
