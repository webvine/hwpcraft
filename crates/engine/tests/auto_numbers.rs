use hwpcraft_doc::model::control::{AutoNumberType, Control};
use hwpcraft_engine::{Command, Cursor, Session};
use serde_json::json;

fn field_session() -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("현재 ".into()))?;
    s.run("insert.auto_text", json!({"kind":"page_of_total"}))?;
    s.execute(Command::Insert(" 끝".into()))?;
    Ok(s)
}

#[test]
fn body_page_fields_share_dynamic_screen_and_pdf_values() -> anyhow::Result<()> {
    let mut s = field_session()?;
    let before = s.core.document().sections[0].paragraphs[0].clone();
    let svg = s.render_page_svg(0)?;
    assert!(svg.contains("data-hwpcraft-page-field=\"page\" data-value=\"1\""));
    assert!(svg.contains("data-hwpcraft-page-field=\"total\" data-value=\"1\""));
    assert_eq!(
        s.core.document().sections[0].paragraphs[0].text,
        before.text
    );
    assert_eq!(
        s.core.document().sections[0].paragraphs[0].control_text_positions(),
        before.control_text_positions()
    );
    assert_eq!(
        s.core.document().sections[0].paragraphs[0].tab_extended,
        before.tab_extended
    );
    assert!(s.render_pdf(&[0])?.starts_with(b"%PDF-"));
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("page-fields.{extension}"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        let p = &reopened.core.document().sections[0].paragraphs[0];
        assert_eq!(p.text, before.text);
        let kinds: Vec<_> = p
            .controls
            .iter()
            .filter_map(|c| match c {
                Control::AutoNumber(n) => Some(n.number_type),
                _ => None,
            })
            .collect();
        assert!(kinds.contains(&AutoNumberType::Page));
        assert!(kinds.contains(&AutoNumberType::TotalPage));
        assert!(
            reopened.render_page_svg(0)?.contains("data-value=\"1\""),
            "{extension}"
        );
    }
    Ok(())
}

#[test]
fn multi_digit_page_fields_reserve_width_and_recompute_on_undo() -> anyhow::Result<()> {
    let mut s = field_session()?;
    s.run("page.new_number", json!({"start":123}))?;
    let svg = s.render_page_svg(0)?;
    assert!(svg.contains("data-hwpcraft-page-field=\"page\" data-value=\"123\""));
    let a = s.caret_at(Cursor {
        offset: 3,
        ..Default::default()
    })?;
    let b = s.caret_at(Cursor {
        offset: 4,
        ..Default::default()
    })?;
    assert!(
        b["x"].as_f64().unwrap() - a["x"].as_f64().unwrap() > 15.0,
        "{a} {b}"
    );
    let hit = s.hit_test(
        0,
        (a["x"].as_f64().unwrap() + b["x"].as_f64().unwrap()) / 2.0,
        a["y"].as_f64().unwrap() + a["height"].as_f64().unwrap() / 2.0,
    )?;
    assert!(matches!(hit.offset, 3 | 4), "{hit:?}");
    s.execute(Command::Undo)?;
    let svg = s.render_page_svg(0)?;
    assert!(svg.contains("data-hwpcraft-page-field=\"page\" data-value=\"1\""));
    assert!(!svg.contains("data-value=\"123\""));
    s.execute(Command::Redo)?;
    assert!(s.render_page_svg(0)?.contains("data-value=\"123\""));
    Ok(())
}

