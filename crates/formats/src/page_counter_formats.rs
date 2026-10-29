//! Protect HWP page fields from the native parser's unrelated u16 sequence.
//!
//! PAGE is a physical page display, not a counter increment per field. Native
//! HWP parsing nevertheless increments that counter for each PAGE control; two
//! fields on valid page 65535 overflow in debug and silently wrap in release.
//! Parse a private CFB copy with PAGE and TotalPage carrying unique identities
//! as TotalPage (the native non-counter kind), then restore their original type
//! and stored num. NewNumber/section starts, formats and source bytes stay exact.
use anyhow::{Result, anyhow, ensure};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use hwpcraft_doc::{
    DocumentCore,
    model::{
        control::{AutoNumber, AutoNumberType, Control},
        paragraph::Paragraph,
        shape::ShapeObject,
    },
    password_crypto::{decrypt_hwp5_stream, encrypt_hwp5_stream},
};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read, Write},
};

const CFB_MAGIC: &[u8] = &[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
const MAX_FIELDS: usize = u16::MAX as usize;
const MAX_DEPTH: usize = 64;
const TOKEN_FORMAT: u8 = 255;

#[derive(Clone, Debug)]
struct Identity {
    section: usize,
    kind: AutoNumberType,
    number: u16,
    payload: Vec<u8>,
}
struct Body {
    index: usize,
    path: std::path::PathBuf,
    raw: Vec<u8>,
    marked: Vec<u8>,
}

#[derive(Clone, Copy)]
struct RawContext<'a> {
    section: usize,
    identities: &'a [Identity],
    seen: &'a BTreeSet<usize>,
}

fn restore_cached(
    records: &mut [hwpcraft_doc::model::document::RawRecord],
    context: RawContext<'_>,
) -> Result<()> {
    for record in records {
        let data = &record.data;
        if record.tag_id != 71 || data.get(..4) != Some(b"onta") {
            continue;
        }
        ensure!(data.len() >= 10, "바탕쪽 원본 번호 레코드가 잘렸습니다.");
        let attr = u32::from_le_bytes(data[4..8].try_into()?);
        if !matches!(attr & 15, 0 | 6) {
            continue;
        }
        ensure!(attr & 15 == 6, "바탕쪽 원본 번호 종류가 일치하지 않습니다.");
        let id = usize::from(u16::from_le_bytes(data[8..10].try_into()?))
            .checked_sub(1)
            .filter(|id| *id < context.identities.len())
            .ok_or_else(|| anyhow!("바탕쪽 raw 번호 식별자 불일치"))?;
        let identity = &context.identities[id];
        ensure!(
            identity.section == context.section && context.seen.contains(&id),
            "바탕쪽 raw 번호의 원문 위치를 확인하지 못했습니다."
        );
        let mut expected = identity.payload.clone();
        let original = u32::from_le_bytes(expected[4..8].try_into()?);
        expected[4..8].copy_from_slice(
            &((original & !0xfff) | 6 | (u32::from(TOKEN_FORMAT) << 4)).to_le_bytes(),
        );
        expected[8..10].copy_from_slice(&((id + 1) as u16).to_le_bytes());
        ensure!(
            record.data == expected,
            "바탕쪽 raw 번호 레코드의 식별 payload가 다릅니다."
        );
        record.data = identity.payload.clone();
    }
    Ok(())
}

fn read_stream(
    cfb: &mut cfb::CompoundFile<Cursor<Vec<u8>>>,
    path: &std::path::Path,
) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    cfb.open_stream(path)?
        .take(crate::MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut result)?;
    ensure!(
        result.len() as u64 <= crate::MAX_DOCUMENT_BYTES,
        "HWP 쪽 번호 스트림이 128 MB 한도를 초과했습니다."
    );
    Ok(result)
}
fn decode(stored: &[u8], compressed: bool, password: Option<&[u8]>) -> Result<Vec<u8>> {
    let plain = password.map_or_else(|| stored.to_vec(), |p| decrypt_hwp5_stream(stored, p));
    if !compressed {
        return Ok(plain);
    }
    let mut result = Vec::new();
    DeflateDecoder::new(plain.as_slice())
        .take(crate::MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut result)
        .map_err(|error| {
            if password.is_some() {
                anyhow!("비밀번호가 일치하지 않거나 암호 문서가 손상되었습니다: {error}")
            } else {
                error.into()
            }
        })?;
    ensure!(
        result.len() as u64 <= crate::MAX_DOCUMENT_BYTES,
        "HWP 쪽 번호 본문이 128 MB 한도를 초과했습니다."
    );
    Ok(result)
}
fn encode(raw: &[u8], compressed: bool, password: Option<&[u8]>) -> Result<Vec<u8>> {
    let stored = if compressed {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
        e.write_all(raw)?;
        e.finish()?
    } else {
        raw.to_vec()
    };
    Ok(password.map_or_else(|| stored.clone(), |p| encrypt_hwp5_stream(&stored, p)))
}

