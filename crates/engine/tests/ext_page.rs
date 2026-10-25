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
    let (generation, dirty) = (s.generation, s.dirty());
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
    assert!(!s.can_undo());
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
        assert_eq!(
            header_text(&r).as_deref(),
            Some("\u{17}쪽 \u{15}/\u{16}"),
            "{ext}"
        );
        let v = r.run("page.header_footer.text.get", json!({"header": true}))?;
        assert_eq!(v["text"], "쪽 /", "{ext}");
        assert_eq!(v["fields"].as_array().map(Vec::len), Some(3), "{ext}");
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
