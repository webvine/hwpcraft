//! Owner: shell/page area. 쪽/파일 서비스 명령 (crates/engine/src/ext_page.rs).
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::{Command, Session};
use serde_json::json;

fn page_number_controls(s: &Session) -> Vec<(u8, u8, char)> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::PageNumberPos(n) => Some((n.position, n.format, n.dash_char)),
            _ => None,
        })
        .collect()
}

#[test]
fn change_tracking_records_are_reported_only_when_present() {
    let mut s = Session::new().unwrap();
    assert!(!s.has_track_change_records());
    // A TRACKCHANGE settings record alone (tag 32, written by every Hangul file) is not a record of changes.
    let record = |tag_id| hwpcraft_doc::model::document::RawRecord {
        tag_id,
        level: 0,
        data: vec![0; 4],
    };
    let extra = &mut s.core.document_mut().doc_info.extra_records;
    extra.push(record(32));
    assert!(!s.has_track_change_records());
    s.core
        .document_mut()
        .doc_info
        .extra_records
        .push(record(97));
    assert!(s.has_track_change_records());
}

fn line(kind: u8, width: u8, color: &str) -> serde_json::Value {
    json!({"type": kind, "width": width, "color": color})
}

#[test]
fn page_number_inserts_once_updates_in_place_and_roundtrips() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let before = s.run("page.number.get", json!({}))?;
    assert_eq!(before["exists"], false);
    let r = s.run(
        "page.number",
        json!({"position":"bottom_center","format":0,"dash":true,"start":3}),
    )?;
    assert_eq!(r["exists"], true);
    assert_eq!(r["position"], "bottom_center");
    assert_eq!(r["start"], 3);
    assert_eq!(page_number_controls(&s), [(5, 0, '-')]);
    // Text and cursor survive the inline control insertion.
    assert_eq!(s.entries()[0].1.trim(), "본문");
    // A second call edits the existing control instead of adding another.
    s.run(
        "page.number",
        json!({"position":"outside_top","format":2,"dash":false}),
    )?;
    assert_eq!(page_number_controls(&s), [(7, 2, '\0')]);
    s.execute(Command::Undo)?;
    assert_eq!(page_number_controls(&s), [(5, 0, '-')]);
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("page-number.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        assert_eq!(page_number_controls(&r), [(5, 0, '-')], "{ext}");
        assert_eq!(r.run("page.number.get", json!({}))?["start"], 3, "{ext}");
    }
    Ok(())
}

#[test]
fn page_number_rejects_bad_input_without_changing_document() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let generation = s.generation;
    for bad in [
        json!({"position":"middle"}),
        json!({"position":"top_left","format":99}),
        json!({"position":"top_left","start":0}),
        json!({"position":"top_left","extra":1}),
    ] {
        assert!(s.run("page.number", bad).is_err());
    }
    assert_eq!(s.generation, generation);
    assert!(page_number_controls(&s).is_empty());
    Ok(())
}

#[test]
fn page_border_fill_sets_lines_spacing_and_fill_with_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let original = s.run("page.border_fill.get", json!({}))?;
    let r = s.run(
        "page.border_fill",
        json!({
            "basis": "page",
            "spacingMm": [5.0, 6.0, 7.0, 8.0],
            "headerInside": true,
            "footerInside": false,
            "applyPage": "all",
            "borders": [line(1, 3, "#ff0000"), line(1, 3, "#ff0000"), line(2, 0, "#0000ff"), line(0, 0, "#000000")],
            "fill": "#ffeecc",
        }),
    )?;
    assert_eq!(r["basis"], "page");
    assert_eq!(r["headerInside"], true);
    assert_eq!(r["borderLeft"]["color"], "#ff0000");
    assert_eq!(r["borderTop"]["type"], 2);
    assert_eq!(r["fillType"], "solid");
    assert_eq!(r["fillColor"], "#ffeecc");
    let spacing: Vec<f64> = serde_json::from_value(r["spacingMm"].clone())?;
    for (got, want) in spacing.iter().zip([5.0, 6.0, 7.0, 8.0]) {
        assert!((got - want).abs() < 0.05, "{spacing:?}");
    }
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("border.hwpx");
    s.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    let saved = reopened.run("page.border_fill.get", json!({}))?;
    assert_eq!(saved["borderLeft"]["color"], "#ff0000");
    assert_eq!(saved["fillColor"], "#ffeecc");
    s.execute(Command::Undo)?;
    assert_eq!(s.run("page.border_fill.get", json!({}))?, original);
    // Invalid colour and unsupported 첫 쪽만 are rejected before any change.
    let generation = s.generation;
    assert!(
        s.run(
            "page.border_fill",
            json!({"basis":"page","spacingMm":[0,0,0,0],"headerInside":false,"footerInside":false,
                "applyPage":"firstOnly","borders":[line(1,0,"#000000"),line(1,0,"#000000"),line(1,0,"#000000"),line(1,0,"#000000")]}),
        )
        .is_err()
    );
    assert!(
        s.run(
            "page.border_fill",
            json!({"basis":"page","spacingMm":[0,0,0,0],"headerInside":false,"footerInside":false,
                "applyPage":"all","borders":[line(1,0,"red"),line(1,0,"#000000"),line(1,0,"#000000"),line(1,0,"#000000")]}),
        )
        .is_err()
    );
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn header_footer_templates_by_apply_kind_and_delete() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("page.header", json!({"apply_to":"odd","template":3}))?;
    s.run("page.footer", json!({"apply_to":"both","template":2}))?;
    let kinds = |s: &Session| {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter_map(|c| match c {
                Control::Header(h) => Some(format!("header:{:?}", h.apply_to)),
                Control::Footer(f) => Some(format!("footer:{:?}", f.apply_to)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut found = kinds(&s);
    found.sort();
    assert_eq!(found, ["footer:Both", "header:Odd"]);
    s.run(
        "page.header_footer.delete",
        json!({"header":true,"apply_to":"odd"}),
    )?;
    assert_eq!(kinds(&s), ["footer:Both"]);
    s.execute(Command::Undo)?;
    assert_eq!(kinds(&s).len(), 2);
    assert!(s.run("page.header", json!({"template":11})).is_err());
    assert!(s.run("page.header", json!({"apply_to":"left"})).is_err());
    Ok(())
}

#[test]
fn new_number_hide_and_document_info() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("첫 쪽 글자".into()))?;
    s.run("page.new_number", json!({"start":7}))?;
    assert!(
        s.core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .any(|c| matches!(c, Control::NewNumber(n) if n.number == 7))
    );
    assert!(s.run("page.new_number", json!({"start":0})).is_err());
    let hide = s.run(
        "page.hide",
        json!({"header":true,"footer":false,"master_page":false,"border":true,"fill":false,"page_number":true}),
    )?;
    assert_eq!(hide["hideHeader"], true, "{hide}");
    assert_eq!(hide["hidePageNum"], true, "{hide}");
    let info = s.run("file.doc_info", json!({}))?;
    assert_eq!(info["pages"], 1);
    assert_eq!(info["characters"], 6);
    assert_eq!(info["words"], 3);
    assert_eq!(info["summarySupported"], false);
    let masters = s.run("page.master.list", json!({}))?;
    assert_eq!(masters["masterPages"], json!([]));
    Ok(())
}

#[test]
fn print_pdf_writes_selected_page_ranges() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("1쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("2쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("3쪽".into()))?;
    assert_eq!(s.core.page_count(), 3);
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("print.pdf");
    for (range, current, want) in [
        ("all", 0, vec![1, 2, 3]),
        ("current", 1, vec![2]),
        ("to_current", 1, vec![1, 2]),
        ("from_current", 1, vec![2, 3]),
        ("1, 3", 0, vec![1, 3]),
        ("2-3", 0, vec![2, 3]),
    ] {
        let r = s.run(
            "file.print_pdf",
            json!({"path": path, "pages": range, "current": current}),
        )?;
        assert_eq!(r["pages"], json!(want), "{range}");
        assert!(std::fs::read(&path)?.starts_with(b"%PDF"));
    }
    for bad in ["0", "4", "3-1", "x", " , "] {
        assert!(
            s.run("file.print_pdf", json!({"path": path, "pages": bad}))
                .is_err(),
            "{bad}"
        );
    }
    Ok(())
}

