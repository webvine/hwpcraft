use hwpcraft_doc::model::control::{Control, Parameter};
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::{Value, json};

#[test]
fn author_update_preserves_other_native_field_metadata() -> anyhow::Result<()> {
    let mut s = document("메모 기준")?;
    select(&mut s, 0, 2)?;
    let created = create(&mut s, "서식 있는 메모")?;
    let id = created["id"].as_u64().unwrap() as u32;
    for control in &mut s.core.document_mut().sections[0].paragraphs[0].controls {
        if let Control::Field(field) = control
            && field.field_id == id
        {
            field.parameters.items.push(Parameter::String {
                name: Some("CreateDateTime".into()),
                value: "2026-10-25T12:00:00Z".into(),
                preserve_space: true,
            });
            field.parameters.items.push(Parameter::String {
                name: Some("OtherApplicationParameter".into()),
                value: "keep & preserve".into(),
                preserve_space: true,
            });
            field.instance_id = Some(12345);
            field.memo_text_direction = Some("VERTICAL".into());
        }
    }
    s.run("memo.update", json!({"id":id,"author":"변경 작성자"}))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("metadata.hwpx");
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    let field = r.core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Field(f) if f.field_id == id => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(field.instance_id, Some(12345));
    assert_eq!(field.memo_text_direction.as_deref(), Some("VERTICAL"));
    assert!(field.parameters.items.iter().any(|p|matches!(p,Parameter::String{name,value,..} if name.as_deref()==Some("OtherApplicationParameter")&&value=="keep & preserve")));
    assert!(field.parameters.items.iter().any(|p|matches!(p,Parameter::String{name,value,..} if name.as_deref()==Some("CreateDateTime")&&value=="2026-10-25T12:00:00Z")));
    assert_eq!(
        r.run("memo.get", json!({"id":id}))?["author"],
        "변경 작성자"
    );
    Ok(())
}

#[test]
fn stale_memo_edits_are_rejected_before_mutation() -> anyhow::Result<()> {
    let mut s = document("공동 검토")?;
    select(&mut s, 0, 2)?;
    let created = create(&mut s, "초기 메모")?;
    let id = created["id"].clone();
    let loaded = created["generation"].clone();
    s.run(
        "memo.update",
        json!({"id":id,"text":"다른 변경","expected_generation":loaded}),
    )?;
    let generation = s.generation;
    let before = s.run("memo.get", json!({"id":id}))?;
    for (command, params) in [
        (
            "memo.update",
            json!({"id":id,"text":"오래된 편집","expected_generation":loaded}),
        ),
        ("memo.delete", json!({"id":id,"expected_generation":loaded})),
        (
            "memo.create",
            json!({"text":"오래된 선택","expected_generation":loaded}),
        ),
    ] {
        let error = s.run(command, params).unwrap_err();
        assert!(
            error.to_string().contains("변경되었습니다"),
            "{command}: {error}"
        );
        assert_eq!(s.generation, generation);
        assert_eq!(s.run("memo.get", json!({"id":id}))?, before);
    }
    Ok(())
}

fn at(paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}
fn document(text: &str) -> anyhow::Result<Session> {
    let mut session = Session::new()?;
    session.execute(Command::Insert(text.into()))?;
    Ok(session)
}
fn select(session: &mut Session, start: usize, end: usize) -> anyhow::Result<()> {
    session.select(at(0, start), false)?;
    session.select(at(0, end), true)
}
fn create(session: &mut Session, text: &str) -> anyhow::Result<Value> {
    session.run("memo.create", json!({"text":text,"author":"검토자"}))
}

#[test]
fn memo_range_content_author_and_removal_are_transactional() -> anyhow::Result<()> {
    let mut s = document("첫째😀 문장\n둘째 문장")?;
    select(&mut s, 0, 4)?;
    let before_text = s.text().to_owned();
    let before_format = s.character_properties()?;
    let created = create(&mut s, "첫 검토\n두 번째 줄 <내용> & \"따옴표\"")?;
    let id = created["id"].as_u64().unwrap();
    assert_eq!(created["anchor"], "첫째😀 ");
    assert_eq!(created["author"], "검토자");
    assert_eq!(created["text"], "첫 검토\n두 번째 줄 <내용> & \"따옴표\"");
    assert_eq!(s.text(), before_text);
    assert_eq!(s.character_properties()?, before_format);
    let updated = s.run(
        "memo.update",
        json!({"id":id,"text":"새 본문 😀","author":"다른 작성자"}),
    )?;
    assert_eq!(updated["author"], "다른 작성자");
    assert_eq!(updated["text"], "새 본문 😀");
    s.execute(Command::Undo)?;
    assert_eq!(s.run("memo.get", json!({"id":id}))?["author"], "검토자");
    s.execute(Command::Redo)?;
    s.run("memo.delete", json!({"id":id}))?;
    assert_eq!(s.text(), before_text);
    assert_eq!(s.character_properties()?, before_format);
    assert!(
        s.run("memo.list", json!({}))?
            .as_array()
            .unwrap()
            .is_empty()
    );
    s.execute(Command::Undo)?;
    assert_eq!(s.run("memo.get", json!({"id":id}))?["text"], "새 본문 😀");
    Ok(())
}

