//! Undo and redo lay out again only the sections a step changed. Every result
//! must match a full layout of the same model, page by page.
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Cell, Command, Cursor, Session};
use serde_json::{Value, json};

fn sample(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples")
        .join(name)
}

/// Page count and every page of `s` against a full layout of the same model,
/// made the way restore did before it reused sections: a snapshot round trip
/// after mutable model access, which keeps the snapshot's table reflow marks.
fn assert_full_layout(s: &mut Session, label: &str) -> anyhow::Result<()> {
    let render = |s: &Session| -> anyhow::Result<Vec<String>> {
        (0..s.core.page_count())
            .map(|page| {
                s.core
                    .render_page_svg_native(page)
                    .map_err(|e| anyhow::anyhow!("{e}"))
            })
            .collect()
    };
    let ours = render(s)?;
    full_layout(s)?;
    let full = render(s)?;
    assert_eq!(ours.len(), full.len(), "{label}: page count");
    for (page, (ours, full)) in ours.iter().zip(&full).enumerate() {
        assert!(
            ours == full,
            "{label}: page {} differs from a full layout",
            page + 1
        );
    }
    Ok(())
}

/// Lay the whole model out again, through restore's full path.
fn full_layout(s: &mut Session) -> anyhow::Result<()> {
    let id = s.core.save_snapshot_native();
    s.core.document_mut();
    let restored = s.core.restore_snapshot_native(id);
    s.core.discard_snapshot_native(id);
    restored.map(|_| ()).map_err(|e| anyhow::anyhow!("{e}"))
}

/// A body paragraph with plain text, past the middle of `section`.
fn body_paragraph(s: &Session, section: usize) -> Option<usize> {
    let paragraphs = &s.core.document().sections.get(section)?.paragraphs;
    let n = paragraphs.len();
    (n / 2..n).chain(1..n / 2).find(|&i| {
        let p = &paragraphs[i];
        p.controls.is_empty()
            && p.text.chars().count() > 10
            && paragraphs
                .get(i + 1)
                .is_some_and(|next| next.controls.is_empty() && next.text.chars().count() > 3)
    })
}

enum Step {
    Do(Command),
    Run(&'static str, Value),
    Select(usize, usize, usize, usize),
}

fn at(section: usize, paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section,
        paragraph,
        offset,
        cell: None,
    }
}

/// Run `steps` at `(section, paragraph)`, then undo, redo and undo them one by
/// one, comparing each state with a full layout.
fn round_trip(s: &mut Session, start: Cursor, name: &str, steps: Vec<Step>) -> anyhow::Result<()> {
    let (section, paragraph) = (start.section, start.paragraph);
    s.select(start, false)?;
    let mut edits = 0;
    for step in steps {
        match step {
            Step::Do(command) => {
                s.execute(command)?;
                edits += 1;
            }
            Step::Run(id, params) => {
                s.run(id, params)?;
                edits += 1;
            }
            Step::Select(p0, o0, p1, o1) => {
                let end = |p: usize, o: usize| {
                    let len = s.core.document().sections[section].paragraphs[paragraph + p]
                        .text
                        .chars()
                        .count();
                    at(
                        section,
                        paragraph + p,
                        if o == usize::MAX { len } else { o },
                    )
                };
                let (from, to) = (end(p0, o0), end(p1, o1));
                s.select(from, false)?;
                s.select(to, true)?;
            }
        }
    }
    let label = |what: &str, k: usize| format!("{name} @ section {section}: {what} {k}");
    for k in 0..edits {
        s.execute(Command::Undo)?;
        assert_full_layout(s, &label("undo", k))?;
    }
    for k in 0..edits {
        s.execute(Command::Redo)?;
        assert_full_layout(s, &label("redo", k))?;
    }
    for _ in 0..edits {
        s.execute(Command::Undo)?;
    }
    assert_full_layout(s, &label("undo all", edits))
}

