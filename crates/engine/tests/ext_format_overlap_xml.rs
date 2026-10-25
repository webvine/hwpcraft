//! Owner: format area. The 글자 겹치기 control must keep its text position after typing on
//! either side of it, across HWP and HWPX round trips. With `HWPCRAFT_OVERLAP_OUT` set the
//! HWPX is also written there so the run order can be inspected in the XML.
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

#[test]
fn char_overlap_keeps_its_place_after_typing_on_both_sides() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나 다라".into()))?;
    s.select(
        Cursor {
            offset: 2,
            ..s.cursor
        },
        false,
    )?;
    s.run("format.char_overlap", json!({"chars": "주의"}))?;
    s.select(
        Cursor {
            offset: 5,
            ..s.cursor
        },
        false,
    )?;
    s.execute(Command::Insert("마".into()))?;
    s.select(
        Cursor {
            offset: 0,
            ..s.cursor
        },
        false,
    )?;
    s.execute(Command::Insert("앞".into()))?;
    assert_eq!(s.paragraph(s.cursor)?.text, "앞가나 다라마");
    assert_eq!(
        s.run("format.char_overlap.get", json!({}))?[0]["position"],
        3
    );
    let dir = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = match std::env::var("HWPCRAFT_OVERLAP_OUT") {
            Ok(out) if ext == "hwpx" => std::path::PathBuf::from(out),
            _ => dir.path().join(format!("overlap.{ext}")),
        };
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(r.paragraph(r.cursor)?.text, "앞가나 다라마", "{ext}");
        let list = r.run("format.char_overlap.get", json!({}))?;
        assert_eq!(list[0]["position"], 3, "{ext}");
        assert_eq!(list[0]["chars"], "주의", "{ext}");
    }
    Ok(())
}
