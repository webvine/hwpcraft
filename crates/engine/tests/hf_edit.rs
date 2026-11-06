//! 머리말·꼬리말 편집 영역: each page's header and footer areas, the header or footer
//! a point or a page shows, and entering and leaving one.
use hwpcraft_engine::{Command, Cursor, HfApply, HfPoint, HfTarget, Motion, Session};
use serde_json::{Value, json};

/// Two pages with a centred page-number header and footer on both.
fn two_pages() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("둘째 쪽".into()))?;
    s.run("page.header", json!({"apply_to": "both", "template": 2}))?;
    s.run("page.footer", json!({"apply_to": "both", "template": 2}))?;
    assert_eq!(s.page_count()?, 2);
    Ok(s)
}

fn centre(rect: &Value) -> (f64, f64) {
    let r: Vec<f64> = rect
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_f64)
        .collect();
    (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
}

fn both(section: usize, footer: bool) -> HfTarget {
    HfTarget {
        section,
        footer,
        apply_to: HfApply::Both,
    }
}

#[test]
fn page_areas_name_the_header_and_footer_they_show() -> anyhow::Result<()> {
    let s = two_pages()?;
    for page in 0..2 {
        let info = s.hf_page_info(page)?;
        for (key, footer) in [("header", false), ("footer", true)] {
            let side = &info[key];
            assert_eq!(side["exists"], true, "{info}");
            assert_eq!(side["hidden"], false, "{info}");
            assert_eq!(
                side["target"],
                json!({"section": 0, "footer": footer, "apply_to": "both"})
            );
            let (x, y) = centre(&side["rect"]);
            let hit = s.hf_hit(page, x, y)?.expect("the area is hit");
            assert_eq!(hit.target, both(0, footer));
            assert!(hit.exists && !hit.hidden);
        }
        let header = centre(&info["header"]["rect"]);
        let footer = centre(&info["footer"]["rect"]);
        assert!(header.1 < footer.1, "{info}");
        // The body between them is neither.
        assert!(
            s.hf_hit(page, header.0, (header.1 + footer.1) / 2.0)?
                .is_none()
        );
    }
    Ok(())
}

/// Entering and leaving are not edits: `generation` stays and `hf_revision` moves.
/// The caret starts at the start of the header (footer), inside its area.
#[test]
fn entering_and_leaving_a_header_or_footer() -> anyhow::Result<()> {
    let mut s = two_pages()?;
    for footer in [false, true] {
        let generation = s.generation;
        let revision = s.hf_revision();
        s.hf_enter(both(0, footer), 1)?;
        assert_eq!(s.hf_target(), Some(both(0, footer)));
        assert_eq!(s.hf_page(), Some(1));
        assert_eq!(s.generation, generation);
        assert!(s.hf_revision() > revision);
        let caret = s.hf_caret_rect()?;
        assert_eq!(caret["pageIndex"], 1, "{caret}");
        let area = &s.hf_page_info(1)?[if footer { "footer" } else { "header" }]["rect"];
        let (top, height) = (area[1].as_f64().unwrap(), area[3].as_f64().unwrap());
        let y = caret["y"].as_f64().unwrap();
        assert!(
            y >= top - 1.0 && y <= top + height + 1.0,
            "{caret} in {area}"
        );
        let revision = s.hf_revision();
        s.hf_exit();
        assert_eq!(s.hf_target(), None);
        assert!(s.hf_revision() > revision);
        assert_eq!(s.generation, generation);
    }
    Ok(())
}

/// Clicking at the right end of the centred page number puts the caret after it, a
/// shift-click keeps the anchor for a selection, and the selection has line boxes.
#[test]
fn clicks_place_the_caret_in_the_entered_footer() -> anyhow::Result<()> {
    let mut s = two_pages()?;
    s.hf_enter(both(0, true), 0)?;
    let area = s.hf_page_info(0)?["footer"]["rect"].clone();
    let (x0, y) = centre(&area);
    let right = area[0].as_f64().unwrap() + area[2].as_f64().unwrap() - 2.0;
    assert!(s.hf_click(0, right, y, false)?);
    let (end, anchor) = s.hf_caret().unwrap();
    assert!(end.offset > 0, "{end:?}");
    assert_eq!(anchor, None);
    assert!(s.hf_click(0, area[0].as_f64().unwrap() + 1.0, y, true)?);
    let (start, anchor) = s.hf_caret().unwrap();
    assert_eq!(start.offset, 0);
    assert_eq!(anchor, Some(end));
    let rects = s.hf_selection_rects(0)?;
    assert!(rects.as_array().is_some_and(|r| !r.is_empty()), "{rects}");
    // A point in the body is not in the footer.
    assert!(!s.hf_click(0, x0, 300.0, false)?);
    Ok(())
}

