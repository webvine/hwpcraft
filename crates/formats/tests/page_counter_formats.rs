use anyhow::{Result, anyhow};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use hwpcraft_doc::{
    DocumentCore, HwpRecord,
    model::{
        control::{AutoNumber, AutoNumberType, Control},
        paragraph::Paragraph,
    },
    password_crypto::{decrypt_hwp5_stream, encrypt_hwp5_stream},
};
use std::io::{Cursor, Read, Write};

fn field(kind: AutoNumberType, number: u16) -> Paragraph {
    Paragraph {
        text: "\u{fffc}".into(),
        char_offsets: vec![0],
        char_count: 9,
        has_para_text: true,
        controls: vec![Control::AutoNumber(AutoNumber {
            number_type: kind,
            number,
            assigned_number: number,
            user_symbol: '★',
            prefix_char: '[',
            suffix_char: ']',
            ..Default::default()
        })],
        ..Default::default()
    }
}
fn source(start: u16, count: usize, restart: bool) -> Result<DocumentCore> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .map_err(|e| anyhow!("{e}"))?;
    core.insert_text_native(0, 0, 0, "자체 번호")
        .map_err(|e| anyhow!("{e}"))?;
    if restart {
        core.insert_new_number_native(0, 0, 5, start)
            .map_err(|e| anyhow!("{e}"))?;
    }
    let mut doc = core.document().clone();
    doc.sections[0].section_def.page_num = if restart { 0 } else { start };
    for c in &mut doc.sections[0].paragraphs[0].controls {
        if let Control::SectionDef(sd) = c {
            sd.page_num = if restart { 0 } else { start };
        }
    }
    let template = doc.sections[0].paragraphs[0].clone();
    let stride = template
        .line_segs
        .last()
        .map_or(1600, |line| line.line_height + line.line_spacing)
        .max(1);
    for index in 0..count {
        let mut paragraph = field(AutoNumberType::Page, 0);
        paragraph.line_segs = template.line_segs.clone();
        for line in &mut paragraph.line_segs {
            line.text_start = 0;
            line.vertical_pos = stride * (index as i32 + 1);
        }
        paragraph.char_shapes = template.char_shapes.clone();
        for shape in &mut paragraph.char_shapes {
            shape.start_pos = 0;
        }
        doc.sections[0].paragraphs.push(paragraph);
    }
    doc.sections[0].raw_stream = None;
    core.set_document(doc);
    Ok(core)
}
fn numbers(core: &DocumentCore) -> Vec<(AutoNumberType, u16, u16)> {
    core.document()
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .flat_map(|p| &p.controls)
        .filter_map(|c| {
            if let Control::AutoNumber(n) = c {
                Some((n.number_type, n.number, n.assigned_number))
            } else {
                None
            }
        })
        .collect()
}
fn raw_stream(path: &std::path::Path, name: &str, password: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut cfb = cfb::CompoundFile::open(Cursor::new(std::fs::read(path)?))?;
    let mut header = Vec::new();
    cfb.open_stream("/FileHeader")?.read_to_end(&mut header)?;
    let flags = u32::from_le_bytes(header[36..40].try_into()?);
    let mut stored = Vec::new();
    cfb.open_stream(name)?.read_to_end(&mut stored)?;
    let stored = if flags & 2 != 0 {
        decrypt_hwp5_stream(&stored, password.unwrap())
    } else {
        stored
    };
    if flags & 1 == 0 {
        return Ok(stored);
    }
    let mut raw = Vec::new();
    DeflateDecoder::new(stored.as_slice()).read_to_end(&mut raw)?;
    Ok(raw)
}
fn transport(path: &std::path::Path, compressed: bool, password: Option<&[u8]>) -> Result<()> {
    let bytes = std::fs::read(path)?;
    let mut cfb = cfb::CompoundFile::open(Cursor::new(bytes))?;
    let paths = cfb
        .walk()
        .filter(|e| {
            e.is_stream()
                && (e.path() == std::path::Path::new("/DocInfo")
                    || e.path().parent() == Some(std::path::Path::new("/BodyText")))
        })
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let raws = paths
        .iter()
        .map(|name| raw_stream(path, name, password))
        .collect::<Result<Vec<_>>>()?;
    for (name, raw) in paths.iter().zip(raws) {
        let stored = if compressed {
            let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
            e.write_all(&raw)?;
            e.finish()?
        } else {
            raw
        };
        let stored = password.map_or_else(|| stored.clone(), |p| encrypt_hwp5_stream(&stored, p));
        let mut s = cfb.open_stream(name)?;
        s.set_len(stored.len() as u64)?;
        s.write_all(&stored)?;
    }
    let mut header = Vec::new();
    cfb.open_stream("/FileHeader")?.read_to_end(&mut header)?;
    let mut flags = u32::from_le_bytes(header[36..40].try_into()?);
    flags = (flags & !1) | u32::from(compressed);
    header[36..40].copy_from_slice(&flags.to_le_bytes());
    cfb.open_stream("/FileHeader")?.write_all(&header)?;
    cfb.flush()?;
    std::fs::write(path, cfb.into_inner().into_inner())?;
    Ok(())
}