// ----- 0.13 -----

fn header_text(s: &Session) -> Option<String> {
    s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Header(h) => Some(h.paragraphs.iter().map(|p| p.text.clone()).collect()),
            _ => None,
        })
}

fn svg(s: &Session) -> anyhow::Result<String> {
    s.core
        .render_page_svg_native(0)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn hwpx(s: &Session) -> anyhow::Result<Vec<u8>> {
    s.core
        .export_hwpx_native()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn texts(s: &Session) -> Vec<String> {
    s.entries().into_iter().map(|(_, t)| t).collect()
}

#[test]
fn view_marks_change_the_screen_but_not_the_document() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("한 줄 본문".into()))?;
    let plain_svg = svg(&s)?;
    let plain_bytes = hwpx(&s)?;
    let (generation, dirty, undo) = (s.generation, s.dirty(), s.can_undo());
    assert_eq!(
        s.run("view.marks.get", json!({}))?,
        json!({"paragraphMarks": false, "controlCodes": false, "transparentBorders": false})
    );
    let on = json!({"paragraphMarks": true, "controlCodes": true, "transparentBorders": true});
    let r = s.run("view.marks", on.clone())?;
    assert_eq!(r["paragraphMarks"], true);
    assert_eq!(r["controlCodes"], true);
    assert_eq!(
        s.run("view.marks.get", json!({}))?["transparentBorders"],
        true
    );
    // The page cache must be redrawn, but nothing is recorded as an edit.
    assert_ne!(s.generation, generation);
    assert_eq!(s.dirty(), dirty);
    assert_eq!(s.can_undo(), undo);
    // Undo still targets the text insertion, not the view change.
    s.execute(Command::Undo)?;
    assert!(s.entries().iter().all(|(_, t)| t.trim().is_empty()));
    assert_eq!(s.run("view.marks.get", json!({}))?["paragraphMarks"], true);
    s.execute(Command::Redo)?;
    assert_ne!(svg(&s)?, plain_svg, "marks must be drawn");
    assert_eq!(hwpx(&s)?, plain_bytes, "saved bytes unchanged");
    // Setting the same flags again is a no-op for the cache generation.
    let generation = s.generation;
    s.run("view.marks", on)?;
    assert_eq!(s.generation, generation);
    s.run("view.marks", json!({}))?;
    assert_eq!(svg(&s)?, plain_svg);
    assert!(s.run("view.marks", json!({"bogus": true})).is_err());
    // A new document starts without marks.
    s.run("view.marks", json!({"paragraphMarks": true}))?;
    s.reset()?;
    assert_eq!(s.run("view.marks.get", json!({}))?["paragraphMarks"], false);
    Ok(())
}

