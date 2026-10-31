//! Edits at the start, inside and the end of a memo or a form field (누름틀) must
//! survive an HWP and an HWPX round trip unchanged and undo cleanly. Rare objects
//! that cannot be placed consistently at such a boundary are refused with the
//! document as is.
use hwpcraft_doc::model::control::Control;
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

/// Everything a reopened file must reproduce, paragraph by paragraph.
fn model(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| {
            let controls: Vec<String> = p
                .controls
                .iter()
                .zip(p.control_text_positions())
                .map(|(c, pos)| {
                    let kind = match c {
                        Control::Field(_) => "field",
                        Control::AutoNumber(_) => "number",
                        Control::Footnote(_) => "footnote",
                        Control::Equation(_) => "equation",
                        Control::Table(_) => "table",
                        Control::SectionDef(_) | Control::ColumnDef(_) => "def",
                        _ => "other",
                    };
                    format!("{kind}@{pos}")
                })
                .collect();
            let ranges: Vec<_> = p
                .field_ranges
                .iter()
                .map(|r| (r.start_char_idx, r.end_char_idx, r.control_idx))
                .collect();
            let orphans: Vec<_> = p.orphan_field_ends.iter().map(|o| o.char_idx).collect();
            format!(
                "{:?} {:?} {ranges:?} {controls:?} orphan_ends {orphans:?}",
                p.text, p.char_offsets
            )
        })
        .collect()
}

/// Memo and form fields in the document.
fn memos(s: &Session) -> usize {
    use hwpcraft_doc::model::control::FieldType;
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .filter(|c| {
            matches!(c, Control::Field(f)
                if matches!(f.field_type, FieldType::Memo | FieldType::ClickHere))
        })
        .count()
}

/// A memo over "다라마바" (2..6), the same memo starting with a page number
/// (2..7), or a form field holding "홍길동" at 2 (2..5).
/// Returns the start, an inside and the end caret offsets.
fn setup(s: &mut Session, kind: &str) -> anyhow::Result<[usize; 3]> {
    s.execute(Command::Insert("가나다라마바사아자".into()))?;
    s.select(at(2), false)?;
    if kind.starts_with("memo") {
        s.select(at(6), true)?;
        s.run(
            "memo.create",
            json!({"text": "메모 내용", "author": "검토"}),
        )?;
        if kind == "memo" {
            return Ok([2, 4, 6]);
        }
        s.select(at(2), false)?;
        s.run("insert.auto_text", json!({"kind": "page_number"}))?;
        return Ok([2, 4, 7]);
    }
    s.run("insert.field", json!({"guide": "이름", "name": "who"}))?;
    s.run("field.set", json!({"name": "who", "value": "홍길동"}))?;
    Ok([2, 3, 5])
}

fn round_trip(s: &mut Session, label: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let live = model(s);
    for ext in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("memo.{ext}"));
        s.save(&path)
            .map_err(|e| anyhow::anyhow!("{label}: {ext} save failed: {e}"))?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        anyhow::ensure!(
            model(&reopened) == live,
            "{label}: {ext} reopened model differs
  live {live:?}
  back {:?}",
            model(&reopened)
        );
        anyhow::ensure!(memos(&reopened) == memos(s), "{label}: {ext} memo count");
    }
    Ok(())
}

type Edit = fn(&mut Session) -> anyhow::Result<()>;

fn edits() -> Vec<(&'static str, Edit)> {
    vec![
        ("typing", |s| s.execute(Command::Insert("글".into()))),
        ("enter", |s| s.execute(Command::Split)),
        ("pasted text", |s| s.paste("붙인 글")),
        ("pasted copy", |s| {
            let at = s.cursor;
            s.select(Cursor { offset: 7, ..at }, false)?;
            s.select(Cursor { offset: 9, ..at }, true)?;
            let text = s.copy()?;
            s.select(at, false)?;
            s.paste(&text)
        }),
        ("table", |s| {
            s.execute(Command::Table {
                rows: 2,
                columns: 2,
            })
        }),
        ("footnote", |s| {
            s.run("insert.footnote", json!({"text": "각주"}))
                .map(|_| ())
        }),
        ("equation", |s| {
            s.run("insert.equation", json!({"script": "x", "size_pt": 10.0}))
                .map(|_| ())
        }),
        ("page number", |s| {
            s.run("insert.auto_text", json!({"kind": "page_number"}))
                .map(|_| ())
        }),
    ]
}

fn case(kind: &str, place: usize, name: &str, edit: Edit) -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let carets = setup(&mut s, kind)?;
    let caret = carets[place];
    let place = ["start", "inside", "end"][place];
    let label = format!("{kind} {place}: {name}");
    let before = model(&s);
    s.select(at(caret), false)?;
    match edit(&mut s) {
        Ok(()) => {
            anyhow::ensure!(memos(&s) == 1, "{label}: field lost");
            round_trip(&mut s, &label)?;
            s.execute(Command::Undo)?;
            anyhow::ensure!(model(&s) == before, "{label}: undo differs");
            s.execute(Command::Redo)?;
            round_trip(&mut s, &format!("{label} (redo)"))
        }
        Err(error) => {
            anyhow::ensure!(
                error.to_string().contains("메모·누름틀"),
                "{label}: unexpected error {error}"
            );
            anyhow::ensure!(model(&s) == before, "{label}: refused but changed");
            round_trip(&mut s, &format!("{label} (refused)"))
        }
    }
}

#[test]
fn edits_at_memo_and_form_field_boundaries_round_trip_and_undo() {
    let mut failures = Vec::new();
    for kind in ["memo", "memo starting with a page number", "form field"] {
        for place in 0..3 {
            for (name, edit) in edits() {
                if let Err(e) = std::panic::catch_unwind(|| case(kind, place, name, edit))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("{kind} {place}: {name}: panicked")))
                {
                    failures.push(e.to_string());
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}
