//! Owner: shell/page area. 문서 이력 (C05): versions Hangul recorded in an HWP file are
//! listed, the last one can be written out as HML, and saving keeps the history.
use hwpcraft_engine::{Command, Session};
use serde_json::json;

const SAMPLE: &str = "../../../rhwp/samples/basic/treatise sample.hwp";

#[test]
fn recorded_versions_are_listed_and_kept_through_a_save() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.open(std::path::Path::new(SAMPLE))?;
    let list = s.run("review.history.list", json!({}))?;
    let entries = list["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 4, "{list}");
    assert_eq!(entries[0]["writer"], "(주)한글과컴퓨터");
    assert_eq!(entries[0]["date"], "2004-09-16 14:25:45");
    assert_eq!(entries[3]["index"], 3);
    assert_eq!(list["lastVersion"], true);

    let dir = tempfile::tempdir()?;
    let hml = dir.path().join("last.hml");
    let saved = s.run("review.history.save_last", json!({"path": hml}))?;
    let bytes = std::fs::read(&hml)?;
    assert_eq!(saved["bytes"], bytes.len());
    assert_eq!(&bytes[..2], &[0xFF, 0xFE], "UTF-16 with a byte order mark");
    let text: Vec<u16> = bytes[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    assert!(String::from_utf16_lossy(&text).starts_with("<HWPML"));
    assert!(
        s.run(
            "review.history.save_last",
            json!({"path": dir.path().join("last.txt")})
        )
        .is_err()
    );

    // An edit saved as HWP keeps the recorded versions.
    s.execute(Command::Insert("고침".into()))?;
    let out = dir.path().join("edited.hwp");
    s.save(&out)?;
    let mut back = Session::new()?;
    back.open(&out)?;
    let again = back.run("review.history.list", json!({}))?;
    assert_eq!(again["entries"], list["entries"]);
    Ok(())
}

#[test]
fn a_document_without_history_lists_nothing() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let list = s.run("review.history.list", json!({}))?;
    assert_eq!(list["entries"], json!([]));
    assert_eq!(list["lastVersion"], false);
    let dir = tempfile::tempdir()?;
    assert!(
        s.run(
            "review.history.save_last",
            json!({"path": dir.path().join("x.hml")})
        )
        .is_err()
    );
    let plain = dir.path().join("plain.hwp");
    s.execute(Command::Insert("이력 없음".into()))?;
    s.save(&plain)?;
    let list = s.run("review.history.list", json!({}))?;
    assert_eq!(list["entries"], json!([]));
    Ok(())
}
