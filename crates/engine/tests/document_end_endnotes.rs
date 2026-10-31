//! An endnote added in an earlier section shows at the end of the document at once:
//! the last section, which collects the endnotes, is laid out again (RHWP patch
//! 0045) and the screen matches a fresh layout of the same document.
use hwpcraft_doc::{Cursor, DocumentCore};
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn last_page_svg(core: &DocumentCore) -> anyhow::Result<String> {
    core.render_page_svg_native(core.page_count() - 1)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

#[test]
fn an_endnote_in_the_first_section_shows_at_the_end_of_the_document() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for i in 0..10 {
        s.execute(Command::Insert(format!("첫 구역 문단 {i}")))?;
        s.execute(Command::Split)?;
    }
    s.run("page.section_break", json!({}))?;
    s.execute(Command::Insert("둘째 구역".into()))?;
    assert_eq!(s.core.document().sections.len(), 2);
    let before = last_page_svg(&s.core)?;
    s.select(
        Cursor {
            section: 0,
            paragraph: 2,
            offset: 3,
            cell: None,
        },
        false,
    )?;
    s.insert_note(true, "미주 본문")?;
    let mut fresh = DocumentCore::new_empty();
    fresh.set_document(s.core.document().clone());
    assert_eq!(s.core.page_count(), fresh.page_count());
    let live = last_page_svg(&s.core)?;
    assert_ne!(live, before, "the endnote joins the last page");
    assert_eq!(live, last_page_svg(&fresh)?);
    Ok(())
}
