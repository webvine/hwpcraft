//! "쪽/전체" next to an existing page number: the "/" typed between two number
//! placeholders must not land inside the first number's control slot.
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

fn at(offset: usize) -> Cursor {
    Cursor {
        section: 0,
        paragraph: 0,
        offset,
        cell: None,
    }
}

fn numbers(s: &Session) -> Vec<(String, usize)> {
    let p = &s.core.document().sections[0].paragraphs[0];
    p.controls
        .iter()
        .zip(p.control_text_positions())
        .filter_map(|(c, pos)| match c {
            hwpcraft_doc::model::control::Control::AutoNumber(n) => {
                Some((format!("{:?}", n.number_type), pos))
            }
            _ => None,
        })
        .collect()
}

fn ranges(s: &Session) -> Vec<(usize, usize, usize)> {
    let p = &s.core.document().sections[0].paragraphs[0];
    p.field_ranges
        .iter()
        .map(|r| (r.start_char_idx, r.end_char_idx, r.control_idx))
        .collect()
}

/// Save as HWP and HWPX, reopen, and require the same text, offsets, numbers and
/// field ranges.
fn round_trip(s: &mut Session, label: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let p = s.core.document().sections[0].paragraphs[0].clone();
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("page.{ext}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let q = &reopened.core.document().sections[0].paragraphs[0];
        assert_eq!(q.text, p.text, "{label}: {ext} text");
        assert_eq!(q.char_offsets, p.char_offsets, "{label}: {ext} offsets");
        assert_eq!(numbers(&reopened), numbers(s), "{label}: {ext} numbers");
        assert_eq!(ranges(&reopened), ranges(s), "{label}: {ext} field ranges");
    }
    Ok(())
}

/// Every number placeholder keeps its eight-unit slot and "/" one unit.
fn assert_slots(s: &Session, label: &str) {
    let p = &s.core.document().sections[0].paragraphs[0];
    let chars: Vec<char> = p.text.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        if *ch == '/' && i > 0 {
            assert_eq!(
                p.char_offsets[i] - p.char_offsets[i - 1],
                8,
                "{label}: the number before '/' lost its slot"
            );
            if let Some(next) = p.char_offsets.get(i + 1) {
                assert_eq!(next - p.char_offsets[i], 1, "{label}: '/' took a slot");
            }
        }
    }
}

/// (label, text, numbers already there as (kind, offset), caret)
type Case = (
    &'static str,
    &'static str,
    Vec<(&'static str, usize)>,
    usize,
);

#[test]
fn page_of_total_next_to_page_numbers_inserts_and_round_trips() -> anyhow::Result<()> {
    let cases: Vec<Case> = vec![
        ("plain", "가나다라마바사", vec![], 2),
        ("paragraph end", "가나", vec![], 2),
        (
            "number after caret",
            "가나다라마바사",
            vec![("page_number", 2)],
            2,
        ),
        (
            "number before caret",
            "가나다라마바사",
            vec![("page_number", 2)],
            3,
        ),
        (
            "numbers around caret",
            "가나다라",
            vec![("page_number", 2), ("total_pages", 4)],
            3,
        ),
    ];
    for (label, text, pre, caret) in cases {
        let mut s = Session::new()?;
        s.execute(Command::Insert(text.into()))?;
        for (kind, offset) in pre {
            s.select(at(offset), false)?;
            s.run("insert.auto_text", json!({"kind": kind}))?;
        }
        let before = numbers(&s).len();
        s.select(at(caret), false)?;
        s.run("insert.auto_text", json!({"kind": "page_of_total"}))?;
        assert_eq!(numbers(&s).len(), before + 2, "{label}: numbers");
        assert!(
            s.core.document().sections[0].paragraphs[0]
                .text
                .contains('/')
        );
        assert_slots(&s, label);
        round_trip(&mut s, label)?;
    }
    Ok(())
}

/// Page numbers at the start, inside and at the end of a memo. A number at the
/// memo start used to shift the memo by one on HWP reopen, drop it on HWPX
/// reopen, or fail the HWP save ("쪽/전체"). The number goes in at the caret
/// character, behind the memo begin, as RHWP's own insertion places it.
#[test]
fn page_numbers_at_a_memo_boundary_round_trip_and_undo() -> anyhow::Result<()> {
    for (place, caret) in [("start", 2), ("inside", 4), ("end", 6)] {
        for kind in ["page_number", "page_of_total"] {
            let label = format!("memo {place} {kind}");
            let mut s = Session::new()?;
            s.execute(Command::Insert("가나다라마바사아자".into()))?;
            s.select(at(2), false)?;
            s.select(at(6), true)?;
            s.run(
                "memo.create",
                json!({"text": "메모 내용", "author": "검토"}),
            )?;
            let before = s.core.document().sections[0].paragraphs[0].clone();
            s.select(at(caret), false)?;
            s.run("insert.auto_text", json!({"kind": kind}))?;
            assert_eq!(ranges(&s).len(), 1, "{label}: memo kept");
            assert_slots(&s, &label);
            round_trip(&mut s, &label)?;
            s.execute(Command::Undo)?;
            let undone = &s.core.document().sections[0].paragraphs[0];
            assert_eq!(undone.text, before.text, "{label}: undo text");
            assert_eq!(
                undone.char_offsets, before.char_offsets,
                "{label}: undo offsets"
            );
            s.execute(Command::Redo)?;
            round_trip(&mut s, &format!("{label} redo"))?;
        }
    }
    Ok(())
}

/// The sequence first reported: memo, page number kept through undo/redo, then
/// "쪽/전체" at the same memo boundary. It saved a corrupt paragraph (HWP refused,
/// HWPX reopened different); RHWP now inserts in front of the number placeholder
/// behind the memo begin, so both formats reopen the same and undo restores it.
#[test]
fn a_second_number_at_a_memo_boundary_round_trips_and_undoes() -> anyhow::Result<()> {
    for (place, caret) in [("start", 2), ("end", 6)] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("가나다라마바사아자".into()))?;
        s.select(at(2), false)?;
        s.select(at(6), true)?;
        s.run(
            "memo.create",
            json!({"text": "메모 내용", "author": "검토"}),
        )?;
        s.select(at(caret), false)?;
        s.run("insert.auto_text", json!({"kind": "page_number"}))?;
        s.execute(Command::Undo)?;
        s.execute(Command::Redo)?;
        let before = s.core.document().sections[0].paragraphs[0].clone();
        s.select(at(caret), false)?;
        s.run("insert.auto_text", json!({"kind": "page_of_total"}))?;
        assert_eq!(numbers(&s).len(), 3, "{place}: numbers");
        assert_eq!(ranges(&s).len(), 1, "{place}: memo kept");
        assert_slots(&s, place);
        round_trip(&mut s, &format!("memo {place}"))?;
        s.execute(Command::Undo)?;
        let undone = &s.core.document().sections[0].paragraphs[0];
        assert_eq!(undone.text, before.text, "{place}: undo text");
        assert_eq!(
            undone.char_offsets, before.char_offsets,
            "{place}: undo offsets"
        );
    }
    Ok(())
}

/// Typing by hand between two adjacent page numbers keeps both slots.
#[test]
fn typing_between_adjacent_page_numbers_keeps_their_slots() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라".into()))?;
    for _ in 0..2 {
        s.select(at(2), false)?;
        s.run("insert.auto_text", json!({"kind": "page_number"}))?;
    }
    s.select(at(3), false)?;
    s.execute(Command::Insert("/".into()))?;
    assert_eq!(numbers(&s).len(), 2);
    assert_slots(&s, "typed");
    round_trip(&mut s, "typed")
}
