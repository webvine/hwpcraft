//! Owner: format area. A 날짜 코드 in a header (made by `page.header_footer.text`
//! fields) is refreshed by `tools.date_field.update` like one in the body, and stays
//! a field after the rewrite and after saving.
use hwpcraft_doc::model::control::{Control, FieldType};
use hwpcraft_engine::{Cursor, Session};
use serde_json::json;

/// (header paragraph text, its date field's command and shown text).
fn header(s: &Session) -> (String, Option<(String, String)>) {
    let para = s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Header(h) => h.paragraphs.first(),
            _ => None,
        })
        .expect("a header");
    let field = para
        .field_ranges
        .iter()
        .find_map(|r| match para.controls.get(r.control_idx) {
            Some(Control::Field(f)) if f.field_type == FieldType::Summary => Some((
                f.command.clone(),
                para.text
                    .chars()
                    .skip(r.start_char_idx)
                    .take(r.end_char_idx - r.start_char_idx)
                    .collect(),
            )),
            _ => None,
        });
    (para.text.clone(), field)
}

/// The text a body 마지막 저장 날짜 field shows now.
fn body_saved(s: &mut Session) -> anyhow::Result<String> {
    // At the end of a paragraph without the header control.
    let paragraphs = &s.core.document().sections[0].paragraphs;
    let p = paragraphs
        .iter()
        .position(|p| !p.controls.iter().any(|c| matches!(c, Control::Header(_))))
        .expect("a paragraph without the header");
    s.cursor = Cursor {
        paragraph: p,
        offset: paragraphs[p].text.chars().count(),
        ..Default::default()
    };
    let made = s.run("insert.date_field", json!({"kind": "modified"}))?;
    let text = made["text"].as_str().unwrap_or_default().to_owned();
    s.run("edit.undo", json!({}))?;
    Ok(text)
}

#[test]
fn a_header_date_field_follows_the_save_time() -> anyhow::Result<()> {
    hwpcraft_engine::fixed_clock(Some(1_790_000_000));
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("dated.hwpx");
    let mut s = Session::new()?;
    s.run(
        "text.insert",
        json!({"text": "본문
둘째 문단"}),
    )?;
    s.save(&path)?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "text": "저장 ", "fields": [{"offset": 3, "kind": "date_saved"}]}),
    )?;
    let first = body_saved(&mut s)?;
    let (text, field) = header(&s);
    assert_eq!(text, format!("저장 {first}"));
    assert_eq!(field, Some(("$modifiedtime".to_owned(), first.clone())));

    // Saved again a day later: the refresh rewrites the header field like a body one.
    hwpcraft_engine::fixed_clock(Some(1_790_086_400));
    s.save(&path)?;
    let second = body_saved(&mut s)?;
    assert_ne!(first, second);
    let refreshed = s.run("tools.date_field.update", json!({}))?;
    hwpcraft_engine::fixed_clock(None);
    assert_eq!(refreshed["fields"], 1, "{refreshed}");
    assert_eq!(refreshed["updated"], 1, "{refreshed}");
    let (text, field) = header(&s);
    assert_eq!(text, format!("저장 {second}"));
    assert_eq!(field, Some(("$modifiedtime".to_owned(), second.clone())));
    // One undo takes the refresh back; redo brings it again.
    s.run("edit.undo", json!({}))?;
    assert_eq!(header(&s).1, Some(("$modifiedtime".to_owned(), first)));
    s.run("edit.redo", json!({}))?;
    assert_eq!(
        header(&s).1,
        Some(("$modifiedtime".to_owned(), second.clone()))
    );
    let generation = s.generation;
    let again = s.run("tools.date_field.update", json!({}))?;
    assert_eq!(again["updated"], 0, "{again}");
    assert_eq!(s.generation, generation, "nothing to rewrite, no undo step");

    for ext in ["hwpx", "hwp"] {
        let out = dir.path().join(format!("back.{ext}"));
        s.save(&out)?;
        let mut back = Session::new()?;
        back.open(&out)?;
        assert_eq!(
            header(&back).1.map(|(c, _)| c).as_deref(),
            Some("$modifiedtime"),
            "{ext}"
        );
    }
    Ok(())
}
