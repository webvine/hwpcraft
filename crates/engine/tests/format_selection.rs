//! Character formatting of a multi-paragraph selection edits the live document and
//! paginates once at the end. The saved HWPX must be byte-identical to formatting
//! the paragraphs one at a time, and the page count must match the saved file
//! reopened (the former staged copy re-laid the result out from scratch, which lost
//! the open-time layout state: 380 instead of 381 pages on a 381-page handbook).
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};

const PARAGRAPHS: usize = 40;

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

/// A saved and reopened file, so the layout starts from an opened document.
fn document(dir: &tempfile::TempDir) -> anyhow::Result<Session> {
    let path = dir.path().join("base.hwpx");
    if !path.exists() {
        let mut s = Session::new()?;
        for i in 0..PARAGRAPHS {
            if i > 0 {
                s.execute(Command::Split)?;
            }
            let line = format!("문단 {i}: 글자 모양을 한꺼번에 바꾸는 본문입니다. Format {i}. ");
            s.execute(Command::Insert(line.repeat(1 + i % 3)))?;
        }
        s.save(&path)?;
    }
    let mut s = Session::new()?;
    s.open(&path)?;
    Ok(s)
}

fn paragraph_len(s: &Session, paragraph: usize) -> usize {
    s.core.document().sections[0].paragraphs[paragraph]
        .text
        .chars()
        .count()
}

fn select_paragraphs(s: &mut Session, first: usize, last: usize) -> anyhow::Result<()> {
    s.select(at(first, 0), false)?;
    s.select(at(last, paragraph_len(s, last)), true)
}

fn saved(s: &mut Session, dir: &tempfile::TempDir, name: &str) -> anyhow::Result<Vec<u8>> {
    let path = dir.path().join(name);
    s.save(&path)?;
    Ok(std::fs::read(path)?)
}

fn pages(s: &Session) -> anyhow::Result<Vec<String>> {
    (0..s.page_count()?).map(|p| s.render_page_svg(p)).collect()
}

fn patch() -> Value {
    // Size and font change line breaks; bold and italic do not.
    json!({"bold":true,"italic":true,"fontSize":1600,"fontFamily":"맑은 고딕"})
}

#[test]
fn selection_matches_paragraph_by_paragraph_and_the_reopened_file() -> anyhow::Result<()> {
    // Saving stamps the document's times; the byte comparison needs one instant.
    hwpcraft_engine::fixed_clock(Some(1_767_225_600));
    let dir = tempfile::tempdir()?;
    let mut whole = document(&dir)?;
    let original = saved(&mut whole, &dir, "original.hwpx")?;
    let before = whole.page_count()?;
    select_paragraphs(&mut whole, 0, PARAGRAPHS - 1)?;
    whole.execute(Command::Character(patch()))?;
    let formatted = saved(&mut whole, &dir, "whole.hwpx")?;
    assert!(whole.page_count()? > before, "the larger size adds pages");

    let mut single = document(&dir)?;
    for p in 0..PARAGRAPHS {
        select_paragraphs(&mut single, p, p)?;
        single.execute(Command::Character(patch()))?;
    }
    assert_eq!(saved(&mut single, &dir, "single.hwpx")?, formatted);
    assert_eq!(pages(&single)?, pages(&whole)?);

    let mut reopened = Session::new()?;
    reopened.open(&dir.path().join("whole.hwpx"))?;
    // Only the page count: RHWP's live body clip after an edit can differ from a
    // fresh open (908.8 vs 876.8 px here, the same before this change).
    assert_eq!(reopened.page_count()?, whole.page_count()?);

    whole.execute(Command::Undo)?;
    assert_eq!(saved(&mut whole, &dir, "undone.hwpx")?, original);
    assert_eq!(whole.page_count()?, before);
    Ok(())
}

