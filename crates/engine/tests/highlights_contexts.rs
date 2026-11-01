//! Owner: format area. 형광펜 in a header, a footnote and a text box is painted like
//! body highlighting (a header's on every page). Before, only the body and
//! first-level table cells were measured, so highlighting read from a Hangul file
//! there was kept on save but never shown on screen, in PNG or in PDF.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::paragraph::{MarkpenMark, Paragraph};
use hwpcraft_engine::Session;
use serde_json::json;

fn mark(p: &mut Paragraph, color: &str) {
    let end = p.text.chars().count();
    p.markpen_marks = vec![
        MarkpenMark {
            char_idx: 0,
            color: Some(color.to_owned()),
            utf16_pos: None,
        },
        MarkpenMark {
            char_idx: end,
            color: None,
            utf16_pos: None,
        },
    ];
}

#[test]
fn highlighting_outside_the_body_is_painted() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리말 글"}),
    )?;
    s.run("text.insert", json!({"text": "첫 쪽"}))?;
    s.run("insert.footnote", json!({"text": "각주 글"}))?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 60.0, "height_mm": 20.0, "paragraphs": ["글상자 글"]}),
    )?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("insert.page_break", json!({}))?;
    s.run("text.insert", json!({"text": "둘째 쪽"}))?;
    assert_eq!(s.page_count()?, 2);
    assert!(s.highlight_rects().is_empty());

    for para in &mut s.core.document_mut().sections[0].paragraphs {
        for control in &mut para.controls {
            match control {
                Control::Header(h) => mark(&mut h.paragraphs[0], "#ffff00"),
                Control::Footnote(n) => mark(&mut n.paragraphs[0], "#00ff00"),
                Control::Shape(shape) => {
                    if let Some(text) = shape.drawing_mut().and_then(|d| d.text_box.as_mut()) {
                        mark(&mut text.paragraphs[0], "#00ffff");
                    }
                }
                _ => {}
            }
        }
    }
    let rects = s.highlight_rects();
    let pages = |color: &str| -> Vec<u32> {
        let mut pages: Vec<u32> = rects
            .iter()
            .filter(|r| r.color == color)
            .map(|r| r.page)
            .collect();
        pages.dedup();
        pages
    };
    assert_eq!(
        pages("#ffff00"),
        [0, 1],
        "the header on both pages: {rects:?}"
    );
    assert_eq!(pages("#00ff00"), [0], "the footnote: {rects:?}");
    assert_eq!(pages("#00ffff"), [0], "the text box: {rects:?}");
    for r in &rects {
        assert!(r.width > 5.0 && r.height > 5.0, "{r:?}");
    }
    // The header run sits in the header area, above the body.
    let header = rects.iter().find(|r| r.color == "#ffff00").unwrap();
    let footnote = rects.iter().find(|r| r.color == "#00ff00").unwrap();
    assert!(header.y < footnote.y, "{header:?} {footnote:?}");
    // And the SVG of page 2 carries the header highlight.
    let svg = s.render_page_svg(1)?;
    assert!(svg.contains("fill=\"#ffff00\""), "page 2 SVG");
    Ok(())
}
