//! Canonical HWPX pageNum format/sideChar around the native compressed enum.
//! Only standard attributes are emitted; unsupported HWP-only decorations fail.

use anyhow::{Result, anyhow, bail, ensure};
use hwpcraft_doc::{
    DocumentCore,
    model::{
        control::{Control, PageNumberPos},
        document::Document,
        paragraph::Paragraph,
        shape::ShapeObject,
    },
};
use quick_xml::{
    NsReader,
    events::{BytesStart, Event},
    name::ResolveResult,
};
use std::{collections::BTreeSet, ops::Range};

const PARAGRAPH_NAMESPACE: &[u8] = b"http://www.hancom.co.kr/hwpml/2011/paragraph";
const TOKEN_START: u32 = 0xe000;
const TOKEN_COUNT: usize = 6400;

#[derive(Clone)]
struct Identity {
    format: u8,
    side: char,
}
struct Patch {
    range: Range<usize>,
    value: String,
}

fn token(index: usize) -> Result<char> {
    ensure!(index < TOKEN_COUNT, "쪽 번호 위치 컨트롤 6400개 제한 초과");
    char::from_u32(TOKEN_START + index as u32).ok_or_else(|| anyhow!("쪽 번호 임시 식별자 오류"))
}

fn token_index(value: char, count: usize) -> Result<usize> {
    let index = (value as u32)
        .checked_sub(TOKEN_START)
        .map(|n| n as usize)
        .filter(|index| *index < count)
        .ok_or_else(|| anyhow!("HWPX 쪽 번호 식별자 불일치"))?;
    Ok(index)
}