#[test]
fn header_footer_text_with_fields_roundtrips_and_undoes() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    assert_eq!(
        s.run("page.header_footer.text.get", json!({"header": true}))?["exists"],
        false
    );
    let r = s.run(
        "page.header_footer.text",
        json!({"header": true, "text": "쪽 /", "fields": [
            {"offset": 2, "kind": "page_number"},
            {"offset": 3, "kind": "total_pages"},
            {"offset": 0, "kind": "file_name"}
        ]}),
    )?;
    assert_eq!(r["exists"], true);
    assert_eq!(r["text"], "쪽 /");
    assert_eq!(
        r["fields"],
        json!([
            {"offset": 0, "kind": "file_name"},
            {"offset": 2, "kind": "page_number"},
            {"offset": 3, "kind": "total_pages"}
        ])
    );
    assert_eq!(header_text(&s).as_deref(), Some("\u{17}쪽 \u{15}/\u{16}"));
    // Fields at the same offset keep the listed order.
    s.run(
        "page.header_footer.text",
        json!({"header": true, "text": "", "fields": [
            {"offset": 0, "kind": "page_number"},
            {"offset": 0, "kind": "total_pages"}
        ]}),
    )?;
    assert_eq!(header_text(&s).as_deref(), Some("\u{15}\u{16}"));
    s.execute(Command::Undo)?;
    assert_eq!(header_text(&s).as_deref(), Some("\u{17}쪽 \u{15}/\u{16}"));
    s.execute(Command::Undo)?;
    assert_eq!(header_text(&s), None);
    s.execute(Command::Redo)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("hf-fields.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        // On save RHWP lowers the markers to AutoNumber (page/total) and a path Field
        // around the literal file name, so the raw text now holds placeholders.
        let raw = header_text(&r).unwrap_or_default();
        assert!(raw.contains("쪽") && raw.contains('/'), "{ext} {raw:?}");
        let v = r.run("page.header_footer.text.get", json!({"header": true}))?;
        assert_eq!(v["text"], "쪽 /", "{ext}");
        let kinds: Vec<&str> = v["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f["kind"].as_str())
            .collect();
        assert_eq!(kinds, ["file_name", "page_number", "total_pages"], "{ext}");
        // Editing a reopened header rebuilds it from the lowered controls.
        r.run(
            "page.header_footer.text",
            json!({"header": true, "text": "다시", "fields": [{"offset": 2, "kind": "page_number"}]}),
        )?;
        assert_eq!(header_text(&r).as_deref(), Some("다시\u{15}"), "{ext}");
    }
    let generation = s.generation;
    for bad in [
        json!({"header": true, "text": "a\tb"}),
        json!({"header": true, "text": "ab", "fields": [{"offset": 3, "kind": "page_number"}]}),
        json!({"header": true, "text": "ab", "fields": [{"offset": 0, "kind": "date"}]}),
        json!({"header": true, "text": "ab", "apply_to": "weekend"}),
    ] {
        assert!(s.run("page.header_footer.text", bad).is_err());
    }
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn section_def_updates_numbers_and_tab_spacing_with_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let before = s.run("page.section.get", json!({}))?;
    assert_eq!(before["section"], 0);
    assert_eq!(before["lineNumbersSupported"], false);
    let r = s.run(
        "page.section",
        json!({"pageNum": 5, "tableNum": 3, "defaultTabSpacingMm": 10.0, "hideEmptyLine": true}),
    )?;
    assert_eq!(r["pageNum"], 5);
    assert_eq!(r["tableNum"], 3);
    assert_eq!(r["defaultTabSpacingMm"], 10.0);
    assert_eq!(r["hideEmptyLine"], true);
    assert_eq!(r["pictureNum"], before["pictureNum"]);
    s.run("page.section", json!({"pageNum": 7, "wholeDocument": true}))?;
    assert_eq!(s.run("page.section.get", json!({}))?["pageNum"], 7);
    s.execute(Command::Undo)?;
    assert_eq!(s.run("page.section.get", json!({}))?["pageNum"], 5);
    s.execute(Command::Undo)?;
    assert_eq!(s.run("page.section.get", json!({}))?, before);
    let temp = tempfile::tempdir()?;
    s.run("page.section", json!({"pageNum": 9, "tableNum": 4}))?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("section.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let v = r.run("page.section.get", json!({}))?;
        assert_eq!(v["pageNum"], 9, "{ext}");
        assert_eq!(v["tableNum"], 4, "{ext}");
    }
    let generation = s.generation;
    for bad in [
        json!({}),
        json!({"pageNumType": 9}),
        json!({"defaultTabSpacingMm": -1.0}),
        json!({"pageNum": 70000}),
        json!({"unknown": 1}),
    ] {
        assert!(s.run("page.section", bad).is_err());
    }
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn privacy_scan_finds_validated_values_and_mask_replaces_them_in_one_undo_step()
-> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert(
        "연락처 010-1234-5678 메일 kim@example.com 끝".into(),
    ))?;
    s.execute(Command::Split)?;
    s.execute(Command::Table {
        rows: 1,
        columns: 1,
    })?;
    let cell = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .map(|(c, _)| c)
        .ok_or_else(|| anyhow::anyhow!("표 셀이 없습니다"))?;
    s.select(cell, false)?;
    s.execute(Command::Insert("셀 010-1234-5678".into()))?;
    let scan = s.run("security.privacy.scan", json!({}))?;
    let findings = scan["findings"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<&str> = findings.iter().filter_map(|f| f["kind"].as_str()).collect();
    assert!(kinds.contains(&"phone"), "{findings:?}");
    assert!(kinds.contains(&"email"), "{findings:?}");
    let phone = findings
        .iter()
        .find(|f| f["kind"] == "phone")
        .cloned()
        .unwrap_or_default();
    assert_eq!(phone["raw"], "010-1234-5678");
    assert_eq!(phone["inBody"], true);
    assert_eq!(
        phone["masked"].as_str().map(|m| m.chars().count()),
        Some(13)
    );
    assert_eq!(scan["hiddenText"]["clean"], true);
    // Only the requested kinds are reported.
    let only_mail = s.run("security.privacy.scan", json!({"kinds": ["email"]}))?;
    assert!(
        only_mail["findings"]
            .as_array()
            .is_some_and(|a| a.iter().all(|f| f["kind"] == "email") && !a.is_empty())
    );
    let r = s.run(
        "security.privacy.mask",
        json!({"items": [{"raw": "010-1234-5678", "masked": "010-****-****"}]}),
    )?;
    assert_eq!(r["replaced"], 2);
    let t = texts(&s);
    assert!(
        t.iter().any(|x| x.contains("연락처 010-****-**** 메일")),
        "{t:?}"
    );
    assert!(t.iter().any(|x| x.contains("셀 010-****-****")), "{t:?}");
    assert!(!t.iter().any(|x| x.contains("1234")), "{t:?}");
    s.execute(Command::Undo)?;
    let t = texts(&s);
    assert_eq!(t.iter().filter(|x| x.contains("010-1234-5678")).count(), 2);
    let generation = s.generation;
    for bad in [
        json!({"items": []}),
        json!({"items": [{"raw": "010-1234-5678", "masked": "***"}]}),
        json!({"items": [{"raw": "", "masked": ""}]}),
    ] {
        assert!(s.run("security.privacy.mask", bad).is_err());
    }
    assert!(
        s.run("security.privacy.scan", json!({"kinds": ["passport"]}))
            .is_err()
    );
    assert!(
        s.run("security.privacy.scan", json!({"mask": "**"}))
            .is_err()
    );
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn save_pages_writes_the_range_and_leaves_the_document_alone() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("1쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("2쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("3쪽".into()))?;
    assert_eq!(s.core.page_count(), 3);
    let (generation, cursor, undo) = (s.generation, s.cursor, s.can_undo());
    let temp = tempfile::tempdir()?;
    for ext in ["hwpx", "hwp"] {
        let path = temp.path().join(format!("page-2.{ext}"));
        let r = s.run("file.save_pages", json!({"path": path, "from": 2, "to": 2}))?;
        assert_eq!(r["pagesBefore"], 3, "{ext}");
        assert_eq!(r["pagesAfter"], 1, "{ext}");
        let mut part = Session::new()?;
        part.open(&path)?;
        assert_eq!(part.core.page_count(), 1, "{ext}");
        let t = texts(&part);
        assert!(t.iter().any(|x| x.contains("2쪽")), "{ext} {t:?}");
        assert!(
            !t.iter().any(|x| x.contains("1쪽") || x.contains("3쪽")),
            "{ext} {t:?}"
        );
    }
    assert_eq!(s.core.page_count(), 3);
    assert_eq!(s.generation, generation);
    assert_eq!(s.cursor, cursor);
    assert_eq!(s.can_undo(), undo);
    assert!(s.path.is_none());
    let all = texts(&s);
    assert!(all.iter().any(|x| x.contains("1쪽")) && all.iter().any(|x| x.contains("3쪽")));
    let path = temp.path().join("bad.hwpx");
    for (from, to) in [(0, 1), (2, 1), (1, 4)] {
        assert!(
            s.run(
                "file.save_pages",
                json!({"path": path, "from": from, "to": to})
            )
            .is_err()
        );
    }
    assert!(
        s.run(
            "file.save_pages",
            json!({"path": temp.path().join("x.docx"), "from": 1, "to": 1})
        )
        .is_err()
    );
    assert!(!path.exists());
    // RHWP only serialises HML for documents that were opened from HML.
    let hml = temp.path().join("out.hml");
    assert!(s.run("file.save_hml", json!({"path": hml})).is_err());
    assert!(!hml.exists());
    assert!(
        s.run(
            "file.save_hml",
            json!({"path": temp.path().join("out.txt")})
        )
        .is_err()
    );
    Ok(())
}

