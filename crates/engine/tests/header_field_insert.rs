//! Owner: page area. `page.header_footer.field.insert`: a field at a place in a header or
//! footer without its editing caret (what an MCP client has), the rest of it kept.
use hwpcraft_engine::{Command, Session};
use serde_json::json;

#[test]
fn a_field_goes_in_at_a_place_in_a_footer() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    // No footer yet: refused.
    let e = s
        .run(
            "page.header_footer.field.insert",
            json!({"kind": "page_number", "header": false}),
        )
        .unwrap_err();
    assert!(e.to_string().contains("page.footer"), "{e}");
    s.run(
        "page.header_footer.text",
        json!({"header": false, "text": "쪽 / ", "fields": [{"offset": 4, "kind": "total_pages"}]}),
    )?;
    let fields = |s: &mut Session| -> anyhow::Result<serde_json::Value> {
        Ok(s.run("page.header_footer.text.get", json!({"header": false}))?["fields"].clone())
    };
    assert_eq!(
        fields(&mut s)?,
        json!([{"offset": 4, "kind": "total_pages"}])
    );
    // The page number in front of "쪽", the total kept after "/ ".
    let r = s.run(
        "page.header_footer.field.insert",
        json!({"kind": "page_number", "header": false, "offset": 0}),
    )?;
    assert_eq!(r["paragraph"], 0);
    let after = r["offset"].as_u64().unwrap_or(0);
    assert!(after >= 1, "{r}");
    let listed = fields(&mut s)?;
    let kinds: Vec<&str> = listed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["page_number", "total_pages"], "{listed}");
    // Bad requests change nothing.
    let generation = s.generation;
    for params in [
        json!({"kind": "clock", "header": false}),
        json!({"kind": "page_number", "header": false, "offset": 999}),
        json!({"kind": "page_number", "header": false, "paragraph": 5}),
        json!({"kind": "date_saved", "header": false}),
    ] {
        assert!(
            s.run("page.header_footer.field.insert", params.clone())
                .is_err(),
            "{params}"
        );
        assert_eq!(s.generation, generation, "{params}");
    }
    // One undo takes the field out.
    s.execute(Command::Undo)?;
    assert_eq!(
        fields(&mut s)?,
        json!([{"offset": 4, "kind": "total_pages"}])
    );
    Ok(())
}

/// Editing the same footer on screen: a field put in before the caret moves the caret
/// along, so it stays after the same text.
#[test]
fn the_caret_of_the_edited_footer_keeps_its_place() -> anyhow::Result<()> {
    use hwpcraft_engine::{HfApply, HfTarget};
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.run(
        "page.header_footer.text",
        json!({"header": false, "text": "쪽 번호"}),
    )?;
    let t = HfTarget {
        section: 0,
        footer: true,
        apply_to: HfApply::Both,
    };
    s.hf_enter(t, 0)?;
    s.hf_select_all();
    s.hf_move(hwpcraft_engine::Motion::LineEnd, false)?;
    let before = s.hf_caret().map(|(c, _)| c.offset);
    assert_eq!(before, Some(4));
    let r = s.run(
        "page.header_footer.field.insert",
        json!({"kind": "page_number", "header": false, "offset": 0}),
    )?;
    let added = r["offset"].as_u64().unwrap_or(0) as usize;
    assert!(added >= 1, "{r}");
    assert_eq!(s.hf_caret().map(|(c, _)| c.offset), Some(4 + added));
    Ok(())
}
