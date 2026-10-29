//! Deferred pagination (`Session::defer_layout`): nesting and equality with an
//! undeferred run (engine performance, table-object area).
use hwpcraft_doc::Cursor;
use hwpcraft_engine::{Command, Session};
use serde_json::{Value, json};

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

#[test]
fn nested_deferral_paginates_only_at_the_outermost_resume() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽".into()))?;
    assert!(s.resume_layout().is_err(), "resume without defer");
    let before = s.core.page_count();
    s.defer_layout()?;
    s.defer_layout()?;
    for _ in 0..3 {
        s.execute(Command::PageBreak)?;
    }
    s.resume_layout()?;
    assert!(s.layout_deferred(), "inner resume keeps the outer deferral");
    assert_eq!(s.core.page_count(), before, "pagination still deferred");
    s.resume_layout()?;
    assert!(!s.layout_deferred());
    assert_eq!(s.core.page_count(), before + 3);
    Ok(())
}

/// Model-only edits used for the equality check.
fn edits(s: &mut Session) -> anyhow::Result<()> {
    for i in 0..40 {
        s.execute(Command::Insert(
            format!("문단 {i} 본문 글자입니다. ").repeat(6),
        ))?;
        s.execute(Command::Split)?;
    }
    s.run("table.create", json!({"rows":30,"columns":3}))?;
    s.run(
        "table.cell_properties",
        json!({"range":{"start_row":0,"start_col":0,"end_row":29,"end_col":2},
            "props":{"fillType":"solid","fillColor":"#dbe5f1"}}),
    )?;
    s.run("table.style", json!({"preset":"blue_header"}))?;
    s.select(at(3, 0), false)?;
    s.run("format.paragraph", json!({"alignment":"center"}))?;
    Ok(())
}
fn snapshot(s: &mut Session) -> anyhow::Result<(u32, Value, Value)> {
    let last = s
        .entries()
        .into_iter()
        .rfind(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("cells"))?
        .0;
    Ok((s.page_count()?, s.caret_at(last)?, s.caret_at(at(20, 5))?))
}

#[test]
fn deferred_edits_match_an_undeferred_run_live_and_after_reopen() -> anyhow::Result<()> {
    let mut plain = Session::new()?;
    edits(&mut plain)?;
    let expected = snapshot(&mut plain)?;

    let mut deferred = Session::new()?;
    deferred.defer_layout()?;
    edits(&mut deferred)?;
    deferred.resume_layout()?;
    assert_eq!(snapshot(&mut deferred)?, expected);
    assert_eq!(deferred.text(), plain.text());
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("deferred.{ext}"));
        deferred.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(r.page_count()?, expected.0, "{ext}");
    }
    Ok(())
}
