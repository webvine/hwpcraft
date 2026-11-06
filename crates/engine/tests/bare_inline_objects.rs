//! Owner: caret area. The caret beside a picture treated as a character that has no
//! character of its own, as Hangul stores one (HwpCraft gives the pictures it inserts a
//! placeholder character): a click left of it, the place behind it, typing, the arrows,
//! Home/End, Backspace, Delete and Enter.
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};

fn png() -> anyhow::Result<Vec<u8>> {
    let img = image::RgbImage::from_pixel(96, 48, image::Rgb([10, 20, 200]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// A picture treated as a character put in at text offset `offset` the way Hangul stores
/// it: no placeholder character beside it.
fn bare_picture(s: &mut Session, paragraph: usize, offset: usize) -> anyhow::Result<usize> {
    let bytes = png()?;
    let r = s
        .core
        .insert_picture_native(
            0,
            paragraph,
            offset,
            &[],
            &bytes,
            7200,
            3600,
            96,
            48,
            "png",
            "",
            None,
            None,
        )
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let v: Value = serde_json::from_str(&r)?;
    let ci = v["controlIdx"].as_u64().unwrap_or(0) as usize;
    s.core
        .set_picture_properties_native(0, paragraph, ci, r#"{"treatAsChar":true}"#)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(ci)
}

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph,
        offset,
        ..Default::default()
    }
}

/// The paragraph's text and where its pictures stand in it.
fn shape(s: &Session, paragraph: usize) -> anyhow::Result<(String, Vec<usize>)> {
    let p = s.paragraph(at(paragraph, 0))?;
    let pictures = p
        .controls
        .iter()
        .zip(p.control_text_positions())
        .filter(|(c, _)| matches!(c, hwpcraft_doc::model::control::Control::Picture(_)))
        .map(|(_, at)| at)
        .collect();
    Ok((p.text.clone(), pictures))
}

/// The picture's box on page 1.
fn picture_box(s: &Session, paragraph: usize) -> anyhow::Result<(f64, f64, f64, f64)> {
    let layout: Value = serde_json::from_str(&s.page_control_layout(0)?)?;
    let c = layout["controls"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|c| c["type"] == "image" && c["paraIdx"] == paragraph)
        })
        .ok_or_else(|| anyhow::anyhow!("no picture in paragraph {paragraph}"))?;
    let n = |k: &str| c[k].as_f64().unwrap_or(0.0);
    Ok((n("x"), n("y"), n("w"), n("h")))
}

/// Click at (x, y) on page 1 as the window does: hit test, then select.
fn click(s: &mut Session, x: f64, y: f64) -> anyhow::Result<Cursor> {
    let c = s.hit_test(0, x, y)?;
    s.select(c, false)?;
    Ok(s.cursor)
}

fn caret_x(s: &Session) -> anyhow::Result<f64> {
    Ok(s.caret()?["x"].as_f64().unwrap_or(-1.0))
}

fn right(s: &mut Session) -> anyhow::Result<()> {
    s.run("caret.move", json!({"motion": "right"}))?;
    Ok(())
}

fn left(s: &mut Session) -> anyhow::Result<()> {
    s.run("caret.move", json!({"motion": "left"}))?;
    Ok(())
}

/// "앞" then, in paragraph 1, a picture alone (the user's logo line).
fn picture_alone() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞".into()))?;
    s.execute(Command::Split)?;
    bare_picture(&mut s, 1, 0)?;
    assert_eq!(shape(&s, 1)?, (String::new(), vec![0]));
    Ok(s)
}

#[test]
fn a_click_beside_a_picture_alone_puts_the_caret_on_that_side() -> anyhow::Result<()> {
    let mut s = picture_alone()?;
    let (x, y, w, h) = picture_box(&s, 1)?;
    let mid = y + h / 2.0;
    // Left of it: in front, drawn at its left edge; typing goes in front.
    for dx in [40.0, 3.0] {
        assert_eq!(click(&mut s, x - dx, mid)?, at(1, 0), "{dx} px left");
        assert!(!s.caret_after_object());
        assert!((caret_x(&s)? - x).abs() < 1.0);
    }
    s.execute(Command::Insert("A".into()))?;
    assert_eq!(
        shape(&s, 1)?,
        ("A".into(), vec![1]),
        "A in front of the picture"
    );
    s.execute(Command::Undo)?;
    assert_eq!(shape(&s, 1)?, (String::new(), vec![0]));
    // Right of it: behind, drawn at its right edge; typing goes behind.
    for dx in [3.0, 40.0] {
        assert_eq!(click(&mut s, x + w + dx, mid)?, at(1, 0), "{dx} px right");
        assert!(s.caret_after_object(), "{dx} px right");
        assert!((caret_x(&s)? - (x + w)).abs() < 1.0, "{}", caret_x(&s)?);
    }
    s.execute(Command::Insert("B".into()))?;
    assert_eq!(shape(&s, 1)?, ("B".into(), vec![0]), "B behind the picture");
    assert_eq!(s.cursor, at(1, 1));
    assert!(!s.caret_after_object());
    // One undo takes the typing back, the picture where it was.
    s.execute(Command::Undo)?;
    assert_eq!(shape(&s, 1)?, (String::new(), vec![0]));
    Ok(())
}