fn scenarios(path: &std::path::Path) -> anyhow::Result<()> {
    assert!(path.exists(), "RHWP sample missing: {path:?}");
    let mut s = Session::new()?;
    s.open(path)?;
    assert_full_layout(&mut s, "opened")?;
    let sections = s.core.document().sections.len();
    assert!(sections >= 3, "the sample must have several sections");
    // Leave one edit in place: undoing to the untouched file changes RHWP's
    // document-wide edited layout, which always lays out every section. The
    // first edit itself keeps the other sections' unedited layout until a full
    // layout, as RHWP always did, so settle it before comparing.
    let first = (0..sections).find_map(|section| body_paragraph(&s, section).map(|p| (section, p)));
    if let Some((section, p)) = first {
        s.select(at(section, p, 0), false)?;
        s.execute(Command::Insert("·".into()))?;
        full_layout(&mut s)?;
    }
    let mut tried = 0;
    let editable: Vec<(usize, usize)> = (0..sections)
        .filter_map(|section| body_paragraph(&s, section).map(|p| (section, p)))
        .collect();
    let picks = [
        editable.first(),
        editable.get(editable.len() / 2),
        editable.last(),
    ];
    let mut seen = Vec::new();
    for &(section, p) in picks.into_iter().flatten() {
        if seen.contains(&section) {
            continue;
        }
        seen.push(section);
        tried += 1;
        let bold = || Step::Do(Command::Character(json!({"bold": true})));
        let cases: Vec<(&str, Vec<Step>)> = vec![
            ("insert", vec![Step::Do(Command::Insert("가나다".into()))]),
            ("split", vec![Step::Do(Command::Split)]),
            (
                "enter at the end",
                vec![
                    Step::Select(0, usize::MAX, 0, usize::MAX),
                    Step::Do(Command::Split),
                ],
            ),
            ("page break", vec![Step::Do(Command::PageBreak)]),
            (
                "bold from the start",
                vec![Step::Select(0, 0, 0, 5), bold()],
            ),
            ("bold inside", vec![Step::Select(0, 2, 0, 6), bold()]),
            (
                "pending bold then typing",
                vec![bold(), Step::Do(Command::Insert("굵게".into()))],
            ),
            (
                "paragraph",
                vec![Step::Do(Command::Paragraph(json!({"alignment": "center"})))],
            ),
            (
                "table",
                vec![
                    Step::Do(Command::Table {
                        rows: 2,
                        columns: 2,
                    }),
                    Step::Do(Command::Insert("셀".into())),
                ],
            ),
            (
                "footnote",
                vec![Step::Run("insert.footnote", json!({"text": "각주"}))],
            ),
            (
                "endnote",
                vec![Step::Run("insert.endnote", json!({"text": "미주"}))],
            ),
            (
                "memo",
                vec![
                    Step::Select(0, 1, 0, 4),
                    Step::Run("memo.create", json!({"text": "메모", "author": "검토"})),
                ],
            ),
            (
                "hyperlink",
                vec![Step::Run(
                    "insert.hyperlink",
                    json!({"uri": "https://example.com", "text": "링크"}),
                )],
            ),
            (
                "delete across paragraphs",
                vec![Step::Select(0, 2, 1, 2), Step::Do(Command::Delete)],
            ),
        ];
        for (name, steps) in cases {
            round_trip(&mut s, at(section, p, 3), name, steps)?;
        }
    }
    // Table cells: the first table of the first, middle and last sections that have one.
    let tables: Vec<Cursor> = (0..sections)
        .filter_map(|section| {
            let paragraphs = &s.core.document().sections[section].paragraphs;
            paragraphs.iter().enumerate().find_map(|(paragraph, p)| {
                let control = p.controls.iter().position(|c| {
                    matches!(c, Control::Table(t) if t.cells.first().is_some_and(|c| !c.paragraphs.is_empty()))
                })?;
                Some(Cursor {
                    section,
                    paragraph,
                    offset: 0,
                    cell: Some(Cell {
                        control,
                        index: 0,
                        paragraph: 0,
                    }),
                })
            })
        })
        .collect();
    let picks = [tables.first(), tables.get(tables.len() / 2), tables.last()];
    let mut seen = Vec::new();
    for &cell in picks.into_iter().flatten() {
        if seen.contains(&cell.section) {
            continue;
        }
        seen.push(cell.section);
        tried += 1;
        let cases: Vec<(&str, Vec<Step>)> = vec![
            (
                "cell insert",
                vec![Step::Do(Command::Insert("셀 글자 ".repeat(30)))],
            ),
            (
                "cell split",
                vec![Step::Do(Command::Split), Step::Do(Command::Split)],
            ),
            (
                "cell pending bold",
                vec![
                    Step::Do(Command::Character(json!({"bold": true}))),
                    Step::Do(Command::Insert("굵게".into())),
                ],
            ),
        ];
        for (name, steps) in cases {
            round_trip(&mut s, cell, name, steps)?;
        }
    }
    assert!(tried >= 2, "too few editable places in {path:?}");
    Ok(())
}

#[test]
fn undo_and_redo_match_a_full_layout_in_a_table_document() -> anyhow::Result<()> {
    scenarios(&sample("table-ipc.hwp"))
}

#[test]
fn undo_and_redo_match_a_full_layout_in_a_multi_section_decree() -> anyhow::Result<()> {
    scenarios(&sample("issue6117/52690_higher_education_decree.hwp"))
}