// ----- 0.14: 쪽 여백 사전 값 · 구역 이동 · 쪽 복사/붙이기/지우기 · 끼워 넣기 · 블록 저장 -----

fn three_pages() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("1쪽 가".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("1쪽 나".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("2쪽 다".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("2쪽 라".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("3쪽 마".into()))?;
    assert_eq!(s.core.page_count(), 3);
    Ok(s)
}

fn has_section_def(s: &Session) -> bool {
    s.core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .any(|c| matches!(c, Control::SectionDef(_)))
}

fn at(paragraph: usize, offset: usize) -> hwpcraft_engine::Cursor {
    hwpcraft_engine::Cursor {
        paragraph,
        offset,
        ..Default::default()
    }
}

#[test]
fn margin_presets_match_hangul_gallery_and_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let list = s.run("page.margins.presets", json!({}))?;
    let names: Vec<&str> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["preset"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "default",
            "narrow1_hf",
            "narrow2_hf",
            "wide_hf",
            "narrow",
            "wide"
        ]
    );
    let before = s.page_setup()?;
    let r = s.run("page.margins.preset", json!({"preset": "narrow"}))?;
    assert_eq!(r["label"], "좁게");
    let after = s.page_setup()?;
    // RHWP stores 1/7200 inch units, so millimetres come back within 0.01 mm.
    let mm = |v: f64| (v * 100.0).round() / 100.0;
    assert_eq!(
        [
            after.top_mm,
            after.bottom_mm,
            after.left_mm,
            after.right_mm,
            after.header_mm,
            after.footer_mm
        ]
        .map(mm),
        [10.0, 10.0, 10.0, 10.0, 0.0, 0.0]
    );
    assert_eq!(after.width_mm, before.width_mm);
    assert_eq!(after.gutter_mm, before.gutter_mm);
    s.run("page.margins.preset", json!({"preset": "wide_hf"}))?;
    let wide = s.page_setup()?;
    assert_eq!(
        (mm(wide.top_mm), mm(wide.left_mm), mm(wide.header_mm)),
        (35.0, 40.0, 10.0)
    );
    s.execute(Command::Undo)?;
    s.execute(Command::Undo)?;
    assert_eq!(s.page_setup()?, before);
    let generation = s.generation;
    assert!(
        s.run("page.margins.preset", json!({"preset": "huge"}))
            .is_err()
    );
    assert!(s.run("page.margins.preset", json!({})).is_err());
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn section_navigation_stops_at_document_ends() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    assert!(s.run("page.section.next", json!({})).is_err());
    assert!(s.run("page.section.prev", json!({})).is_err());
    assert_eq!(s.cursor.offset, 2);
    Ok(())
}

#[test]
fn page_copy_paste_delete_follow_hangul_order() -> anyhow::Result<()> {
    let mut s = three_pages()?;
    let r = s.run("page.copy", json!({"page": 2}))?;
    assert_eq!(r["paragraphs"], 2);
    // Caret on page 1: the copy becomes a new page 2, caret at the end of the pasted text.
    s.select(at(0, 1), false)?;
    let r = s.run("page.paste", json!({}))?;
    assert_eq!(r["afterPage"], 1, "{r}");
    assert_eq!(r["pages"], 4, "{r}");
    assert_eq!(
        texts(&s),
        [
            "1쪽 가", "1쪽 나", "2쪽 다", "2쪽 라", "2쪽 다", "2쪽 라", "3쪽 마"
        ]
    );
    assert_eq!((s.cursor.paragraph, s.cursor.offset), (3, 4));
    // The pasted page survives a save and reopen.
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("pasted.hwpx");
    s.save(&path)?;
    let mut again = Session::new()?;
    again.open(&path)?;
    assert_eq!(again.core.page_count(), 4);
    assert_eq!(texts(&again).len(), 7);
    s.execute(Command::Undo)?;
    assert_eq!(s.core.page_count(), 3);
    assert_eq!(texts(&s).len(), 5);
    // Paste after the last page.
    let r = s.run("page.paste", json!({"page": 3}))?;
    assert_eq!(r["pages"], 4);
    assert_eq!(
        texts(&s),
        [
            "1쪽 가", "1쪽 나", "2쪽 다", "2쪽 라", "3쪽 마", "2쪽 다", "2쪽 라"
        ]
    );
    s.execute(Command::Undo)?;
    // Delete the middle page: the pages after it move up.
    let r = s.run("page.delete", json!({"page": 2}))?;
    assert_eq!(r["paragraphsRemoved"], 2);
    assert_eq!(r["pages"], 2);
    assert_eq!(texts(&s), ["1쪽 가", "1쪽 나", "3쪽 마"]);
    assert_eq!(s.cursor.paragraph, 2);
    // Delete the first page: the section definition stays on the first paragraph.
    let r = s.run("page.delete", json!({"page": 1}))?;
    assert_eq!(r["pages"], 1, "{r}");
    assert_eq!(texts(&s), ["3쪽 마"]);
    assert!(has_section_def(&s));
    assert!(s.run("page.delete", json!({})).is_err());
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["1쪽 가", "1쪽 나", "3쪽 마"]);
    s.execute(Command::Undo)?;
    assert_eq!(s.core.page_count(), 3);
    assert_eq!(texts(&s).len(), 5);
    for bad in [json!({"page": 0}), json!({"page": 9}), json!({"pages": 1})] {
        assert!(s.run("page.delete", bad.clone()).is_err(), "{bad}");
        assert!(s.run("page.copy", bad).is_err());
    }
    Ok(())
}

