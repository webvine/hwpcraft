//! ↓ and ↑ cross a page break inside a long paragraph: from the last line of a
//! page the caret goes on to the first line of the next one (and back), instead of
//! creeping along the same line a character at a time.
use hwpcraft_doc::Cursor;
use hwpcraft_engine::{Motion, Session};
use serde_json::json;

fn caret(s: &Session) -> anyhow::Result<(u64, f64)> {
    let v = s.caret()?;
    Ok((
        v["pageIndex"].as_u64().unwrap_or(0),
        v["y"].as_f64().unwrap_or(0.0),
    ))
}

#[test]
fn down_and_up_cross_a_page_break_inside_a_paragraph() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let text = "쪽을 넘어가는 긴 문단의 캐럿 이동을 확인하는 문장입니다. ".repeat(160);
    s.run("text.insert", json!({"text": text}))?;
    s.run("caret.move", json!({"motion": "document_start"}))?;
    assert!(s.page_count()? > 1);

    // Down line by line: every move goes to a later line, and page 2 is reached.
    let mut at = caret(&s)?;
    let mut reached = false;
    for n in 0..80 {
        s.move_caret(Motion::Down, false)?;
        let next = caret(&s)?;
        assert!(
            next.0 > at.0 || (next.0 == at.0 && next.1 > at.1 + 1.0),
            "↓ #{n} stayed on its line: {at:?} → {next:?} ({:?})",
            s.cursor
        );
        at = next;
        if at.0 == 1 {
            reached = true;
            break;
        }
    }
    assert!(reached, "↓ did not reach page 2");

    // Up from the first line of page 2 goes back to the last line of page 1.
    s.run("caret.move", json!({"motion": "up"}))?;
    let back = caret(&s)?;
    assert_eq!(back.0, 0, "↑ back to page 1: {back:?}");
    Ok(())
}

/// In two columns ↓ from the first column's last line goes to the second column's
/// first line on the same page, and ↑ comes back.
#[test]
fn down_and_up_cross_from_one_column_to_the_next() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("page.columns.count", json!({"count": 2}))?;
    let text = "쪽을 넘어가는 긴 문단의 캐럿 이동을 확인하는 문장입니다. ".repeat(168);
    s.run("text.insert", json!({"text": text}))?;
    let at = |offset| Cursor {
        section: 0,
        paragraph: 0,
        offset,
        cell: None,
    };
    // The lines' second positions and where they are shown.
    let p = s.core.document().sections[0].paragraphs[0].clone();
    let mut lines = Vec::new();
    for l in &p.line_segs {
        let start = p
            .char_offsets
            .iter()
            .position(|&st| st >= l.text_start)
            .unwrap_or(p.char_offsets.len());
        let v = s.caret_at(at(start + 1))?;
        lines.push((
            start + 1,
            v["pageIndex"].as_u64().unwrap(),
            v["x"].as_f64().unwrap(),
            v["y"].as_f64().unwrap(),
        ));
    }
    // The first column's last line: the next line starts higher on the same page.
    let k = (0..lines.len() - 1)
        .find(|&k| lines[k + 1].1 == lines[k].1 && lines[k + 1].3 < lines[k].3)
        .expect("a second column");
    s.select(at(lines[k].0), false)?;
    s.move_caret(Motion::Down, false)?;
    let down = caret(&s)?;
    assert_eq!(
        (down.0, down.1.round()),
        (lines[k + 1].1, lines[k + 1].3.round()),
        "↓ to the second column's first line ({:?})",
        s.cursor
    );
    s.move_caret(Motion::Up, false)?;
    let up = caret(&s)?;
    assert_eq!(
        (up.0, up.1.round()),
        (lines[k].1, lines[k].3.round()),
        "↑ back to the first column's last line ({:?})",
        s.cursor
    );
    Ok(())
}
