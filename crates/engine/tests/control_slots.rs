//! Controls inserted or removed by section breaks, column-definition breaks, page hiding and
//! header deletion keep field ranges, highlighter marks and later controls where they were.
use hwpcraft_doc::Cursor;
use hwpcraft_doc::model::control::Control;
use hwpcraft_engine::Session;
use serde_json::json;

fn at(section: usize, paragraph: usize, offset: usize) -> Cursor {
    Cursor {
        section,
        paragraph,
        offset,
        cell: None,
    }
}

/// Field ranges that do not point at a field or hyperlink control.
fn broken_ranges(s: &Session) -> Vec<String> {
    let mut out = Vec::new();
    for (si, sec) in s.core.document().sections.iter().enumerate() {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            for fr in &p.field_ranges {
                if !matches!(
                    p.controls.get(fr.control_idx),
                    Some(Control::Field(_) | Control::Hyperlink(_))
                ) {
                    out.push(format!("s{si} p{pi} control {}", fr.control_idx));
                }
            }
        }
    }
    out
}

/// Hyperlinks of the paragraph at `c` (`hyperlink.list` lists the caret's paragraph).
fn links(s: &mut Session, c: Cursor) -> anyhow::Result<usize> {
    s.select(c, false)?;
    let list = s.run("hyperlink.list", json!({}))?;
    Ok(list["items"]
        .as_array()
        .or(list["links"].as_array())
        .or(list.as_array())
        .map_or(0, Vec::len))
}

fn reopen(s: &mut Session, name: &str) -> anyhow::Result<Session> {
    let dir = std::env::temp_dir().join(format!("hwpcraft-control-slots-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    s.save(&path)?;
    let mut back = Session::new()?;
    back.open(&path)?;
    Ok(back)
}

/// (kind, text position) of each control of a body paragraph.
fn controls(s: &Session, section: usize, paragraph: usize) -> Vec<(String, usize)> {
    let p = &s.core.document().sections[section].paragraphs[paragraph];
    p.controls
        .iter()
        .zip(p.control_text_positions())
        .map(|(c, pos)| {
            let kind = format!("{c:?}");
            (
                kind[..kind.find('(').unwrap_or(kind.len())].to_string(),
                pos,
            )
        })
        .collect()
}

/// "가나다 링크 라마" with a hyperlink over "링크" and a highlight over "라마".
fn linked() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": "가나다 "}))?;
    s.run(
        "insert.hyperlink",
        json!({"uri": "https://a.example", "text": "링크"}),
    )?;
    s.run("text.insert", json!({"text": " 라마"}))?;
    let len = s.core.document().sections[0].paragraphs[0]
        .text
        .chars()
        .count();
    s.select(at(0, 0, len - 2), false)?;
    s.select(at(0, 0, len), true)?;
    s.run("format.highlight", json!({"color": "#ffff00"}))?;
    assert_eq!(links(&mut s, at(0, 0, 0))?, 1);
    Ok(s)
}

fn highlight(s: &mut Session, c: Cursor) -> anyhow::Result<serde_json::Value> {
    s.select(c, false)?;
    Ok(s.run("format.highlight.get", json!({}))?["ranges"].clone())
}

#[test]
fn a_section_break_keeps_the_hyperlink_and_highlight_it_moves() -> anyhow::Result<()> {
    let mut s = linked()?;
    s.select(at(0, 0, 2), false)?;
    s.run("page.section_break", json!({}))?;
    assert_eq!(s.core.document().sections.len(), 2);
    assert_eq!(broken_ranges(&s), Vec::<String>::new());
    assert_eq!(links(&mut s, at(1, 0, 0))?, 1);
    let ranges = highlight(&mut s, at(1, 0, 0))?;
    // "다 링크 라마": the highlight stays on "라마".
    assert_eq!(
        ranges,
        json!([{"start": 5, "end": 7, "color": "#ffff00"}]),
        "{ranges}"
    );
    for name in ["break.hwp", "break.hwpx"] {
        let mut back = reopen(&mut s, name)?;
        assert_eq!(broken_ranges(&back), Vec::<String>::new(), "{name}");
        assert_eq!(links(&mut back, at(1, 0, 0))?, 1, "{name}");
        assert_eq!(highlight(&mut back, at(1, 0, 0))?, ranges, "{name}");
    }
    // Joining the sections again (Backspace at the second one's start) gives the head back.
    s.select(at(1, 0, 0), false)?;
    s.execute(hwpcraft_engine::Command::Backspace)?;
    assert_eq!(s.core.document().sections.len(), 1);
    assert_eq!(broken_ranges(&s), Vec::<String>::new());
    assert_eq!(links(&mut s, at(0, 1, 0))?, 1);
    assert_eq!(highlight(&mut s, at(0, 1, 0))?, ranges);
    let mut back = reopen(&mut s, "joined.hwp")?;
    assert_eq!(links(&mut back, at(0, 1, 0))?, 1);
    Ok(())
}

/// "가나[수식]다라" style paragraph: `before` characters, an equation, then `after`.
fn with_equation(before: &str, after: &str) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text": before}))?;
    s.run("insert.equation", json!({"script": "x", "size_pt": 12}))?;
    s.run("text.insert", json!({"text": after}))?;
    Ok(s)
}