/// Entering an area with no footer makes one (one undo step); undoing it removes it
/// and `revalidate_hf` leaves it.
#[test]
fn entering_an_empty_footer_area_makes_the_footer() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let area = s.hf_page_info(0)?["footer"].clone();
    assert_eq!(area["exists"], false, "{area}");
    let (x, y) = centre(&area["rect"]);
    let hit = s.hf_hit(0, x, y)?.expect("the empty footer area");
    assert!(!hit.exists);
    assert_eq!(hit.target, both(0, true));
    let generation = s.generation;
    s.hf_enter_at(hit.target, 0, x, y)?;
    assert_eq!(s.generation, generation + 1);
    assert_eq!(s.hf_page_info(0)?["footer"]["exists"], true);
    s.execute(Command::Undo)?;
    assert_eq!(s.hf_page_info(0)?["footer"]["exists"], false);
    s.revalidate_hf();
    assert_eq!(s.hf_target(), None);
    Ok(())
}

/// With odd and even headers, each page names the one it shows; a page that hides the
/// header (쪽 감추기) says so.
#[test]
fn odd_even_and_hidden_headers_are_told_apart() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("둘째 쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("셋째 쪽".into()))?;
    s.run("page.header", json!({"apply_to": "odd", "template": 1}))?;
    s.run("page.header", json!({"apply_to": "even", "template": 3}))?;
    let shown: Vec<Value> = (0..3)
        .map(|p| {
            s.hf_page_info(p)
                .map(|i| i["header"]["target"]["apply_to"].clone())
        })
        .collect::<anyhow::Result<_>>()?;
    assert_eq!(shown, [json!("odd"), json!("even"), json!("odd")]);
    // Hide the header on the second page.
    let paragraph = s.core.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.text.contains("둘째"))
        .unwrap();
    s.cursor = Cursor {
        section: 0,
        paragraph,
        offset: 0,
        cell: None,
    };
    s.run(
        "page.hide",
        json!({"header": true, "footer": false, "master_page": false, "border": false,
            "fill": false, "page_number": false}),
    )?;
    let hidden: Vec<Value> = (0..3)
        .map(|p| s.hf_page_info(p).map(|i| i["header"]["hidden"].clone()))
        .collect::<anyhow::Result<_>>()?;
    assert_eq!(hidden, [json!(false), json!(true), json!(false)]);
    let info = s.hf_page_info(1)?;
    let (x, y) = centre(&info["header"]["rect"]);
    let hit = s
        .hf_hit(1, x, y)?
        .expect("the hidden header area is still an area");
    assert!(hit.hidden && hit.exists);
    Ok(())
}

