//! A save changes the model only for the writers (tab widths, hidden line caches,
//! fresh rows for HWP, save times) and puts it back, so the next undo and typing reuse
//! the kept layout and still show what a fresh layout of the same document shows.
use hwpcraft_doc::DocumentCore;
use hwpcraft_engine::{Command, Session};

fn same_as_fresh(s: &Session) -> anyhow::Result<()> {
    let mut fresh = DocumentCore::new_empty();
    fresh.set_document(s.core.document().clone());
    assert_eq!(s.page_count()?, fresh.page_count());
    for page in 0..fresh.page_count() {
        assert_eq!(
            s.render_page_svg(page)?,
            fresh
                .render_page_svg_native(page)
                .map_err(|e| anyhow::anyhow!("{e}"))?,
            "page {page}"
        );
    }
    Ok(())
}

#[test]
fn undo_and_typing_after_a_save_match_a_fresh_layout() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let mut s = Session::new()?;
        for i in 0..40 {
            s.execute(Command::Insert(format!("문단 {i}\t탭 뒤 글자\n")))?;
        }
        s.execute(Command::Insert("마지막".into()))?;
        let path = dir.path().join(format!("doc.{ext}"));
        s.save(&path)?;
        s.execute(Command::Undo)?;
        same_as_fresh(&s)?;
        s.execute(Command::Insert(" 더".into()))?;
        s.save(&path)?;
        s.execute(Command::Insert("하나".into()))?;
        same_as_fresh(&s)?;
        s.execute(Command::Undo)?;
        s.execute(Command::Undo)?;
        same_as_fresh(&s)?;
    }
    Ok(())
}