#[test]
fn valid_high_page_starts_reopen_and_keep_stored_page_numbers_exact() -> Result<()> {
    let dir = tempfile::tempdir()?;
    for (start, count, restart) in [
        (65535, 2, false),
        (65530, 7, false),
        (65530, 7, true),
        (65530, 90, true),
    ] {
        let core = source(start, count, restart)?;
        let path = dir.path().join(format!("{start}-{count}-{restart}.hwp"));
        hwpcraft_formats::save(&core, &path)?;
        let before = std::fs::read(&path)?;
        let original = raw_stream(&path, "/BodyText/Section0", None)?;
        let opened = hwpcraft_formats::open(&path)?;
        assert_eq!(numbers(&opened), vec![(AutoNumberType::Page, 0, 0); count]);
        assert_eq!(
            opened.document().sections[0].raw_stream.as_ref(),
            Some(&original)
        );
        assert!(opened.document().sections[0].raw_provenance_permits_reuse());
        assert_eq!(std::fs::read(&path)?, before);
        if restart {
            assert!(
                opened.document().sections[0].paragraphs[0]
                    .controls
                    .iter()
                    .any(|c| matches!(c,Control::NewNumber(n) if n.number==start))
            );
        } else {
            assert_eq!(opened.document().sections[0].section_def.page_num, start);
        }
        let again = dir.path().join("again.hwp");
        hwpcraft_formats::save(&opened, &again)?;
        let payloads = |raw: &[u8]| -> Result<Vec<Vec<u8>>> {
            Ok(HwpRecord::read_all(raw)
                .map_err(|e| anyhow!("{e}"))?
                .into_iter()
                .filter(|record| record.tag_id == 71 && record.data.get(..4) == Some(b"onta"))
                .map(|record| record.data)
                .collect())
        };
        // Existing output preparation may rebuild paragraph line caches; every
        // persisted field payload still has its original kind/num/decorations.
        assert_eq!(
            payloads(&raw_stream(&again, "/BodyText/Section0", None)?)?,
            payloads(&original)?
        );
        assert_eq!(numbers(&hwpcraft_formats::open(&again)?), numbers(&opened));
    }
    Ok(())
}

#[test]
fn encrypted_compressed_and_uncompressed_inputs_preserve_error_and_raw_contracts() -> Result<()> {
    let dir = tempfile::tempdir()?;
    for compressed in [false, true] {
        for encrypted in [false, true] {
            let path = dir
                .path()
                .join(format!("boundary-{compressed}-{encrypted}.hwp"));
            let core = source(65535, 2, false)?;
            let key = encrypted.then_some(b"own-fixture-password".as_slice());
            if encrypted {
                hwpcraft_formats::save_with_password(&core, &path, "own-fixture-password")?;
            } else {
                hwpcraft_formats::save(&core, &path)?;
            }
            transport(&path, compressed, key)?;
            let before = std::fs::read(&path)?;
            let original = raw_stream(&path, "/BodyText/Section0", key)?;
            let doc_info = raw_stream(&path, "/DocInfo", key)?;
            let opened = if encrypted {
                assert!(hwpcraft_formats::needs_password(
                    &hwpcraft_formats::open(&path).err().unwrap()
                ));
                assert!(hwpcraft_formats::wrong_password(
                    &hwpcraft_formats::open_with_password(&path, "wrong")
                        .err()
                        .unwrap()
                ));
                hwpcraft_formats::open_with_password(&path, "own-fixture-password")?
            } else {
                hwpcraft_formats::open(&path)?
            };
            assert_eq!(numbers(&opened), vec![(AutoNumberType::Page, 0, 0); 2]);
            assert_eq!(
                opened.document().sections[0].raw_stream.as_ref(),
                Some(&original)
            );
            assert_eq!(
                opened.document().doc_info.raw_stream.as_ref(),
                Some(&doc_info)
            );
            assert_eq!(std::fs::read(path)?, before);
        }
    }
    Ok(())
}

