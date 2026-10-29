use hwpcraft_engine::{Command, Session};
use serde_json::json;

/// PDF export drops RHWP's per-page render caches afterwards; pages shown or
/// exported later must come out exactly as before.
#[test]
fn pages_render_the_same_after_pdf_export_releases_caches() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽 본문 😀".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("둘째 쪽 ".into()))?;
    s.run("insert.auto_text", json!({"kind":"page_number"}))?;
    let before = [s.render_page_svg(0)?, s.render_page_svg(1)?];
    let first = s.render_pdf(&[0, 1])?;
    assert!(first.starts_with(b"%PDF"));
    let after = [s.render_page_svg(0)?, s.render_page_svg(1)?];
    assert_eq!(before, after);
    let job = s.pdf_job(&[0, 1])?;
    assert_eq!(job.page_count(), 2);
    assert!(job.convert()?.starts_with(b"%PDF"));
    assert_eq!([s.render_page_svg(0)?, s.render_page_svg(1)?], before);
    assert_eq!(s.page_count()?, 2);
    Ok(())
}

/// Preparing a PDF a page at a time gives the same bytes as preparing it at once,
/// and an edit in between invalidates the preparation.
#[test]
fn incremental_pdf_preparation_matches_and_refuses_a_changed_document() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽".into()))?;
    for page in 2..=4 {
        s.execute(Command::PageBreak)?;
        s.execute(Command::Insert(format!("{page}쪽 본문")))?;
    }
    let pages = [0, 1, 2, 3];
    let whole = s.pdf_job(&pages)?.convert()?;
    let mut prep = s.pdf_prepare(&pages)?;
    let mut steps = 0;
    while !s.pdf_prepare_step(&mut prep, std::time::Duration::ZERO)? {
        steps += 1;
        assert_eq!(prep.ready(), steps);
    }
    assert_eq!((prep.ready(), prep.total()), (4, 4));
    assert_eq!(s.pdf_finish(prep)?.convert()?, whole);

    let mut stale = s.pdf_prepare(&pages)?;
    assert!(!s.pdf_prepare_step(&mut stale, std::time::Duration::ZERO)?);
    s.execute(Command::Insert("편집".into()))?;
    assert!(
        s.pdf_prepare_step(&mut stale, std::time::Duration::ZERO)
            .is_err()
    );
    assert!(s.pdf_finish(stale).is_err());
    let unfinished = s.pdf_prepare(&pages)?;
    assert!(s.pdf_finish(unfinished).is_err());
    Ok(())
}
