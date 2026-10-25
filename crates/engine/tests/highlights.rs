//! Owner: core area. Markpen overlay geometry built from RHWP markpen marks.
use hwpcraft_engine::{Command, Session, markpen_ranges};
use serde_json::json;

#[test]
fn highlight_rects_follow_markpen_marks_and_disappear_when_cleared() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("형광펜 표시 검사 문장입니다".into()))?;
    assert!(s.highlight_rects().is_empty());
    s.anchor = Some(0);
    s.cursor.offset = 3;
    s.run("format.highlight", json!({"color": "#ffff00"}))?;
    let paragraph = s.paragraph(s.cursor)?;
    let ranges = markpen_ranges(paragraph);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].0, 0..3);
    assert!(ranges[0].1.eq_ignore_ascii_case("#ffff00"), "{ranges:?}");
    let rects = s.highlight_rects();
    assert_eq!(rects.len(), 1, "{rects:?}");
    assert_eq!(rects[0].page, 0);
    assert!(rects[0].color.eq_ignore_ascii_case("#ffff00"));
    assert!(rects[0].width > rects[0].height, "three glyphs on one line");
    // Clearing the highlight removes the overlay.
    s.anchor = Some(0);
    s.cursor.offset = 3;
    s.run("format.highlight", json!({}))?;
    assert!(s.highlight_rects().is_empty());
    s.execute(Command::Undo)?;
    assert_eq!(s.highlight_rects().len(), 1);
    Ok(())
}

#[test]
fn highlight_spanning_lines_yields_one_rect_per_line() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let long: String = "가나다라마바사아자차카타파하 ".repeat(12);
    s.execute(Command::Insert(long.clone()))?;
    s.anchor = Some(0);
    s.cursor.offset = long.chars().count();
    s.run("format.highlight", json!({"color": "#00ff00"}))?;
    let rects = s.highlight_rects();
    assert!(rects.len() >= 2, "a long run wraps: {rects:?}");
    let mut ys: Vec<i64> = rects.iter().map(|r| r.y.round() as i64).collect();
    ys.dedup();
    assert_eq!(ys.len(), rects.len(), "one rectangle per line");
    Ok(())
}