#[test]
fn page_field_projection_keeps_nondefault_dpi_and_font_environment() -> anyhow::Result<()> {
    let mut scaled = field_session()?;
    scaled.run("page.new_number", json!({"start":12345}))?;
    scaled.core.set_dpi(144.0);
    scaled
        .core
        .set_font_environment(Some(serde_json::from_value(json!({
            "id":"own-page-field-144-dpi", "substitutions":{}
        }))?))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let source = scaled.core.document().clone();
    scaled.core.set_document(source.clone());
    scaled.generation += 1;
    let source_info: serde_json::Value = serde_json::from_str(
        &scaled
            .core
            .get_page_info_native(0)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    )?;
    assert_eq!(scaled.page_info(0)?["width"], source_info["width"]);
    let a = scaled.caret_at(Cursor {
        offset: 3,
        ..Default::default()
    })?;
    let b = scaled.caret_at(Cursor {
        offset: 4,
        ..Default::default()
    })?;
    let scaled_width = b["x"].as_f64().unwrap() - a["x"].as_f64().unwrap();
    // The actual native literal advance is the reference for the field spacer.
    let mut literal = Session::new()?;
    literal.core.set_dpi(144.0);
    literal.core.set_document(literal.core.document().clone());
    literal.execute(Command::Insert("12345".into()))?;
    let from = literal.caret_at(Cursor::default())?;
    let to = literal.caret_at(Cursor {
        offset: 5,
        ..Default::default()
    })?;
    let literal_width = to["x"].as_f64().unwrap() - from["x"].as_f64().unwrap();
    assert!(
        scaled_width >= literal_width - 0.01,
        "{scaled_width}/{literal_width}"
    );
    assert!(
        scaled_width - literal_width < 0.15,
        "{scaled_width}/{literal_width}"
    );
    assert!(scaled.render_page_svg(0)?.contains("data-value=\"12345\""));
    assert_eq!(
        scaled.core.font_environment().unwrap().id(),
        "own-page-field-144-dpi"
    );
    assert_eq!(
        scaled.core.document().sections[0].paragraphs[0].text,
        source.sections[0].paragraphs[0].text
    );
    assert_eq!(
        scaled.core.document().sections[0].paragraphs[0].tab_extended,
        source.sections[0].paragraphs[0].tab_extended
    );
    Ok(())
}

#[test]
fn fields_follow_page_changes_and_preserve_character_style() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run(
        "format.character",
        json!({"bold":true,"textColor":"#ff0000","fontSize":1800}),
    )?;
    s.run("insert.auto_text", json!({"kind":"page_of_total"}))?;
    s.execute(Command::Insert(" 뒤".into()))?;
    s.run("insert.page_break", json!({}))?;
    s.run("insert.auto_text", json!({"kind":"page_of_total"}))?;
    let first = s.render_page_svg(0)?;
    let second = s.render_page_svg(1)?;
    assert!(first.contains("data-hwpcraft-page-field=\"page\" data-value=\"1\""));
    assert!(first.contains("data-hwpcraft-page-field=\"total\" data-value=\"2\""));
    assert!(second.contains("data-hwpcraft-page-field=\"page\" data-value=\"2\""));
    assert!(first.contains("font-weight=\"bold\""));
    assert!(first.to_ascii_lowercase().contains("#ff0000"));
    Ok(())
}

#[test]
fn multi_digit_field_near_right_margin_does_not_overprint() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("가".repeat(43)))?;
    s.run("insert.auto_text", json!({"kind":"page_number"}))?;
    s.execute(Command::Insert("끝".into()))?;
    s.run("page.new_number", json!({"start":123}))?;
    let svg = s.render_page_svg(0)?;
    let pattern = regex::Regex::new(
        r#"data-hwpcraft-page-field="page" data-value="123" data-x="([^"]+)" data-width="([^"]+)""#,
    )?;
    let caps = pattern.captures(&svg).expect("page field");
    let x = caps[1].parse::<f64>()?;
    let width = caps[2].parse::<f64>()?;
    let info: serde_json::Value =
        serde_json::from_str(&s.core.get_page_info_native(0).map_err(anyhow::Error::msg)?)?;
    let right =
        info["columns"][0]["x"].as_f64().unwrap() + info["columns"][0]["width"].as_f64().unwrap();
    assert!(
        x + width <= right + 0.5,
        "field right={} body right={right}",
        x + width
    );
    Ok(())
}

