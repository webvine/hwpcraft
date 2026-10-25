use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

#[test]
fn a_highlighted_continuation_scalar_uses_its_visible_page_independently_of_the_caret()
-> anyhow::Result<()> {
    for with_field in [false, true] {
        let mut session = Session::new()?;
        session.run(
            "page.setup",
            json!({"width_mm":80,"height_mm":80,
            "left_mm":5,"right_mm":5,"top_mm":5,"bottom_mm":5,"header_mm":5,"footer_mm":5}),
        )?;
        session.execute(Command::Insert("가나다라마".repeat(80)))?;
        if with_field {
            session.run("insert.auto_text", json!({"kind":"page_number"}))?;
        }
        let boundary = session.page_first_position(1).expect("continuation page");
        assert!(boundary.offset > 0);
        session.run("edit.goto", json!({"page":2}))?;
        let visible = session.caret()?;
        session.select(
            Cursor {
                offset: boundary.offset + 1,
                ..boundary
            },
            false,
        )?;
        session.anchor = Some(boundary.offset);
        session.run("format.highlight", json!({"color":"#ffff00"}))?;
        let source = format!("{:?}", session.core.document());
        let generation = session.generation;

        session.select(Cursor::default(), false)?;
        let rects = session.highlight_rects();
        assert_eq!(rects.len(), 1);
        let rect = &rects[0];
        assert_eq!(rect.page, 1, "field={with_field}: {rect:?}");
        assert!((rect.x - visible["x"].as_f64().unwrap()).abs() < 0.01);
        assert!((rect.y - visible["y"].as_f64().unwrap()).abs() < 0.01);
        assert!(rect.width > 10.0);
        let first_page = session.render_page_svg(0)?;
        let continuation = session.render_page_svg(1)?;
        assert!(!first_page.contains("data-hwpcraft-markpen"));
        assert!(continuation.contains("data-hwpcraft-markpen"));
        let pdf = session.render_pdf(&[0, 1])?;
        assert!(pdf.starts_with(b"%PDF-"));

        // Moving a caret/affinity has no document generation or paint effect.
        session.run("edit.goto", json!({"page":2}))?;
        assert_eq!(session.highlight_rects(), rects);
        assert_eq!(session.render_page_svg(0)?, first_page);
        assert_eq!(session.render_page_svg(1)?, continuation);
        assert_eq!(session.render_pdf(&[0, 1])?, pdf);
        session.select(
            Cursor {
                offset: boundary.offset + 50,
                ..boundary
            },
            false,
        )?;
        assert_eq!(session.highlight_rects(), rects);
        assert_eq!(session.render_page_svg(1)?, continuation);
        assert_eq!(session.render_pdf(&[0, 1])?, pdf);
        assert_eq!(session.generation, generation);
        assert_eq!(format!("{:?}", session.core.document()), source);
    }
    Ok(())
}

#[test]
fn highlighted_page_field_reserves_the_whole_measured_label_and_keeps_native_controls()
-> anyhow::Result<()> {
    let mut session = Session::new()?;
    session.execute(Command::Insert("앞 ".into()))?;
    session.run("insert.auto_text", json!({"kind":"page_number"}))?;
    session.execute(Command::Insert(" 뒤".into()))?;
    session.run("page.new_number", json!({"start":12345}))?;
    session.select(
        Cursor {
            offset: 3,
            ..Default::default()
        },
        false,
    )?;
    session.anchor = Some(2);
    session.run("format.highlight", json!({"color":"#00ff00"}))?;
    let from = session.caret_at(Cursor {
        offset: 2,
        ..Default::default()
    })?;
    let to = session.caret_at(Cursor {
        offset: 3,
        ..Default::default()
    })?;
    let expected = to["x"].as_f64().unwrap() - from["x"].as_f64().unwrap();
    let rects = session.highlight_rects();
    assert_eq!(rects.len(), 1);
    assert!(
        (rects[0].width - expected).abs() < 0.1,
        "{rects:?}; width={expected}"
    );
    assert!(rects[0].width > 30.0);
    let text = session.core.document().sections[0].paragraphs[0]
        .text
        .clone();
    assert_eq!(text, "앞   뒤");
    assert!(session.render_page_svg(0)?.contains("data-value=\"12345\""));
    let directory = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = directory
            .path()
            .join(format!("field-highlight.{extension}"));
        session.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(
            reopened.core.document().sections[0].paragraphs[0].text,
            text
        );
        let rects = reopened.highlight_rects();
        assert_eq!(rects.len(), 1, "{extension}");
        assert!(rects[0].width > 30.0, "{extension}: {rects:?}");
    }
    Ok(())
}
