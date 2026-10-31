//! 색인: index marks as Hangul stores them and the index made from them.
use hwpcraft_engine::{Cursor, Session};
use serde_json::{Value, json};

fn texts(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect()
}

fn marks(s: &mut Session) -> anyhow::Result<Vec<(String, String, u64)>> {
    let list = s.run("index.mark.list", json!({}))?;
    Ok(list["marks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            (
                m["first"].as_str().unwrap_or_default().to_owned(),
                m["second"].as_str().unwrap_or_default().to_owned(),
                m["page"].as_u64().unwrap_or(0),
            )
        })
        .collect())
}

/// Three pages: 사과 on 1 and 3, 배 (under 과일) on 2.
fn document() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "사과는 빨갛다."}))?;
    // The selected word becomes the first key; the mark goes after it.
    s.cursor = Cursor::default();
    s.anchor = Some(0);
    s.cursor.offset = 2;
    let made = s.run("index.mark.insert", json!({}))?;
    assert_eq!(made["first"], "사과", "{made}");
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("insert.page_break", json!({}))?;
    s.run("text.insert", json!({"text": "배는 노랗다."}))?;
    s.run(
        "index.mark.insert",
        json!({"first": "배", "second": "과일"}),
    )?;
    s.run("insert.page_break", json!({}))?;
    s.run("text.insert", json!({"text": "사과 주스."}))?;
    s.run("index.mark.insert", json!({"first": "사과"}))?;
    Ok(s)
}

#[test]
fn marks_survive_saves_and_the_index_reads_like_hangul() -> anyhow::Result<()> {
    let mut s = document()?;
    // The text is unchanged by the marks.
    let page_texts: Vec<String> = texts(&s).into_iter().filter(|t| !t.is_empty()).collect();
    assert_eq!(page_texts, ["사과는 빨갛다.", "배는 노랗다.", "사과 주스."]);
    let expected = vec![
        ("사과".to_owned(), String::new(), 1),
        ("배".to_owned(), "과일".to_owned(), 2),
        ("사과".to_owned(), String::new(), 3),
    ];
    assert_eq!(marks(&mut s)?, expected);

    let dir = tempfile::tempdir()?;
    for name in ["index.hwp", "index.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(marks(&mut back)?, expected, "{name}");
    }

    // The index at the end, as Hangul's 색인 만들기 writes it.
    let before = texts(&s);
    s.run("caret.move", json!({"motion": "document_end"}))?;
    s.run("text.insert", json!({"text": "\n"}))?;
    let made = s.run("tools.index", json!({}))?;
    assert_eq!(made["entries"], 2, "{made}");
    let first = made["first"].as_u64().unwrap_or(0) as usize;
    let index: Vec<String> = texts(&s)[first..first + 6].to_vec();
    assert_eq!(
        index,
        ["(ᄇ)", "배", "   과일\t2", "", "(ᄉ)", "사과\t1, 3"]
    );
    for name in ["made.hwp", "made.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(texts(&back)[first..first + 6], index[..], "{name}");
    }
    s.run("edit.undo", json!({}))?;
    s.run("edit.undo", json!({}))?;
    assert_eq!(texts(&s), before);

    // Update and delete, each one undo step.
    let list = s.run("index.mark.list", json!({}))?;
    let second = list["marks"][1].clone();
    let at = json!({"section": second["section"], "paragraph": second["paragraph"],
        "control": second["control"]});
    let mut update = at.clone();
    update["second"] = json!("열매");
    s.run("index.mark.update", update)?;
    assert_eq!(marks(&mut s)?[1].1, "열매");
    s.run("index.mark.delete", at.clone())?;
    assert_eq!(marks(&mut s)?.len(), 2);
    assert_eq!(texts(&s), before);
    s.run("edit.undo", json!({}))?;
    assert_eq!(marks(&mut s)?[1].1, "열매");
    for name in ["edited.hwp", "edited.hwpx"] {
        let path = dir.path().join(name);
        s.run("index.mark.delete", at.clone())?;
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(marks(&mut back)?.len(), 2, "{name}");
        assert_eq!(texts(&back), before, "{name}");
        s.run("edit.undo", json!({}))?;
    }

    // Refusals change nothing.
    let unchanged = (texts(&s), marks(&mut s)?);
    s.cursor = Cursor::default();
    assert!(
        s.run("index.mark.insert", json!({})).is_err(),
        "no key, no selection"
    );
    assert!(
        s.run("index.mark.insert", json!({"first": "a\u{1}"}))
            .is_err()
    );
    assert!(
        s.run("index.mark.insert", json!({"first": "가".repeat(201)}))
            .is_err()
    );
    assert!(
        s.run(
            "index.mark.delete",
            json!({"section":0,"paragraph":0,"control":99})
        )
        .is_err()
    );
    assert!(
        s.run(
            "index.mark.update",
            json!({"section":0,"paragraph":0,"control":0,"first":" "})
        )
        .is_err()
    );
    assert!(s.run("tools.index", json!({"x": 1})).is_err());
    assert_eq!((texts(&s), marks(&mut s)?), unchanged);
    let empty = Session::new()?.run("tools.index", json!({}));
    assert!(empty.is_err());
    Ok(())
}

/// A Hangul-written document with three index marks reads and indexes.
#[test]
fn hangul_written_marks_are_listed() -> anyhow::Result<()> {
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../rhwp/samples");
    for name in ["HWP5-nopassword-123456.hwpx", "HWP5-nopassword-123456.hwp"] {
        let path = samples.join(name);
        if !path.exists() {
            continue;
        }
        let mut s = Session::new()?;
        s.open(&path)?;
        let keys: Vec<String> = marks(&mut s)?.into_iter().map(|(k, _, _)| k).collect();
        assert_eq!(keys, ["입력기", "바탕글", "팩스 머지"], "{name}");
        let list: Value = s.run("index.mark.list", json!({}))?;
        assert!(
            list["marks"]
                .as_array()
                .is_some_and(|m| m.iter().all(|m| m["page"].as_u64().is_some()))
        );
    }
    Ok(())
}