#[test]
fn view_marks_keep_real_tabs_but_hide_page_field_spacers() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("정상\t".into()))?;
    s.run("insert.auto_text", json!({"kind":"page_number"}))?;
    s.run(
        "view.marks",
        json!({"paragraphMarks":true,"controlCodes":true,"transparentBorders":true}),
    )?;
    let svg = s.render_page_svg(0)?;
    assert!(svg.contains("data-value=\"1\""));
    assert_eq!(
        svg.matches(">→</text>").count(),
        1,
        "only the genuine source tab"
    );
    assert_eq!(
        svg.matches(">∨</text>").count(),
        0,
        "the number carrier is not a space marker"
    );
    assert_eq!(
        svg.matches(">↵</text>").count(),
        1,
        "paragraph mark survives"
    );
    assert!(s.view_marks().paragraph_marks);
    assert!(s.render_pdf(&[0])?.starts_with(b"%PDF-"));
    Ok(())
}

#[test]
fn canonical_page_shapes_survive_saved_native_fields_and_common_output() -> anyhow::Result<()> {
    let cases = [
        (0, "1"),
        (1, "①"),
        (2, "I"),
        (3, "i"),
        (4, "A"),
        (5, "a"),
        (6, "Ⓐ"),
        (7, "ⓐ"),
        (8, "가"),
        (9, "㉮"),
        (10, "ㄱ"),
        (11, "㉠"),
        (12, "일"),
        (13, "一"),
        (14, "㊀"),
        (0x81, "★"),
    ];
    let dir = tempfile::tempdir()?;
    let mut fixture = None;
    let mut fixture_paragraphs = Vec::new();
    for (format, value) in cases {
        let mut s = field_session()?;
        for control in &mut s.core.document_mut().sections[0].paragraphs[0].controls {
            if let Control::AutoNumber(n) = control {
                n.format = format;
                n.user_symbol = '★';
            }
        }
        let svg = s.render_page_svg(0)?;
        for kind in ["page", "total"] {
            assert!(
                svg.contains(&format!(
                    "data-hwpcraft-page-field=\"{kind}\" data-value=\"{value}\""
                )),
                "format {format}: {kind}"
            );
        }
        assert!(s.render_pdf(&[0])?.starts_with(b"%PDF-"));
        let path = dir.path().join(format!("canonical-{format}.hwpx"));
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert!(
            reopened.core.document().sections[0].paragraphs[0]
                .controls
                .iter()
                .filter_map(|c| match c {
                    Control::AutoNumber(n) => Some(n.format),
                    _ => None,
                })
                .all(|f| f == format)
        );
        assert!(
            reopened
                .render_page_svg(0)?
                .contains(&format!("data-value=\"{value}\"")),
            "reopened format {format}"
        );
        if std::env::var_os("HWPCRAFT_WRITE_LAYOUT_FIXTURES").is_some() {
            if fixture.is_none() {
                fixture = Some(s.core.document().clone());
            }
            let mut p = s.core.document().sections[0].paragraphs[0].clone();
            let label = format!("형식 {format}: ");
            let slots: std::collections::BTreeSet<_> = p
                .controls
                .iter()
                .zip(p.control_text_positions())
                .filter_map(|(c, pos)| {
                    matches!(c, Control::AutoNumber(_)).then_some(pos + label.chars().count())
                })
                .collect();
            p.controls.retain(|c| matches!(c, Control::AutoNumber(_)));
            p.ctrl_data_records.clear();
            p.text = format!("{label}{}", p.text);
            p.char_offsets.clear();
            let mut units = 0;
            for (i, ch) in p.text.chars().enumerate() {
                p.char_offsets.push(units);
                units += if slots.contains(&i) {
                    8
                } else {
                    ch.len_utf16() as u32
                };
            }
            p.char_count = units + 1;
            p.line_segs.clear();
            p.stored_text_partition_dirty = true;
            fixture_paragraphs.push(p);
        }
    }
    if let Some(mut doc) = fixture {
        doc.sections[0].paragraphs = fixture_paragraphs;
        doc.sections[0].raw_stream = None;
        doc.sections[0].raw_provenance = None;
        let mut s = Session::new()?;
        s.core.set_document(doc);
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.16/fixtures");
        std::fs::create_dir_all(&output)?;
        let path = output.join("number-shapes.hwpx");
        s.save(&path)?;
        let mut reopened = Session::new()?;
        reopened.open(&path)?;
        assert_eq!(
            reopened
                .render_page_svg(0)?
                .matches("data-hwpcraft-page-field=")
                .count(),
            cases.len() * 2
        );
    }
    Ok(())
}

