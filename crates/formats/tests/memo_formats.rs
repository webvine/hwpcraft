use hwpcraft_doc::model::control::{Control, FieldType, FormObject};
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_doc::{DocumentCore, HwpRecord, HyperlinkTarget};

fn memo_document(trailing_empty: bool) -> anyhow::Result<DocumentCore> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut doc = core.document().clone();
    doc.sections[0].paragraphs[0].insert_text_at(0, "검토 본문");
    doc.sections[0].raw_stream = None;
    if trailing_empty {
        doc.sections[0].paragraphs.push(Paragraph::new_empty());
    }
    core.set_document(doc);
    let id = core
        .insert_hyperlink_native(
            &HyperlinkTarget::body(0, 0),
            0,
            2,
            "https://invalid.invalid/field-anchor",
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut doc = core.document().clone();
    let mut content = Paragraph::new_empty();
    content.insert_text_at(0, "검토 메모");
    for control in &mut doc.sections[0].paragraphs[0].controls {
        if let Control::Field(field) = control
            && field.field_id == id
        {
            field.field_type = FieldType::Memo;
            field.ctrl_id = 0x2525_6d65;
            field.command = format!("MEMO/65535/1/{id}/{id}/검토자/\\;;");
            field.properties = 0x8001;
            field.memo_index = 1;
            field.memo_paragraphs = vec![content.clone()];
        }
    }
    doc.sections[0].raw_stream = None;
    core.set_document(doc);
    Ok(core)
}

fn assert_container(core: &DocumentCore, expected: usize) -> anyhow::Result<()> {
    let section = &core.document().sections[0];
    assert_eq!(section.paragraphs.len(), expected);
    let records = HwpRecord::read_all(section.raw_stream.as_ref().unwrap())?;
    assert_eq!(
        records
            .iter()
            .filter(|r| r.tag_id == 66 && r.level == 0)
            .count(),
        expected
    );
    assert_eq!(records.iter().filter(|r| r.tag_id == 93).count(), 1);
    assert!(records.iter().any(|r| {
        r.tag_id == 67
            && String::from_utf16_lossy(
                &r.data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect::<Vec<_>>(),
            )
            .contains("검토 메모")
    }));
    assert_eq!(section.paragraphs[0].text, "검토 본문");
    Ok(())
}

#[test]
fn memo_save_preserves_a_genuine_empty_body_paragraph() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for trailing in [false, true] {
        let core = memo_document(trailing)?;
        let expected = if trailing { 2 } else { 1 };
        let path = dir.path().join(format!("memo-{trailing}.hwp"));
        hwpcraft_formats::save(&core, &path)?;
        let reopened =
            DocumentCore::from_bytes(&std::fs::read(&path)?).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_container(&reopened, expected)?;
        assert_eq!(core.document().sections[0].paragraphs.len(), expected);
        let memo = core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .find_map(|c| match c {
                Control::Field(f) if f.field_type == FieldType::Memo => Some(f),
                _ => None,
            })
            .unwrap();
        assert_eq!(memo.memo_paragraphs[0].text, "검토 메모");
        // Cached native bytes must retain that genuine empty paragraph too.
        hwpcraft_formats::save(&reopened, &path)?;
        let twice =
            DocumentCore::from_bytes(&std::fs::read(&path)?).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_container(&twice, expected)?;
    }
    Ok(())
}

#[test]
fn memo_container_correction_precedes_password_encryption() -> anyhow::Result<()> {
    let core = memo_document(true)?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("encrypted-memo.hwp");
    hwpcraft_formats::save_with_password(&core, &path, "self-created-fixture-password")?;
    let bytes = std::fs::read(path)?;
    assert!(DocumentCore::from_bytes(&bytes).is_err());
    let reopened = DocumentCore::from_bytes_with_password(&bytes, b"self-created-fixture-password")
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_container(&reopened, 2)?;
    assert_eq!(core.document().sections[0].paragraphs.len(), 2);
    Ok(())
}

#[test]
fn memo_correction_keeps_native_form_identity_assignment() -> anyhow::Result<()> {
    let mut core = memo_document(false)?;
    let mut doc = core.document().clone();
    for name in ["first", "second"] {
        doc.sections[0].paragraphs[0]
            .controls
            .push(Control::Form(Box::new(FormObject {
                name: name.into(),
                width: 3000,
                height: 1200,
                enabled: true,
                ..Default::default()
            })));
    }
    core.set_document(doc);
    let baseline = DocumentCore::from_bytes(
        &core
            .export_hwp_with_adapter_snapshot()
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let identities = |core: &DocumentCore| {
        core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .filter_map(|c| match c {
                Control::Form(form) => Some(form.common.instance_id),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let expected = identities(&baseline);
    assert_eq!(expected.len(), 2);
    assert_ne!(expected[0], expected[1]);
    assert!(expected.iter().all(|id| *id != 0));
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("memo-with-forms.hwp");
    hwpcraft_formats::save(&core, &path)?;
    let reopened =
        DocumentCore::from_bytes(&std::fs::read(path)?).map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(identities(&reopened), expected);
    assert_eq!(
        identities(&core),
        [0, 0],
        "export cannot assign IDs in the live document"
    );
    assert_container(&reopened, 1)?;
    Ok(())
}
