use hwpcraft_doc::{HyperlinkTarget, model::control::Control};
use hwpcraft_engine::{Command, Cursor, Session, TableEdit};
use serde_json::json;

fn select_cell(s: &mut Session) -> anyhow::Result<()> {
    let c = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .ok_or_else(|| anyhow::anyhow!("no cell"))?
        .0;
    s.select(c, false)
}
#[test]
fn header_footer_and_page_number_roundtrip_preserve_existing_content() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.set_header_footer(true, "머리말 🦀")?;
    s.set_header_footer(true, "수정 머리말")?;
    s.execute(Command::Undo)?;
    assert_eq!(s.header_footer_text(true)?, "머리말 🦀");
    s.insert_page_number()?;
    let generation = s.generation;
    assert!(s.insert_page_number().is_err());
    assert!(s.set_header_footer(false, "덮어쓰기").is_err());
    assert_eq!(s.generation, generation);
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("header.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(r.header_footer_text(true)?, "머리말 🦀");
        let footer: serde_json::Value =
            serde_json::from_str(&r.core.get_header_footer_native(0, false, 0).unwrap())?;
        assert!(r.core.document().sections[0].paragraphs.iter().flat_map(|p|&p.controls).any(|c|matches!(c,Control::Footer(f) if f.paragraphs.iter().any(|p|p.text.contains('\u{0015}')||p.controls.iter().any(|c|matches!(c,Control::AutoNumber(_)))))), "{ext}: {footer}");
    }
    Ok(())
}
#[test]
fn table_edits_keep_cursor_valid_and_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Table {
        rows: 2,
        columns: 2,
    })?;
    select_cell(&mut s)?;
    s.execute(Command::Insert("셀 내용".into()))?;
    for action in [
        TableEdit::RowAbove,
        TableEdit::RowBelow,
        TableEdit::ColumnLeft,
        TableEdit::ColumnRight,
    ] {
        let before = s.table_info()?;
        let cursor = s.cursor;
        s.edit_table(action)?;
        assert!(s.paragraph(s.cursor).is_ok());
        s.execute(Command::Undo)?;
        assert_eq!(s.table_info()?, before);
        assert_eq!(s.cursor, cursor);
        s.execute(Command::Redo)?;
        assert!(s.paragraph(s.cursor).is_ok());
    }
    s.edit_table(TableEdit::DeleteRow)?;
    s.edit_table(TableEdit::DeleteColumn)?;
    assert_eq!(s.table_info()?["rows"], 3);
    assert_eq!(s.table_info()?["columns"], 3);
    s.move_cell(false)?;
    assert!(s.paragraph(s.cursor).is_ok());
    s.move_cell(true)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("table.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        select_cell(&mut r)?;
        assert_eq!(r.table_info()?["rows"], 3);
        assert_eq!(r.table_info()?["columns"], 3);
    }
    Ok(())
}
#[test]
fn table_rejections_preserve_document_and_history() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    assert!(s.edit_table(TableEdit::RowBelow).is_err());
    assert!(!s.dirty());
    s.execute(Command::Table {
        rows: 1,
        columns: 1,
    })?;
    select_cell(&mut s)?;
    let generation = s.generation;
    let cursor = s.cursor;
    assert!(s.edit_table(TableEdit::DeleteRow).is_err());
    assert!(s.edit_table(TableEdit::DeleteColumn).is_err());
    assert_eq!(s.generation, generation);
    assert_eq!(s.cursor, cursor);
    assert!(s.run("table.edit", json!({"action":"unknown"})).is_err());
    Ok(())
}
#[test]
fn merge_split_preserve_cell_text_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Table {
        rows: 1,
        columns: 2,
    })?;
    select_cell(&mut s)?;
    s.execute(Command::Insert("왼쪽".into()))?;
    s.move_cell(false)?;
    s.execute(Command::Insert("오른쪽".into()))?;
    s.move_cell(true)?;
    s.edit_table(TableEdit::MergeRight)?;
    assert_eq!(
        s.entries()
            .iter()
            .filter(|(c, _)| c.cell.is_some_and(|v| v.paragraph == 0))
            .count(),
        1
    );
    assert!(s.entries().iter().any(|(_, t)| t.contains("오른쪽")));
    s.edit_table(TableEdit::SplitMerged)?;
    assert_eq!(
        s.entries()
            .iter()
            .filter(|(c, _)| c.cell.is_some_and(|v| v.paragraph == 0))
            .count(),
        2
    );
    s.execute(Command::Undo)?;
    assert_eq!(
        s.entries()
            .iter()
            .filter(|(c, _)| c.cell.is_some_and(|v| v.paragraph == 0))
            .count(),
        1
    );
    s.execute(Command::Undo)?;
    assert_eq!(
        s.entries()
            .iter()
            .filter(|(c, _)| c.cell.is_some_and(|v| v.paragraph == 0))
            .count(),
        2
    );
    Ok(())
}
#[test]
fn bookmarks_roundtrip_duplicate_delete_undo_and_navigation() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다 🦀\n두 번째 문단".into()))?;
    s.select(
        Cursor {
            offset: 3,
            ..Cursor::default()
        },
        false,
    )?;
    s.add_bookmark("중간")?;
    let generation = s.generation;
    assert!(s.add_bookmark("중간").is_err());
    assert_eq!(s.generation, generation);
    assert!(s.add_bookmark("bad\nname").is_err());
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("bookmark.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        r.goto_bookmark("중간")?;
        assert_eq!(r.cursor.offset, 3);
        r.delete_bookmark("중간")?;
        assert_eq!(r.bookmarks()?.as_array().unwrap().len(), 0);
        r.execute(Command::Undo)?;
        assert_eq!(r.bookmarks()?.as_array().unwrap().len(), 1);
    }
    Ok(())
}
#[test]
fn hyperlink_selection_insertion_cell_and_failed_uri_are_atomic() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한글 🦀 링크".into()))?;
    s.anchor = Some(3);
    s.cursor.offset = 4;
    s.insert_hyperlink("https://example.com/한글", None)?;
    let target = HyperlinkTarget::body(0, 0);
    assert_eq!(s.core.hyperlinks_native(&target).unwrap()[0].text, "🦀");
    s.execute(Command::Undo)?;
    assert!(s.core.hyperlinks_native(&target).unwrap().is_empty());
    s.anchor = None;
    s.cursor.offset = s.len();
    let before = s.text().to_owned();
    let generation = s.generation;
    assert!(
        s.insert_hyperlink("file:///tmp/x", Some("금지된 주소"))
            .is_err()
    );
    assert_eq!(s.text(), before);
    assert_eq!(s.generation, generation);
    s.insert_hyperlink("https://example.com", Some(" 웹"))?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("link.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let links = r.core.hyperlinks_native(&target).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://example.com");
    }
    s.execute(Command::Table {
        rows: 1,
        columns: 1,
    })?;
    select_cell(&mut s)?;
    s.insert_hyperlink("https://example.org", Some("셀 링크"))?;
    assert_eq!(s.text(), "셀 링크");
    Ok(())
}
#[test]
fn columns_and_column_break_roundtrip_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 단".into()))?;
    s.set_columns(2, 8.0)?;
    let before = s.generation;
    assert!(s.set_columns(6, 50.0).is_err());
    assert!(s.set_columns(0, 0.0).is_err());
    assert!(s.set_columns(2, f64::NAN).is_err());
    assert_eq!(s.generation, before);
    s.column_break()?;
    s.execute(Command::Insert("둘째 단".into()))?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("columns.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert!(
            r.core.document().sections[0]
                .paragraphs
                .iter()
                .flat_map(|p| &p.controls)
                .any(|c| matches!(c,Control::ColumnDef(d) if d.column_count==2))
        );
        assert_eq!(r.core.document().sections[0].paragraphs.len(), 2);
    }
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(s.core.document().sections[0].paragraphs.len(), 1);
    Ok(())
}
#[test]
fn png_picture_validates_bytes_and_roundtrips() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("picture.png");
    let img = image::RgbaImage::from_pixel(32, 24, image::Rgba([30, 120, 210, 255]));
    img.save(&path)?;
    let mut s = Session::new()?;
    s.insert_png(&path)?;
    assert!(
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .any(|c| matches!(c, Control::Picture(_)))
    );
    for ext in ["hwp", "hwpx"] {
        let dest = temp.path().join(format!("picture.{ext}"));
        s.save(&dest)?;
        let mut r = Session::new()?;
        r.open(&dest)?;
        assert!(
            r.core.document().sections[0]
                .paragraphs
                .iter()
                .flat_map(|p| &p.controls)
                .any(|c| matches!(c, Control::Picture(_)))
        );
    }
    s.execute(Command::Undo)?;
    assert!(
        !s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .any(|c| matches!(c, Control::Picture(_)))
    );
    std::fs::write(&path, b"not a png")?;
    let generation = s.generation;
    assert!(s.insert_png(&path).is_err());
    assert_eq!(s.generation, generation);
    Ok(())
}