/// Texts of the paragraphs of section 0's both-pages header or footer.
fn hf_texts(s: &Session, footer: bool) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            hwpcraft_doc::model::control::Control::Header(h) if !footer => {
                Some(h.paragraphs.iter().map(|p| p.text.clone()).collect())
            }
            hwpcraft_doc::model::control::Control::Footer(f) if footer => {
                Some(f.paragraphs.iter().map(|p| p.text.clone()).collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Typing, Enter, Backspace and Delete edit the entered header or footer, each one
/// undo step, and the body stays as it was.
fn typing_edits_the_entered_area(footer: bool) -> anyhow::Result<()> {
    let mut s = two_pages()?;
    let body = s.run("document.text", json!({}))?;
    let before = hf_texts(&s, footer);
    s.hf_enter(both(0, footer), 0)?;
    s.hf_insert("보고서 ")?;
    assert_eq!(hf_texts(&s, footer)[0], format!("보고서 {}", before[0]));
    assert_eq!(s.hf_caret().unwrap().0.offset, 4);
    // Enter splits; Backspace at the start of the new paragraph joins them again.
    s.hf_enter_key()?;
    assert_eq!(hf_texts(&s, footer).len(), 2);
    assert_eq!(s.hf_caret().unwrap().0.paragraph, 1);
    s.hf_backspace()?;
    assert_eq!(hf_texts(&s, footer).len(), 1);
    assert_eq!(s.hf_caret().unwrap().0.offset, 4);
    // Backspace deletes the space; Delete the character after the caret.
    s.hf_backspace()?;
    assert_eq!(hf_texts(&s, footer)[0], format!("보고서{}", before[0]));
    s.hf_move(Motion::ParagraphStart, false)?;
    s.hf_delete()?;
    assert_eq!(hf_texts(&s, footer)[0], format!("고서{}", before[0]));
    // Each edit is one undo step: five back to the first state.
    for _ in 0..5 {
        s.execute(Command::Undo)?;
    }
    s.revalidate_hf();
    assert_eq!(hf_texts(&s, footer), before);
    assert_eq!(s.hf_target(), Some(both(0, footer)));
    assert_eq!(s.run("document.text", json!({}))?, body);
    Ok(())
}

#[test]
fn typing_edits_the_entered_header() -> anyhow::Result<()> {
    typing_edits_the_entered_area(false)
}

#[test]
fn typing_edits_the_entered_footer() -> anyhow::Result<()> {
    typing_edits_the_entered_area(true)
}

/// Shift+arrows select; character format goes to the selection only.
#[test]
fn a_selection_in_the_header_takes_character_format() -> anyhow::Result<()> {
    let mut s = two_pages()?;
    s.hf_enter(both(0, false), 0)?;
    s.hf_insert("굵게 보통")?;
    s.hf_move(Motion::ParagraphStart, false)?;
    s.hf_move(Motion::Right, true)?;
    s.hf_move(Motion::Right, true)?;
    assert_eq!(s.hf_selected_text()?, "굵게");
    s.hf_character(json!({"bold": true}))?;
    let header = s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            hwpcraft_doc::model::control::Control::Header(h) => Some(h.paragraphs[0].clone()),
            _ => None,
        })
        .unwrap();
    let (bold, plain) = (
        header.char_shape_id_at(0).unwrap(),
        header.char_shape_id_at(3).unwrap(),
    );
    assert_ne!(bold, plain);
    let shapes = &s.core.document().doc_info.char_shapes;
    assert!(shapes[bold as usize].bold && !shapes[plain as usize].bold);
    // The selection is kept after formatting.
    assert_eq!(s.hf_selected_text()?, "굵게");
    Ok(())
}

/// A page-number field typed into the footer shows each page's own number.
#[test]
fn a_page_number_field_goes_in_at_the_footer_caret() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("둘째 쪽".into()))?;
    let info = s.hf_page_info(1)?["footer"].clone();
    let (x, y) = centre(&info["rect"]);
    let hit = s.hf_hit(1, x, y)?.unwrap();
    s.hf_enter_at(hit.target, 1, x, y)?;
    s.hf_insert("- ")?;
    s.hf_insert_field("page_number")?;
    s.hf_insert(" -")?;
    let got = s.run("page.header_footer.text.get", json!({"header": false}))?;
    assert_eq!(got["text"], "-  -", "{got}");
    assert_eq!(got["fields"], json!([{"offset": 2, "kind": "page_number"}]));
    for (page, number) in [(0, "1"), (1, "2")] {
        let svg = s.render_page_svg(page)?;
        let texts: String = svg
            .split("<text")
            .skip(1)
            .filter_map(|t| t.split_once('>').map(|(_, r)| r))
            .filter_map(|r| r.split_once("</text>").map(|(t, _)| t))
            .collect();
        assert!(
            texts.contains(&format!("-{number}-")),
            "page {page}: {texts}"
        );
    }
    Ok(())
}

/// Line start / end and up / down follow the laid-out lines of the header.
#[test]
fn arrows_follow_the_header_lines() -> anyhow::Result<()> {
    let mut s = two_pages()?;
    s.hf_enter(both(0, false), 0)?;
    s.hf_insert("첫째 줄")?;
    s.hf_enter_key()?;
    s.hf_insert("둘째 줄입니다")?;
    s.hf_move(Motion::LineStart, false)?;
    assert_eq!(
        s.hf_caret().unwrap().0,
        HfPoint {
            paragraph: 1,
            offset: 0
        }
    );
    s.hf_move(Motion::LineEnd, false)?;
    // The page number the split carried over stays at the end of the second line.
    let second = hf_texts(&s, false)[1].chars().count();
    assert_eq!(s.hf_caret().unwrap().0.offset, second);
    s.hf_move(Motion::Up, false)?;
    assert_eq!(s.hf_caret().unwrap().0.paragraph, 0);
    s.hf_move(Motion::Up, false)?;
    assert_eq!(
        s.hf_caret().unwrap().0.paragraph,
        0,
        "stays on the first line"
    );
    s.hf_move(Motion::Down, true)?;
    let (caret, anchor) = s.hf_caret().unwrap();
    assert_eq!(caret.paragraph, 1);
    assert_eq!(anchor.map(|a| a.paragraph), Some(0));
    Ok(())
}
