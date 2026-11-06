//! ↑ on the document's first line and ↓ on its last line leave the caret where it
//! is; they used to creep along the line a character at a time.
use hwpcraft_doc::Cursor;
use hwpcraft_engine::{Motion, Session};
use serde_json::json;

fn at(offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph: 0,
        offset,
        cell: None,
    }
}

#[test]
fn up_on_the_first_line_and_down_on_the_last_stay() -> anyhow::Result<()> {
    for columns in [1, 2] {
        let mut s = Session::new()?;
        if columns > 1 {
            s.run("page.columns.count", json!({"count": columns}))?;
        }
        let text = "첫 줄과 마지막 줄에서 화살표를 누르는 문장입니다. ".repeat(6);
        s.run("text.insert", json!({"text": text}))?;
        let len = s.core.document().sections[0].paragraphs[0]
            .text
            .chars()
            .count();
        s.select(at(3), false)?;
        s.move_caret(Motion::Up, false)?;
        assert_eq!(s.cursor, at(3), "↑ on the first line ({columns} columns)");
        s.select(at(len - 3), false)?;
        s.move_caret(Motion::Down, false)?;
        assert_eq!(
            s.cursor,
            at(len - 3),
            "↓ on the last line ({columns} columns)"
        );
    }
    Ok(())
}
