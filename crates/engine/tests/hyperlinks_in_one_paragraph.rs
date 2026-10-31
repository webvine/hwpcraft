//! Two links typed one after another in a paragraph both come back from an HWPX
//! save (RHWP patch 0048: text typed after a link that ends the paragraph goes
//! after the link's end slot).
use hwpcraft_engine::Session;
use serde_json::{Value, json};

fn links(s: &mut Session) -> anyhow::Result<Vec<(Value, Value, Value)>> {
    let list = s.run("hyperlink.list", json!({}))?;
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| (l["text"].clone(), l["uri"].clone(), l["start"].clone()))
        .collect())
}

#[test]
fn two_links_typed_in_a_row_survive_hwp_and_hwpx() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 "}))?;
    s.run(
        "insert.hyperlink",
        json!({"uri": "https://a.example", "text": "링크A"}),
    )?;
    s.run("text.insert", json!({"text": " 가운데 "}))?;
    s.run(
        "insert.hyperlink",
        json!({"uri": "https://b.example", "text": "링크B"}),
    )?;
    s.run("text.insert", json!({"text": " 끝"}))?;
    let live = links(&mut s)?;
    assert_eq!(live.len(), 2, "{live:?}");
    let dir = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("links.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(links(&mut r)?, live, "{ext}");
    }
    Ok(())
}
