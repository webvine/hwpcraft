//! 문서 비교 나란히 보기와 병합: the file's version of chosen differences comes over.
use hwpcraft_engine::{Cell, Cursor, Session};
use serde_json::{Value, json};

fn texts(s: &Session) -> Vec<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect()
}

/// A document of `lines`, with a 1×2 table after paragraph `table_after` holding `cell`.
fn document(lines: &str, table_after: usize, cell: &str) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({ "text": lines }))?;
    s.cursor = Cursor {
        paragraph: table_after,
        offset: s.core.document().sections[0].paragraphs[table_after]
            .text
            .chars()
            .count(),
        ..Default::default()
    };
    s.run("text.insert", json!({"text": "\n"}))?;
    s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
    let (p, c) = s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, hwpcraft_doc::model::control::Control::Table(_)))
                .map(|c| (p, c))
        })
        .ok_or_else(|| anyhow::anyhow!("table"))?;
    s.cursor = Cursor {
        paragraph: p,
        offset: 0,
        cell: Some(Cell {
            control: c,
            index: 0,
            paragraph: 0,
        }),
        ..Default::default()
    };
    s.run("text.insert", json!({ "text": cell }))?;
    s.cursor = Cursor::default();
    Ok(s)
}

fn kinds(plan: &Value) -> Vec<(String, Option<String>, Option<String>)> {
    plan["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            (
                c["kind"].as_str().unwrap_or_default().to_owned(),
                c["current_text"].as_str().map(str::to_owned),
                c["other_text"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

#[test]
fn chosen_differences_come_over_from_the_file_in_one_undo_step() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let original = dir.path().join("original.hwpx");
    document("보고서 제목\n첫째 문단\n둘째 문단\n셋째 문단", 3, "원본 셀")?.save(&original)?;
    let mut s = document(
        "보고서 제목 수정\n첫째 문단 고침\n셋째 문단\n넷째 추가",
        2,
        "고친 셀",
    )?;
    let page = s.core.document().sections[0].section_def.page_def.width;
    let before = texts(&s);

    let plan = s.run("review.merge.plan", json!({ "path": original }))?;
    let listed = kinds(&plan);
    assert!(
        listed.contains(&(
            "changed".into(),
            Some("보고서 제목 수정".into()),
            Some("보고서 제목".into())
        )),
        "{listed:?}"
    );
    assert!(
        listed.contains(&(
            "changed".into(),
            Some("첫째 문단 고침".into()),
            Some("첫째 문단".into())
        )),
        "{listed:?}"
    );
    assert!(
        listed.contains(&("added".into(), None, Some("둘째 문단".into()))),
        "{listed:?}"
    );
    assert!(
        listed.contains(&("removed".into(), Some("넷째 추가".into()), None)),
        "{listed:?}"
    );
    // The table paragraph whose cell differs is a change too (its own text is empty).
    assert!(
        listed
            .iter()
            .filter(|(k, c, o)| k == "changed"
                && c.as_deref() == Some("")
                && o.as_deref() == Some(""))
            .count()
            == 1,
        "{listed:?}"
    );

    // Only the first-paragraph change: the section's page settings stay.
    let first = plan["changes"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["current_text"] == "보고서 제목 수정"))
        .map(|c| c["id"].clone())
        .ok_or_else(|| anyhow::anyhow!("no first change"))?;
    let one = s.run("review.merge", json!({"path": original, "ids": [first]}))?;
    assert_eq!(one["applied"], 1, "{one}");
    assert_eq!(texts(&s)[0], "보고서 제목");
    assert_eq!(
        s.core.document().sections[0].section_def.page_def.width,
        page
    );
    s.run("edit.undo", json!({}))?;
    assert_eq!(texts(&s), before);

    // Everything: the document reads like the file, in one undo step.
    let all = s.run("review.merge", json!({ "path": original }))?;
    let after = s.run("review.merge.plan", json!({ "path": original }))?;
    assert_eq!(all["changes"], 0, "{all} {after}");
    let reference = {
        let mut r = Session::new()?;
        r.open(&original)?;
        texts(&r)
    };
    assert_eq!(texts(&s), reference);
    let cell = s.run("review.merge.plan", json!({ "path": original }))?;
    assert_eq!(cell["changes"].as_array().map(Vec::len), Some(0), "{cell}");
    for name in ["merged.hwp", "merged.hwpx"] {
        let path = dir.path().join(name);
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(texts(&back), reference, "{name}");
        let left = back.run("review.merge.plan", json!({ "path": original }))?;
        assert_eq!(
            left["changes"].as_array().map(Vec::len),
            Some(0),
            "{name}: {left}"
        );
        assert_eq!(
            back.core.document().sections[0].section_def.page_def.width,
            page,
            "{name}"
        );
    }
    s.run("edit.undo", json!({}))?;
    assert_eq!(texts(&s), before);
    assert!(
        s.run("review.merge", json!({"path": original, "ids": [99]}))
            .is_err()
    );
    Ok(())
}
