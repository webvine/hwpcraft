//! ↑ from the first line of a paragraph at the top of a page goes to the last line
//! of the paragraph before it on the previous page, and ↓ comes back, keeping the
//! caret's x as between paragraphs on one page.
use hwpcraft_engine::{Command, Motion, Session};

fn caret(s: &Session) -> anyhow::Result<(u64, f64, f64)> {
    let v = s.caret()?;
    Ok((
        v["pageIndex"].as_u64().unwrap_or(0),
        v["x"].as_f64().unwrap_or(0.0),
        v["y"].as_f64().unwrap_or(0.0),
    ))
}

#[test]
fn up_and_down_between_paragraphs_on_two_pages_keep_x() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "앞 문단은 여러 줄에 걸쳐 이어지는 문장입니다. ".repeat(8),
    ))?;
    let last_line_y = caret(&s)?.2;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert(
        "뒤 문단도 여러 줄에 걸쳐 이어지는 문장입니다. ".repeat(8),
    ))?;
    let mut first = s.cursor;
    first.offset = 6;
    s.select(first, false)?;
    let below = caret(&s)?;
    assert_eq!(below.0, 1);

    s.move_caret(Motion::Up, false)?;
    let above = caret(&s)?;
    assert_eq!(
        (s.cursor.paragraph, above.0),
        (0, 0),
        "↑ to the paragraph before ({:?})",
        s.cursor
    );
    assert!(
        (above.2 - last_line_y).abs() < 1.0,
        "↑ lands on its last line: {above:?}, last line at {last_line_y}"
    );
    assert!(
        (above.1 - below.1).abs() < 16.0,
        "↑ keeps x: {below:?} → {above:?}"
    );

    s.move_caret(Motion::Down, false)?;
    let back = caret(&s)?;
    assert_eq!(
        (s.cursor.paragraph, back.0),
        (1, 1),
        "↓ back ({:?})",
        s.cursor
    );
    assert!(
        (back.1 - below.1).abs() < 16.0,
        "↓ keeps x: {below:?} → {back:?}"
    );
    Ok(())
}