#[test]
fn insert_file_merges_another_document_at_the_caret_with_undo() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let other = temp.path().join("other.hwpx");
    {
        let mut o = Session::new()?;
        o.execute(Command::Insert("끼워 넣을 첫 문단".into()))?;
        o.execute(Command::Split)?;
        o.execute(Command::Insert("둘째 문단".into()))?;
        o.execute(Command::Character(json!({"bold": true})))?;
        o.save(&other)?;
    }
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞뒤".into()))?;
    s.select(at(0, 1), false)?;
    let r = s.run("file.insert", json!({"path": other}))?;
    assert_eq!(r["paragraphs"], 2, "{r}");
    let t = texts(&s);
    assert_eq!(t, ["앞끼워 넣을 첫 문단", "둘째 문단뒤"]);
    assert_eq!((s.cursor.paragraph, s.cursor.offset), (1, 5));
    let path = temp.path().join("merged.hwp");
    s.save(&path)?;
    let mut again = Session::new()?;
    again.open(&path)?;
    assert_eq!(texts(&again), t);
    s.execute(Command::Undo)?;
    assert_eq!(texts(&s), ["앞뒤"]);
    let generation = s.generation;
    assert!(
        s.run(
            "file.insert",
            json!({"path": temp.path().join("missing.hwpx")})
        )
        .is_err()
    );
    assert!(s.run("file.insert", json!({})).is_err());
    assert_eq!(s.generation, generation);
    Ok(())
}

#[test]
fn save_block_writes_only_the_selection() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let mut s = Session::new()?;
    s.execute(Command::Insert("가나다라마".into()))?;
    let path = temp.path().join("block.hwpx");
    assert!(s.run("file.save_block", json!({"path": path})).is_err());
    s.select(at(0, 1), false)?;
    s.select(at(0, 4), true)?;
    let (generation, cursor) = (s.generation, s.cursor);
    let r = s.run("file.save_block", json!({"path": path}))?;
    assert_eq!(r["text"], "나다라", "{r}");
    let mut part = Session::new()?;
    part.open(&path)?;
    assert_eq!(texts(&part), ["나다라"]);
    assert_eq!(s.generation, generation);
    assert_eq!(s.cursor, cursor);
    assert_eq!(texts(&s), ["가나다라마"]);
    // A span over three paragraphs keeps the middle one whole.
    s.select(at(0, 5), false)?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("둘째".into()))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("셋째 끝".into()))?;
    s.select(at(0, 3), false)?;
    s.select(at(2, 2), true)?;
    let span = temp.path().join("span.hwp");
    let r = s.run("file.save_block", json!({"path": span}))?;
    assert_eq!(r["paragraphs"], 3, "{r}");
    let mut part = Session::new()?;
    part.open(&span)?;
    assert_eq!(texts(&part), ["라마", "둘째", "셋째"]);
    assert!(
        s.run(
            "file.save_block",
            json!({"path": temp.path().join("x.docx")})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn background_pdf_jobs_cover_print_ranges_and_blocks() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("1쪽".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("2쪽 블록".into()))?;
    s.execute(Command::PageBreak)?;
    s.execute(Command::Insert("3쪽".into()))?;
    let (pages, job) = s.print_pdf_job("2-3", 0)?;
    assert_eq!(pages, [1, 2]);
    assert_eq!(job.page_count(), 2);
    // The conversion needs no session: run it on another thread like the desktop.
    let bytes = std::thread::spawn(move || job.convert())
        .join()
        .expect("worker")?;
    assert!(bytes.starts_with(b"%PDF"));
    assert!(s.print_pdf_job("9", 0).is_err());
    assert!(s.save_block_pdf_job().is_err(), "no selection yet");
    let block = s
        .cursors()
        .into_iter()
        .find(|c| s.paragraph(*c).is_ok_and(|p| p.text.contains("블록")))
        .ok_or_else(|| anyhow::anyhow!("block paragraph"))?;
    s.select(at_offset(block, 3), false)?;
    s.select(at_offset(block, 5), true)?;
    let (generation, cursor) = (s.generation, s.cursor);
    let job = s.save_block_pdf_job()?;
    assert_eq!(job.page_count(), 1);
    assert!(job.convert()?.starts_with(b"%PDF"));
    assert_eq!(
        (s.generation, s.cursor),
        (generation, cursor),
        "document untouched"
    );
    // Page lists for a PDF prepared a few pages per frame: the same pages, and the
    // block as its own copy that later edits do not change.
    assert_eq!(s.print_pdf_pages("2-3", 0)?, [1, 2]);
    assert!(s.print_pdf_pages("9", 0).is_err());
    let copy = s.block_pdf_session()?;
    assert_eq!(copy.page_count()?, 1);
    let mut prep = copy.pdf_prepare(&[0])?;
    assert!(copy.pdf_prepare_step(&mut prep, std::time::Duration::ZERO)?);
    assert!(copy.pdf_finish(prep)?.convert()?.starts_with(b"%PDF"));
    assert_eq!((s.generation, s.cursor), (generation, cursor));
    // The synchronous command keeps its behaviour.
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("block.hwpx");
    let r = s.run("file.save_block", json!({"path": path}))?;
    assert_eq!(r["text"], "블록");
    Ok(())
}

fn at_offset(c: hwpcraft_engine::Cursor, offset: usize) -> hwpcraft_engine::Cursor {
    hwpcraft_engine::Cursor { offset, ..c }
}

/// Page 1's drawn text: the SVG has one text element per character.
fn drawn(s: &Session) -> anyhow::Result<String> {
    let svg = svg(s)?;
    Ok(svg
        .split("</text>")
        .filter_map(|part| part.rsplit('>').next())
        .collect())
}

#[test]
fn master_page_text_is_created_edited_deleted_and_roundtrips() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    let none = s.run("page.master.get", json!({}))?;
    assert_eq!(
        (none["exists"].as_bool(), none["editable"].as_bool()),
        (Some(false), Some(true))
    );
    let v = s.run("page.master.set", json!({"text": "대외비\n둘째 줄"}))?;
    assert_eq!(v["text"], "대외비\n둘째 줄");
    assert!(drawn(&s)?.contains("대외비"), "drawn on the page");
    // Like Hangul, master-page text starts at the body area, not the paper corner
    // (RHWP patch 0014).
    let svg = svg(&s)?;
    let at = svg.find(">대</text>").expect("master glyph");
    let tag = &svg[svg[..at].rfind("<text").expect("text element")..at];
    let number = |key: &str| -> f64 {
        let start = tag.find(key).expect("attribute") + key.len();
        tag[start..]
            .split('"')
            .next()
            .unwrap_or("")
            .parse()
            .unwrap_or(f64::NAN)
    };
    let info = s.page_info(0)?;
    let body_left = info["bodyLeft"].as_f64().unwrap_or(f64::NAN);
    let body_top =
        info["marginTop"].as_f64().unwrap_or(0.0) + info["marginHeader"].as_f64().unwrap_or(0.0);
    assert!((number(" x=\"") - body_left).abs() < 1.0, "{tag}");
    assert!(number(" y=\"") > body_top, "{tag}");
    // One undo step back to no master page.
    s.execute(Command::Undo)?;
    assert_eq!(s.run("page.master.get", json!({}))?["exists"], false);
    s.execute(Command::Redo)?;
    s.run(
        "page.master.set",
        json!({"applyTo": "both", "text": "기밀"}),
    )?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("master.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        let back = r.run("page.master.get", json!({}))?;
        assert_eq!(back["text"], "기밀", "{ext}");
        assert!(drawn(&r)?.contains("기밀"), "{ext} drawn after reopening");
        assert_eq!(texts(&r).first().map(String::as_str), Some("본문"), "{ext}");
    }
    s.run("page.master.delete", json!({}))?;
    assert_eq!(s.run("page.master.get", json!({}))?["exists"], false);
    assert!(!drawn(&s)?.contains("기밀"));
    assert!(
        s.run("page.master.delete", json!({})).is_err(),
        "nothing left to delete"
    );
    Ok(())
}

