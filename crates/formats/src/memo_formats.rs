//! HWP5 memo container correction, after the complete native export lowering.
//!
//! The underlying writer adds an empty body paragraph solely to hold MEMO_LIST.
//! A native Hancom document attaches the list to the existing final paragraph.
//! Remove only the independently verified extra group; legitimate empty body
//! paragraphs, native field bytes, layout and encryption remain intact.
use anyhow::{Result, anyhow, bail};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use hwpcraft_doc::model::{
    control::{Control, FieldType},
    document::Document,
};
use hwpcraft_doc::{
    DocumentCore, HwpRecord,
    password_crypto::{decrypt_hwp5_stream, encrypt_hwp5_stream},
    write_hwp_records,
};
use std::io::{Cursor, Read, Write};

const PARA_HEADER: u16 = 66;
const PARA_CHAR_SHAPE: u16 = 68;
const PARA_LINE_SEG: u16 = 69;
const MEMO_LIST: u16 = 93;

pub(crate) fn export_hwp(core: &DocumentCore, password: Option<&[u8]>) -> Result<Vec<u8>> {
    let snapshot = core.prepare_hwp_export_snapshot();
    // Memos in table cells count too: the section tail lists them with the body's.
    fn has_memo(paragraphs: &[hwpcraft_doc::model::paragraph::Paragraph]) -> bool {
        paragraphs.iter().any(|paragraph| {
            paragraph.controls.iter().any(|control| match control {
                Control::Field(field) => {
                    field.field_type == FieldType::Memo && !field.memo_paragraphs.is_empty()
                }
                Control::Table(table) => table.cells.iter().any(|cell| has_memo(&cell.paragraphs)),
                _ => false,
            })
        })
    }
    let has_memos = snapshot
        .document()
        .sections
        .iter()
        .any(|section| has_memo(&section.paragraphs));
    if !has_memos {
        return match password {
            Some(password) => snapshot.serialize_with_password(password),
            None => snapshot.serialize(),
        }
        .map_err(|e| anyhow!("{e}"));
    }

    // Complete all serializer-owned section splitting, form identities, source
    // normalization and password work once. Mutate only existing body streams
    // in that authoritative CFB; do not parse and reserialize the document.
    let baseline = match password {
        Some(password) => snapshot.serialize_with_password(password),
        None => snapshot.serialize(),
    }
    .map_err(|e| anyhow!("{e}"))?;
    patch_container(
        baseline,
        &emitted_body_counts(snapshot.document()),
        password,
    )
}

fn patch_container(
    baseline: Vec<u8>,
    counts: &[usize],
    password: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut compound = cfb::CompoundFile::open(Cursor::new(baseline))?;
    let mut header = Vec::new();
    compound
        .open_stream("/FileHeader")?
        .read_to_end(&mut header)?;
    let flags = header
        .get(36..40)
        .ok_or_else(|| anyhow!("HWP 파일 머리 정보가 잘렸습니다."))?;
    let flags = u32::from_le_bytes([flags[0], flags[1], flags[2], flags[3]]);
    let compressed = flags & 1 != 0;
    let encrypted = flags & 2 != 0;
    if encrypted && password.is_none() {
        bail!("암호화된 메모 스트림을 고치려면 문서 암호가 필요합니다.")
    }
    for (index, &expected) in counts.iter().enumerate() {
        let path = format!("/BodyText/Section{index}");
        if !compound.is_stream(&path) {
            continue;
        }
        let mut stored = Vec::new();
        compound.open_stream(&path)?.read_to_end(&mut stored)?;
        let stored = if encrypted {
            decrypt_hwp5_stream(&stored, password.unwrap_or_default())
        } else {
            stored
        };
        let mut raw = Vec::new();
        if compressed {
            DeflateDecoder::new(stored.as_slice())
                .take(crate::MAX_DOCUMENT_BYTES + 1)
                .read_to_end(&mut raw)?;
            if raw.len() as u64 > crate::MAX_DOCUMENT_BYTES {
                bail!("메모 본문이 128 MB 제한을 초과했습니다.")
            }
        } else {
            raw = stored
        }
        let mut records = HwpRecord::read_all(&raw).map_err(|e| anyhow!("{e}"))?;
        if !remove_synthetic_root(&mut records, expected) {
            continue;
        }
        let raw = write_hwp_records(&records);
        let stored = if compressed {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&raw)?;
            encoder.finish()?
        } else {
            raw
        };
        let stored = if encrypted {
            encrypt_hwp5_stream(&stored, password.unwrap_or_default())
        } else {
            stored
        };
        let mut stream = compound.open_stream(&path)?;
        stream.set_len(stored.len() as u64)?;
        stream.write_all(&stored)?;
        stream.flush()?;
    }
    compound.flush()?;
    Ok(compound.into_inner().into_inner())
}

