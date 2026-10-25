//! Owner: core area. Markpen overlay geometry built from RHWP markpen marks.
use hwpcraft_engine::{Command, Session, markpen_ranges};
use serde_json::json;

#[test]
fn highlight_rects_follow_markpen_marks_and_disappear_when_cleared() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("형광펜 표시 검사 문장입니다".into()))?;
    assert!(s.highlight_rects().is_empty());
    s.anchor = Some(0);
    s.cursor.offset = 3;
    s.run("format.highlight", json!({"color": "#ffff00"}))?;
    let paragraph = s.paragraph(s.cursor)?;
    let ranges = markpen_ranges(paragraph);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].0, 0..3);
    assert!(ranges[0].1.eq_ignore_ascii_case("#ffff00"), "{ranges:?}");
    let rects = s.highlight_rects();
    assert_eq!(rects.len(), 1, "{rects:?}");
    assert_eq!(rects[0].page, 0);
    assert!(rects[0].color.eq_ignore_ascii_case("#ffff00"));
    assert!(rects[0].width > rects[0].height, "three glyphs on one line");
    // Clearing the highlight removes the overlay.
    s.anchor = Some(0);
    s.cursor.offset = 3;
    s.run("format.highlight", json!({}))?;
    assert!(s.highlight_rects().is_empty());
    s.execute(Command::Undo)?;
    assert_eq!(s.highlight_rects().len(), 1);
    Ok(())
}

#[test]
fn highlight_spanning_lines_yields_one_rect_per_line() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let long: String = "가나다라마바사아자차카타파하 ".repeat(12);
    s.execute(Command::Insert(long.clone()))?;
    s.anchor = Some(0);
    s.cursor.offset = long.chars().count();
    s.run("format.highlight", json!({"color": "#00ff00"}))?;
    let rects = s.highlight_rects();
    assert!(rects.len() >= 2, "a long run wraps: {rects:?}");
    let mut ys: Vec<i64> = rects.iter().map(|r| r.y.round() as i64).collect();
    ys.dedup();
    assert_eq!(ys.len(), rects.len(), "one rectangle per line");
    Ok(())
}

#[test]
fn whole_paragraph_highlight_keeps_end_position_after_leading_controls() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("😀한\t글".into()))?;
    s.run("format.highlight", json!({"color":"#ffff00"}))?;
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("whole.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(reopened.text(), "😀한\t글");
        let ranges = markpen_ranges(reopened.paragraph(reopened.cursor)?);
        assert_eq!(ranges.len(), 1, "{extension}");
        assert_eq!(ranges[0].0, 0..4, "{extension}");
    }
    Ok(())
}

#[test]
fn shared_output_highlights_survive_roundtrip_and_refresh_after_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("형광펜 출력\n다음 문단".into()))?;
    s.cursor.paragraph = 0;
    s.cursor.offset = 3;
    s.anchor = Some(0);
    s.run("format.highlight", json!({"color":"#ff00ff"}))?;
    let svg = s.render_page_svg(0)?;
    assert!(svg.contains("data-hwpcraft-markpen"));
    assert!(svg.to_ascii_lowercase().contains("fill=\"#ff00ff\""));
    assert!(s.render_pdf(&[0])?.starts_with(b"%PDF-"));
    assert!(s.render_pdf(&[]).is_err());
    assert!(s.render_pdf(&[s.core.page_count()]).is_err());
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("highlight.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let svg = reopened.render_page_svg(0)?;
        assert!(svg.contains("data-hwpcraft-markpen"), "{extension}");
        assert!(
            svg.to_ascii_lowercase().contains("fill=\"#ff00ff\""),
            "{extension}"
        );
    }
    s.execute(Command::Undo)?;
    assert!(!s.render_page_svg(0)?.contains("data-hwpcraft-markpen"));
    s.execute(Command::Redo)?;
    assert!(s.render_page_svg(0)?.contains("data-hwpcraft-markpen"));
    let pdf = dir.path().join("out.pdf");
    let path_before = s.path.clone();
    let dirty_before = s.dirty();
    s.save(&pdf)?;
    assert!(std::fs::read(pdf)?.starts_with(b"%PDF-"));
    assert_eq!(s.path, path_before);
    assert_eq!(s.dirty(), dirty_before);
    s.run("select.text", json!({"text":"형광펜"}))?;
    assert_eq!(
        markpen_ranges(s.paragraph(s.cursor)?).len(),
        1,
        "source before block"
    );
    let block = dir.path().join("block.pdf");
    s.run("file.save_block", json!({"path":block}))?;
    assert!(std::fs::read(block)?.starts_with(b"%PDF-"));
    assert_eq!(
        markpen_ranges(s.paragraph(s.cursor)?).len(),
        1,
        "source after block"
    );
    s.run("select.text", json!({"text":"광"}))?;
    for extension in ["hwp", "hwpx"] {
        let block = dir.path().join(format!("clipped-block.{extension}"));
        s.run("file.save_block", json!({"path":block}))?;
        let mut reopened = Session::new()?;
        reopened.open(&block)?;
        assert_eq!(reopened.text(), "광");
        let ranges = markpen_ranges(reopened.paragraph(reopened.cursor)?);
        assert_eq!(
            ranges.len(),
            1,
            "{extension}: marks={:?}, tags={:?}, offsets={:?}",
            reopened.paragraph(reopened.cursor)?.markpen_marks,
            reopened.paragraph(reopened.cursor)?.range_tags,
            reopened.paragraph(reopened.cursor)?.char_offsets
        );
        assert_eq!(ranges[0].0, 0..1);
        assert!(ranges[0].1.eq_ignore_ascii_case("#ff00ff"));
    }
    Ok(())
}

#[test]
fn highlight_only_in_a_table_cell_is_found() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.execute(Command::Table {
        rows: 1,
        columns: 2,
    })?;
    assert!(s.highlight_rects().is_empty());
    let cell = s
        .cursors()
        .into_iter()
        .find(|c| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("no cell"))?;
    s.select(cell, false)?;
    s.execute(Command::Insert("셀 형광펜".into()))?;
    s.anchor = Some(0);
    s.cursor.offset = 1;
    s.run("format.highlight", json!({"color": "#00ffff"}))?;
    let rects = s.highlight_rects();
    assert_eq!(rects.len(), 1, "{rects:?}");
    assert!(rects[0].color.eq_ignore_ascii_case("#00ffff"));
    Ok(())
}
