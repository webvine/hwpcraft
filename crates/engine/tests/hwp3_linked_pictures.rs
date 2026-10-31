//! An HWP3 document's linked (external) pictures keep their file names through an
//! HWP save: an HWP5 picture names its BinData by position, and the HWP3 reader
//! once listed the links in hash-map order (RHWP 0043), so each picture came back
//! under another one's name, differently from process to process.
use hwpcraft_engine::Session;

const SAMPLE: &str = "../../../rhwp/samples/hwp3-sample10.hwp";

/// The `[외부: name]` placeholders drawn for linked pictures on page 1, in order.
fn linked_names(s: &Session) -> anyhow::Result<Vec<String>> {
    let svg = s.render_page_svg(0)?;
    Ok(svg
        .split("[외부: ")
        .skip(1)
        .filter_map(|rest| rest.split(']').next().map(str::to_owned))
        .collect())
}

#[test]
fn linked_pictures_keep_their_names_through_an_hwp_save() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    if !path.exists() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    // Each open hashes the names with a new seed: two opens make a lucky order unlikely.
    for round in 0..2 {
        let mut s = Session::new()?;
        s.open(&path)?;
        let names = linked_names(&s)?;
        assert!(
            names.len() >= 2,
            "the sample links several pictures: {names:?}"
        );
        let copy = dir.path().join(format!("copy{round}.hwp"));
        s.save(&copy)?;
        let mut back = Session::new()?;
        back.open(&copy)?;
        assert_eq!(linked_names(&back)?, names);
    }
    Ok(())
}
