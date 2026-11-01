//! A line made tall by a shape treated as a character holds its text at the bottom.
//! ↓ from the start of that line used to hit the same line again and stay put; it
//! now reaches the next line, and ↑ from there comes back.
use hwpcraft_doc::Cursor;
use hwpcraft_engine::{Command, Motion, Session};
use serde_json::json;

fn at(offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph: 0,
        offset,
        cell: None,
    }
}

fn caret_y(s: &Session) -> anyhow::Result<f64> {
    Ok(s.caret()?["y"].as_f64().unwrap_or_default())
}

#[test]
#[ignore = "0.21: RHWP hit_test_native returns offsets one short on lines after an inline object, so the probe lands a line too far (being fixed)"]
fn down_and_up_leave_a_line_made_tall_by_an_inline_shape() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let text = "글자처럼 취급한 도형이 첫 줄을 높게 만든다. ".repeat(12);
    s.execute(Command::Insert(text))?;
    s.select(at(10), false)?;
    s.run(
        "shape.insert",
        json!({"kind": "rectangle", "width_mm": 40.0, "height_mm": 27.0, "treat_as_char": true}),
    )?;

    s.select(at(0), false)?;
    let first = caret_y(&s)?;
    // The second line: the y of the first offset whose caret leaves the first line.
    let len = s.core.document().sections[0].paragraphs[0]
        .text
        .chars()
        .count();
    let mut second = None;
    for offset in 1..=len {
        s.select(at(offset), false)?;
        let y = caret_y(&s)?;
        if y > first + 1.0 {
            second = Some(y);
            break;
        }
    }
    let second = second.expect("a second line");
    s.select(at(0), false)?;
    s.move_caret(Motion::Down, false)?;
    assert!(
        (caret_y(&s)? - second).abs() < 1.0,
        "↓ lands on the second line {second}, not {:?} at {}",
        s.cursor,
        caret_y(&s)?
    );

    // MCP caret.move does the same.
    s.select(at(0), false)?;
    s.run("caret.move", json!({"motion": "down"}))?;
    assert!((caret_y(&s)? - second).abs() < 1.0, "{:?}", s.cursor);

    // ↑ from the second line goes back to the tall first line.
    s.move_caret(Motion::Up, false)?;
    assert!((caret_y(&s)? - first).abs() < 1.0, "{:?}", s.cursor);
    Ok(())
}