#[test]
fn total_page_fields_cannot_be_confused_with_page_tokens_and_hwpx_stays_unchanged() -> Result<()> {
    let mut core = source(65535, 2, false)?;
    let mut doc = core.document().clone();
    doc.sections[0]
        .paragraphs
        .insert(2, field(AutoNumberType::TotalPage, 17));
    doc.sections[0]
        .paragraphs
        .push(field(AutoNumberType::Picture, 0));
    core.set_document(doc);
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("mixed.{extension}"));
        hwpcraft_formats::save(&core, &path)?;
        let before = std::fs::read(&path)?;
        let opened = hwpcraft_formats::open(&path)?;
        let values = numbers(&opened);
        assert_eq!(
            values
                .iter()
                .filter(|n| n.0 == AutoNumberType::Page)
                .map(|n| n.1)
                .collect::<Vec<_>>(),
            [0, 0]
        );
        assert!(
            values
                .iter()
                .any(|n| n.0 == AutoNumberType::TotalPage && n.1 == 17)
        );
        if extension == "hwp" {
            assert!(
                values
                    .iter()
                    .any(|n| n.0 == AutoNumberType::Picture && n.1 == 1)
            );
        }
        assert_eq!(std::fs::read(&path)?, before);
        let mut next = opened;
        let result = next
            .insert_footnote_native(0, 0, 0)
            .map_err(|e| anyhow!("{e}"))?;
        assert!(result.contains("\"footnoteNumber\":1"), "{result}");
    }
    Ok(())
}

#[test]
fn numeric_section_order_and_opaque_metadata_are_preserved() -> Result<()> {
    let mut core = source(65530, 7, false)?;
    let mut doc = core.document().clone();
    let template = doc.sections[0].clone();
    doc.sections = (0..12)
        .map(|index| {
            let mut sec = template.clone();
            sec.section_def.page_num = 65520 + index;
            for c in &mut sec.paragraphs[0].controls {
                if let Control::SectionDef(sd) = c {
                    sd.page_num = 65520 + index;
                }
            }
            sec
        })
        .collect();
    core.set_document(doc);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("many-sections.hwp");
    hwpcraft_formats::save(&core, &path)?;
    let mut cfb = cfb::CompoundFile::open(Cursor::new(std::fs::read(&path)?))?;
    cfb.create_storage_all("/Opaque")?;
    cfb.create_stream("/Opaque/original-meta")?
        .write_all(&[1, 3, 5, 7, 0, 255, 19])?;
    cfb.set_state_bits("/Opaque/original-meta", 0x1234)?;
    cfb.flush()?;
    std::fs::write(&path, cfb.into_inner().into_inner())?;
    let before = std::fs::read(&path)?;
    let opened = hwpcraft_formats::open(&path)?;
    assert_eq!(opened.document().sections.len(), 12);
    for (index, sec) in opened.document().sections.iter().enumerate() {
        assert_eq!(sec.section_def.page_num, 65520 + index as u16);
        assert_eq!(
            sec.raw_stream.as_ref(),
            Some(&raw_stream(
                &path,
                &format!("/BodyText/Section{index}"),
                None
            )?)
        );
    }
    assert!(
        opened
            .document()
            .extra_streams
            .iter()
            .any(|(name, data)| name.ends_with("Opaque/original-meta")
                && data == &[1, 3, 5, 7, 0, 255, 19])
    );
    assert_eq!(std::fs::read(path)?, before);
    Ok(())
}
