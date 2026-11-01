//! HWP files with Hangul change tracking written from two models: the accepted
//! result goes to `BodyText`, the marked text (deleted text and range tags 16–19)
//! to `ViewText`, as Hangul stores them (HWP 5 change-tracking documents keep
//! both, and the FileHeader bit 14 and the DocInfo TRACK_CHANGE records).
use super::*;
use anyhow::ensure;
use std::io::{Cursor, Read, Write};

/// Write `path` from `view` (the live, marked model) and `body` (the same document
/// with every tracked change accepted, records kept). Both go through the normal
/// HWP pipeline; `view`'s `BodyText/SectionN` streams then become `ViewText/SectionN`
/// in `body`'s file. Sections the model has not changed are written from the model
/// in `view` (its stored bytes are the accepted result, RHWP 0070), and kept as read
/// in `body`.
pub fn save_tracked_hwp(
    view: &DocumentCore,
    mut body: hwpcraft_doc::model::document::Document,
    path: &Path,
) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("저장할 파일 경로가 비어 있습니다.");
    }
    file_access::check_write(&path.to_string_lossy()).map_err(anyhow::Error::msg)?;
    let format = view.source_format();
    let mut shown = view.document().clone();
    for section in &mut shown.sections {
        section.raw_stream = None;
    }
    let not_view = |(name, _): &(String, Vec<u8>)| !name.starts_with("/ViewText/");
    shown.extra_streams.retain(not_view);
    body.extra_streams.retain(not_view);
    let shown_bytes = hwp_bytes(&export_core(shown, format), None)?;
    let body_bytes = hwp_bytes(&export_core(body, format), None)?;
    let mut shown_file = cfb::CompoundFile::open(Cursor::new(shown_bytes))?;
    let mut file = cfb::CompoundFile::open(Cursor::new(body_bytes))?;
    let sections = |f: &cfb::CompoundFile<Cursor<Vec<u8>>>| -> Vec<String> {
        let mut names: Vec<String> = f
            .read_storage("/BodyText")
            .map(|entries| {
                entries
                    .filter(|e| e.is_stream())
                    .map(|e| e.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let names = sections(&shown_file);
    ensure!(
        names == sections(&file),
        "변경 추적 문서의 결과본과 표시본의 구역 수가 다릅니다."
    );
    if !file.is_storage("/ViewText") {
        file.create_storage("/ViewText")?;
    }
    for name in names {
        let mut bytes = Vec::new();
        shown_file
            .open_stream(format!("/BodyText/{name}"))?
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let mut stream = file.create_stream(format!("/ViewText/{name}"))?;
        stream.write_all(&bytes)?;
        stream.flush()?;
    }
    file.flush()?;
    atomic_write(path, &file.into_inner().into_inner())
}