#[test]
fn object_number_insertion_keeps_controls_before_the_paragraph_split() -> anyhow::Result<()> {
    for cross_entry in [false, true] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("그림 ".into()))?;
        s.run("insert.auto_number", json!({"kind":"picture"}))?;
        s.execute(Command::Insert(" / 쪽 ".into()))?;
        s.run("insert.auto_number", json!({"kind":"total"}))?;
        if cross_entry {
            s.run("insert.auto_text", json!({"kind":"page_number"}))?;
        } else {
            s.run("insert.auto_number", json!({"kind":"page"}))?;
        }
        s.run("insert.auto_number", json!({"kind":"table"}))?;
        s.execute(Command::Undo)?;
        let before = s.core.document().sections[0].paragraphs[0].clone();
        let types = |p: &hwpcraft_doc::model::paragraph::Paragraph| {
            p.controls
                .iter()
                .filter_map(|c| match c {
                    Control::AutoNumber(n) => Some(n.number_type),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            types(&before),
            [
                AutoNumberType::Picture,
                AutoNumberType::TotalPage,
                AutoNumberType::Page
            ]
        );
        let positions: Vec<_> = before
            .controls
            .iter()
            .zip(before.control_text_positions())
            .filter_map(|(c, pos)| {
                matches!(c, Control::AutoNumber(_))
                    .then_some(pos.min(before.text.chars().count() - 1))
            })
            .collect();
        assert_eq!(positions, [3, 9, 10]);
        s.execute(Command::Split)?;
        assert_eq!(
            types(&s.core.document().sections[0].paragraphs[0]),
            types(&before)
        );
        assert!(
            !s.core.document().sections[0].paragraphs[1]
                .controls
                .iter()
                .any(|c| matches!(c, Control::AutoNumber(_)))
        );
        s.execute(Command::Insert("본문".into()))?;
        assert!(
            s.render_page_svg(0)?
                .contains("data-hwpcraft-page-field=\"page\"")
        );
        let dir = tempfile::tempdir()?;
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("adjacent.{extension}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let p = &reopened.core.document().sections[0].paragraphs[0];
            assert_eq!(types(p), types(&before), "{extension}");
            assert_eq!(p.text, before.text);
            assert!(
                !reopened.core.document().sections[0].paragraphs[1]
                    .controls
                    .iter()
                    .any(|c| matches!(c, Control::AutoNumber(_)))
            );
            assert!(
                reopened
                    .render_page_svg(0)?
                    .contains("data-hwpcraft-page-field=\"page\" data-value=\"1\"")
            );
        }
    }
    Ok(())
}

