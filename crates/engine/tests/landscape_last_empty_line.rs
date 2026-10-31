//! A new document turned landscape keeps the same pages after an HWPX save and
//! reopen. HwpCraft writes a new document's HWPX without line rows, so the reopen
//! lays it out fresh; the last empty paragraph, which fits the body, must not open a
//! blank page there (RHWP patch 0044).
use hwpcraft_engine::{Command, Session};
use serde_json::json;

#[test]
fn a_landscape_document_ending_in_an_empty_line_reopens_with_the_same_pages() -> anyhow::Result<()>
{
    let mut s = Session::new()?;
    s.run("page.setup", json!({"landscape": true}))?;
    // A landscape A4 body holds 25 one-row paragraphs; the empty 26th row on the
    // second page ends inside the body, just within the layout safety margin.
    for i in 0..50 {
        s.execute(Command::Insert(format!("줄 {i}")))?;
        s.execute(Command::Split)?;
    }
    assert_eq!(s.core.page_count(), 2);
    let dir = tempfile::tempdir()?;
    for name in ["landscape.hwpx", "landscape.hwp"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(reopened.core.page_count(), 2, "{name}");
    }
    Ok(())
}