#[test]
fn master_pages_that_would_lose_content_are_refused() -> anyhow::Result<()> {
    use hwpcraft_doc::model::header_footer::{HeaderFooterApply, MasterPage};
    let mut s = Session::new()?;
    assert!(
        s.run("page.master.set", json!({"applyTo": "last", "text": "x"}))
            .is_err()
    );
    assert!(
        s.run("page.master.set", json!({"text": "a\u{1}b"}))
            .is_err()
    );
    // A section that already has a last-page master page stays as it is.
    s.core.document_mut().sections[0]
        .section_def
        .master_pages
        .push(MasterPage {
            apply_to: HeaderFooterApply::Both,
            is_extension: true,
            ..Default::default()
        });
    let list = s.run("page.master.list", json!({}))?;
    assert_eq!(list["editable"], false);
    assert!(
        list["reason"]
            .as_str()
            .is_some_and(|r| r.contains("마지막"))
    );
    assert!(s.run("page.master.set", json!({"text": "x"})).is_err());
    assert_eq!(
        s.core.document().sections[0].section_def.master_pages.len(),
        1
    );
    Ok(())
}

#[test]
fn odd_and_even_master_pages_follow_hangul() -> anyhow::Result<()> {
    use hwpcraft_doc::model::header_footer::HeaderFooterApply::{Both, Even, Odd};
    let mut s = Session::new()?;
    s.execute(Command::Insert("일".into()))?;
    s.run("insert.page_break", json!({}))?;
    s.execute(Command::Insert("이".into()))?;
    for (kind, text) in [("odd", "홀"), ("both", "양"), ("even", "짝")] {
        let v = s.run("page.master.set", json!({"applyTo": kind, "text": text}))?;
        assert_eq!(
            (v["applyTo"].as_str(), v["text"].as_str()),
            (Some(kind), Some(text))
        );
    }
    let glyphs = |s: &Session, page: u32| -> anyhow::Result<Vec<&'static str>> {
        let svg = s
            .core
            .render_page_svg_native(page)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(["양", "홀", "짝"]
            .into_iter()
            .filter(|g| svg.contains(&format!(">{g}</text>")))
            .collect())
    };
    // Hangul stores 양쪽, 짝수, 홀수 in that order with one section-flag bit each, and an
    // odd or even master page takes the place of the both-pages one on its pages.
    let check = |s: &Session, flags: bool| -> anyhow::Result<()> {
        let sd = &s.core.document().sections[0].section_def;
        let kinds: Vec<_> = sd.master_pages.iter().map(|m| m.apply_to).collect();
        assert_eq!(kinds, [Both, Even, Odd]);
        if flags {
            assert_eq!(sd.flags & 0xe000_0000, 0xe000_0000);
        }
        assert_eq!(glyphs(s, 0)?, ["홀"], "page 1 is odd");
        assert_eq!(glyphs(s, 1)?, ["짝"], "page 2 is even");
        Ok(())
    };
    check(&s, true)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("masters.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        check(&r, ext == "hwp")?;
        let even = r.run("page.master.get", json!({"applyTo": "even"}))?;
        assert_eq!(even["text"], "짝");
    }
    // One undo step per change; deleting the odd one leaves 양쪽 on odd pages.
    s.execute(Command::Undo)?;
    let sd = &s.core.document().sections[0].section_def;
    assert_eq!(sd.master_pages.len(), 2);
    assert_eq!(sd.flags & 0xe000_0000, 0xa000_0000);
    s.execute(Command::Redo)?;
    s.run("page.master.delete", json!({"applyTo": "odd"}))?;
    assert_eq!(
        s.core.document().sections[0].section_def.flags & 0xe000_0000,
        0x6000_0000
    );
    assert_eq!(glyphs(&s, 0)?, ["양"]);
    assert_eq!(glyphs(&s, 1)?, ["짝"]);
    assert!(
        s.run("page.master.delete", json!({"applyTo": "odd"}))
            .is_err_and(|e| e.to_string().contains("홀수 쪽"))
    );
    Ok(())
}

/// PDF page objects (`/Type /Page`, not `/Pages`).
fn pdf_pages(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    text.match_indices("/Type /Page")
        .filter(|(i, _)| !text[i + 11..].starts_with('s'))
        .count()
        + text
            .match_indices("/Type/Page")
            .filter(|(i, _)| !text[i + 10..].starts_with('s'))
            .count()
}

#[test]
fn print_details_pick_pages_repeat_them_and_put_several_on_a_sheet() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    for i in 0..5 {
        if i > 0 {
            s.execute(Command::PageBreak)?;
        }
        s.execute(Command::Insert(format!("{}쪽", i + 1)))?;
    }
    let before = hwpx(&s)?;
    let temp = tempfile::tempdir()?;
    let print = |s: &mut Session, extra: serde_json::Value| -> anyhow::Result<(usize, usize)> {
        let path = temp.path().join("print.pdf");
        let mut params = json!({"path": path, "pages": "all"});
        for (k, v) in extra.as_object().into_iter().flatten() {
            params[k] = v.clone();
        }
        let v = s.run("file.print_pdf", params)?;
        let bytes = std::fs::read(&path)?;
        Ok((
            v["sheets"].as_u64().unwrap_or(0) as usize,
            pdf_pages(&bytes),
        ))
    };
    assert_eq!(print(&mut s, json!({}))?, (5, 5));
    assert_eq!(print(&mut s, json!({"oddEven": "odd"}))?, (3, 3));
    assert_eq!(
        print(&mut s, json!({"oddEven": "even", "copies": 3}))?,
        (6, 6)
    );
    assert_eq!(print(&mut s, json!({"nUp": 2}))?, (3, 3));
    assert_eq!(print(&mut s, json!({"nUp": 4, "copies": 2}))?, (3, 3));
    assert_eq!(
        print(
            &mut s,
            json!({"watermark": {"text": "대외비"}, "reverse": true})
        )?,
        (5, 5)
    );
    assert!(print(&mut s, json!({"nUp": 3})).is_err());
    assert!(print(&mut s, json!({"watermark": {"text": ""}})).is_err());
    // Printing never changes the document.
    assert_eq!(hwpx(&s)?, before);
    Ok(())
}

