//! Lossless attribute patches around the native HWPX AutoNumber adapter.
//!
//! HWP5 uses the public NUMBER_SHAPE values. The native HWPX AutoNumber
//! adapter uses an older, compressed page-number enum. Temporary identities
//! let us restore the public values without depending on XML/control order.
use anyhow::{Context, Result, anyhow, bail, ensure};
use hwpcraft_doc::{
    DocumentCore,
    model::{
        control::{AutoNumber, Control},
        paragraph::Paragraph,
        shape::ShapeObject,
    },
};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read, Write},
    ops::Range,
};
use zip::{ZipArchive, ZipWriter};

/// Real documents have a handful of sections; this only stops entry floods.
const MAX_SECTION_ENTRIES: usize = 4096;

/// Canonical HWP NUMBER_SHAPE to the HWPX number-format token. Legacy 18 is
/// accepted only as the native adapter's USER_CHAR alias.
pub fn format_token(format: u8) -> Result<&'static str> {
    Ok(match format {
        0 => "DIGIT",
        1 => "CIRCLED_DIGIT",
        2 => "ROMAN_CAPITAL",
        3 => "ROMAN_SMALL",
        4 => "LATIN_CAPITAL",
        5 => "LATIN_SMALL",
        6 => "CIRCLED_LATIN_CAPITAL",
        7 => "CIRCLED_LATIN_SMALL",
        8 => "HANGUL_SYLLABLE",
        9 => "CIRCLED_HANGUL_SYLLABLE",
        10 => "HANGUL_JAMO",
        11 => "CIRCLED_HANGUL_JAMO",
        12 => "HANGUL_PHONETIC",
        13 => "IDEOGRAPH",
        14 => "CIRCLED_IDEOGRAPH",
        0x80 => "SYMBOL",
        18 | 0x81 => "USER_CHAR",
        value => bail!("지원하지 않는 자동 번호 모양({value})을 숫자로 바꾸어 저장할 수 없습니다."),
    })
}

pub(crate) fn format_code(token: &str) -> Result<u8> {
    Ok(match token {
        "DIGIT" => 0,
        "CIRCLE_DIGIT" | "CIRCLED_DIGIT" => 1,
        "ROMAN_CAPITAL" => 2,
        "ROMAN_SMALL" => 3,
        "LATIN_CAPITAL" => 4,
        "LATIN_SMALL" => 5,
        "CIRCLED_LATIN_CAPITAL" => 6,
        "CIRCLED_LATIN_SMALL" => 7,
        "HANGUL" | "HANGUL_SYLLABLE" => 8,
        "CIRCLED_HANGUL_SYLLABLE" => 9,
        "HANGUL_JAMO" => 10,
        "CIRCLED_HANGUL_JAMO" => 11,
        "HANGUL_PHONETIC" => 12,
        "HANJA" | "IDEOGRAPH" => 13,
        "CIRCLED_IDEOGRAPH" => 14,
        "SYMBOL" => 0x80,
        "USER_CHAR" => 0x81,
        value => bail!("지원하지 않는 HWPX 자동 번호 모양({value})을 손실 없이 열 수 없습니다."),
    })
}

#[derive(Clone)]
struct Identity {
    number: u16,
    format: u8,
}

struct Patch {
    range: Range<usize>,
    value: String,
}

fn attribute<'a>(
    element: &'a BytesStart<'a>,
    name: &[u8],
) -> Result<Option<quick_xml::events::attributes::Attribute<'a>>> {
    for attr in element.attributes() {
        let attr = attr?;
        if attr.key.as_ref().as_bytes() == name {
            return Ok(Some(attr));
        }
    }
    Ok(None)
}

fn attr_text(
    element: &BytesStart<'_>,
    name: &[u8],
    _reader: &Reader<&[u8]>,
) -> Result<Option<String>> {
    attribute(element, name)?
        .map(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|s| s.into_owned())
                .map_err(Into::into)
        })
        .transpose()
}