#[test]
fn the_arrows_home_and_end_step_over_a_picture_alone() -> anyhow::Result<()> {
    let mut s = picture_alone()?;
    let (x, _, w, _) = picture_box(&s, 1)?;
    s.select(at(1, 0), false)?;
    right(&mut s)?;
    assert!(s.caret_after_object() && s.cursor == at(1, 0));
    assert!((caret_x(&s)? - (x + w)).abs() < 1.0);
    left(&mut s)?;
    assert!(!s.caret_after_object() && s.cursor == at(1, 0));
    // ← from in front of it goes to the previous paragraph; → from its end comes back
    // in front of it, → again behind it.
    left(&mut s)?;
    assert_eq!(s.cursor, at(0, 1));
    right(&mut s)?;
    assert!(s.cursor == at(1, 0) && !s.caret_after_object());
    right(&mut s)?;
    assert!(s.caret_after_object());
    // ← from the next paragraph's start lands behind it.
    s.execute(Command::Split)?;
    assert_eq!(
        shape(&s, 1)?,
        (String::new(), vec![0]),
        "Enter behind it keeps it"
    );
    assert_eq!(s.cursor, at(2, 0));
    left(&mut s)?;
    assert!(s.cursor == at(1, 0) && s.caret_after_object());
    // Home / End.
    s.run("caret.move", json!({"motion": "line_start"}))?;
    assert!(!s.caret_after_object());
    s.run("caret.move", json!({"motion": "line_end"}))?;
    assert!(s.caret_after_object());
    Ok(())
}

#[test]
fn backspace_behind_and_delete_in_front_remove_the_picture() -> anyhow::Result<()> {
    let mut s = picture_alone()?;
    s.select(at(1, 0), false)?;
    right(&mut s)?;
    s.execute(Command::Backspace)?;
    assert_eq!(shape(&s, 1)?.1, Vec::<usize>::new(), "Backspace behind it");
    s.execute(Command::Undo)?;
    assert_eq!(shape(&s, 1)?.1, vec![0]);
    s.select(at(1, 0), false)?;
    s.execute(Command::Delete)?;
    assert_eq!(
        shape(&s, 1)?.1,
        Vec::<usize>::new(),
        "Delete in front of it"
    );
    Ok(())
}

#[test]
fn a_picture_after_the_text_at_the_paragraph_end() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("끝".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("다음".into()))?;
    bare_picture(&mut s, 0, 1)?;
    assert_eq!(shape(&s, 0)?, ("끝".into(), vec![1]));
    let (x, y, w, h) = picture_box(&s, 0)?;
    // → steps: in front of 끝, in front of the picture, behind it, the next paragraph.
    s.select(at(0, 0), false)?;
    let mut steps = vec![(s.cursor, s.caret_after_object())];
    for _ in 0..3 {
        right(&mut s)?;
        steps.push((s.cursor, s.caret_after_object()));
    }
    assert_eq!(
        steps,
        [
            (at(0, 0), false),
            (at(0, 1), false),
            (at(0, 1), true),
            (at(1, 0), false)
        ]
    );
    let mut back = Vec::new();
    for _ in 0..3 {
        left(&mut s)?;
        back.push((s.cursor, s.caret_after_object()));
    }
    assert_eq!(
        back,
        [(at(0, 1), true), (at(0, 1), false), (at(0, 0), false)]
    );
    // A click right of it, then typing: the text goes behind it.
    click(&mut s, x + w + 30.0, y + h / 2.0)?;
    assert!(s.caret_after_object());
    assert!((caret_x(&s)? - (x + w)).abs() < 1.0, "{}", caret_x(&s)?);
    s.execute(Command::Insert("X".into()))?;
    assert_eq!(shape(&s, 0)?, ("끝X".into(), vec![1]));
    // Typed in front of it (after 끝), the text goes in front.
    s.execute(Command::Undo)?;
    s.select(at(0, 1), false)?;
    s.execute(Command::Insert("Y".into()))?;
    assert_eq!(shape(&s, 0)?, ("끝Y".into(), vec![2]));
    Ok(())
}

/// Backspace on the character next to the object: the caret stays on the side it was.
#[test]
fn backspace_beside_the_picture_keeps_the_caret_on_its_side() -> anyhow::Result<()> {
    // "가[그림]": the caret in front of the picture deletes 가 and stays in front.
    let mut s = Session::new()?;
    s.execute(Command::Insert("가".into()))?;
    bare_picture(&mut s, 0, 1)?;
    s.select(at(0, 1), false)?;
    s.execute(Command::Backspace)?;
    assert!(!s.caret_after_object());
    s.execute(Command::Insert("A".into()))?;
    assert_eq!(shape(&s, 0)?, ("A".into(), vec![1]));
    // "[그림]가": the caret behind 가 deletes it and stands behind the picture.
    let mut s = Session::new()?;
    s.execute(Command::Insert("가".into()))?;
    bare_picture(&mut s, 0, 0)?;
    assert_eq!(shape(&s, 0)?, ("가".into(), vec![0]));
    s.select(at(0, 1), false)?;
    s.execute(Command::Backspace)?;
    assert!(s.caret_after_object());
    s.execute(Command::Insert("B".into()))?;
    assert_eq!(shape(&s, 0)?, ("B".into(), vec![0]));
    Ok(())
}