#[test]
fn memo_two_anchors_content_and_author_roundtrip_both_formats() -> anyhow::Result<()> {
    let mut s = document("검토할 첫 문장\n검토할 둘째 문장")?;
    select(&mut s, 0, 3)?;
    let first = create(&mut s, "첫 메모 😀\n둘째 문단\n")?;
    s.select(at(1, 4), false)?;
    s.select(at(1, 6), true)?;
    let second = s.run(
        "memo.create",
        json!({"text":"두 번째 메모","author":"공동 작성자"}),
    )?;
    assert_ne!(first["id"], second["id"]);
    assert_ne!(first["index"], second["index"]);
    // Native Hancom silently discards the body for a malformed synthetic
    // MemoShape, and needs %%me in the text begin marker even though the field
    // header is %unk. These contracts require independent native validation.
    assert_eq!(s.core.document().doc_info.memo_shape_count, 0);
    assert!(
        !s.core
            .document()
            .doc_info
            .extra_records
            .iter()
            .any(|r| r.tag_id == 92)
    );
    for paragraph in &s.core.document().sections[0].paragraphs {
        for control in &paragraph.controls {
            if let Control::Field(field) = control {
                assert_eq!(field.ctrl_id, 0x2525_6d65);
                assert_eq!(field.properties & 0x8001, 0x8001);
                assert!(field.command.starts_with("MEMO/65535/"));
            }
        }
    }
    let dir = tempfile::tempdir()?;
    for extension in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("comments.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(
            reopened.core.document().sections[0].paragraphs.len(),
            2,
            "{extension}: memo tail must not become body paragraphs"
        );
        let items = reopened.run("memo.list", json!({}))?;
        assert_eq!(items.as_array().unwrap().len(), 2, "{extension}");
        for expected in [&first, &second] {
            let actual = reopened.run("memo.get", json!({"id":expected["id"]}))?;
            for key in ["id", "index", "author", "anchor", "text", "start", "end"] {
                assert_eq!(actual[key], expected[key], "{extension}: {key}");
            }
        }
        // Saving an already reopened memo must not append another editable tail.
        reopened.save(&path)?;
        let mut twice = Session::new()?;
        twice.open(&path)?;
        assert_eq!(
            twice.core.document().sections[0].paragraphs.len(),
            2,
            "{extension}"
        );
        assert_eq!(
            twice.run("memo.list", json!({}))?.as_array().unwrap().len(),
            2
        );
    }
    if std::env::var_os("HWPCRAFT_WRITE_MEMO_FIXTURES").is_some() {
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.16/fixtures");
        std::fs::create_dir_all(&output)?;
        for extension in ["hwp", "hwpx"] {
            s.save(&output.join(format!("memo-native.{extension}")))?;
        }
    }
    // Optional independent oracle: a new isolated Hancom instance imports the
    // self-created fixture through the public memory API and exports it again.
    // Keep that local evidence out of tracked fixtures and normal test runs.
    if let Some(path) = std::env::var_os("HWPCRAFT_NATIVE_MEMO_ROUNDTRIP") {
        let mut native = Session::new()?;
        native.open(std::path::Path::new(&path))?;
        assert_eq!(native.core.document().sections[0].paragraphs.len(), 2);
        for expected in [&first, &second] {
            let actual = native.run("memo.get", json!({"id":expected["id"]}))?;
            for key in ["index", "author", "anchor", "text", "start", "end"] {
                assert_eq!(actual[key], expected[key], "native Hancom roundtrip: {key}");
            }
        }
    }
    Ok(())
}