/// Patch only existing atno attribute/type and UINT16 number bytes. Keep record
/// headers (including extended sizes), order, unknown tails and every other byte.
fn mark(raw: &[u8], section: usize, identities: &mut Vec<Identity>) -> Result<Vec<u8>> {
    let mut marked = raw.to_vec();
    let mut offset = 0usize;
    let mut memo_level = None;
    while offset < raw.len() {
        let header = raw
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow!("HWP 본문 레코드 머리가 잘렸습니다."))?;
        let value = u32::from_le_bytes(header.try_into()?);
        offset += 4;
        let tag = value & 1023;
        let level = (value >> 10) & 1023;
        if memo_level.is_some_and(|memo| level < memo) {
            memo_level = None;
        }
        if tag == 93 {
            memo_level = Some(level);
        }
        let mut size = (value >> 20) as usize;
        if size == 4095 {
            let bytes = raw
                .get(offset..offset + 4)
                .ok_or_else(|| anyhow!("HWP 확장 레코드 크기가 잘렸습니다."))?;
            size = u32::from_le_bytes(bytes.try_into()?) as usize;
            offset += 4;
        }
        let end = offset
            .checked_add(size)
            .filter(|end| *end <= raw.len())
            .ok_or_else(|| anyhow!("HWP 본문 레코드 길이가 올바르지 않습니다."))?;
        if tag == 71 && raw.get(offset..offset + 4) == Some(b"onta") {
            ensure!(size >= 10, "HWP 자동 번호 레코드가 잘렸습니다.");
            let attr = u32::from_le_bytes(raw[offset + 4..offset + 8].try_into()?);
            if matches!(attr & 15, 0 | 6) {
                ensure!(
                    ((attr >> 4) & 255) != u32::from(TOKEN_FORMAT),
                    "쪽 번호의 지원하지 않는 모양 255가 임시 식별자와 충돌합니다."
                );
            }
            if memo_level.is_some() {
                offset = end;
                continue;
            }
            if matches!(attr & 15, 0 | 6) {
                ensure!(
                    identities.len() < MAX_FIELDS,
                    "쪽 번호 필드 65535개 한도를 초과했습니다."
                );
                let kind = if attr & 15 == 0 {
                    AutoNumberType::Page
                } else {
                    AutoNumberType::TotalPage
                };
                identities.push(Identity {
                    section,
                    kind,
                    number: u16::from_le_bytes(raw[offset + 8..offset + 10].try_into()?),
                    payload: raw[offset..end].to_vec(),
                });
                marked[offset + 4..offset + 8].copy_from_slice(
                    &((attr & !0xfff) | 6 | (u32::from(TOKEN_FORMAT) << 4)).to_le_bytes(),
                );
                marked[offset + 8..offset + 10]
                    .copy_from_slice(&(identities.len() as u16).to_le_bytes());
            }
        }
        offset = end;
    }
    Ok(marked)
}