fn patch_attribute(
    xml: &[u8],
    element: &BytesStart<'_>,
    name: &[u8],
    value: String,
    patches: &mut Vec<Patch>,
) -> Result<()> {
    let attr = attribute(element, name)?.ok_or_else(|| {
        anyhow!(
            "HWPX 자동 번호의 필수 속성이 없습니다: {}",
            String::from_utf8_lossy(name)
        )
    })?;
    // Slice readers borrow attribute bytes directly from the input. Replace
    // only those value bytes, retaining quotes, whitespace, and other attrs.
    let start = (attr.value.as_ptr() as usize)
        .checked_sub(xml.as_ptr() as usize)
        .ok_or_else(|| anyhow!("HWPX 속성 바이트 위치 오류"))?;
    let end = start
        .checked_add(attr.value.len())
        .filter(|n| *n <= xml.len())
        .ok_or_else(|| anyhow!("HWPX 속성 범위 오류"))?;
    patches.push(Patch {
        range: start..end,
        value,
    });
    Ok(())
}

fn apply_patches(xml: &[u8], mut patches: Vec<Patch>) -> Result<Vec<u8>> {
    patches.sort_by_key(|p| p.range.start);
    let mut result = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for patch in patches {
        ensure!(patch.range.start >= cursor, "중복 HWPX 속성 수정 범위");
        result.extend_from_slice(&xml[cursor..patch.range.start]);
        result.extend_from_slice(patch.value.as_bytes());
        cursor = patch.range.end;
    }
    result.extend_from_slice(&xml[cursor..]);
    Ok(result)
}

