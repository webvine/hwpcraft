//! Owner: format area. Tabs typed in a header, a footnote or a text box are saved with
//! their laid-out width like body tabs. Before, only the body and first-level table
//! cells were measured: other tabs were written as zero-width markers and Hangul drew
//! them with no width.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_engine::Session;
use serde_json::json;

/// (where, text, tab widths in HWPUNIT) of every tabbed paragraph outside the body.
fn tabbed(s: &Session) -> Vec<(&'static str, String, Vec<u32>)> {
    fn widths(p: &Paragraph) -> Vec<u32> {
        let tabs = p.text.chars().filter(|c| *c == '\t').count();
        (0..tabs)
            .map(|n| {
                p.tab_extended
                    .get(n)
                    .map_or(0, |r| u32::from(r[0]) | (u32::from(r[1]) << 16))
            })
            .collect()
    }
    let mut out = Vec::new();
    for para in &s.core.document().sections[0].paragraphs {
        for control in &para.controls {
            let (kind, paragraphs): (&str, &[Paragraph]) = match control {
                Control::Header(h) => ("header", &h.paragraphs),
                Control::Footnote(n) => ("footnote", &n.paragraphs),
                Control::Shape(shape) => match shape.drawing().and_then(|d| d.text_box.as_ref()) {
                    Some(text) => ("textbox", &text.paragraphs),
                    None => continue,
                },
                _ => continue,
            };
            for p in paragraphs.iter().filter(|p| p.text.contains('\t')) {
                out.push((kind, p.text.clone(), widths(p)));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn tabs_outside_the_body_are_saved_with_their_width() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리말"}),
    )?;
    // A tab typed into the header (the header command takes one line of plain text).
    let list = s.run("context.v2.list", json!({"include": ["header"]}))?;
    let header = &list["items"][0];
    s.run(
        "context.v2.replace",
        json!({"address": header["address"], "start": 2, "end": 2, "text": "\t",
            "expected_generation": list["generation"]}),
    )?;
    s.run("text.insert", json!({"text": "본문"}))?;
    s.run("insert.footnote", json!({"text": "각\t주"}))?;
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run(
        "shape.insert",
        json!({"kind": "textbox", "width_mm": 80.0, "height_mm": 20.0, "paragraphs": ["글\t상자"]}),
    )?;
    let live = tabbed(&s);
    assert_eq!(live.len(), 3, "{live:?}");
    let dir = tempfile::tempdir()?;
    for name in ["contexts.hwp", "contexts.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        // The live model keeps its unsized tabs so later edits re-flow them.
        assert_eq!(tabbed(&s), live, "{name}");
        let mut back = Session::new()?;
        back.open(&path)?;
        let saved = tabbed(&back);
        assert_eq!(saved.len(), 3, "{name}: {saved:?}");
        for (kind, text, widths) in &saved {
            assert_eq!(widths.len(), 1, "{name} {kind} {text:?}");
            // A default 40 pt tab grid gives a tab of a few millimetres up to 40 pt.
            assert!(
                widths[0] > 100 && widths[0] <= 4000,
                "{name} {kind} {text:?}: width {widths:?}"
            );
        }
    }
    Ok(())
}