fn paragraphs(
    paras: &mut [Paragraph],
    depth: usize,
    mirrors: Option<RawContext<'_>>,
    visit: &mut impl FnMut(&mut AutoNumber) -> Result<()>,
) -> Result<()> {
    ensure!(
        depth <= MAX_DEPTH,
        "쪽 번호 모델 중첩 깊이 한도를 초과했습니다."
    );
    for paragraph in paras {
        for control in &mut paragraph.controls {
            match control {
                Control::AutoNumber(n) if mirrors.is_none() => visit(n)?,
                Control::SectionDef(sd) => {
                    if let Some(context) = mirrors {
                        restore_cached(&mut sd.extra_child_records, context)?;
                        for page in &mut sd.master_pages {
                            paragraphs(&mut page.paragraphs, depth + 1, None, visit)?;
                            paragraphs(&mut page.paragraphs, depth + 1, mirrors, visit)?;
                        }
                    }
                }
                Control::Table(t) => {
                    for cell in &mut t.cells {
                        paragraphs(&mut cell.paragraphs, depth + 1, mirrors, visit)?;
                    }
                    if let Some(c) = &mut t.caption {
                        paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
                    }
                }
                Control::Header(h) => paragraphs(&mut h.paragraphs, depth + 1, mirrors, visit)?,
                Control::Footer(h) => paragraphs(&mut h.paragraphs, depth + 1, mirrors, visit)?,
                Control::Footnote(h) => paragraphs(&mut h.paragraphs, depth + 1, mirrors, visit)?,
                Control::Endnote(h) => paragraphs(&mut h.paragraphs, depth + 1, mirrors, visit)?,
                Control::HiddenComment(h) => {
                    paragraphs(&mut h.paragraphs, depth + 1, mirrors, visit)?
                }
                // HWP MEMO_LIST is decoded later by HwpCraft from restored raw
                // bytes. It is never part of the native assignment/model pass.
                Control::Field(f) => ensure!(
                    f.memo_paragraphs.is_empty(),
                    "HWP 지연 메모 본문의 원문 매칭 계약이 다릅니다."
                ),
                Control::Picture(p) => {
                    if let Some(c) = &mut p.caption {
                        paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
                    }
                }
                Control::Shape(s) => shape(s, depth + 1, mirrors, visit)?,
                _ => {}
            }
        }
    }
    Ok(())
}
fn shape(
    s: &mut ShapeObject,
    depth: usize,
    mirrors: Option<RawContext<'_>>,
    visit: &mut impl FnMut(&mut AutoNumber) -> Result<()>,
) -> Result<()> {
    ensure!(
        depth <= MAX_DEPTH,
        "쪽 번호 도형 중첩 깊이 한도를 초과했습니다."
    );
    if let Some(d) = s.drawing_mut() {
        if let Some(t) = &mut d.text_box {
            paragraphs(&mut t.paragraphs, depth + 1, mirrors, visit)?;
        }
        if let Some(c) = &mut d.caption {
            paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
        }
    }
    match s {
        ShapeObject::Group(g) => {
            for child in &mut g.children {
                shape(child, depth + 1, mirrors, visit)?;
            }
            if let Some(c) = &mut g.caption {
                paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
            }
        }
        ShapeObject::Picture(p) => {
            if let Some(c) = &mut p.caption {
                paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
            }
        }
        ShapeObject::Chart(c) => {
            if let Some(c) = &mut c.caption {
                paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
            }
        }
        ShapeObject::Ole(o) => {
            if let Some(c) = &mut o.caption {
                paragraphs(&mut c.paragraphs, depth + 1, mirrors, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn restore(
    doc: &mut hwpcraft_doc::model::document::Document,
    identities: &[Identity],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (section, sec) in doc.sections.iter_mut().enumerate() {
        let mut visit = |number: &mut AutoNumber| -> Result<()> {
            if !matches!(
                number.number_type,
                AutoNumberType::Page | AutoNumberType::TotalPage
            ) {
                return Ok(());
            }
            ensure!(
                number.number_type == AutoNumberType::TotalPage && number.format == TOKEN_FORMAT,
                "임시 쪽 번호 종류가 일치하지 않습니다."
            );
            let id = usize::from(number.number)
                .checked_sub(1)
                .filter(|id| *id < identities.len())
                .ok_or_else(|| anyhow!("쪽 번호 식별자가 일치하지 않습니다."))?;
            ensure!(
                identities[id].section == section && seen.insert(id),
                "쪽 번호 식별자의 구역 또는 중복 상태가 올바르지 않습니다."
            );
            number.number_type = identities[id].kind;
            number.number = identities[id].number;
            number.format =
                ((u32::from_le_bytes(identities[id].payload[4..8].try_into()?) >> 4) & 255) as u8;
            // HWP has no assigned_number field. Preserve its stored cache value
            // rather than inventing an increment per PAGE display occurrence.
            number.assigned_number = number.number;
            Ok(())
        };
        paragraphs(&mut sec.paragraphs, 0, None, &mut visit)?;
        for master in &mut sec.section_def.master_pages {
            paragraphs(&mut master.paragraphs, 0, None, &mut visit)?;
        }
    }
    ensure!(
        seen.len() == identities.len(),
        "일부 쪽 번호를 지원 모델 트리에서 찾지 못했습니다. 원문을 보존하기 위해 열기를 중단했습니다."
    );
    // SectionDef controls can hold native clones of the authoritative section's
    // master pages. They are mirrors, not another physical record occurrence.
    for (section, sec) in doc.sections.iter_mut().enumerate() {
        let context = RawContext {
            section,
            identities,
            seen: &seen,
        };
        restore_cached(&mut sec.section_def.extra_child_records, context)?;
        let mut mirror = |number: &mut AutoNumber| -> Result<()> {
            if !matches!(
                number.number_type,
                AutoNumberType::Page | AutoNumberType::TotalPage
            ) {
                return Ok(());
            }
            ensure!(
                number.number_type == AutoNumberType::TotalPage && number.format == TOKEN_FORMAT,
                "바탕쪽 복제의 쪽 번호 종류가 일치하지 않습니다."
            );
            let id = usize::from(number.number)
                .checked_sub(1)
                .filter(|id| *id < identities.len())
                .ok_or_else(|| anyhow!("바탕쪽 복제의 쪽 번호 식별자가 일치하지 않습니다."))?;
            ensure!(
                seen.contains(&id) && identities[id].section == section,
                "바탕쪽 복제의 식별자를 확인하지 못했습니다."
            );
            number.number_type = identities[id].kind;
            number.number = identities[id].number;
            number.format =
                ((u32::from_le_bytes(identities[id].payload[4..8].try_into()?) >> 4) & 255) as u8;
            number.assigned_number = number.number;
            Ok(())
        };
        paragraphs(&mut sec.paragraphs, 0, Some(context), &mut mirror)?;
        for master in &mut sec.section_def.master_pages {
            paragraphs(&mut master.paragraphs, 0, Some(context), &mut mirror)?;
        }
    }
    Ok(())
}

/// None delegates non-HWP, unsupported encryption envelopes, and documents with
/// no PAGE/TotalPage fields to the unchanged native input path.
pub(crate) fn open(bytes: &[u8], password: Option<&[u8]>) -> Result<Option<DocumentCore>> {
    if !bytes.starts_with(CFB_MAGIC) {
        return Ok(None);
    }
    let mut cfb = cfb::CompoundFile::open(Cursor::new(bytes.to_vec()))?;
    if !cfb.is_stream("/FileHeader") {
        return Ok(None);
    }
    let header = read_stream(&mut cfb, std::path::Path::new("/FileHeader"))?;
    let flags = u32::from_le_bytes(
        header
            .get(36..40)
            .ok_or_else(|| anyhow!("HWP 파일 머리가 잘렸습니다."))?
            .try_into()?,
    );
    let compressed = flags & 1 != 0;
    let encrypted = flags & 2 != 0;
    if flags & 4 != 0 {
        return Ok(None);
    } // ViewText distribution has its own envelope.
    if cfb.walk().any(|entry| {
        entry.is_stream()
            && entry.path().parent() == Some(std::path::Path::new("/ViewText"))
            && entry
                .name()
                .strip_prefix("Section")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }) {
        return Ok(None); // Preserve native ViewText preference and its existing support.
    }
    if encrypted
        && (password.is_none() || header.get(44..48) != Some(4_u32.to_le_bytes().as_slice()))
    {
        return Ok(None);
    }
    let key = if encrypted { password } else { None };
    if encrypted {
        let stored = read_stream(&mut cfb, std::path::Path::new("/DocInfo"))?;
        let raw = decode(&stored, compressed, key)?;
        let plausible = hwpcraft_doc::HwpRecord::read_all(&raw)
            .ok()
            .is_some_and(|records| {
                records
                    .first()
                    .is_some_and(|r| r.tag_id == 16 && r.level == 0 && r.data.len() >= 26)
            });
        ensure!(
            plausible,
            "비밀번호가 일치하지 않거나 암호 문서가 손상되었습니다."
        );
    }
    let paths = {
        let mut paths = Vec::new();
        for entry in cfb.walk().filter(|e| {
            e.is_stream() && e.path().parent() == Some(std::path::Path::new("/BodyText"))
        }) {
            if let Some(index) = entry.name().strip_prefix("Section")
                && let Ok(index) = index.parse::<usize>()
            {
                ensure!(
                    entry.name() == format!("Section{index}"),
                    "HWP 구역 스트림 이름이 모호합니다."
                );
                paths.push((index, entry.path().to_path_buf()));
            }
        }
        paths.sort_by_key(|(index, _)| *index);
        paths
    };
    if paths.is_empty() {
        return Ok(None);
    }
    let mut identities = Vec::new();
    let mut bodies = Vec::new();
    let mut total = 0u64;
    for (expected, (index, path)) in paths.into_iter().enumerate() {
        ensure!(
            expected == index,
            "HWP 구역 번호가 연속되지 않아 원문 매칭을 확인할 수 없습니다."
        );
        let stored = read_stream(&mut cfb, &path)?;
        let raw = decode(&stored, compressed, key)?;
        total = total
            .checked_add(raw.len() as u64)
            .ok_or_else(|| anyhow!("HWP 본문 크기 한도 초과"))?;
        ensure!(
            total <= crate::MAX_DOCUMENT_BYTES,
            "HWP 쪽 번호 본문 합계가 128 MB 한도를 초과했습니다."
        );
        let marked = mark(&raw, index, &mut identities)?;
        bodies.push(Body {
            index,
            path,
            raw,
            marked,
        });
    }
    if identities.is_empty() {
        return Ok(None);
    }
    for body in &bodies {
        if body.raw == body.marked {
            continue;
        }
        let stored = encode(&body.marked, compressed, key)?;
        let mut stream = cfb.open_stream(&body.path)?;
        stream.set_len(stored.len() as u64)?;
        stream.write_all(&stored)?;
        stream.flush()?;
    }
    cfb.flush()?;
    let marked = cfb.into_inner().into_inner();
    // Restore before the first layout: restoring after it and calling set_document
    // laid the whole document out twice (about 0.25 s of a 0.8 s handbook open).
    // RHWP seals the original raw cache after that layout, as on a native load.
    let mut failure = None;
    let core = DocumentCore::from_bytes_prepared(&marked, password, |doc| {
        let restored = restore_parsed(doc, &identities, &mut bodies);
        restored.map_err(|e| {
            let message = e.to_string();
            failure = Some(e);
            message
        })
    });
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(Some(core.map_err(|e| anyhow!("{e}"))?))
}

fn restore_parsed(
    doc: &mut hwpcraft_doc::model::document::Document,
    identities: &[Identity],
    bodies: &mut [Body],
) -> Result<()> {
    ensure!(
        doc.sections.len() == bodies.len(),
        "HWP 원본 구역 수와 파싱 모델이 다릅니다."
    );
    for body in bodies.iter() {
        ensure!(
            doc.sections[body.index].raw_stream.as_ref() == Some(&body.marked),
            "파싱된 쪽 번호의 원본 BodyText 스트림이 다릅니다."
        );
    }
    restore(doc, identities)?;
    for body in bodies {
        doc.sections[body.index].raw_stream = Some(std::mem::take(&mut body.raw));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hwpcraft_doc::{HwpRecord, write_hwp_records};
    fn record(tag_id: u16, level: u16, data: Vec<u8>) -> HwpRecord {
        HwpRecord {
            tag_id,
            level,
            size: data.len() as u32,
            data,
        }
    }
    fn atno(kind: u32, number: u16, level: u16) -> HwpRecord {
        let mut data = b"onta".to_vec();
        data.extend_from_slice(&(0x8000_1000 | kind).to_le_bytes());
        data.extend_from_slice(&number.to_le_bytes());
        data.extend_from_slice(&[11, 13, 17, 19, 23, 29, 31]);
        record(71, level, data)
    }
    #[test]
    fn tokenization_is_byte_scoped_and_leaves_deferred_memo_numbers_untouched() -> Result<()> {
        let records = vec![
            atno(0, 0, 1),
            atno(6, 1, 1),
            atno(3, 47, 1),
            record(101, 1, vec![41; 5000]),
            record(93, 1, 1_u32.to_le_bytes().to_vec()),
            record(72, 1, vec![0; 16]),
            record(66, 1, vec![0; 24]),
            atno(6, 1, 2),
            atno(0, 1, 2),
        ];
        let original = write_hwp_records(&records);
        let mut identities = Vec::new();
        let patched = mark(&original, 0, &mut identities)?;
        assert_eq!(identities.len(), 2);
        let after = HwpRecord::read_all(&patched).map_err(|e| anyhow!("{e}"))?;
        for (index, (old, new)) in records.iter().zip(&after).enumerate() {
            assert_eq!(
                (old.tag_id, old.level, old.size),
                (new.tag_id, new.level, new.size)
            );
            let mut expected = old.data.clone();
            if index < 2 {
                expected[4..8].copy_from_slice(&(0x8000_1ff6_u32).to_le_bytes());
                expected[8..10].copy_from_slice(&((index + 1) as u16).to_le_bytes());
            }
            assert_eq!(new.data, expected);
        }
        Ok(())
    }
    #[test]
    fn token_limit_and_malformed_records_fail_before_native_parse() -> Result<()> {
        let one = write_hwp_records(&[atno(0, 0, 1)]);
        let raw = one.repeat(MAX_FIELDS + 1);
        let mut identities = Vec::new();
        assert!(
            mark(&raw, 0, &mut identities)
                .err()
                .unwrap()
                .to_string()
                .contains("65535")
        );
        assert!(mark(&[0, 1, 2], 0, &mut Vec::new()).is_err());
        let short = write_hwp_records(&[record(71, 1, b"onta".to_vec())]);
        assert!(mark(&short, 0, &mut Vec::new()).is_err());
        Ok(())
    }
    #[test]
    fn missing_duplicate_section_or_deferred_model_tokens_are_rejected() -> Result<()> {
        let raw = write_hwp_records(&[atno(0, 0, 1)]);
        let mut identities = Vec::new();
        mark(&raw, 0, &mut identities)?;
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native()
            .map_err(|e| anyhow!("{e}"))?;
        let mut doc = core.document().clone();
        assert!(restore(&mut doc, &identities).is_err());
        let token = Control::AutoNumber(AutoNumber {
            number_type: AutoNumberType::TotalPage,
            number: 1,
            format: TOKEN_FORMAT,
            ..Default::default()
        });
        let mut duplicate = core.document().clone();
        duplicate.sections[0].paragraphs[0]
            .controls
            .extend([token.clone(), token.clone()]);
        assert!(restore(&mut duplicate, &identities).is_err());
        let mut other = core.document().clone();
        other.sections.push(other.sections[0].clone());
        other.sections[1].paragraphs[0].controls.push(token.clone());
        assert!(restore(&mut other, &identities).is_err());
        let mut deferred = core.document().clone();
        deferred.sections[0].paragraphs[0]
            .controls
            .push(Control::Field(hwpcraft_doc::model::control::Field {
                memo_paragraphs: vec![Paragraph {
                    controls: vec![token],
                    ..Default::default()
                }],
                ..Default::default()
            }));
        assert!(restore(&mut deferred, &identities).is_err());
        let mut unmarked = core.document().clone();
        unmarked.sections[0].paragraphs[0]
            .controls
            .push(Control::AutoNumber(AutoNumber {
                number_type: AutoNumberType::TotalPage,
                number: 1,
                ..Default::default()
            }));
        assert!(
            restore(&mut unmarked, &identities).is_err(),
            "unpatched TotalPage must never masquerade as a token"
        );
        Ok(())
    }

    /// Restoring before the first layout gives the same model and pages as the old
    /// restore-then-`set_document`, and a failed restore keeps its own message.
    #[test]
    fn restore_before_layout_matches_the_relayout_path() -> Result<()> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../rhwp/samples/exam_math_8.hwp");
        let bytes = std::fs::read(&path)?;
        let core = open(&bytes, None)?.expect("the sample has PAGE fields");
        let native = DocumentCore::from_bytes(&bytes).map_err(|e| anyhow!("{e}"))?;
        assert_eq!(core.page_count(), native.page_count());
        assert_eq!(
            core.document().sections[0].raw_stream,
            native.document().sections[0].raw_stream,
            "the original BodyText bytes are restored"
        );
        let refused = DocumentCore::from_bytes_prepared(&bytes, None, |_| Err("거절".into()));
        assert!(refused.is_err_and(|e| e.to_string().contains("거절")));
        Ok(())
    }
}
