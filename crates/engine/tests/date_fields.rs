//! Document times recorded at save (문서 요약 정보) and the date fields that show them.
//! One test: the fixed clock is shared by the whole binary.
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use hwpcraft_engine::{Session, fixed_clock};
use serde_json::json;

const E1: i64 = 1_767_225_600; // 2026-01-01T00:00:00Z
const E2: i64 = 1_767_315_845; // 2026-01-02T01:04:05Z
const E3: i64 = 1_772_323_200; // 2026-03-01T00:00:00Z

/// Hangul's long local form, written independently of the engine.
fn long(secs: i64) -> String {
    let t = Utc.timestamp_opt(secs, 0).unwrap().with_timezone(&Local);
    let day =
        ["월", "화", "수", "목", "금", "토", "일"][t.weekday().num_days_from_monday() as usize];
    let (pm, hour) = t.hour12();
    format!(
        "{}년 {}월 {}일 {day}요일 {} {hour}:{:02}:{:02}",
        t.year(),
        t.month(),
        t.day(),
        if pm { "오후" } else { "오전" },
        t.minute(),
        t.second()
    )
}

fn at(secs: i64) -> Option<chrono::DateTime<Utc>> {
    Utc.timestamp_opt(secs, 0).single()
}

fn hpf(s: &Session) -> String {
    s.core
        .document()
        .hwpx_aux_entries
        .iter()
        .find(|(p, _)| p == "Contents/content.hpf")
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

fn body(s: &Session) -> String {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn save_times_and_date_fields() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    fixed_clock(Some(E1));

    // A new document: created is now; it has no last save yet.
    let mut s = Session::new()?;
    assert_eq!(s.document_times(), (None, None));
    s.run("text.insert", json!({"text": "만든 날: "}))?;
    let created = s.run("insert.date_field", json!({}))?;
    assert_eq!(created["text"], long(E1), "{created}");
    let error = s
        .run("insert.date_field", json!({"kind": "modified"}))
        .unwrap_err();
    assert!(error.to_string().contains("저장한 적 없는"), "{error}");

    // HWPX: the metadata gets Hangul's notation; reopening reads it back.
    let a = dir.path().join("a.hwpx");
    s.save(&a)?;
    let mut r = Session::new()?;
    r.open(&a)?;
    assert_eq!(r.document_times(), (at(E1), at(E1)));
    let meta = hpf(&r);
    assert!(
        meta.contains(
            r#"<opf:meta name="CreatedDate" content="text">2026-01-01T00:00:00Z</opf:meta>"#
        ),
        "{meta}"
    );
    assert!(
        meta.contains(
            r#"<opf:meta name="ModifiedDate" content="text">2026-01-01T00:00:00Z</opf:meta>"#
        ),
        "{meta}"
    );
    assert!(body(&r).contains(&format!("만든 날: {}", long(E1))));

    // Opening alone changes nothing.
    let mut again = Session::new()?;
    again.open(&a)?;
    assert_eq!(hpf(&again), meta);

    // A second save keeps the creation time and records the new save time; the
    // fields change on tools.date_field.update.
    fixed_clock(Some(E2));
    r.save(&a)?;
    assert_eq!(r.document_times(), (at(E1), at(E2)));
    r.run("caret.move", json!({"motion": "document_end"}))?;
    r.run("text.insert", json!({"text": "\n저장한 날: "}))?;
    let modified = r.run("insert.date_field", json!({"kind": "modified"}))?;
    assert_eq!(modified["text"], long(E2), "{modified}");
    fixed_clock(Some(E3));
    r.save(&a)?;
    let updated = r.run("tools.date_field.update", json!({}))?;
    assert_eq!(updated["fields"], 2, "{updated}");
    assert_eq!(updated["updated"], 1, "{updated}");
    let text = body(&r);
    assert!(text.contains(&format!("만든 날: {}", long(E1))), "{text}");
    assert!(text.contains(&format!("저장한 날: {}", long(E3))), "{text}");
    // One undo takes the refresh back.
    r.run("edit.undo", json!({}))?;
    assert!(body(&r).contains(&format!("저장한 날: {}", long(E2))));
    // The fields survive both formats.
    for name in ["fields.hwp", "fields.hwpx"] {
        let path = dir.path().join(name);
        r.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        let fields = back.run("tools.date_field.update", json!({}))?;
        assert_eq!(fields["fields"], 2, "{name}: {fields}");
    }

    // HWP: the summary's FILETIMEs.
    fixed_clock(Some(E1));
    let mut h = Session::new()?;
    h.run("text.insert", json!({"text": "본문"}))?;
    let b = dir.path().join("b.hwp");
    h.save(&b)?;
    let mut hr = Session::new()?;
    hr.open(&b)?;
    assert_eq!(hr.document_times(), (at(E1), at(E1)));
    fixed_clock(Some(E2));
    hr.save(&b)?;
    let mut hr2 = Session::new()?;
    hr2.open(&b)?;
    assert_eq!(hr2.document_times(), (at(E1), at(E2)));

    // PDF is not stamped.
    let streams = hr2.core.document().extra_streams.clone();
    fixed_clock(Some(E3));
    hr2.save(&dir.path().join("b.pdf"))?;
    assert_eq!(hr2.core.document().extra_streams, streams);
    assert_eq!(hr2.document_times(), (at(E1), at(E2)));

    // A failed save puts the times back.
    // (a directory where the file would go makes the write fail)
    let blocked = dir.path().join("blocked");
    std::fs::create_dir_all(blocked.join("x.hwpx"))?;
    std::fs::create_dir_all(blocked.join("x.hwp"))?;
    let before_hpf = hpf(&again);
    let before_times = again.document_times();
    assert!(again.save(&blocked.join("x.hwpx")).is_err());
    assert_eq!(hpf(&again), before_hpf);
    assert_eq!(again.document_times(), before_times);
    let before_streams = hr2.core.document().extra_streams.clone();
    assert!(hr2.save(&blocked.join("x.hwp")).is_err());
    assert_eq!(hr2.core.document().extra_streams, before_streams);

    // The same clock gives the same metadata.
    let (c1, c2) = (dir.path().join("c1.hwpx"), dir.path().join("c2.hwpx"));
    fixed_clock(Some(E2));
    let mut d = Session::new()?;
    d.run("text.insert", json!({"text": "같은 문서"}))?;
    d.save(&c1)?;
    let first = hpf(&d);
    let mut d2 = Session::new()?;
    d2.run("text.insert", json!({"text": "같은 문서"}))?;
    d2.save(&c2)?;
    assert_eq!(hpf(&d2), first);
    fixed_clock(None);
    Ok(())
}

/// Text typed right after a date field stays outside it, so a refresh keeps it.
#[test]
fn text_typed_after_a_date_field_stays_outside_it() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "만든 날: "}))?;
    let field = s.run("insert.date_field", json!({}))?;
    s.run("text.insert", json!({"text": "에 작성"}))?;
    let shown = field["text"].as_str().unwrap_or_default().to_owned();
    assert_eq!(body(&s), format!("만든 날: {shown}에 작성"));
    s.run("tools.date_field.update", json!({}))?;
    assert!(body(&s).ends_with("에 작성"), "{}", body(&s));
    Ok(())
}
