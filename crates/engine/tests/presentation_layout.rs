use hwpcraft_engine::{Command, Cursor, Motion, Session};
use serde_json::json;

#[test]
fn failed_field_layout_open_preserves_existing_session_and_password() -> anyhow::Result<()> {
    let mut unsupported = Session::new()?;
    unsupported.run("insert.auto_text", json!({"kind":"page_number"}))?;
    for control in &mut unsupported.core.document_mut().sections[0].paragraphs[0].controls {
        if let hwpcraft_doc::model::control::Control::AutoNumber(number) = control {
            number.format = 127;
        }
    }
    let dir = tempfile::tempdir()?;
    let plain = dir.path().join("unsupported.hwp");
    let encrypted = dir.path().join("unsupported-password.hwp");
    hwpcraft_formats::save(&unsupported.core, &plain)?;
    hwpcraft_formats::save_with_password(&unsupported.core, &encrypted, "fixture-only")?;
    let mut current = Session::new()?;
    current.execute(Command::Insert("현재 문서 보존 😀".into()))?;
    current.set_password(Some("existing-fixture-password".into()))?;
    let before = current.info();
    let generation = current.generation;
    let cursor = current.cursor;
    assert!(current.open(&plain).is_err());
    assert!(
        current
            .open_with_password(&encrypted, "fixture-only")
            .is_err()
    );
    assert_eq!(current.info(), before);
    assert_eq!(current.generation, generation);
    assert_eq!(current.cursor, cursor);
    assert!(current.has_password());
    assert!(current.can_undo());
    assert_eq!(
        current.core.document().sections[0].paragraphs[0].text,
        "현재 문서 보존 😀"
    );
    let preserved = dir.path().join("preserved-password.hwp");
    current.save(&preserved)?;
    let mut reopened = Session::new()?;
    reopened.open_with_password(&preserved, "existing-fixture-password")?;
    assert_eq!(
        reopened.core.document().sections[0].paragraphs[0].text,
        "현재 문서 보존 😀"
    );
    Ok(())
}

fn reflowed_number_document() -> anyhow::Result<Session> {
    // Find a real boundary where a five-digit field occupies another line.
    // This exercises pagination rather than comparing two wrappers of one API.
    for length in 35..=44 {
        let mut s = Session::new()?;
        s.execute(Command::Insert(format!("\n{}", "가".repeat(length))))?;
        s.run("insert.auto_text", json!({"kind":"page_number"}))?;
        let mut document = s.core.document().clone();
        let first = document.sections[0].paragraphs[0].clone();
        let paragraph = document.sections[0].paragraphs[1].clone();
        document.sections[0].paragraphs = std::iter::once(first)
            .chain(std::iter::repeat_n(paragraph, 90))
            .collect();
        s.core.set_document(document);
        s.run("page.new_number", json!({"start":65530}))?;
        if s.page_count()? > s.core.page_count() {
            return Ok(s);
        }
    }
    anyhow::bail!("fixture did not reach a different field pagination boundary")
}

#[test]
fn number_reflow_is_shared_by_navigation_statistics_and_page_export() -> anyhow::Result<()> {
    let mut s = reflowed_number_document()?;
    let pages = s.page_count()?;
    assert_eq!(s.info()["pages"], pages);
    assert_eq!(s.statistics()["pages"], pages);
    assert_eq!(s.run("file.doc_info", json!({}))?["pages"], pages);
    s.run("edit.goto", json!({"page":pages}))?;
    assert_eq!(s.caret()?["pageIndex"], pages - 1);
    assert!(s.page_control_layout(pages - 1).is_ok());
    let dir = tempfile::tempdir()?;
    let pdf = dir.path().join("last-page.pdf");
    s.run(
        "file.save_pages",
        json!({"path":pdf,"from":pages,"to":pages}),
    )?;
    assert!(std::fs::read(pdf)?.starts_with(b"%PDF-"));
    let target = dir.path().join("preserved.hwp");
    std::fs::write(&target, b"existing file must survive")?;
    let before = s.core.document().sections[0].paragraphs[0].text.clone();
    assert!(
        s.run("file.save_pages", json!({"path":target,"from":1,"to":1}))
            .is_err()
    );
    assert_eq!(std::fs::read(target)?, b"existing file must survive");
    assert_eq!(s.core.document().sections[0].paragraphs[0].text, before);
    assert_eq!(s.page_count()?, pages);
    if std::env::var_os("HWPCRAFT_WRITE_LAYOUT_FIXTURES").is_some() {
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.16/fixtures");
        std::fs::create_dir_all(&output)?;
        s.save(&output.join("number-reflow.hwpx"))?;
    }
    Ok(())
}

fn long_plain_paragraph() -> anyhow::Result<Session> {
    let mut session = Session::new()?;
    session.execute(Command::Insert("가나다라마".repeat(1800)))?;
    assert!(session.page_count()? >= 3);
    Ok(session)
}