#[test]
fn print_preview_shows_sheets_as_they_print() -> anyhow::Result<()> {
    use hwpcraft_engine::{PrintLayout, Watermark};
    let mut s = Session::new()?;
    for i in 0..5 {
        if i > 0 {
            s.execute(Command::PageBreak)?;
        }
        s.execute(Command::Insert(format!("{}쪽", i + 1)))?;
    }
    let (one, sheets) = s.print_preview_sheet(0, &PrintLayout::default())?;
    assert_eq!(sheets, 5);
    assert!(one.contains(">1<"));
    let four = PrintLayout {
        n_up: 4,
        watermark: Some(Watermark {
            text: "초안".into(),
            opacity: None,
        }),
        ..Default::default()
    };
    let (sheet, sheets) = s.print_preview_sheet(1, &four)?;
    assert_eq!(sheets, 2);
    assert!(
        sheet.contains(">5<") && !sheet.contains(">1<"),
        "second sheet holds page 5"
    );
    assert!(sheet.contains("초안"));
    let even = PrintLayout {
        odd_even: "even".into(),
        ..Default::default()
    };
    let (sheet, sheets) = s.print_preview_sheet(9, &even)?;
    assert_eq!(sheets, 2, "pages 2 and 4");
    assert!(sheet.contains(">4<"), "past the end shows the last sheet");
    Ok(())
}

#[test]
fn a_label_sheet_is_one_borderless_table_on_a_new_a4_page() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("고친 글".into()))?;
    let params = json!({"rows": 8, "columns": 2, "labelWidthMm": 99.1, "labelHeightMm": 34.0, "gapXMm": 2.5});
    assert!(
        s.run("page.label", params.clone()).is_err(),
        "unsaved changes kept"
    );
    let mut force = params.clone();
    force["force"] = json!(true);
    let v = s.run("page.label", force)?;
    assert_eq!(
        (v["labels"].as_u64(), v["pages"].as_u64()),
        (Some(16), Some(1))
    );
    assert!(!s.dirty() && s.path.is_none());
    let table = s.core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t.clone()),
            _ => None,
        })
        .expect("label table");
    assert_eq!(
        (table.row_count, table.col_count),
        (8, 3),
        "a spacer column between labels"
    );
    let setup = s.page_setup()?;
    assert!((setup.width_mm - 210.0).abs() < 0.1 && (setup.left_mm - 4.65).abs() < 0.1);
    // Too large for A4.
    assert!(
        s.run("page.label", json!({"rows": 9, "columns": 3, "labelWidthMm": 80.0, "labelHeightMm": 40.0, "force": true}))
            .is_err()
    );
    Ok(())
}

#[test]
fn a_column_definition_break_starts_new_columns_and_roundtrips() -> anyhow::Result<()> {
    use hwpcraft_doc::model::paragraph::ColumnBreakType;
    let mut s = Session::new()?;
    s.execute(Command::Insert("제목 본문".into()))?;
    // Caret after "제목 ": the rest starts two columns.
    s.select(
        hwpcraft_doc::Cursor {
            offset: 3,
            ..s.cursor
        },
        false,
    )?;
    let v = s.run(
        "page.column_def_break",
        json!({"count": 2, "spacing_mm": 6.0}),
    )?;
    assert_eq!(v["columns"], 2);
    let check = |s: &Session| {
        let paras = &s.core.document().sections[0].paragraphs;
        let second = paras.get(1).expect("split paragraph");
        assert_eq!(second.column_type, ColumnBreakType::MultiColumn);
        let columns = second.controls.iter().find_map(|c| match c {
            Control::ColumnDef(d) => Some(d.column_count),
            _ => None,
        });
        assert_eq!(columns, Some(2));
        assert!(second.text.starts_with("본문"));
    };
    check(&s);
    s.execute(Command::Undo)?;
    assert_eq!(
        s.core.document().sections[0].paragraphs.len(),
        1,
        "one undo step"
    );
    s.execute(Command::Redo)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("columns.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        check(&r);
    }
    assert!(s.run("page.column_def_break", json!({"count": 9})).is_err());
    Ok(())
}

#[test]
fn a_section_break_starts_a_section_with_the_same_page_setup() -> anyhow::Result<()> {
    use hwpcraft_doc::model::paragraph::ColumnBreakType;
    let mut s = Session::new()?;
    s.run("page.setup", json!({"landscape": true}))?;
    s.run(
        "page.header_footer",
        json!({"is_header": true, "text": "머리글"}),
    )?;
    s.run("page.master.set", json!({"text": "바탕"}))?;
    s.execute(Command::Insert("첫째 문단\n둘째 셋째".into()))?;
    // Caret after "둘째 ": the rest starts section 2.
    s.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 1,
            offset: 3,
            ..s.cursor
        },
        false,
    )?;
    let v = s.run("page.section_break", json!({}))?;
    assert_eq!(
        (v["sections"].as_u64(), v["pages"].as_u64()),
        (Some(2), Some(2))
    );
    assert_eq!(
        (s.cursor.section, s.cursor.paragraph, s.cursor.offset),
        (1, 0, 0)
    );
    let check = |s: &Session| -> anyhow::Result<()> {
        let doc = s.core.document();
        let texts: Vec<Vec<&str>> = doc
            .sections
            .iter()
            .map(|sec| sec.paragraphs.iter().map(|p| p.text.as_str()).collect())
            .collect();
        assert_eq!(texts, [vec!["첫째 문단", "둘째 "], vec!["셋째"]]);
        let (a, b) = (&doc.sections[0].section_def, &doc.sections[1].section_def);
        assert_eq!(
            (b.page_def.width, b.page_def.height, b.page_def.landscape),
            (a.page_def.width, a.page_def.height, a.page_def.landscape)
        );
        assert_eq!(b.master_pages.len(), 1, "master page carried on");
        let head = &doc.sections[1].paragraphs[0];
        assert_eq!(head.column_type, ColumnBreakType::Section);
        assert!(matches!(
            head.controls.first(),
            Some(Control::SectionDef(_))
        ));
        assert!(matches!(head.controls.get(1), Some(Control::ColumnDef(_))));
        // Page 2 shows the header (carried on), the master page and the new section.
        let page2 = s
            .core
            .render_page_svg_native(1)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        for glyph in [">머</text>", ">바</text>", ">셋</text>"] {
            assert!(page2.contains(glyph), "page 2 shows {glyph}");
        }
        Ok(())
    };
    check(&s)?;
    s.execute(Command::Undo)?;
    assert_eq!(s.core.document().sections.len(), 1, "one undo step");
    assert_eq!(
        s.core.document().sections[0].paragraphs[1].text,
        "둘째 셋째"
    );
    s.execute(Command::Redo)?;
    check(&s)?;
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("sections.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        check(&r)?;
        if ext == "hwp" {
            // Hangul reads no further than DocInfo's section count.
            assert_eq!(r.core.document().doc_properties.section_count, 2);
        }
    }

    // At the start of a section's first paragraph an empty paragraph stays behind.
    let mut s = Session::new()?;
    s.execute(Command::Insert("본문".into()))?;
    s.select(hwpcraft_doc::Cursor::default(), false)?;
    s.run("page.section_break", json!({}))?;
    let doc = s.core.document();
    let texts: Vec<Vec<&str>> = doc
        .sections
        .iter()
        .map(|sec| sec.paragraphs.iter().map(|p| p.text.as_str()).collect())
        .collect();
    assert_eq!(texts, [vec![""], vec!["본문"]]);
    assert!(matches!(
        doc.sections[0].paragraphs[0].controls.first(),
        Some(Control::SectionDef(_))
    ));
    Ok(())
}