#[test]
fn empty_caret_memo_does_not_insert_or_remove_anchor_text() -> anyhow::Result<()> {
    let mut s = document("본문😀")?;
    let caret = s.cursor;
    let created = create(&mut s, "")?;
    let id = created["id"].clone();
    assert_eq!(created["start"], created["end"]);
    assert_eq!(created["text"], "");
    assert_eq!(s.text(), "본문😀");
    assert_eq!(s.cursor, caret);
    let dir = tempfile::tempdir()?;
    for extension in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("caret.{extension}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let memo = r.run("memo.get", json!({"id":id}))?;
        assert_eq!(memo["start"], memo["end"], "{extension}");
        assert_eq!(memo["text"], "", "{extension}");
        assert_eq!(r.text(), "본문😀");
    }
    s.run("memo.delete", json!({"id":id}))?;
    assert_eq!(s.text(), "본문😀");
    Ok(())
}

#[test]
fn memo_follows_native_anchor_edits_and_undo() -> anyhow::Result<()> {
    let mut s = document("가나다라")?;
    select(&mut s, 1, 3)?;
    let created = create(&mut s, "범위")?;
    let id = created["id"].clone();
    s.select(at(0, 2), false)?;
    s.execute(Command::Insert("😀".into()))?;
    assert_eq!(s.run("memo.get", json!({"id":id}))?["anchor"], "나😀다");
    s.execute(Command::Undo)?;
    assert_eq!(s.run("memo.get", json!({"id":id}))?["anchor"], "나다");
    Ok(())
}

#[test]
fn unsupported_memo_requests_fail_without_mutating_document() -> anyhow::Result<()> {
    let mut s = document("본문\n둘째")?;
    let generation = s.generation;
    let document_before = s.info();
    for (command, params) in [
        ("memo.create", json!({"text":"메모","author":"나/다"})),
        ("memo.create", json!({"text":"메모","replyTo":1})),
        ("memo.update", json!({"id":999})),
        ("memo.update", json!({"id":999,"text":"없는 메모"})),
        ("memo.delete", json!({"id":999})),
        ("memo.list", json!({"unused":true})),
    ] {
        assert!(s.run(command, params).is_err(), "{command}");
        assert_eq!(s.generation, generation, "{command}");
        assert_eq!(s.info(), document_before, "{command}");
    }
    s.select(at(0, 0), false)?;
    s.select(at(1, 1), true)?;
    let before = s.selected_text();
    assert!(create(&mut s, "다문단 범위").is_err());
    assert_eq!(s.selected_text(), before);
    assert!(
        s.run("memo.list", json!({}))?
            .as_array()
            .unwrap()
            .is_empty()
    );
    Ok(())
}

#[test]
fn memo_edits_relayout_only_the_anchor_paragraph_with_the_same_geometry() -> anyhow::Result<()> {
    // Memo create/update rebuild the anchor paragraph's section instead of the whole
    // document; the visible layout must match a full rebuild.
    let mut s = document("앞 문단\n메모를 달 문장입니다 😀 끝\n뒤 문단")?;
    s.select(at(1, 0), false)?;
    s.select(at(1, 5), true)?;
    let pages = s.page_count()?;
    let end = s.caret_at(at(1, 12))?;
    let after_text = s.caret_at(at(2, 3))?;
    let created = create(&mut s, "검토 메모")?;
    let id = created["id"].clone();
    assert_eq!(created["anchor"], "메모를 달");
    assert_eq!(s.page_count()?, pages);
    assert_eq!(s.caret_at(at(1, 12))?, end);
    assert_eq!(s.caret_at(at(2, 3))?, after_text);
    s.run(
        "memo.update",
        json!({"id":id,"text":"고친 메모","author":"다른 사람"}),
    )?;
    assert_eq!(s.caret_at(at(1, 12))?, end);
    assert_eq!(s.caret_at(at(2, 3))?, after_text);
    // An empty caret memo starts as a click-here field; its paragraph is re-laid out too.
    s.select(at(2, 1), false)?;
    let caret = create(&mut s, "커서 메모")?;
    assert_eq!(caret["anchor"], "");
    assert_eq!(s.caret_at(at(2, 3))?, after_text);
    for ext in ["hwp", "hwpx"] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(format!("memo.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let list = r.run("memo.list", json!({}))?;
        let texts: Vec<&str> = list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["text"].as_str())
            .collect();
        assert_eq!(texts, ["고친 메모", "커서 메모"], "{ext}");
    }
    Ok(())
}