#[test]
fn plain_partial_paragraph_navigation_preserves_each_side_of_the_page_boundary()
-> anyhow::Result<()> {
    let mut session = long_plain_paragraph()?;
    let source = session.core.document().sections[0].paragraphs[0].clone();
    let start = session.page_first_position(1).expect("second page body");
    assert!(start.offset > 0);
    // A normal source cursor retains the native previous-line affinity.
    session.select(start, false)?;
    let native: serde_json::Value = serde_json::from_str(
        &session
            .core
            .get_cursor_rect_native(0, 0, start.offset)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    )?;
    assert_eq!(session.caret()?, native);

    session.run("edit.goto", json!({"page":2}))?;
    assert_eq!(session.cursor, start);
    assert_eq!(session.caret()?["pageIndex"], 1);
    assert_eq!(session.page_of(session.cursor), Some(1));
    session.move_caret(Motion::PageStart, false)?;
    assert_eq!(session.cursor, start);
    assert_eq!(session.caret()?["pageIndex"], 1);

    session.run("edit.goto", json!({"page":1}))?;
    session.move_caret(Motion::PageEnd, false)?;
    assert_eq!(session.cursor, start);
    assert_eq!(session.caret()?["pageIndex"], 0);
    session.move_caret(Motion::PageStart, false)?;
    assert_eq!(session.cursor.offset, 0);
    assert_eq!(session.caret()?["pageIndex"], 0);

    let last = session.page_count()?;
    session.run("edit.goto", json!({"page":last}))?;
    assert_eq!(session.caret()?["pageIndex"], last - 1);
    assert_eq!(session.page_of(session.cursor), Some(last as usize - 1));
    let current = &session.core.document().sections[0].paragraphs[0];
    assert_eq!(current.text, source.text);
    assert_eq!(current.char_offsets, source.char_offsets);
    assert_eq!(session.presentation_report()?["pageFields"], 0);
    Ok(())
}

#[test]
fn page_affinity_survives_view_flags_undo_and_checkpoint_without_moving_on_hit_probe()
-> anyhow::Result<()> {
    use hwpcraft_engine::presentation_geometry::{Affinity, CaretLookup, GeometryCache};
    let mut session = long_plain_paragraph()?;
    session.run("edit.goto", json!({"page":2}))?;
    let boundary = session.cursor;
    let downstream = session.caret()?;
    let checkpoint = session.checkpoint();
    session.execute(Command::Insert("입력😀".into()))?;
    session.rollback(checkpoint)?;
    assert_eq!(session.cursor, boundary);
    assert_eq!(session.caret()?["pageIndex"], 1);

    session.execute(Command::Insert("새".into()))?;
    session.execute(Command::Undo)?;
    assert_eq!(session.cursor, boundary);
    assert_eq!(session.caret()?["pageIndex"], 1);
    let mut marks = session.view_marks();
    marks.paragraph_marks = true;
    marks.control_codes = true;
    marks.transparent_borders = true;
    session.set_view_marks(marks);
    assert_eq!(session.caret()?["pageIndex"], 1);
    assert_eq!(session.caret()?["x"], downstream["x"]);

    let geometry = GeometryCache::default();
    let position = boundary.into();
    let CaretLookup::Text(upstream) = geometry.caret_on_page(
        &session.core,
        session.generation,
        0,
        &position,
        Affinity::Upstream,
    )?
    else {
        anyhow::bail!("upstream boundary must be text")
    };
    let hit = session.hit_test(0, upstream.x, upstream.y + upstream.height / 2.0)?;
    assert_eq!(hit, boundary);
    // Inspecting another page is read-only until selection accepts that hit.
    assert_eq!(session.cursor, boundary);
    assert_eq!(session.caret()?["pageIndex"], 1);
    session.select(hit, false)?;
    assert_eq!(session.caret()?["pageIndex"], 0);
    let CaretLookup::Text(downstream) = geometry.caret_on_page(
        &session.core,
        session.generation,
        1,
        &position,
        Affinity::Downstream,
    )?
    else {
        anyhow::bail!("downstream boundary must be text")
    };
    let hit = session.hit_test(1, downstream.x, downstream.y + downstream.height / 2.0)?;
    assert_eq!(hit, boundary);
    session.select(hit, false)?;
    assert_eq!(session.caret()?["pageIndex"], 1);

    let before = session.presentation_geometry_stats()?;
    session.page_first_position(1).expect("cached page");
    session.caret()?;
    let after = session.presentation_geometry_stats()?;
    assert_eq!(after.tree_builds, before.tree_builds);
    assert_eq!(after.layout_builds, before.layout_builds);
    assert!(after.hits > before.hits);
    // A selection with no accepted hit deliberately returns to native affinity.
    session.select(
        Cursor {
            offset: boundary.offset,
            ..boundary
        },
        false,
    )?;
    let native: serde_json::Value = serde_json::from_str(
        &session
            .core
            .get_cursor_rect_native(0, 0, boundary.offset)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    )?;
    assert_eq!(session.caret()?, native);
    Ok(())
}
