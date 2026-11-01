//! Owner: format area. A 덧말's base text sits in its control, not in the paragraph
//! text; RHWP drew it at its width but line breaking counted it as nothing, so a
//! 덧말 near the end of a line ran past the right margin. With RHWP 0071 the line
//! breaks before it.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

/// Body line count of paragraph 0 and the paragraph's 덧말 count.
fn lines(s: &Session) -> (usize, usize) {
    let para = &s.core.document().sections[0].paragraphs[0];
    let rubies = para
        .controls
        .iter()
        .filter(|c| matches!(c, Control::Ruby(_)))
        .count();
    (para.line_segs.len(), rubies)
}

#[test]
fn a_ruby_near_the_line_end_moves_to_the_next_line() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    // One line of 가 that fits, then six base characters and a last one.
    let mut fits = 0;
    for n in 30..80 {
        let mut probe = Session::new()?;
        probe.run("text.insert", json!({"text": "가".repeat(n)}))?;
        if lines(&probe).0 > 1 {
            fits = n - 1;
            break;
        }
    }
    assert!(fits > 10, "a body line holds {fits} 가");
    let before = fits - 3;
    s.run(
        "text.insert",
        json!({"text": format!("{}漢字漢字漢字끝", "가".repeat(before))}),
    )?;
    assert_eq!(lines(&s).0, 2, "the base text wraps as paragraph text");
    s.cursor = Cursor {
        offset: before + 6,
        ..Default::default()
    };
    s.anchor = Some(before);
    s.run("format.ruby", json!({"text": "한자"}))?;
    assert_eq!(
        lines(&s),
        (2, 1),
        "the base text in the control still wraps"
    );
    // Saved and reopened, the stored lines keep the break.
    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("ruby.{ext}"));
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(lines(&back), (2, 1), "{ext}");
    }
    Ok(())
}
