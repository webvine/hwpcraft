use std::io::{Cursor, Read, Write};
use std::path::Path;

/// A damaged HWPX table declaring fewer rows than its cells address used to
/// open fine and then panic inside RHWP's border renderer.
#[test]
fn undersized_table_counts_are_repaired_before_rendering() -> anyhow::Result<()> {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/issue5590_per_row_column_widths.hwpx");
    let bytes = std::fs::read(sample)?;
    let mut source = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut damaged = false;
    for i in 0..source.len() {
        let mut entry = source.by_index(i)?;
        if entry.name() == "Contents/section0.xml" {
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            let at = xml
                .find("rowCnt=\"")
                .ok_or_else(|| anyhow::anyhow!("no table"))?
                + 8;
            let end = at + xml[at..].find('"').ok_or_else(|| anyhow::anyhow!("attr"))?;
            xml.replace_range(at..end, "0");
            damaged = true;
            out.start_file(entry.name(), zip::write::SimpleFileOptions::default())?;
            out.write_all(xml.as_bytes())?;
        } else {
            out.raw_copy_file(entry)?;
        }
    }
    assert!(damaged);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("rowcnt0.hwpx");
    std::fs::write(&path, out.finish()?.into_inner())?;

    let core = hwpcraft_formats::open(&path)?;
    assert!(core.page_count() > 0);
    for page in 0..core.page_count() {
        core.render_page_svg_native(page)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}