fn attribute(element: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>> {
    for attribute in element.attributes() {
        let attribute = attribute?;
        if attribute.key.as_ref().as_bytes() == name {
            return Ok(Some(
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn patch(
    xml: &[u8],
    element: &BytesStart<'_>,
    name: &[u8],
    value: String,
    patches: &mut Vec<Patch>,
) -> Result<()> {
    for attribute in element.attributes() {
        let attribute = attribute?;
        if attribute.key.as_ref().as_bytes() == name {
            let start = (attribute.value.as_ptr() as usize)
                .checked_sub(xml.as_ptr() as usize)
                .ok_or_else(|| anyhow!("쪽 번호 XML 속성 위치 오류"))?;
            let end = start
                .checked_add(attribute.value.len())
                .filter(|end| *end <= xml.len())
                .ok_or_else(|| anyhow!("쪽 번호 XML 속성 범위 오류"))?;
            patches.push(Patch {
                range: start..end,
                value,
            });
            return Ok(());
        }
    }
    // Missing standard optional attributes are inserted into this element only.
    let raw = element.as_ref();
    let start = (raw.as_ptr() as usize)
        .checked_sub(xml.as_ptr() as usize)
        .ok_or_else(|| anyhow!("쪽 번호 XML 요소 위치 오류"))?;
    let at = start
        .checked_add(raw.len())
        .filter(|end| *end <= xml.len())
        .ok_or_else(|| anyhow!("쪽 번호 XML 요소 범위 오류"))?;
    patches.push(Patch {
        range: at..at,
        value: format!(" {}=\"{}\"", std::str::from_utf8(name)?, value),
    });
    Ok(())
}

fn side(value: &str) -> Result<char> {
    let mut characters = value.chars();
    let first = characters.next().unwrap_or('\0');
    ensure!(
        characters.next().is_none(),
        "한 글자를 넘는 HWPX 쪽 번호 줄표를 손실 없이 열 수 없습니다."
    );
    Ok(first)
}

fn side_xml(value: char) -> Result<String> {
    if value == '\0' {
        return Ok(String::new());
    }
    if let Some(entity) = match value {
        '\t' => Some("&#9;"),
        '\n' => Some("&#10;"),
        '\r' => Some("&#13;"),
        _ => None,
    } {
        return Ok(entity.into());
    }
    ensure!(
        matches!(value as u32, 0x20..=0xd7ff|0xe000..=0xfffd|0x10000..=0x10ffff),
        "XML에 표현할 수 없는 쪽 번호 줄표 문자입니다."
    );
    Ok(quick_xml::escape::escape(value.to_string()).into_owned())
}

fn rewrite_xml(
    xml: &[u8],
    mut edit: impl FnMut(&BytesStart<'_>, &mut Vec<Patch>) -> Result<()>,
) -> Result<Vec<u8>> {
    let mut reader = NsReader::from_reader(xml);
    let mut patches = Vec::new();
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element)
                if element.local_name().as_ref().as_bytes() == b"pageNum" =>
            {
                ensure!(
                    matches!(namespace,ResolveResult::Bound(ns) if ns.as_ref().as_bytes()==PARAGRAPH_NAMESPACE),
                    "지원하지 않는 pageNum XML namespace"
                );
                edit(element, &mut patches)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    patches.sort_by_key(|patch| patch.range.start);
    let mut output = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for patch in patches {
        ensure!(patch.range.start >= cursor, "겹치는 쪽 번호 XML 수정 범위");
        output.extend_from_slice(&xml[cursor..patch.range.start]);
        output.extend_from_slice(patch.value.as_bytes());
        cursor = patch.range.end;
    }
    output.extend_from_slice(&xml[cursor..]);
    Ok(output)
}

fn mark_xml(xml: &[u8], identities: &mut Vec<Identity>) -> Result<Vec<u8>> {
    rewrite_xml(xml, |element, patches| {
        let format = crate::auto_number_formats::format_code(
            &attribute(element, b"formatType")?.unwrap_or_else(|| "DIGIT".into()),
        )?;
        let side = side(&attribute(element, b"sideChar")?.unwrap_or_default())?;
        let index = identities.len();
        let marker = token(index)?;
        identities.push(Identity { format, side });
        patch(xml, element, b"sideChar", marker.to_string(), patches)
    })
}

fn restore_xml(xml: &[u8], identities: &[Identity], seen: &mut BTreeSet<usize>) -> Result<Vec<u8>> {
    rewrite_xml(xml, |element, patches| {
        let marker =
            side(&attribute(element, b"sideChar")?.ok_or_else(|| anyhow!("쪽 번호 식별자 누락"))?)?;
        let index = token_index(marker, identities.len())?;
        ensure!(seen.insert(index), "중복 HWPX 쪽 번호 식별자");
        patch(
            xml,
            element,
            b"formatType",
            crate::auto_number_formats::format_token(identities[index].format)?.into(),
            patches,
        )?;
        patch(
            xml,
            element,
            b"sideChar",
            side_xml(identities[index].side)?,
            patches,
        )
    })
}

fn paragraphs(
    paras: &mut [Paragraph],
    depth: usize,
    visit: &mut impl FnMut(&mut PageNumberPos) -> Result<()>,
) -> Result<()> {
    ensure!(depth <= 64, "쪽 번호 컨트롤 중첩 깊이 제한 초과");
    for paragraph in paras {
        for control in &mut paragraph.controls {
            match control {
                Control::PageNumberPos(number) => visit(number)?,
                Control::Table(table) => {
                    for cell in &mut table.cells {
                        paragraphs(&mut cell.paragraphs, depth + 1, visit)?;
                    }
                    if let Some(caption) = &mut table.caption {
                        paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
                    }
                }
                Control::Header(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Footer(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Footnote(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Endnote(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::HiddenComment(h) => paragraphs(&mut h.paragraphs, depth + 1, visit)?,
                Control::Field(field) => paragraphs(&mut field.memo_paragraphs, depth + 1, visit)?,
                Control::Picture(picture) => {
                    if let Some(caption) = &mut picture.caption {
                        paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
                    }
                }
                Control::Shape(object) => shape(object, depth + 1, visit)?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn shape(
    object: &mut ShapeObject,
    depth: usize,
    visit: &mut impl FnMut(&mut PageNumberPos) -> Result<()>,
) -> Result<()> {
    ensure!(depth <= 64, "쪽 번호 도형 중첩 깊이 제한 초과");
    if let Some(drawing) = object.drawing_mut() {
        if let Some(text) = &mut drawing.text_box {
            paragraphs(&mut text.paragraphs, depth + 1, visit)?;
        }
        if let Some(caption) = &mut drawing.caption {
            paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
        }
    }
    match object {
        ShapeObject::Group(group) => {
            for child in &mut group.children {
                shape(child, depth + 1, visit)?;
            }
            if let Some(caption) = &mut group.caption {
                paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
            }
        }
        ShapeObject::Picture(picture) => {
            if let Some(caption) = &mut picture.caption {
                paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
            }
        }
        ShapeObject::Chart(chart) => {
            if let Some(caption) = &mut chart.caption {
                paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
            }
        }
        ShapeObject::Ole(ole) => {
            if let Some(caption) = &mut ole.caption {
                paragraphs(&mut caption.paragraphs, depth + 1, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn document(
    doc: &mut Document,
    mut visit: impl FnMut(&mut PageNumberPos) -> Result<()>,
) -> Result<()> {
    for section in &mut doc.sections {
        paragraphs(&mut section.paragraphs, 0, &mut visit)?;
        for master in &mut section.section_def.master_pages {
            paragraphs(&mut master.paragraphs, 0, &mut visit)?;
        }
    }
    Ok(())
}

/// PageNumberPos identities, marked and restored with the AutoNumber ones.
#[derive(Default)]
struct PageMarks(Vec<Identity>);

impl crate::auto_number_formats::Marks for PageMarks {
    fn mark(&mut self, xml: &[u8]) -> Result<Vec<u8>> {
        mark_xml(xml, &mut self.0)
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn restore(&self, doc: &mut Document) -> Result<()> {
        let identities = &self.0;
        let mut seen = BTreeSet::new();
        document(doc, |number| {
            let index = token_index(number.dash_char, identities.len())?;
            ensure!(seen.insert(index), "중복 네이티브 쪽 번호 식별자");
            number.format = identities[index].format;
            number.dash_char = identities[index].side;
            Ok(())
        })?;
        ensure!(
            seen.len() == identities.len(),
            "HWPX 쪽 번호 일부를 모델에서 찾지 못했습니다. 원본 보존을 위해 열기를 중단했습니다."
        );
        Ok(())
    }
}

pub(crate) fn open(bytes: &[u8], password: Option<&[u8]>) -> Result<DocumentCore> {
    crate::auto_number_formats::open_marked(bytes, password, &mut PageMarks::default())
}

pub(crate) fn export(core: &DocumentCore, password: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut doc = core.document().clone();
    let mut identities = Vec::new();
    document(&mut doc, |number| {
        crate::auto_number_formats::format_token(number.format)?;
        if number.user_symbol != '\0' || number.prefix_char != '\0' || number.suffix_char != '\0' {
            bail!(
                "HWP 전용 쪽 번호 사용자 문자·앞뒤 장식을 표준 HWPX pageNum에 보존할 수 없습니다. HWP로 저장하세요."
            );
        }
        side_xml(number.dash_char)?;
        let marker = token(identities.len())?;
        identities.push(Identity {
            format: number.format,
            side: number.dash_char,
        });
        number.dash_char = marker;
        Ok(())
    })?;
    if identities.is_empty() {
        return crate::auto_number_formats::export(core, password);
    }
    let baseline =
        crate::auto_number_formats::export(&crate::export_core(doc, core.source_format()), None)?;
    let mut seen = BTreeSet::new();
    let corrected = crate::auto_number_formats::rewrite_package(&baseline, |xml| {
        restore_xml(xml, &identities, &mut seen)
    })?;
    ensure!(
        seen.len() == identities.len(),
        "쪽 번호 일부를 HWPX에 저장하지 못했습니다. 기존 파일 보존을 위해 저장을 중단했습니다."
    );
    match password {
        Some(password) => hwpcraft_doc::password_crypto::encrypt_hwpx_package(&corrected, password)
            .map_err(Into::into),
        None => Ok(corrected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

    fn fixture(format: u8, dash: char) -> Result<DocumentCore> {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native()
            .map_err(|e| anyhow!("{e}"))?;
        core.insert_new_number_native(0, 0, 0, 1)
            .map_err(|e| anyhow!("{e}"))?;
        let index = core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|control| matches!(control, Control::NewNumber(_)))
            .ok_or_else(|| anyhow!("fixture number control missing"))?;
        core.document_mut().sections[0].paragraphs[0].controls[index] =
            Control::PageNumberPos(PageNumberPos {
                format,
                position: 6,
                dash_char: dash,
                ..Default::default()
            });
        core.document_mut().sections[0].paragraphs[0].control_mask &= !(1 << 0x12);
        core.document_mut().sections[0].paragraphs[0].control_mask |= 1 << 0x15;
        core.document_mut().sections[0].raw_stream = None;
        core.set_section_def_native(0, "{}")
            .map_err(|e| anyhow!("{e}"))?;
        Ok(core)
    }

    fn numbers(core: &DocumentCore) -> Result<Vec<(u8, u8, char)>> {
        let mut doc = core.document().clone();
        let mut values = Vec::new();
        document(&mut doc, |number| {
            values.push((number.format, number.position, number.dash_char));
            Ok(())
        })?;
        Ok(values)
    }

    #[test]
    fn canonical_page_shapes_and_empty_side_survive_plain_export_and_read() -> Result<()> {
        for format in [0, 1, 2, 3, 4, 5, 8, 10, 12, 13] {
            for dash in ['-', '\0'] {
                let core = fixture(format, dash)?;
                let before = format!("{:?}", core.document());
                let bytes = export(&core, None)?;
                let mut package = ZipArchive::new(Cursor::new(&bytes))?;
                let mut xml = String::new();
                package
                    .by_name("Contents/section0.xml")?
                    .read_to_string(&mut xml)?;
                assert!(xml.contains(&format!(
                    "formatType=\"{}\"",
                    crate::auto_number_formats::format_token(format)?
                )));
                if dash == '\0' {
                    assert!(xml.contains("sideChar=\"\""));
                }
                assert_eq!(numbers(&open(&bytes, None)?)?, vec![(format, 6, dash)]);
                assert_eq!(format!("{:?}", core.document()), before);
            }
        }
        Ok(())
    }

    #[test]
    fn encrypted_package_retains_page_and_body_auto_formats_without_double_encryption() -> Result<()>
    {
        let mut core = fixture(8, '\0')?;
        core.insert_auto_number_at_cursor(0, 0, 0, "total")
            .map_err(|e| anyhow!("{e}"))?;
        for control in &mut core.document_mut().sections[0].paragraphs[0].controls {
            if let Control::AutoNumber(number) = control {
                number.format = 13;
            }
        }
        let before = format!("{:?}", core.document());
        let bytes = export(&core, Some(b"own-12345"))?;
        let opened = open(&bytes, Some(b"own-12345"))?;
        assert_eq!(numbers(&opened)?, vec![(8, 6, '\0')]);
        assert!(
            opened.document().sections[0].paragraphs[0]
                .controls
                .iter()
                .any(|control| matches!(control,Control::AutoNumber(number) if number.format==13))
        );
        assert!(crate::needs_password(
            &open(&bytes, None)
                .err()
                .ok_or_else(|| anyhow!("missing password accepted"))?
        ));
        assert!(crate::wrong_password(
            &open(&bytes, Some(b"wrong"))
                .err()
                .ok_or_else(|| anyhow!("wrong password accepted"))?
        ));
        assert_eq!(format!("{:?}", core.document()), before);
        Ok(())
    }

    /// Page numbers and body AutoNumbers are marked in one rewrite and restored
    /// in one parse; the reopened model saves to the same bytes, and a package
    /// without identities opens exactly as the native parser reads it.
    #[test]
    fn one_parse_restores_both_identity_kinds_and_resaves_identically() -> Result<()> {
        let mut core = fixture(3, '-')?;
        core.insert_auto_number_at_cursor(0, 0, 0, "total")
            .map_err(|e| anyhow!("{e}"))?;
        for control in &mut core.document_mut().sections[0].paragraphs[0].controls {
            if let Control::AutoNumber(number) = control {
                number.format = 12;
            }
        }
        let bytes = export(&core, None)?;
        let opened = open(&bytes, None)?;
        assert_eq!(numbers(&opened)?, vec![(3, 6, '-')]);
        assert!(
            opened.document().sections[0].paragraphs[0]
                .controls
                .iter()
                .any(
                    |control| matches!(control, Control::AutoNumber(number) if number.format == 12)
                )
        );
        assert_eq!(export(&opened, None)?, bytes);

        let mut blank = DocumentCore::new_empty();
        blank
            .create_blank_document_native()
            .map_err(|e| anyhow!("{e}"))?;
        let bytes = export(&blank, None)?;
        let native = DocumentCore::from_bytes(&bytes).map_err(|e| anyhow!("{e}"))?;
        assert_eq!(
            format!("{:?}", open(&bytes, None)?.document()),
            format!("{:?}", native.document())
        );
        Ok(())
    }

    #[test]
    fn xml_attribute_patch_preserves_opaque_entries_and_unrelated_bytes() -> Result<()> {
        let xml=br#"<s xmlns:p='http://www.hancom.co.kr/hwpml/2011/paragraph'><!--keep--><p:pageNum pos='TOP_LEFT' formatType='HANGUL_SYLLABLE' sideChar=''/><p:pageNum sideChar='&amp;' formatType='ROMAN_SMALL' pos='BOTTOM_RIGHT'/><other v='untouched'/></s>"#;
        let mut package = ZipWriter::new(Cursor::new(Vec::new()));
        package.set_comment("own metadata")?;
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        package.start_file("Contents/section0.xml", options)?;
        package.write_all(xml)?;
        package.start_file("opaque/keep.bin", options)?;
        package.write_all(&[0, 255, 1, 128, 9])?;
        let before = package.finish()?.into_inner();
        let mut identities = Vec::new();
        let marked = crate::auto_number_formats::rewrite_package(&before, |xml| {
            mark_xml(xml, &mut identities)
        })?;
        let mut seen = BTreeSet::new();
        let after = crate::auto_number_formats::rewrite_package(&marked, |xml| {
            restore_xml(xml, &identities, &mut seen)
        })?;
        assert_eq!(seen.len(), 2);
        let mut source = ZipArchive::new(Cursor::new(&before))?;
        let mut restored = ZipArchive::new(Cursor::new(&after))?;
        assert_eq!(source.comment(), restored.comment());
        let mut actual = Vec::new();
        restored
            .by_name("Contents/section0.xml")?
            .read_to_end(&mut actual)?;
        assert_eq!(actual, xml);
        let original = source.by_name("opaque/keep.bin")?;
        let copied = restored.by_name("opaque/keep.bin")?;
        assert_eq!(original.compressed_size(), copied.compressed_size());
        let length = usize::try_from(original.compressed_size())?;
        let original_start = usize::try_from(
            original
                .data_start()
                .ok_or_else(|| anyhow!("original ZIP data missing"))?,
        )?;
        let copied_start = usize::try_from(
            copied
                .data_start()
                .ok_or_else(|| anyhow!("copied ZIP data missing"))?,
        )?;
        assert_eq!(
            &before[original_start..][..length],
            &after[copied_start..][..length]
        );
        Ok(())
    }

    #[test]
    fn missing_optional_attributes_and_xml_special_side_char_roundtrip() -> Result<()> {
        let xml=br#"<s xmlns:p="http://www.hancom.co.kr/hwpml/2011/paragraph"><p:pageNum pos="TOP_LEFT"/></s>"#;
        let mut identities = Vec::new();
        let marked = mark_xml(xml, &mut identities)?;
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].side, '\0');
        identities[0].side = '"';
        let restored = restore_xml(&marked, &identities, &mut BTreeSet::new())?;
        let mut check = Vec::new();
        mark_xml(&restored, &mut check)?;
        assert_eq!(check[0].side, '"');
        assert_eq!(check[0].format, 0);
        assert_eq!(side_xml('\n')?, "&#10;");
        assert!(side_xml('\u{ffff}').is_err());
        Ok(())
    }

    #[test]
    fn invalid_namespace_token_duplicates_and_unrepresentable_decorations_fail_closed() -> Result<()>
    {
        assert!(
            mark_xml(
                br#"<p:pageNum xmlns:p="urn:other" formatType="DIGIT"/>"#,
                &mut Vec::new()
            )
            .is_err()
        );
        assert!(token(TOKEN_COUNT).is_err());
        let marker = token(0)?;
        let duplicate = format!(
            r#"<s xmlns:p="http://www.hancom.co.kr/hwpml/2011/paragraph"><p:pageNum formatType="DIGIT" sideChar="{marker}"/><p:pageNum formatType="DIGIT" sideChar="{marker}"/></s>"#
        );
        assert!(
            restore_xml(
                duplicate.as_bytes(),
                &[Identity {
                    format: 8,
                    side: '-'
                }],
                &mut BTreeSet::new()
            )
            .is_err()
        );
        for field in ["user", "prefix", "suffix"] {
            let mut core = fixture(8, '-')?;
            document(core.document_mut(), |number| {
                match field {
                    "user" => number.user_symbol = '※',
                    "prefix" => number.prefix_char = '[',
                    _ => number.suffix_char = ']',
                };
                Ok(())
            })?;
            let before = format!("{:?}", core.document());
            assert!(export(&core, None).is_err());
            assert_eq!(format!("{:?}", core.document()), before);
        }
        Ok(())
    }

    #[test]
    fn hwp_reader_is_unchanged_and_failed_export_preserves_existing_output() -> Result<()> {
        let mut core = fixture(13, '\0')?;
        let hwp = core.export_hwp_native().map_err(|e| anyhow!("{e}"))?;
        assert_eq!(numbers(&open(&hwp, None)?)?, vec![(13, 6, '\0')]);
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("preserve.hwpx");
        std::fs::write(&path, b"existing output")?;
        document(core.document_mut(), |number| {
            number.prefix_char = '[';
            Ok(())
        })?;
        assert!(crate::save(&core, &path).is_err());
        assert_eq!(std::fs::read(path)?, b"existing output");
        Ok(())
    }
}
