//! What "this part" means in a question: the selection, or else the paragraph
//! holding the caret. Fixed when the question is sent
//! and handed to the AI as a [맥락] block; the panel shows it as a chip.
use hwpcraft_engine::{Cursor, Session};

/// Characters of the region's text sent with the question.
const TEXT_LIMIT: usize = 200;
/// Characters of the text shown on the chip.
const CHIP_LIMIT: usize = 24;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Context {
    pub selection: bool,
    /// Caret, and for a selection its other end, as the engine addresses them.
    pub cursor: Cursor,
    pub anchor: Option<Cursor>,
    pub text: String,
}

fn cut(text: &str, limit: usize) -> String {
    let one_line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let one_line = one_line.trim();
    if one_line.chars().count() > limit {
        format!("{}…", one_line.chars().take(limit).collect::<String>())
    } else {
        one_line.to_owned()
    }
}

fn place(c: &Cursor) -> String {
    match c.cell {
        // The table's control index means nothing to the reader: the table
        // is named by the paragraph that holds it.
        Some(cell) if cell.paragraph > 0 => format!(
            "구역 {} 문단 {}의 표, 셀 {}의 문단 {}",
            c.section + 1,
            c.paragraph + 1,
            cell.index + 1,
            cell.paragraph + 1
        ),
        Some(cell) => format!(
            "구역 {} 문단 {}의 표, 셀 {}",
            c.section + 1,
            c.paragraph + 1,
            cell.index + 1
        ),
        None => format!("구역 {} 문단 {}", c.section + 1, c.paragraph + 1),
    }
}

impl Context {
    /// The region now, or `None` when the caret is in a nested table or a text
    /// box (its address is not a body address; v1 sends no context there).
    pub fn of(session: &Session) -> Option<Self> {
        if session.nested_caret().is_some() {
            return None;
        }
        let cursor = session.cursor;
        if session.has_selection() {
            return Some(Self {
                selection: true,
                cursor,
                anchor: session.anchor_cursor(),
                text: session.selected_text(),
            });
        }
        Some(Self {
            selection: false,
            cursor,
            anchor: None,
            text: session.text().to_owned(),
        })
    }

    /// The chip above the input box.
    pub fn chip(&self) -> String {
        let kind = if self.selection {
            "선택 영역"
        } else {
            "커서 문단"
        };
        let text = cut(&self.text, CHIP_LIMIT);
        let text = if text.is_empty() {
            "(빈 문단)".to_owned()
        } else {
            format!("'{text}'")
        };
        format!("{kind}: {} · {text}", place(&self.cursor))
    }

    /// The block put in front of the question.
    pub fn block(&self) -> String {
        let json = |c: &Cursor| serde_json::to_string(c).unwrap_or_default();
        let mut s = String::from("[맥락]\n");
        if self.selection {
            s.push_str("영역: 사용자가 선택한 범위\n");
            if let Some(a) = &self.anchor {
                s.push_str(&format!("선택 시작(anchor): {}\n", json(a)));
            }
            s.push_str(&format!("선택 끝(caret): {}\n", json(&self.cursor)));
        } else {
            s.push_str("영역: 캐럿이 있는 문단\n");
            s.push_str(&format!("캐럿: {}\n", json(&self.cursor)));
        }
        s.push_str(&format!("위치: {}\n", place(&self.cursor)));
        s.push_str(&format!(
            "글(앞 {TEXT_LIMIT}자): \"{}\"\n",
            cut(&self.text, TEXT_LIMIT)
        ));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hwpcraft_engine::Command;

    #[test]
    fn the_caret_paragraph_is_the_context_without_a_selection() {
        let mut s = Session::new().unwrap();
        s.execute(Command::Insert("첫 문단입니다\n둘째 문단은 여기".into()))
            .unwrap();
        let c = Context::of(&s).unwrap();
        assert!(!c.selection);
        assert_eq!(c.text, "둘째 문단은 여기");
        assert_eq!(c.chip(), "커서 문단: 구역 1 문단 2 · '둘째 문단은 여기'");
        let b = c.block();
        assert!(b.starts_with("[맥락]\n영역: 캐럿이 있는 문단\n"), "{b}");
        assert!(b.contains("\"paragraph\":1"), "{b}");
        assert!(b.contains("글(앞 200자): \"둘째 문단은 여기\""), "{b}");
    }

    #[test]
    fn a_selection_is_the_context_with_both_ends() {
        let mut s = Session::new().unwrap();
        s.execute(Command::Insert(
            "본 사업은 계약일로부터 6개월간 수행한다.".into(),
        ))
        .unwrap();
        let start = Cursor {
            offset: 0,
            ..s.cursor
        };
        let end = Cursor {
            offset: 5,
            ..s.cursor
        };
        s.select(start, false).unwrap();
        s.select(end, true).unwrap();
        let c = Context::of(&s).unwrap();
        assert!(c.selection);
        assert_eq!(c.text, "본 사업은");
        assert!(
            c.chip()
                .starts_with("선택 영역: 구역 1 문단 1 · '본 사업은'")
        );
        let b = c.block();
        assert!(
            b.contains("선택 시작(anchor): {\"section\":0,\"paragraph\":0,\"offset\":0"),
            "{b}"
        );
        assert!(
            b.contains("선택 끝(caret): {\"section\":0,\"paragraph\":0,\"offset\":5"),
            "{b}"
        );
    }

    #[test]
    fn a_table_cell_is_named_by_its_paragraph_and_cell() {
        let mut s = Session::new().unwrap();
        s.execute(Command::Insert(
            "앞 문단
"
            .into(),
        ))
        .unwrap();
        s.run("table.create", serde_json::json!({"rows": 2, "columns": 2}))
            .unwrap();
        s.run("table.cell_move", serde_json::json!({"motion": "next"}))
            .unwrap();
        s.execute(Command::Insert("셀 글".into())).unwrap();
        let c = Context::of(&s).unwrap();
        assert!(s.cursor.cell.is_some());
        let chip = c.chip();
        assert!(chip.contains("의 표, 셀 2"), "{chip}");
        assert!(!chip.contains("개체"), "no internal number: {chip}");
        assert!(chip.ends_with("'셀 글'"), "{chip}");
    }

    #[test]
    fn long_text_is_cut_and_kept_on_one_line() {
        let long = "가".repeat(300);
        assert_eq!(cut(&format!("a\tb\n{long}"), 5), "a b 가…");
        assert_eq!(cut("짧다", 5), "짧다");
        let mut s = Session::new().unwrap();
        s.execute(Command::Insert(long)).unwrap();
        let b = Context::of(&s).unwrap().block();
        assert!(b.contains(&format!("\"{}…\"", "가".repeat(200))));
        // An empty paragraph says so on the chip.
        let s = Session::new().unwrap();
        assert!(Context::of(&s).unwrap().chip().ends_with("(빈 문단)"));
    }
}
