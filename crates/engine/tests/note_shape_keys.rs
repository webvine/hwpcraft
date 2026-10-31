//! `note.shape.get` reads the footnote spacing keys from the slots `note.shape`
//! writes, so a shape read and written back stays the same.
use hwpcraft_engine::Session;
use serde_json::json;

#[test]
fn footnote_spacing_reads_back_what_was_set() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for kind in ["footnote", "endnote"] {
        s.run(
            "note.shape",
            json!({"kind": kind, "props": {
                "separatorMarginTop": 300, "separatorMarginBottom": 500, "noteSpacing": 200
            }}),
        )?;
        let got = s.run("note.shape.get", json!({"kind": kind}))?;
        assert_eq!(
            (
                &got["separatorMarginTop"],
                &got["separatorMarginBottom"],
                &got["noteSpacing"]
            ),
            (&json!(300), &json!(500), &json!(200)),
            "{kind}: {got}"
        );
    }
    Ok(())
}