#[test]
fn a_column_definition_break_leaves_an_equation_in_place() -> anyhow::Result<()> {
    let mut s = with_equation("가나다", "라")?;
    s.select(at(0, 0, 2), false)?;
    s.run("page.column_def_break", json!({"count": 2}))?;
    let p = &s.core.document().sections[0].paragraphs[1];
    assert!(p.text.starts_with('다'), "{:?}", p.text);
    let equation = 1;
    assert_eq!(
        controls(&s, 0, 1),
        [("ColumnDef".into(), 0), ("Equation".into(), equation)]
    );
    // Again at the same place: the column definition is replaced, not doubled.
    s.select(at(0, 1, 0), false)?;
    s.run("page.column_def_break", json!({"count": 3}))?;
    assert_eq!(
        controls(&s, 0, 1),
        [("ColumnDef".into(), 0), ("Equation".into(), equation)]
    );
    let back = reopen(&mut s, "columns.hwp")?;
    assert_eq!(
        controls(&back, 0, 1),
        [("ColumnDef".into(), 0), ("Equation".into(), equation)]
    );
    assert_eq!(back.page_count()?, s.page_count()?);
    Ok(())
}

#[test]
fn hiding_page_parts_leaves_an_equation_in_place() -> anyhow::Result<()> {
    let mut s = with_equation("가나", "다라")?;
    let before = controls(&s, 0, 0);
    let equation = before.iter().find(|c| c.0 == "Equation").map(|c| c.1);
    s.select(at(0, 0, 0), false)?;
    let hide = json!({"header": true, "footer": false, "master_page": false,
        "border": false, "fill": false, "page_number": false});
    s.run("page.hide", hide)?;
    let after = controls(&s, 0, 0);
    assert_eq!(
        after.iter().find(|c| c.0 == "Equation").map(|c| c.1),
        equation,
        "{after:?}"
    );
    assert!(after.iter().any(|c| c.0 == "PageHide"), "{after:?}");
    let mut back = reopen(&mut s, "hide.hwp")?;
    assert_eq!(controls(&back, 0, 0), after);
    back.select(at(0, 0, 0), false)?;
    assert_eq!(back.run("page.hide.get", json!({}))?["hideHeader"], true);
    // Clearing every part takes the control and its slot out again.
    s.run(
        "page.hide",
        json!({"header": false, "footer": false, "master_page": false,
            "border": false, "fill": false, "page_number": false}),
    )?;
    assert_eq!(controls(&s, 0, 0), before);
    Ok(())
}

/// The document's fields without their stream positions, which a removed control moves.
fn fields(s: &mut Session) -> anyhow::Result<serde_json::Value> {
    let mut list = s.run("field.list", json!({}))?;
    for field in list.as_array_mut().into_iter().flatten() {
        if let Some(field) = field.as_object_mut() {
            field.remove("startPos");
            field.remove("endPos");
        }
    }
    Ok(list)
}

#[test]
fn deleting_a_header_keeps_a_later_field() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "page.header_footer.text",
        json!({"header": true, "apply_to": "both", "text": "머리"}),
    )?;
    s.select(at(0, 0, 0), false)?;
    s.run("text.insert", json!({"text": "이름: "}))?;
    s.run("insert.field", json!({"guide": "이름", "name": "name1"}))?;
    let id = fields(&mut s)?[0]["fieldId"].clone();
    s.run("field.set", json!({"field_id": id, "value": "홍길동"}))?;
    s.run("text.insert", json!({"text": " 끝"}))?;
    let before = fields(&mut s)?;
    assert_eq!(before.as_array().map(Vec::len), Some(1), "{before}");
    let kinds = controls(&s, 0, 0);
    assert!(
        kinds.iter().position(|c| c.0 == "Header") < kinds.iter().position(|c| c.0 == "Field"),
        "{kinds:?}"
    );
    s.run(
        "page.header_footer.delete",
        json!({"header": true, "apply_to": "both"}),
    )?;
    assert!(!controls(&s, 0, 0).iter().any(|c| c.0 == "Header"));
    assert_eq!(broken_ranges(&s), Vec::<String>::new());
    assert_eq!(fields(&mut s)?, before);
    for name in ["header.hwp", "header.hwpx"] {
        let mut back = reopen(&mut s, name)?;
        assert_eq!(fields(&mut back)?, before, "{name}");
        assert_eq!(
            back.core.document().sections[0].paragraphs[0].text,
            s.core.document().sections[0].paragraphs[0].text,
            "{name}"
        );
    }
    Ok(())
}
