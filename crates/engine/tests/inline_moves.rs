//! Owner: shell/page area. `object.move_inline`: an object treated as a character moves to
//! another text position in one undo step, matching what HWP and HWPX store, and around a
//! memo it follows RHWP 0019 (inside at the start, outside at the end).
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, ObjectAddress, Session};
use serde_json::{Value, json};

fn at(paragraph: usize, offset: usize) -> hwpcraft_doc::Cursor {
    hwpcraft_doc::Cursor {
        section: 0,
        paragraph,
        offset,
        cell: None,
    }
}

/// Per paragraph: text, char offsets, field ranges and control kinds with text positions.
fn model(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| {
            let controls: Vec<_> = p
                .controls
                .iter()
                .zip(p.control_text_positions())
                .map(|(c, pos)| {
                    let kind = match c {
                        Control::Picture(_) => "picture",
                        Control::Field(_) => "field",
                        _ => "other",
                    };
                    (kind, pos)
                })
                .collect();
            let ranges: Vec<_> = p
                .field_ranges
                .iter()
                .map(|r| (r.start_char_idx, r.end_char_idx, r.control_idx))
                .collect();
            format!("{:?} {:?} {ranges:?} {controls:?}", p.text, p.char_offsets)
        })
        .collect()
}

fn round_trip(s: &mut Session, label: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let live = model(s);
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("moved.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        anyhow::ensure!(
            model(&back) == live,
            "{label}: {ext} reopened model differs\n  live {live:?}\n  back {:?}",
            model(&back)
        );
    }
    Ok(())
}

/// (paragraph, text position) of every picture in section 0.
fn pictures(s: &Session) -> Vec<(usize, usize)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            p.controls
                .iter()
                .zip(p.control_text_positions())
                .filter(|(c, _)| matches!(c, Control::Picture(_)))
                .map(move |(_, pos)| (i, pos))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn picture(s: &mut Session, dir: &std::path::Path) -> anyhow::Result<ObjectAddress> {
    let png = dir.join("p.png");
    image::RgbImage::from_fn(20, 20, |x, _| image::Rgb([x as u8 * 9, 60, 90]))
        .save_with_format(&png, image::ImageFormat::Png)?;
    let v = s.run(
        "picture.insert",
        json!({"path": png, "width_mm": 8.0, "treat_as_char": true}),
    )?;
    Ok(serde_json::from_value(v["address"].clone())?)
}

#[test]
fn an_inline_picture_moves_to_another_text_position() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라\n마바사아".into()))?;
    s.select(at(0, 2), false)?;
    let a = picture(&mut s, dir.path())?;
    assert_eq!(pictures(&s), [(0, 2)]);
    let texts = |s: &Session| -> Vec<String> {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect()
    };
    let before = texts(&s);
    // Into the next paragraph, in front of "바".
    let v = s.run(
        "object.move_inline",
        json!({"address": a, "to": {"section": 0, "paragraph": 1, "offset": 1}}),
    )?;
    let moved: ObjectAddress = serde_json::from_value(v["address"].clone())?;
    assert_eq!((moved.paragraph, pictures(&s)), (1, vec![(1, 1)]));
    assert_eq!(texts(&s), before, "the text is unchanged");
    assert_eq!(
        s.object_properties(moved)?["props"]["treatAsChar"],
        Value::Bool(true)
    );
    round_trip(&mut s, "next paragraph")?;
    // Within the paragraph, to its end.
    s.run(
        "object.move_inline",
        json!({"address": moved, "to": {"section": 0, "paragraph": 1, "offset": 4}}),
    )?;
    assert_eq!(pictures(&s), [(1, 4)]);
    round_trip(&mut s, "paragraph end")?;
    // One undo step each.
    s.execute(Command::Undo)?;
    assert_eq!(pictures(&s), [(1, 1)]);
    s.execute(Command::Undo)?;
    assert_eq!(pictures(&s), [(0, 2)]);
    // Refusals: a floating object, a position past the paragraph, a table cell.
    assert!(
        s.run(
            "object.move_inline",
            json!({"address": a, "to": {"section": 0, "paragraph": 1, "offset": 99}}),
        )
        .is_err()
    );
    s.run(
        "object.properties",
        json!({"address": a, "props": {"treatAsChar": false}}),
    )?;
    assert!(
        s.run(
            "object.move_inline",
            json!({"address": a, "to": {"section": 0, "paragraph": 1, "offset": 1}}),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn around_a_memo_the_picture_follows_the_field_boundary_rules() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라마바사아자".into()))?;
    // A memo over "다라마바" (2..6).
    s.select(at(0, 2), false)?;
    s.select(at(0, 6), true)?;
    s.run("memo.create", json!({"text": "메모", "author": "검토"}))?;
    s.select(at(0, 8), false)?;
    let a = picture(&mut s, dir.path())?;
    let memo_anchor = |s: &mut Session| -> anyhow::Result<Value> {
        let list = s.run("memo.list", json!({}))?;
        Ok(list[0]["anchor"].clone())
    };
    let anchor = memo_anchor(&mut s)?;
    for (offset, label) in [(2, "memo start"), (4, "inside the memo"), (6, "memo end")] {
        let current = pictures(&s)[0];
        let control = s.core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .zip(s.core.document().sections[0].paragraphs[0].control_text_positions())
            .position(|(c, pos)| matches!(c, Control::Picture(_)) && pos == current.1)
            .ok_or_else(|| anyhow::anyhow!("picture"))?;
        let address = ObjectAddress { control, ..a };
        s.run(
            "object.move_inline",
            json!({"address": address, "to": {"section": 0, "paragraph": 0, "offset": offset}}),
        )?;
        assert_eq!(pictures(&s), [(0, offset)], "{label}");
        if offset == 2 {
            // At the memo's start the picture goes in behind the field-begin control.
            let kinds: Vec<bool> = s.core.document().sections[0].paragraphs[0]
                .controls
                .iter()
                .filter(|c| matches!(c, Control::Field(_) | Control::Picture(_)))
                .map(|c| matches!(c, Control::Picture(_)))
                .collect();
            assert_eq!(
                kinds,
                [false, true],
                "{label}: field first, then the picture"
            );
        }
        assert_eq!(memo_anchor(&mut s)?, anchor, "{label}: memo text unchanged");
        round_trip(&mut s, label)?;
    }
    Ok(())
}
