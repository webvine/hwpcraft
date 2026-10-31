//! Owner: format area. 조판 부호 지우기 (`edit.delete_ctrl_marks`) on fields: a field's
//! range record goes with its control, so no range is left pointing at a control that
//! is gone or at the one after it (REVIEW-RHWP-EDIT-0.19 H1). HwpCraft's hyperlinks are
//! fields too and are what `hyperlink` removes; their text stays. A 누름틀 goes with its
//! value, as `field.delete` does; other fields (dates, cross references) leave their
//! text. Memos are not 조판 부호 here (`memo.delete`).
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::json;

/// Every field range of every body paragraph names a field control, one each.
fn ranges_consistent(s: &Session) {
    for (si, section) in s.core.document().sections.iter().enumerate() {
        for (pi, p) in section.paragraphs.iter().enumerate() {
            for r in &p.field_ranges {
                assert!(
                    matches!(p.controls.get(r.control_idx), Some(Control::Field(_))),
                    "section {si} paragraph {pi}: range {r:?} of {:?}",
                    p.controls
                );
            }
            let fields = p
                .controls
                .iter()
                .filter(|c| matches!(c, Control::Field(_)))
                .count();
            assert_eq!(fields, p.field_ranges.len(), "section {si} paragraph {pi}");
        }
    }
}

/// The whole body text, paragraphs joined by newlines.
fn body(s: &mut Session) -> anyhow::Result<String> {
    let t = s.run("document.text", json!({"limit": 1000}))?;
    Ok(t["text"]
        .as_str()
        .unwrap_or_default()
        .trim_end()
        .to_string())
}

fn bookmarks(s: &mut Session) -> anyhow::Result<usize> {
    let list = s.run("bookmark.list", json!({}))?;
    Ok(list
        .as_array()
        .or_else(|| list["bookmarks"].as_array())
        .map_or(0, Vec::len))
}

fn reopen(s: &mut Session, dir: &std::path::Path, name: &str) -> anyhow::Result<Session> {
    let path = dir.join(name);
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    Ok(back)
}

#[test]
fn removing_hyperlinks_keeps_their_text_and_the_controls_after_them() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "앞 "}))?;
    s.run(
        "insert.hyperlink",
        json!({"uri": "https://example.com/a", "text": "첫 링크"}),
    )?;
    s.run("text.insert", json!({"text": " 사이 "}))?;
    s.run(
        "insert.hyperlink",
        json!({"uri": "https://example.com/b", "text": "둘째 링크"}),
    )?;
    s.run("text.insert", json!({"text": " 뒤"}))?;
    s.run("bookmark.add", json!({"name": "끝"}))?;
    let text = body(&mut s)?;
    ranges_consistent(&s);
    let r = s.run("edit.delete_ctrl_marks", json!({"kinds": ["hyperlink"]}))?;
    assert_eq!(r["deleted"], 2, "{r}");
    assert_eq!(body(&mut s)?, text);
    assert_eq!(s.run("hyperlink.list", json!({}))?, json!([]));
    ranges_consistent(&s);
    assert_eq!(bookmarks(&mut s)?, 1);
    for name in ["links.hwp", "links.hwpx"] {
        let mut back = reopen(&mut s, dir.path(), name)?;
        assert_eq!(body(&mut back)?, text, "{name}");
        ranges_consistent(&back);
        assert_eq!(bookmarks(&mut back)?, 1, "{name}");
        assert_eq!(back.run("hyperlink.list", json!({}))?, json!([]), "{name}");
    }
    Ok(())
}

#[test]
fn removing_fields_leaves_no_range_behind() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "만든 날: "}))?;
    s.run("insert.date_field", json!({"kind": "created"}))?;
    s.run("text.insert", json!({"text": "\n이름: "}))?;
    s.run("insert.field", json!({"guide": "이름", "name": "이름"}))?;
    s.run("field.set", json!({"name": "이름", "value": "홍길동"}))?;
    s.run("caret.move", json!({"motion": "line_end"}))?;
    s.run("text.insert", json!({"text": " 끝"}))?;
    s.run("bookmark.add", json!({"name": "끝"}))?;
    let date: String = body(&mut s)?.lines().next().unwrap_or_default().to_string();
    assert!(date.contains('년'), "{date}");
    ranges_consistent(&s);
    let r = s.run("edit.delete_ctrl_marks", json!({"kinds": ["field"]}))?;
    assert_eq!(r["deleted"], 2, "{r}");
    ranges_consistent(&s);
    // The date's text stays; the 누름틀 goes with its value, as field.delete does.
    let lines: Vec<String> = body(&mut s)?.lines().map(str::to_string).collect();
    assert_eq!(lines[0], date);
    assert_eq!(lines[1], "이름:  끝");
    assert_eq!(bookmarks(&mut s)?, 1);
    for name in ["fields.hwp", "fields.hwpx"] {
        let mut back = reopen(&mut s, dir.path(), name)?;
        ranges_consistent(&back);
        let again: Vec<String> = body(&mut back)?.lines().map(str::to_string).collect();
        assert_eq!(again, lines, "{name}");
        assert_eq!(bookmarks(&mut back)?, 1, "{name}");
    }
    Ok(())
}

#[test]
fn memos_are_not_removed_as_fields() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "검토할 문장입니다."}))?;
    s.run("select.text", json!({"text": "검토할"}))?;
    s.run("memo.create", json!({"text": "확인 필요"}))?;
    let r = s.run("edit.delete_ctrl_marks", json!({"kinds": ["field"]}))?;
    assert_eq!(r["deleted"], 0, "{r}");
    let memos = s.run("memo.list", json!({}))?;
    assert_eq!(memos.as_array().map_or(0, Vec::len), 1, "{memos}");
    assert_eq!(body(&mut s)?, "검토할 문장입니다.");
    ranges_consistent(&s);
    Ok(())
}
