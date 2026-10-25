//! HWP-only textbox storage marker repair on an already serialized snapshot.
//!
//! A drawing component that owns textbox LIST_HEADER children must advertise
//! that list in its stored component flags. Repair the omitted marker without
//! changing live transform flags, HWPX output, text, styles or opaque streams.
use anyhow::{Result, anyhow, bail};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use hwpcraft_doc::{
    HwpRecord,
    password_crypto::{decrypt_hwp5_stream, encrypt_hwp5_stream},
    write_hwp_records,
};
use std::io::{Cursor, Read, Write};

const SHAPE_COMPONENT: u16 = 76;
const LIST_HEADER: u16 = 72;
const TEXTBOX_LIST_PRESENT: u32 = 0x0100_0000;
const DRAWING_IDS: [u32; 6] = [
    0x24726563, 0x24656c6c, 0x24706f6c, 0x24637572, 0x246c696e, 0x24617263,
];

/// Compose after auto-number preparation and memo export. The native lowering,
/// section splitting and password serializer therefore execute exactly once.
pub(crate) fn repair_hwp(baseline: Vec<u8>, password: Option<&[u8]>) -> Result<Vec<u8>> {
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
        bail!("암호화된 글상자 스트림을 보완하려면 문서 암호가 필요합니다.");
    }
    let paths = compound
        .walk()
        .filter(|entry| {
            entry.is_stream()
                && entry.path().parent() == Some(std::path::Path::new("/BodyText"))
                && entry.name().strip_prefix("Section").is_some_and(|index| {
                    !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit())
                })
        })
        .map(|entry| entry.path().to_path_buf())
        .collect::<Vec<_>>();
    let mut changed = false;
    for path in paths {
        let mut stored = Vec::new();
        compound
            .open_stream(&path)?
            .take(crate::MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut stored)?;
        if stored.len() as u64 > crate::MAX_DOCUMENT_BYTES {
            bail!("글상자 본문 스트림이 128 MB 제한을 초과했습니다.");
        }
        let stored = if encrypted {
            decrypt_hwp5_stream(&stored, password.unwrap_or_default())
        } else {
            stored
        };
        let raw = if compressed {
            let mut raw = Vec::new();
            DeflateDecoder::new(stored.as_slice())
                .take(crate::MAX_DOCUMENT_BYTES + 1)
                .read_to_end(&mut raw)?;
            if raw.len() as u64 > crate::MAX_DOCUMENT_BYTES {
                bail!("글상자 본문이 128 MB 제한을 초과했습니다.");
            }
            raw
        } else {
            stored
        };
        let mut records = HwpRecord::read_all(&raw).map_err(|error| anyhow!("{error}"))?;
        if repair_records(&mut records)? == 0 {
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
        changed = true;
    }
    if changed {
        compound.flush()?;
    }
    Ok(compound.into_inner().into_inner())
}