// Match the native writer's section splitting so the proof compares emitted
// body counts, including HWPX paragraphs containing a later SectionDef.
fn emitted_body_counts(document: &Document) -> Vec<usize> {
    let split = document.layout_profile().hwpx_stored_layout();
    let mut counts = Vec::new();
    for section in &document.sections {
        let mut start = 0;
        if split && !section.raw_provenance_permits_reuse() {
            for (index, paragraph) in section.paragraphs.iter().enumerate().skip(1) {
                if paragraph
                    .controls
                    .iter()
                    .any(|c| matches!(c, Control::SectionDef(_)))
                {
                    counts.push(index - start);
                    start = index;
                }
            }
        }
        counts.push(section.paragraphs.len() - start);
    }
    counts
}

fn remove_synthetic_root(records: &mut Vec<HwpRecord>, expected: usize) -> bool {
    let Some(memo) = records.iter().position(|r| r.tag_id == MEMO_LIST) else {
        return false;
    };
    let headers: Vec<_> = records[..memo]
        .iter()
        .enumerate()
        .filter(|(_, r)| r.tag_id == PARA_HEADER && r.level == 0)
        .map(|(index, _)| index)
        .collect();
    if expected == 0 || headers.len() != expected + 1 {
        return false;
    }
    let root = headers[expected];
    let previous = headers[expected - 1];
    let data = &records[root].data;
    if data.len() < 8
        || data[..4] != 0x8000_0001_u32.to_le_bytes()
        || data[4..8] != [0; 4]
        || !records[root + 1..memo]
            .iter()
            .all(|r| r.level == 1 && matches!(r.tag_id, PARA_CHAR_SHAPE | PARA_LINE_SEG))
        || records[previous].data.len() < 4
    {
        return false;
    }
    let count = u32::from_le_bytes([
        records[previous].data[0],
        records[previous].data[1],
        records[previous].data[2],
        records[previous].data[3],
    ]);
    records[previous].data[..4].copy_from_slice(&(count | 0x8000_0000).to_le_bytes());
    records.drain(root..memo);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(tag: u16, level: u16, count: u32) -> HwpRecord {
        let mut data = count.to_le_bytes().to_vec();
        data.extend_from_slice(&[0; 20]);
        HwpRecord {
            tag_id: tag,
            level,
            size: data.len() as u32,
            data,
        }
    }
    #[test]
    fn only_a_proven_extra_root_is_removed() {
        let mut records = vec![
            record(PARA_HEADER, 0, 5),
            record(PARA_HEADER, 0, 0x8000_0001),
            record(PARA_CHAR_SHAPE, 1, 0),
            record(MEMO_LIST, 1, 1),
        ];
        assert!(
            !remove_synthetic_root(&mut records, 2),
            "a genuine empty paragraph belongs to the body"
        );
        assert!(remove_synthetic_root(&mut records, 1));
        assert_eq!(records.len(), 2);
        assert_eq!(&records[0].data[..4], &0x8000_0005_u32.to_le_bytes());
        assert_eq!(records[1].tag_id, MEMO_LIST);
    }
    #[test]
    fn text_or_controls_in_the_candidate_are_never_removed() {
        for tag in [67, 71, 72] {
            let mut records = vec![
                record(PARA_HEADER, 0, 5),
                record(PARA_HEADER, 0, 0x8000_0001),
                record(tag, 1, 0),
                record(MEMO_LIST, 1, 1),
            ];
            assert!(!remove_synthetic_root(&mut records, 1));
            assert_eq!(records.len(), 4);
        }
    }

    #[test]
    fn untouched_streams_and_directory_metadata_survive_plain_and_encrypted_patches() -> Result<()>
    {
        for encrypted in [false, true] {
            let mut compound = cfb::CompoundFile::create(Cursor::new(Vec::new()))?;
            compound.create_storage_all("/BodyText")?;
            compound.create_storage_all("/UnknownStorage")?;
            let mut header = vec![0; 256];
            header[36..40].copy_from_slice(&(if encrypted { 3_u32 } else { 1 }).to_le_bytes());
            let records = vec![
                record(PARA_HEADER, 0, 5),
                record(PARA_HEADER, 0, 0x8000_0001),
                record(PARA_CHAR_SHAPE, 1, 0),
                record(MEMO_LIST, 1, 1),
            ];
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&write_hwp_records(&records))?;
            let body = encoder.finish()?;
            let body = if encrypted {
                encrypt_hwp5_stream(&body, b"fixture-password")
            } else {
                body
            };
            for (name, data) in [
                ("/FileHeader", header),
                ("/BodyText/Section0", body),
                ("/DocInfo", vec![8; 333]),
                ("/PrvImage", vec![9; 4097]),
                ("/UnknownStorage/opaque", vec![10; 7143]),
                ("/Signatures", vec![11; 14]),
            ] {
                compound.create_stream(name)?.write_all(&data)?;
            }
            compound.set_state_bits("/UnknownStorage", 0x1234_abcd)?;
            compound.set_state_bits("/UnknownStorage/opaque", 0x7654)?;
            compound.set_created_time("/UnknownStorage", std::time::UNIX_EPOCH)?;
            compound.set_modified_time("/UnknownStorage/opaque", std::time::UNIX_EPOCH)?;
            compound.flush()?;
            let baseline = compound.into_inner().into_inner();
            let password = encrypted.then_some(b"fixture-password".as_slice());
            let patched = patch_container(baseline.clone(), &[1], password)?;
            let mut before = cfb::CompoundFile::open(Cursor::new(baseline))?;
            let mut after = cfb::CompoundFile::open(Cursor::new(patched))?;
            let entries: Vec<_> = before.walk().collect();
            assert_eq!(entries.len(), after.walk().count());
            for entry in entries {
                let actual = after.entry(entry.path())?;
                assert_eq!(entry.clsid(), actual.clsid());
                assert_eq!(entry.state_bits(), actual.state_bits());
                assert_eq!(entry.created(), actual.created());
                // Only the edited stream is permitted a new modification time
                // or length; every untouched stream remains exactly opaque.
                if entry.path() != std::path::Path::new("/BodyText/Section0") {
                    assert_eq!(entry.modified(), actual.modified());
                    assert_eq!(entry.len(), actual.len());
                    if entry.is_stream() {
                        let mut left = Vec::new();
                        let mut right = Vec::new();
                        before.open_stream(entry.path())?.read_to_end(&mut left)?;
                        after.open_stream(entry.path())?.read_to_end(&mut right)?;
                        assert_eq!(left, right, "{}", entry.path().display());
                    }
                }
            }
            let mut stored = Vec::new();
            after
                .open_stream("/BodyText/Section0")?
                .read_to_end(&mut stored)?;
            let stored = if encrypted {
                decrypt_hwp5_stream(&stored, b"fixture-password")
            } else {
                stored
            };
            let mut raw = Vec::new();
            DeflateDecoder::new(stored.as_slice()).read_to_end(&mut raw)?;
            let records = HwpRecord::read_all(&raw).map_err(|e| anyhow!("{e}"))?;
            assert_eq!(
                records
                    .iter()
                    .filter(|r| r.tag_id == PARA_HEADER && r.level == 0)
                    .count(),
                1
            );
        }
        Ok(())
    }
}