#[test]
fn number_insertions_preserve_memo_link_indices_and_control_data() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("앞 링크 메모 뒤".into()))?;
    s.select(
        Cursor {
            offset: 2,
            ..Default::default()
        },
        false,
    )?;
    s.select(
        Cursor {
            offset: 4,
            ..Default::default()
        },
        true,
    )?;
    s.insert_hyperlink("https://example.com/keep?x=1&y=2", None)?;
    s.select(
        Cursor {
            offset: 5,
            ..Default::default()
        },
        false,
    )?;
    s.select(
        Cursor {
            offset: 7,
            ..Default::default()
        },
        true,
    )?;
    let memo = s.run(
        "memo.create",
        json!({"text":"번호와 함께 보존할 메모","author":"작성자"}),
    )?;
    let memo_id = memo["id"].clone();
    let fields: Vec<_> = s.core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Control::Field(f) => Some((i, f.field_id)),
            _ => None,
        })
        .collect();
    {
        let p = &mut s.core.document_mut().sections[0].paragraphs[0];
        p.ctrl_data_records.resize(p.controls.len(), None);
        for (i, id) in &fields {
            p.ctrl_data_records[*i] = Some(id.to_le_bytes().to_vec());
        }
    }
    s.select(Cursor::default(), false)?;
    s.run("insert.auto_number", json!({"kind":"page"}))?;
    s.select(
        Cursor {
            offset: 7,
            ..Default::default()
        },
        false,
    )?;
    s.run("insert.auto_text", json!({"kind":"total_pages"}))?;
    s.run("insert.auto_number", json!({"kind":"page"}))?;
    s.execute(Command::Undo)?;
    s.execute(Command::Redo)?;
    let p = s.core.document().sections[0].paragraphs[0].clone();
    let actual: Vec<_> = p
        .controls
        .iter()
        .zip(p.control_text_positions())
        .filter_map(|(c, pos)| matches!(c, Control::AutoNumber(_)).then_some(pos))
        .collect();
    assert_eq!(
        actual,
        [0, 7, 8],
        "source text {:?}, offsets {:?}",
        p.text,
        p.char_offsets
    );
    for (_, id) in &fields {
        let index = p
            .controls
            .iter()
            .position(|c| matches!(c,Control::Field(f) if f.field_id==*id))
            .ok_or_else(|| anyhow::anyhow!("field missing"))?;
        assert!(p.field_ranges.iter().any(|r| r.control_idx == index));
        assert_eq!(p.ctrl_data_records[index], Some(id.to_le_bytes().to_vec()));
    }
    assert_eq!(s.hyperlinks()?[0].text, "링크");
    assert_eq!(
        s.run("memo.get", json!({"id":memo_id}))?["text"],
        "번호와 함께 보존할 메모"
    );
    s.render_page_svg(0).map_err(|e| {
        anyhow::anyhow!(
            "source render: {e}; text {:?}; offsets {:?}; positions {:?}",
            p.text,
            p.char_offsets,
            p.control_text_positions()
        )
    })?;
    let dir = tempfile::tempdir()?;
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("fields.{extension}"));
        s.save(&path)?;
        let mut r = Session::new()?;
        r.open(&path)
            .map_err(|e| anyhow::anyhow!("open {extension}: {e}"))?;
        assert_eq!(r.hyperlinks()?[0].uri, "https://example.com/keep?x=1&y=2");
        assert_eq!(r.hyperlinks()?[0].text, "링크");
        assert_eq!(
            r.run("memo.get", json!({"id":memo_id}))?["text"],
            "번호와 함께 보존할 메모"
        );
        assert!(
            r.render_page_svg(0)
                .map_err(|e| anyhow::anyhow!(
                    "render {extension}: {e}; {:?}",
                    r.core.document().sections[0].paragraphs[0]
                ))?
                .contains("data-hwpcraft-page-field=\"page\"")
        );
    }
    Ok(())
}

#[test]
fn unexpected_page_field_placeholder_keeps_the_document_openable() -> anyhow::Result<()> {
    let mut s = field_session()?;
    // Replace the page-number placeholder with a letter, as an unusual stored
    // layout might have it. The field cannot be projected, but the document must
    // still open and paginate with RHWP's own rendering of that position.
    {
        let p = &mut s.core.document_mut().sections[0].paragraphs[0];
        let positions = p.control_text_positions();
        let offset = p
            .controls
            .iter()
            .zip(&positions)
            .find_map(|(c, at)| match c {
                Control::AutoNumber(n) if n.number_type == AutoNumberType::Page => Some(*at),
                _ => None,
            })
            .expect("page field");
        let mut chars: Vec<char> = p.text.chars().collect();
        chars[offset] = 'X';
        p.text = chars.into_iter().collect();
    }
    s.generation += 1;
    assert_eq!(s.page_count()?, 1);
    assert!(s.render_page_svg(0)?.contains("<svg"));
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("odd-placeholder.hwpx");
    s.save(&path)?;
    let mut reopened = Session::new()?;
    reopened.open(&path)?;
    assert_eq!(reopened.page_count()?, 1);
    assert!(reopened.render_page_svg(0)?.contains("<svg"));
    assert!(reopened.presentation_report()?["pageFields"].is_u64());
    Ok(())
}