fn identity_xml(xml: &[u8], identities: &mut Vec<Identity>) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut patches = Vec::new();
    let mut current = None;
    loop {
        match reader.read_event()? {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty =
                    xml.get(reader.buffer_position().saturating_sub(2) as usize) == Some(&b'/');
                match e.local_name().as_ref().as_bytes() {
                    b"autoNum" => {
                        ensure!(current.is_none(), "중첩 자동 번호 XML은 지원하지 않습니다.");
                        ensure!(
                            identities.len() < u16::MAX as usize,
                            "자동 번호 65535개 제한을 초과했습니다."
                        );
                        let number = attr_text(e, b"num", &reader)?
                            .unwrap_or_else(|| "0".into())
                            .parse()
                            .context("HWPX 자동 번호 값 오류")?;
                        let index = identities.len();
                        identities.push(Identity { number, format: 0 });
                        patch_attribute(xml, e, b"num", (index + 1).to_string(), &mut patches)?;
                        if !empty {
                            current = Some(index);
                        }
                    }
                    b"autoNumFormat" => {
                        if let Some(index) = current {
                            identities[index].format = format_code(
                                &attr_text(e, b"type", &reader)?.unwrap_or_else(|| "DIGIT".into()),
                            )?;
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref e) if e.local_name().as_ref().as_bytes() == b"autoNum" => current = None,
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(current.is_none(), "닫히지 않은 HWPX 자동 번호 XML");
    apply_patches(xml, patches)
}

fn restore_xml(xml: &[u8], identities: &[Identity], seen: &mut BTreeSet<usize>) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut patches = Vec::new();
    let mut current = None;
    loop {
        match reader.read_event()? {
            Event::Start(ref e) | Event::Empty(ref e) => match e.local_name().as_ref().as_bytes() {
                b"autoNum" => {
                    let id: usize = attr_text(e, b"num", &reader)?
                        .ok_or_else(|| anyhow!("자동 번호 식별자 누락"))?
                        .parse()?;
                    let index = id
                        .checked_sub(1)
                        .filter(|i| *i < identities.len())
                        .ok_or_else(|| anyhow!("자동 번호 식별자 불일치"))?;
                    ensure!(seen.insert(index), "중복 자동 번호 식별자");
                    patch_attribute(
                        xml,
                        e,
                        b"num",
                        identities[index].number.to_string(),
                        &mut patches,
                    )?;
                    current = Some(index);
                }
                b"autoNumFormat" => {
                    if let Some(index) = current {
                        patch_attribute(
                            xml,
                            e,
                            b"type",
                            format_token(identities[index].format)?.into(),
                            &mut patches,
                        )?;
                    }
                }
                _ => {}
            },
            Event::End(ref e) if e.local_name().as_ref().as_bytes() == b"autoNum" => current = None,
            Event::Eof => break,
            _ => {}
        }
    }
    apply_patches(xml, patches)
}

/// Copy untouched ZIP entries without decompressing/recompressing them. XML
/// sections retain every byte except the selected attributes.
pub(crate) fn rewrite_package(
    bytes: &[u8],
    mut edit: impl FnMut(&[u8]) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut source = ZipArchive::new(Cursor::new(bytes))?;
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    out.set_raw_comment(source.comment().into())?;
    // Declared ZIP sizes are attacker-controlled, so the budget counts bytes
    // actually inflated and every read is capped by what is left of it.
    let mut total = 0u64;
    let mut sections = 0usize;
    for i in 0..source.len() {
        let mut entry = source.by_index(i)?;
        let lower = entry.name().to_ascii_lowercase();
        if lower.ends_with(".xml") && (lower.contains("section") || lower.contains("masterpage")) {
            sections += 1;
            ensure!(
                sections <= MAX_SECTION_ENTRIES,
                "HWPX 구역 XML 항목이 너무 많습니다."
            );
            let remaining = crate::MAX_DOCUMENT_BYTES - total;
            let mut xml = Vec::new();
            entry.by_ref().take(remaining + 1).read_to_end(&mut xml)?;
            total += xml.len() as u64;
            ensure!(
                total <= crate::MAX_DOCUMENT_BYTES,
                "HWPX XML이 128 MB 제한을 초과했습니다."
            );
            let changed = edit(&xml)?;
            if changed == xml {
                // Reading XML consumed this entry's compressed stream. A raw
                // copy must start from a fresh reader, including when no
                // attribute actually changed (e.g. DIGIT number 1 / id 1).
                drop(entry);
                out.raw_copy_file(source.by_index(i)?)?;
            } else {
                out.start_file(entry.name(), entry.options())?;
                out.write_all(&changed)?;
            }
        } else {
            out.raw_copy_file(entry)?;
        }
    }
    Ok(out.finish()?.into_inner())
}

fn paragraphs(
    paras: &mut [Paragraph],
    depth: usize,
    visit: &mut impl FnMut(&mut AutoNumber) -> Result<()>,
) -> Result<()> {
    ensure!(depth <= 64, "자동 번호 중첩 깊이 제한 초과");
    for p in paras {
        for control in &mut p.controls {
            match control {
                Control::AutoNumber(n) => visit(n)?,
                Control::Table(t) => {
                    for cell in &mut t.cells {
                        paragraphs(&mut cell.paragraphs, depth + 1, visit)?;
                    }
                    if let Some(c) = &mut t.caption {
                        paragraphs(&mut c.paragraphs, depth + 1, visit)?;
                    }
                }
                Control::Header(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Footer(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Footnote(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Endnote(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::HiddenComment(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Field(f) => paragraphs(&mut f.memo_paragraphs, depth + 1, visit)?,
                Control::Picture(p) => {
                    if let Some(c) = &mut p.caption {
                        paragraphs(&mut c.paragraphs, depth + 1, visit)?;
                    }
                }
                Control::Shape(s) => shape(s, depth + 1, visit)?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn shape(
    s: &mut ShapeObject,
    depth: usize,
    visit: &mut impl FnMut(&mut AutoNumber) -> Result<()>,
) -> Result<()> {
    ensure!(depth <= 64, "자동 번호 도형 중첩 깊이 제한 초과");
    if let Some(d) = s.drawing_mut() {
        if let Some(t) = &mut d.text_box {
            paragraphs(&mut t.paragraphs, depth + 1, visit)?;
        }
        if let Some(c) = &mut d.caption {
            paragraphs(&mut c.paragraphs, depth + 1, visit)?;
        }
    }
    match s {
        ShapeObject::Group(g) => {
            for child in &mut g.children {
                shape(child, depth + 1, visit)?;
            }
            if let Some(c) = &mut g.caption {
                paragraphs(&mut c.paragraphs, depth + 1, visit)?;
            }
        }
        ShapeObject::Picture(p) => {
            if let Some(c) = &mut p.caption {
                paragraphs(&mut c.paragraphs, depth + 1, visit)?;
            }
        }
        ShapeObject::Chart(c) => {
            if let Some(c) = &mut c.caption {
                paragraphs(&mut c.paragraphs, depth + 1, visit)?;
            }
        }
        ShapeObject::Ole(o) => {
            if let Some(c) = &mut o.caption {
                paragraphs(&mut c.paragraphs, depth + 1, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn document(
    doc: &mut hwpcraft_doc::model::document::Document,
    mut visit: impl FnMut(&mut AutoNumber) -> Result<()>,
) -> Result<()> {
    for section in &mut doc.sections {
        paragraphs(&mut section.paragraphs, 0, &mut visit)?;
        for master in &mut section.section_def.master_pages {
            paragraphs(&mut master.paragraphs, 0, &mut visit)?;
        }
    }
    Ok(())
}

/// Open through the native parser, restoring canonical shapes before returning
/// the editable model. Non-HWPX formats retain their native parsing path.
pub fn open(bytes: &[u8], password: Option<&[u8]>) -> Result<DocumentCore> {
    // Preserve the native password/error contract before inspecting XML.
    let original = match password {
        Some(p) => DocumentCore::from_bytes_with_password(bytes, p),
        None => DocumentCore::from_bytes(bytes),
    }
    .map_err(|e| anyhow!("{e}"))?;
    if !bytes.starts_with(b"PK\x03\x04") {
        return Ok(original);
    }
    let decrypted = match password {
        Some(p) => hwpcraft_doc::password_crypto::decrypt_hwpx_package(bytes, p)?,
        None => None,
    };
    let plain = decrypted.as_deref().unwrap_or(bytes);
    let mut identities = Vec::new();
    let marked = rewrite_package(plain, |xml| identity_xml(xml, &mut identities))?;
    if identities.is_empty() {
        return Ok(original);
    }
    let mut core = DocumentCore::from_bytes(&marked).map_err(|e| anyhow!("{e}"))?;
    let mut doc = core.document().clone();
    let mut seen = BTreeSet::new();
    document(&mut doc, |n| {
        let index = usize::from(n.assigned_number)
            .checked_sub(1)
            .filter(|i| *i < identities.len())
            .ok_or_else(|| anyhow!("HWPX 자동 번호와 네이티브 제어의 식별자가 다릅니다."))?;
        seen.insert(index);
        let value = &identities[index];
        n.number = value.number;
        n.assigned_number = value.number;
        n.format = value.format;
        Ok(())
    })?;
    ensure!(
        seen.len() == identities.len(),
        "HWPX 자동 번호 일부를 네이티브 모델에서 찾지 못했습니다. 원본을 보존하기 위해 열기를 중단했습니다."
    );
    core.set_document(doc);
    Ok(core)
}

/// Serialize a transient identity-bearing model, restore only the public XML
/// attributes, then encrypt the corrected package when requested.
pub fn export(core: &DocumentCore, password: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut doc = core.document().clone();
    let mut identities = Vec::new();
    document(&mut doc, |n| {
        format_token(n.format)?;
        ensure!(
            identities.len() < u16::MAX as usize,
            "자동 번호 65535개 제한 초과"
        );
        identities.push(Identity {
            number: n.number,
            format: n.format,
        });
        n.number = identities.len() as u16;
        n.assigned_number = n.number;
        Ok(())
    })?;
    if identities.is_empty() {
        return match password {
            Some(p) => core.export_hwpx_native_with_password(p),
            None => core.export_hwpx_native(),
        }
        .map_err(|e| anyhow!("{e}"));
    }
    let mut transient = DocumentCore::new_empty();
    transient.set_document(doc);
    let native = transient.export_hwpx_native().map_err(|e| anyhow!("{e}"))?;
    let mut seen = BTreeSet::new();
    let corrected = rewrite_package(&native, |xml| restore_xml(xml, &identities, &mut seen))?;
    ensure!(
        seen.len() == identities.len(),
        "자동 번호 일부를 HWPX에 저장하지 못했습니다. 원본을 보존하기 위해 저장을 중단했습니다."
    );
    match password {
        Some(p) => {
            hwpcraft_doc::password_crypto::encrypt_hwpx_package(&corrected, p).map_err(Into::into)
        }
        None => Ok(corrected),
    }
}

/// Prepare the native HWP snapshot before disambiguating its body number slots.
/// The native writer only recognizes a space-number placeholder before filling
/// structural/field gaps. Its U+FFFC placeholder path works on both sides of a
/// gap and emits the same native AutoNumber control, whose parser restores SPACE.
/// Only a proven existing control's one-scalar placeholder is changed in this
/// transient export model; the editable document and saved control stay native.
pub fn prepare_hwp(core: &DocumentCore) -> Result<Option<DocumentCore>> {
    if !core
        .document()
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .any(|p| {
            p.controls
                .iter()
                .any(|c| matches!(c, Control::AutoNumber(_)))
        })
    {
        return Ok(None);
    }
    let snapshot = core.prepare_hwp_export_snapshot();
    let mut doc = snapshot.document().clone();
    let mut changed = false;
    for section in &mut doc.sections {
        for p in &mut section.paragraphs {
            let positions = p.control_text_positions();
            let mut chars: Vec<char> = p.text.chars().collect();
            let mut slots = BTreeSet::new();
            for (control, pos) in p.controls.iter().zip(positions) {
                if !matches!(control, Control::AutoNumber(_)) {
                    continue;
                }
                let pos = if pos == chars.len() && pos > 0 && chars.last() == Some(&' ') {
                    pos - 1
                } else {
                    pos
                };
                ensure!(slots.insert(pos), "자동 번호의 HWP 저장 자리표가 겹칩니다.");
                if chars.get(pos) == Some(&'\u{fffc}') {
                    continue;
                }
                ensure!(
                    chars.get(pos) == Some(&' '),
                    "자동 번호의 HWP 저장 자리표를 확인하지 못했습니다."
                );
                let start = *p
                    .char_offsets
                    .get(pos)
                    .ok_or_else(|| anyhow!("자동 번호의 HWP 스트림 시작 위치가 없습니다."))?;
                if let Some(end) = p.char_offsets.get(pos + 1) {
                    ensure!(
                        *end >= start.saturating_add(8),
                        "자동 번호의 HWP 제어 슬롯 폭이 부족합니다."
                    );
                } else {
                    ensure!(
                        pos + 1 == chars.len(),
                        "자동 번호의 HWP 마지막 자리표가 잘못되었습니다."
                    );
                }
                chars[pos] = '\u{fffc}';
                changed = true;
            }
            if !slots.is_empty() {
                p.text = chars.into_iter().collect();
                p.stored_text_partition_dirty = true;
                section.raw_stream = None;
                section.raw_provenance = None;
            }
        }
    }
    if !changed {
        return Ok(None);
    }
    let mut prepared = DocumentCore::new_empty();
    prepared.set_document(doc);
    Ok(Some(prepared))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml_identity_patches_leave_all_unrelated_bytes_intact() -> Result<()> {
        let xml=br#"<s:sec xmlns:s='x' xmlns:p='y'><p:autoNum odd='keep &amp; this' num = '12' numType='PAGE'><p:autoNumFormat custom='same' type = 'CIRCLED_LATIN_SMALL'/></p:autoNum><p:fieldBegin command='https://example.com/a?x=1&amp;y=2'/></s:sec>"#;
        let mut identities = Vec::new();
        let marked = identity_xml(xml, &mut identities)?;
        assert_eq!(identities[0].format, 7);
        let restored = restore_xml(&marked, &identities, &mut BTreeSet::new())?;
        assert_eq!(restored, xml);
        Ok(())
    }
    #[test]
    fn unknown_shapes_fail_before_native_digit_fallback() {
        assert!(format_code("FUTURE_NUMBER_STYLE").is_err());
        assert!(format_token(127).is_err());
    }

    #[test]
    fn package_patch_preserves_unrelated_entries_and_archive_comment() -> Result<()> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.set_raw_comment(b"original archive comment".to_vec().into_boxed_slice())?;
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600);
        writer.start_file("Contents/section0.xml", options)?;
        writer.write_all(br#"<sec><autoNum num="27"><autoNumFormat type="ROMAN_SMALL"/></autoNum><link url="https://example.test/a?x=1&amp;y=2"/></sec>"#)?;
        writer.start_file("custom/untouched.bin", options)?;
        writer.write_all(&[0, 255, 0, 128, 17, 23])?;
        writer.start_file("Contents/section1.xml", options)?;
        writer.write_all(
            b"<sec><paragraph>unchanged XML, compressed stream must rewind</paragraph></sec>",
        )?;
        let before = writer.finish()?.into_inner();
        let mut ids = Vec::new();
        let marked = rewrite_package(&before, |xml| identity_xml(xml, &mut ids))?;
        let after = rewrite_package(&marked, |xml| restore_xml(xml, &ids, &mut BTreeSet::new()))?;
        let mut source = ZipArchive::new(Cursor::new(&before))?;
        let mut restored = ZipArchive::new(Cursor::new(&after))?;
        assert_eq!(source.comment(), restored.comment());
        assert_eq!(source.len(), restored.len());
        for i in 0..source.len() {
            let mut a = source.by_index(i)?;
            let mut b = restored.by_index(i)?;
            assert_eq!(a.name(), b.name());
            assert_eq!(a.compression(), b.compression());
            assert_eq!(a.unix_mode(), b.unix_mode());
            let mut av = Vec::new();
            let mut bv = Vec::new();
            a.read_to_end(&mut av)?;
            b.read_to_end(&mut bv)?;
            assert_eq!(av, bv);
        }
        Ok(())
    }

    #[test]
    fn section_entry_floods_are_refused_before_inflating_them_all() -> Result<()> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for i in 0..=MAX_SECTION_ENTRIES {
            writer.start_file(format!("Contents/section{i}.xml"), options)?;
            writer.write_all(b"<sec/>")?;
        }
        let bytes = writer.finish()?.into_inner();
        let mut visited = 0;
        let result = rewrite_package(&bytes, |xml| {
            visited += 1;
            Ok(xml.to_vec())
        });
        assert!(result.is_err());
        assert_eq!(visited, MAX_SECTION_ENTRIES);
        Ok(())
    }

    #[test]
    fn repeated_unchanged_xml_copies_remain_valid_deflate_streams() -> Result<()> {
        let xml = br#"<sec><autoNum num="1"><autoNumFormat type="DIGIT"/></autoNum></sec>"#;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.start_file(
            "Contents/section0.xml",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )?;
        writer.write_all(xml)?;
        let mut bytes = writer.finish()?.into_inner();
        for _ in 0..5 {
            let mut ids = Vec::new();
            bytes = rewrite_package(&bytes, |value| identity_xml(value, &mut ids))?;
            bytes = rewrite_package(&bytes, |value| {
                restore_xml(value, &ids, &mut BTreeSet::new())
            })?;
            let mut archive = ZipArchive::new(Cursor::new(&bytes))?;
            let mut actual = Vec::new();
            archive
                .by_name("Contents/section0.xml")?
                .read_to_end(&mut actual)?;
            assert_eq!(actual, xml);
        }
        Ok(())
    }
}