#[test]
fn backspace_at_a_section_start_joins_the_sections() -> anyhow::Result<()> {
    use hwpcraft_doc::model::paragraph::ColumnBreakType;
    let mut s = Session::new()?;
    s.run("page.master.set", json!({"text": "바탕"}))?;
    s.execute(Command::Insert("앞 문단\n뒤 문단".into()))?;
    s.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 1,
            offset: 0,
            ..s.cursor
        },
        false,
    )?;
    s.run("page.section_break", json!({}))?;
    s.run("page.setup", json!({"landscape": true}))?;
    let split = |s: &Session| {
        let doc = s.core.document();
        assert_eq!(doc.sections.len(), 2);
        assert!(doc.sections[1].section_def.page_def.landscape);
        assert_eq!(doc.sections[1].section_def.master_pages.len(), 1);
    };
    split(&s);
    let joined = |s: &Session| -> anyhow::Result<()> {
        let doc = s.core.document();
        assert_eq!(doc.sections.len(), 1);
        let sec = &doc.sections[0];
        let texts: Vec<&str> = sec.paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["앞 문단", "뒤 문단"]);
        // The first section's definition wins; the copied master page of the second goes.
        assert!(!sec.section_def.page_def.landscape);
        assert_eq!(sec.section_def.master_pages.len(), 1);
        let second = &sec.paragraphs[1];
        assert_eq!(second.column_type, ColumnBreakType::None);
        assert!(
            !second
                .controls
                .iter()
                .any(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
        );
        assert_eq!(second.char_offsets.first(), Some(&0));
        assert_eq!(s.page_count()?, 1);
        Ok(())
    };
    // Backspace at the second section's start; the caret stays before "뒤".
    s.execute(Command::Backspace)?;
    joined(&s)?;
    assert_eq!(
        (s.cursor.section, s.cursor.paragraph, s.cursor.offset),
        (0, 1, 0)
    );
    s.execute(Command::Undo)?;
    split(&s);
    // Delete at the end of the first section does the same and keeps the caret.
    s.select(
        hwpcraft_doc::Cursor {
            section: 0,
            paragraph: 0,
            offset: 4,
            ..s.cursor
        },
        false,
    )?;
    s.execute(Command::Delete)?;
    joined(&s)?;
    assert_eq!(
        (s.cursor.section, s.cursor.paragraph, s.cursor.offset),
        (0, 0, 4)
    );
    let temp = tempfile::tempdir()?;
    for ext in ["hwp", "hwpx"] {
        let path = temp.path().join(format!("joined.{ext}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)?;
        joined(&r)?;
    }
    // The next Delete joins the paragraphs as usual.
    s.execute(Command::Delete)?;
    assert_eq!(
        s.core.document().sections[0].paragraphs[0].text,
        "앞 문단뒤 문단"
    );
    Ok(())
}

#[test]
fn a_page_inside_a_long_paragraph_is_copied_and_deleted_by_its_text() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let words: String = (0..2400).map(|i| format!("낱말{i} ")).collect();
    s.execute(Command::Insert(words.clone()))?;
    assert!(s.page_count()? >= 3, "one paragraph over three pages");
    let at = |s: &Session, page: usize| s.page_first_position(page).map(|c| c.offset);
    let (Some(p2), Some(p3)) = (at(&s, 1), at(&s, 2)) else {
        anyhow::bail!("page starts");
    };
    assert!(0 < p2 && p2 < p3);
    let chars: Vec<char> = words.chars().collect();
    let page2: String = chars[p2..p3].iter().collect();
    // 쪽 복사하기 takes the text of page 2 only.
    let r = s.run("page.copy", json!({"page": 2}))?;
    assert_eq!(r["paragraphs"], 1);
    s.select(
        hwpcraft_doc::Cursor {
            offset: chars.len(),
            ..s.cursor
        },
        false,
    )?;
    let pages = s.page_count()?;
    s.run("page.paste", json!({}))?;
    let paras = &s.core.document().sections[0].paragraphs;
    assert_eq!(paras.last().map(|p| p.text.as_str()), Some(page2.as_str()));
    assert!(s.page_count()? > pages);
    s.execute(Command::Undo)?;
    // 쪽 지우기 removes that text; the paragraph stays one paragraph.
    let pages = s.page_count()?;
    let r = s.run("page.delete", json!({"page": 2}))?;
    assert_eq!(r["pages"].as_u64(), Some(pages as u64 - 1), "{r}");
    let expected: String = chars[..p2].iter().chain(&chars[p3..]).collect();
    let paras = &s.core.document().sections[0].paragraphs;
    assert_eq!(paras.len(), 1);
    assert_eq!(paras[0].text, expected);
    assert_eq!((s.cursor.paragraph, s.cursor.offset), (0, p2));
    s.execute(Command::Undo)?;
    assert_eq!(s.core.document().sections[0].paragraphs[0].text, words);
    Ok(())
}
