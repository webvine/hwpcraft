//! Owner: format area. A picture's caption number and caption as cross references,
//! in the command form of Hancom's public ActionCrossRef table: `?#<id>;1;1;0;0`
//! shows the number, `?#<id>;1;2;0;0` the caption (object kind 1 = picture), the
//! same form Hangul 2024 was observed to use for tables (kind 0).
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Cursor, Session};
use serde_json::{Value, json};

fn picture_ids(s: &Session) -> Vec<u32> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::Picture(p) => Some(p.common.instance_id),
            _ => None,
        })
        .collect()
}

/// (command, text) of every cross reference in paragraph 0.
fn commands(s: &Session) -> Vec<(String, String)> {
    let para = &s.core.document().sections[0].paragraphs[0];
    para.field_ranges
        .iter()
        .filter_map(|r| match para.controls.get(r.control_idx) {
            Some(Control::Field(f)) => Some((
                f.command.clone(),
                para.text
                    .chars()
                    .skip(r.start_char_idx)
                    .take(r.end_char_idx - r.start_char_idx)
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

/// "참조: " on page 1; a picture captioned "현황 사진" on page 2.
fn document(dir: &std::path::Path) -> anyhow::Result<(Session, Value)> {
    let png = dir.join("photo.png");
    image::RgbImage::from_fn(40, 20, |x, y| image::Rgb([x as u8 * 6, y as u8 * 12, 90]))
        .save(&png)?;
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "참조: \n"}))?;
    s.run("insert.page_break", json!({}))?;
    let pic = s.run("picture.insert", json!({"path": png}))?;
    let address = pic["address"].clone();
    s.run(
        "object.properties",
        json!({"address": address, "props": {"hasCaption": true}}),
    )?;
    s.run(
        "object.caption_text",
        json!({"address": address, "text": "현황 사진"}),
    )?;
    s.cursor = Cursor {
        offset: 4,
        ..Default::default()
    };
    Ok((s, address))
}

#[test]
fn picture_number_and_caption_references_follow_the_picture() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, address) = document(dir.path())?;
    let before = picture_ids(&s);
    s.run("text.insert", json!({"text": "그림 "}))?;
    let number = s.run(
        "insert.cross_reference",
        json!({"target": {"picture": 1}, "show": "number"}),
    )?;
    assert_eq!(number["text"], "1", "{number}");
    let id = number["picture"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("{number}"))? as u32;
    assert_ne!(id, 0);
    assert_eq!(picture_ids(&s), vec![id]);
    s.run("text.insert", json!({"text": ", "}))?;
    let caption = s.run(
        "insert.cross_reference",
        json!({"target": {"picture": 1}, "show": "caption"}),
    )?;
    assert_eq!(caption["text"], "그림 1 현황 사진", "{caption}");
    assert_eq!(caption["picture"], id);
    assert_eq!(
        commands(&s),
        vec![
            (format!("?#{id};1;1;0;0"), "1".to_owned()),
            (format!("?#{id};1;2;0;0"), "그림 1 현황 사진".to_owned()),
        ]
    );
    let listed = s.run("tools.cross_reference.list", json!({}))?;
    let items = listed["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 2, "{listed}");
    assert_eq!(items[0]["picture"], id, "{listed}");
    assert_eq!(items[1]["expected"], "그림 1 현황 사진", "{listed}");

    // The caption changes: a refresh rewrites the caption reference only.
    s.run(
        "object.caption_text",
        json!({"address": address, "text": "현장 사진"}),
    )?;
    let refreshed = s.run("tools.cross_reference.update", json!({}))?;
    assert_eq!(refreshed["updated"], 1, "{refreshed}");
    assert_eq!(commands(&s)[1].1, "그림 1 현장 사진");
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("pictures.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(commands(&back), commands(&s), "{ext}");
        assert_eq!(picture_ids(&back), vec![id], "{ext}");
        let still = back.run("tools.cross_reference.update", json!({}))?;
        assert_eq!(still["updated"], 0, "{ext}: {still}");
        assert_eq!(still["missing"], json!([]), "{ext}: {still}");
    }

    // Undo takes the refresh, the caption edit, then both references and the id.
    for _ in 0..2 {
        s.run("edit.undo", json!({}))?;
    }
    assert_eq!(commands(&s)[1].1, "그림 1 현황 사진");
    for _ in 0..3 {
        s.run("edit.undo", json!({}))?;
    }
    assert!(commands(&s).is_empty());
    assert_eq!(picture_ids(&s), before);
    Ok(())
}

#[test]
fn number_references_need_a_captioned_table_or_picture() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut s, _) = document(dir.path())?;
    s.run("text.insert", json!({"text": "제1장\n"}))?;
    s.cursor = Cursor {
        offset: 4,
        ..Default::default()
    };
    let generation = s.generation;
    for target in [
        json!({"picture": 2}),
        json!({"picture": 0}),
        json!({"heading": "제1장"}),
    ] {
        let refused = s.run(
            "insert.cross_reference",
            json!({"target": target, "show": "number"}),
        );
        assert!(refused.is_err(), "{target}");
    }
    assert_eq!(s.generation, generation);
    Ok(())
}