#[test]
fn a_deferring_caller_keeps_its_deferral_and_gets_the_same_document() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut direct = document(&dir)?;
    select_paragraphs(&mut direct, 3, 30)?;
    direct.execute(Command::Character(patch()))?;
    let expected = saved(&mut direct, &dir, "direct.hwpx")?;

    let mut deferred = document(&dir)?;
    deferred.defer_layout()?;
    select_paragraphs(&mut deferred, 3, 30)?;
    deferred.execute(Command::Character(patch()))?;
    assert!(
        deferred.layout_deferred(),
        "formatting must not end the caller's deferral"
    );
    deferred.resume_layout()?;
    assert_eq!(saved(&mut deferred, &dir, "deferred.hwpx")?, expected);
    assert_eq!(pages(&deferred)?, pages(&direct)?);
    Ok(())
}

#[test]
fn a_failing_selection_format_leaves_the_document_unchanged() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = document(&dir)?;
    let original = saved(&mut s, &dir, "original.hwpx")?;
    let generation = s.generation;
    select_paragraphs(&mut s, 0, PARAGRAPHS - 1)?;
    // Fill the Korean font list so a new family cannot get an id.
    let font = s.core.document().doc_info.font_faces[0][0].clone();
    let faces = &mut s.core.document_mut().doc_info.font_faces[0];
    let count = faces.len();
    faces.resize(usize::from(u16::MAX) + 1, font);
    assert!(
        s.execute(Command::Character(json!({"fontFamily":"새 글꼴 없음"})))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    s.core.document_mut().doc_info.font_faces[0].truncate(count);
    assert_eq!(saved(&mut s, &dir, "after.hwpx")?, original);
    Ok(())
}

/// Pending caret formats, the first input after one and Enter at a paragraph end
/// edit the live document and lay out one paragraph; each used to copy the document
/// and re-lay all of it out (about 0.25-0.5 s each on the 381-page handbook).
fn caret_edits(s: &mut Session, paragraph: usize) -> anyhow::Result<()> {
    let len = paragraph_len(s, paragraph);
    s.select(at(paragraph, len), false)?;
    s.execute(Command::Character(json!({"bold":true})))?;
    s.execute(Command::Insert("가".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Character(json!({"fontSize":2000})))?;
    s.execute(Command::Insert("나".into()))?;
    s.select(at(paragraph, len / 2), false)?;
    s.execute(Command::Split)?;
    Ok(())
}

#[test]
fn caret_edits_match_the_reopened_file_and_undo_to_the_original() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = document(&dir)?;
    let original = saved(&mut s, &dir, "original.hwpx")?;
    caret_edits(&mut s, 5)?;
    saved(&mut s, &dir, "edited.hwpx")?;
    let mut reopened = Session::new()?;
    reopened.open(&dir.path().join("edited.hwpx"))?;
    assert_eq!(reopened.page_count()?, s.page_count()?);
    let new = s.core.document().sections[0].paragraphs.len();
    assert_eq!(new, PARAGRAPHS + 2);
    let doc = s.core.document();
    let typed = doc.sections[0]
        .paragraphs
        .iter()
        .find(|p| p.text.ends_with('가'))
        .and_then(|p| p.char_shape_id_at(p.text.chars().count() - 1));
    assert!(
        typed.is_some_and(|id| doc.doc_info.char_shapes[id as usize].bold),
        "the input after a pending bold is bold"
    );
    while s.can_undo() {
        s.execute(Command::Undo)?;
    }
    assert_eq!(saved(&mut s, &dir, "undone.hwpx")?, original);
    Ok(())
}

fn handbook() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/2025 행정업무운영 편람(최종).hwp")
}

#[test]
fn handbook_edits_keep_the_page_count_of_the_reopened_file() -> anyhow::Result<()> {
    let path = handbook();
    assert!(path.exists(), "RHWP handbook sample missing: {path:?}");
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.open(&path)?;
    let opened = s.page_count()?;
    // Bold does not move line breaks; the old staged copy reported 380 of 381 pages.
    select_paragraphs(&mut s, 0, 50)?;
    s.execute(Command::Character(json!({"bold":true})))?;
    assert_eq!(s.page_count()?, opened);
    // The old copies reported 381 pages where the saved file reopens with 382.
    caret_edits(&mut s, 30)?;
    saved(&mut s, &dir, "edited.hwpx")?;
    let mut reopened = Session::new()?;
    reopened.open(&dir.path().join("edited.hwpx"))?;
    assert_eq!(s.page_count()?, reopened.page_count()?);
    Ok(())
}