#[derive(Clone, Copy)]
struct Component {
    index: usize,
    level: u16,
    drawing: bool,
}
fn repair_records(records: &mut [HwpRecord]) -> Result<usize> {
    let mut stack: Vec<Component> = Vec::new();
    let mut changed = 0;
    for index in 0..records.len() {
        let level = records[index].level;
        while stack
            .last()
            .is_some_and(|component| component.level >= level)
        {
            stack.pop();
        }
        if records[index].tag_id == SHAPE_COMPONENT {
            let data = &records[index].data;
            let id = data
                .get(..4)
                .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            stack.push(Component {
                index,
                level,
                drawing: id.is_some_and(|id| DRAWING_IDS.contains(&id)),
            });
            continue;
        }
        let Some(component) = stack.last().copied() else {
            continue;
        };
        if records[index].tag_id != LIST_HEADER
            || !component.drawing
            || level != component.level + 1
        {
            continue;
        }
        let data = &mut records[component.index].data;
        if data.len() < 8 {
            bail!("글상자 도형의 공통 레코드가 잘렸습니다.");
        }
        let two_ids = data[..4] == data[4..8];
        let offset = if two_ids { 36 } else { 32 };
        let flags = data
            .get_mut(offset..offset + 4)
            .ok_or_else(|| anyhow!("글상자 도형의 저장 속성이 잘렸습니다."))?;
        let original = u32::from_le_bytes([flags[0], flags[1], flags[2], flags[3]]);
        if original & TEXTBOX_LIST_PRESENT == 0 {
            flags.copy_from_slice(&(original | TEXTBOX_LIST_PRESENT).to_le_bytes());
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hwpcraft_doc::{DocumentCore, model::control::Control};
    fn record(tag: u16, level: u16, data: Vec<u8>) -> HwpRecord {
        HwpRecord {
            tag_id: tag,
            level,
            size: data.len() as u32,
            data,
        }
    }
    fn component(level: u16, two_ids: bool, flags: u32) -> HwpRecord {
        let mut data = vec![0; 80];
        data[..4].copy_from_slice(&DRAWING_IDS[0].to_le_bytes());
        if two_ids {
            data[4..8].copy_from_slice(&DRAWING_IDS[0].to_le_bytes());
        }
        let offset = if two_ids { 36 } else { 32 };
        data[offset..offset + 4].copy_from_slice(&flags.to_le_bytes());
        record(SHAPE_COMPONENT, level, data)
    }
    #[test]
    fn direct_text_lists_set_only_the_storage_bit_and_are_idempotent() -> Result<()> {
        let mut records = vec![
            record(LIST_HEADER, 2, vec![0; 33]),
            component(2, true, 0x8008_0003),
            record(79, 3, vec![0; 33]),
            component(2, true, 0x8008_0003),
            record(LIST_HEADER, 3, vec![0; 33]),
            record(66, 3, vec![0; 24]),
            component(4, false, 0x8008_0002),
            record(LIST_HEADER, 5, vec![0; 33]),
            record(66, 5, vec![0; 24]),
        ];
        let original = records.clone();
        assert_eq!(repair_records(&mut records)?, 2);
        assert_eq!(
            records[1].data, original[1].data,
            "a preceding caption must not become a textbox"
        );
        for index in [3, 6] {
            let offset = if index == 3 { 36 } else { 32 };
            let mut expected = original[index].data.clone();
            let flags = u32::from_le_bytes(expected[offset..offset + 4].try_into().unwrap());
            expected[offset..offset + 4]
                .copy_from_slice(&(flags | TEXTBOX_LIST_PRESENT).to_le_bytes());
            assert_eq!(records[index].data, expected);
        }
        for index in [0, 2, 4, 5, 7, 8] {
            assert_eq!(records[index].data, original[index].data);
        }
        assert_eq!(repair_records(&mut records)?, 0);
        Ok(())
    }
    #[test]
    fn malformed_proven_textbox_fails_without_silent_truncation() {
        let mut short = component(2, true, 0);
        short.data.truncate(20);
        assert!(repair_records(&mut [short, record(LIST_HEADER, 3, vec![0; 33])]).is_err());
    }
    #[test]
    fn grouped_textbox_with_caption_marks_only_its_single_id_component() -> Result<()> {
        let mut group = component(2, true, 0x8008_0003);
        let group_id = 0x2463_6f6e_u32.to_le_bytes(); // $con
        group.data[..4].copy_from_slice(&group_id);
        group.data[4..8].copy_from_slice(&group_id);
        let mut records = vec![
            record(LIST_HEADER, 2, vec![7; 33]), // group caption precedes its component
            record(66, 3, vec![11; 24]),
            group,
            record(86, 3, vec![13; 16]),      // group payload
            component(3, false, 0x8008_0002), // actual grouped textbox
            record(LIST_HEADER, 4, vec![17; 33]),
            record(66, 4, vec![19; 24]),
            record(79, 4, vec![23; 33]),
            component(3, false, 0x4000_0001), // caption-only/plain sibling
            record(79, 4, vec![29; 33]),
        ];
        let before = records.clone();
        assert_eq!(repair_records(&mut records)?, 1);
        for (index, (actual, original)) in records.iter().zip(&before).enumerate() {
            assert_eq!(actual.tag_id, original.tag_id);
            assert_eq!(actual.level, original.level);
            assert_eq!(actual.size, original.size);
            let mut expected = original.data.clone();
            if index == 4 {
                let old = u32::from_le_bytes(expected[32..36].try_into().unwrap());
                expected[32..36].copy_from_slice(&(old | TEXTBOX_LIST_PRESENT).to_le_bytes());
            }
            assert_eq!(
                actual.data, expected,
                "unexpected mutation in record {index}"
            );
        }
        assert_eq!(repair_records(&mut records)?, 0);
        Ok(())
    }
    #[test]
    fn opaque_streams_metadata_and_crypto_survive_storage_marker_repair() -> Result<()> {
        for encrypted in [false, true] {
            let mut cfb = cfb::CompoundFile::create(Cursor::new(Vec::new()))?;
            cfb.create_storage_all("/BodyText")?;
            cfb.create_storage_all("/Opaque")?;
            let mut header = vec![0; 256];
            header[36..40].copy_from_slice(&(if encrypted { 3_u32 } else { 1 }).to_le_bytes());
            let records = vec![
                component(2, true, 0x8008_0003),
                record(LIST_HEADER, 3, vec![0; 33]),
                record(66, 3, vec![0; 24]),
            ];
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&write_hwp_records(&records))?;
            let stored = encoder.finish()?;
            let stored = if encrypted {
                encrypt_hwp5_stream(&stored, b"test-password")
            } else {
                stored
            };
            for (path, data) in [
                ("/FileHeader", header),
                ("/BodyText/Section0", stored),
                ("/DocInfo", vec![17; 321]),
                ("/PrvImage", vec![19; 8193]),
                ("/Opaque/unknown", vec![23; 4119]),
            ] {
                cfb.create_stream(path)?.write_all(&data)?;
            }
            cfb.set_state_bits("/Opaque", 0x1234)?;
            cfb.set_state_bits("/Opaque/unknown", 0x9876)?;
            cfb.set_created_time("/Opaque", std::time::UNIX_EPOCH)?;
            cfb.flush()?;
            let baseline = cfb.into_inner().into_inner();
            let password = encrypted.then_some(b"test-password".as_slice());
            let patched = repair_hwp(baseline.clone(), password)?;
            let mut before = cfb::CompoundFile::open(Cursor::new(baseline))?;
            let mut after = cfb::CompoundFile::open(Cursor::new(patched.clone()))?;
            let entries = before.walk().collect::<Vec<_>>();
            assert_eq!(entries.len(), after.walk().count());
            for entry in entries {
                let actual = after.entry(entry.path())?;
                assert_eq!(entry.clsid(), actual.clsid());
                assert_eq!(entry.state_bits(), actual.state_bits());
                assert_eq!(entry.created(), actual.created());
                if entry.path() != std::path::Path::new("/BodyText/Section0") {
                    assert_eq!(entry.modified(), actual.modified());
                    let mut a = Vec::new();
                    let mut b = Vec::new();
                    if entry.is_stream() {
                        before.open_stream(entry.path())?.read_to_end(&mut a)?;
                        after.open_stream(entry.path())?.read_to_end(&mut b)?;
                        assert_eq!(a, b);
                    }
                }
            }
            assert_eq!(
                repair_hwp(patched.clone(), password)?,
                patched,
                "already-correct streams must be byte-identical"
            );
        }
        Ok(())
    }
    #[test]
    fn synthesized_textbox_hwp_and_hwpx_conversion_leave_live_model_unchanged() -> Result<()> {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native()
            .map_err(|e| anyhow!("{e}"))?;
        core.insert_text_native(0, 0, 0, "영역 본문")
            .map_err(|e| anyhow!("{e}"))?;
        core.create_shape_control_native(
            0,
            0,
            5,
            22677,
            8504,
            0,
            0,
            false,
            "InFrontOfText",
            "textbox",
            false,
            false,
            &[],
        )
        .map_err(|e| anyhow!("{e}"))?;
        let control = core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|node| matches!(node, Control::Shape(_)))
            .unwrap();
        core.insert_text_in_cell_native(0, 0, control, 0, 0, 0, "상자 영역")
            .map_err(|e| anyhow!("{e}"))?;
        let original = format!("{:?}", core.document());
        let dir = tempfile::tempdir()?;
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("textbox.{extension}"));
            crate::save(&core, &path)?;
            let imported = crate::open(&path)?;
            let snapshot = format!("{:?}", imported.document());
            for password in [None, Some("fixture-password")] {
                let source = crate::auto_number_formats::prepare_hwp(&imported)?;
                let bytes = crate::memo_formats::export_hwp(
                    source.as_ref().unwrap_or(&imported),
                    password.map(str::as_bytes),
                )?;
                let fixed = repair_hwp(bytes, password.map(str::as_bytes))?;
                let reopened = match password {
                    None => DocumentCore::from_bytes(&fixed),
                    Some(password) => {
                        DocumentCore::from_bytes_with_password(&fixed, password.as_bytes())
                    }
                }
                .map_err(|e| anyhow!("{e}"))?;
                assert!(format!("{:?}", reopened.document()).contains("상자 영역"));
            }
            assert_eq!(format!("{:?}", imported.document()), snapshot);
        }
        assert_eq!(format!("{:?}", core.document()), original);
        Ok(())
    }
}
