use hwpcraft_engine::{Command, EquationOptions, ObjectAddress, Session, ShapeKind, ShapeOptions};
use serde_json::json;

fn first(s: &Session, kind: &str) -> anyhow::Result<ObjectAddress> {
    let list = s.objects();
    let item = list
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["kind"] == kind)
        .ok_or_else(|| anyhow::anyhow!("missing {kind}: {list}"))?;
    Ok(serde_json::from_value(item["address"].clone())?)
}
fn reopen(s: &mut Session, ext: &str) -> anyhow::Result<Session> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(format!("roundtrip.{ext}"));
    s.save(&path)?;
    let mut r = Session::new()?;
    r.open(&path)?;
    Ok(r)
}
#[test]
fn equations_edit_delete_undo_and_roundtrip() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("text.insert", json!({"text":"가😀나"}))?;
    s.run(
        "insert.equation",
        json!({"script":"{a+b} over {c+d}","size_pt":12.0}),
    )?;
    let at = first(&s, "equation")?;
    s.run(
        "equation.set",
        json!({"address":at,"options":{"script":"sqrt {x}","size_pt":16.0}}),
    )?;
    assert_eq!(s.equation(at)?.script, "sqrt {x}");
    s.execute(Command::Undo)?;
    assert_eq!(s.equation(at)?.size_pt, 12.0);
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(
            r.equation(first(&r, "equation")?)?.script,
            "{a+b} over {c+d}"
        );
    }
    s.delete_object(at)?;
    assert!(s.objects().as_array().unwrap().is_empty());
    s.execute(Command::Undo)?;
    assert!(first(&s, "equation").is_ok());
    Ok(())
}
#[test]
fn footnotes_endnotes_keep_number_and_editable_text() -> anyhow::Result<()> {
    for endnote in [false, true] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("본문".into()))?;
        s.insert_note(endnote, "주석 😀")?;
        let kind = if endnote { "endnote" } else { "footnote" };
        let at = first(&s, kind)?;
        assert_eq!(s.note_text(at)?, "주석 😀");
        s.set_note_text(at, "고친 주석")?;
        s.execute(Command::Undo)?;
        assert_eq!(s.note_text(at)?, "주석 😀");
        for ext in ["hwp", "hwpx"] {
            let r = reopen(&mut s, ext)?;
            assert_eq!(r.note_text(first(&r, kind)?)?, "주석 😀", "{ext}");
        }
        s.delete_object(at)?;
        assert!(s.objects().as_array().unwrap().is_empty());
        s.execute(Command::Undo)?;
        assert_eq!(s.note_text(at)?, "주석 😀");
    }
    Ok(())
}
#[test]
fn shapes_textbox_resize_and_roundtrip() -> anyhow::Result<()> {
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Textbox,
    ] {
        let mut s = Session::new()?;
        s.insert_shape(ShapeOptions {
            kind,
            width_mm: 60.0,
            height_mm: 25.0,
            text: if kind == ShapeKind::Textbox {
                "상자 😀".into()
            } else {
                String::new()
            },
        })?;
        let name = if kind == ShapeKind::Textbox {
            "textbox"
        } else {
            "shape"
        };
        let at = first(&s, name)?;
        s.resize_shape(at, 70.0, 30.0)?;
        assert!((s.shape_size(at)?.0 - 70.0).abs() < 0.01);
        s.execute(Command::Undo)?;
        assert!((s.shape_size(at)?.0 - 60.0).abs() < 0.01);
        if kind == ShapeKind::Textbox {
            s.set_textbox_text(at, "수정 😀")?;
            s.execute(Command::Undo)?;
            assert_eq!(s.textbox_text(at)?, "상자 😀");
        }
        for ext in ["hwp", "hwpx"] {
            let r = reopen(&mut s, ext)?;
            let at = first(&r, name)?;
            assert!((r.shape_size(at)?.0 - 60.0).abs() < 0.02);
            if kind == ShapeKind::Textbox {
                assert_eq!(r.textbox_text(at)?, "상자 😀");
            }
        }
        s.delete_object(at)?;
        s.execute(Command::Undo)?;
        assert!(first(&s, name).is_ok());
    }
    Ok(())
}
#[test]
fn style_apply_body_and_cell_undo() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let count = s.styles().as_array().unwrap().len();
    assert!(count > 1);
    s.execute(Command::Insert("스타일 본문".into()))?;
    s.apply_style(1)?;
    assert_eq!(s.paragraph(s.cursor)?.style_id, 1);
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(r.paragraph(r.cursor)?.style_id, 1);
    }
    s.execute(Command::Undo)?;
    assert_eq!(s.paragraph(s.cursor)?.style_id, 0);
    s.execute(Command::Table {
        rows: 2,
        columns: 2,
    })?;
    let cursor = s
        .entries()
        .into_iter()
        .find(|(c, _)| c.cell.is_some())
        .unwrap()
        .0;
    s.select(cursor, false)?;
    s.apply_style(1)?;
    assert_eq!(s.paragraph(s.cursor)?.style_id, 1);
    s.execute(Command::Undo)?;
    assert_eq!(s.paragraph(s.cursor)?.style_id, 0);
    Ok(())
}
#[test]
fn object_validation_preserves_document_and_history() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    let before = s.info();
    let generation = s.generation;
    for p in [
        EquationOptions {
            script: String::new(),
            size_pt: 12.0,
        },
        EquationOptions {
            script: "{".repeat(40),
            size_pt: 12.0,
        },
        EquationOptions {
            script: "x".into(),
            size_pt: f64::NAN,
        },
    ] {
        assert!(s.set_equation(None, p).is_err());
    }
    assert!(
        s.insert_shape(ShapeOptions {
            kind: ShapeKind::Rectangle,
            width_mm: -1.0,
            height_mm: 20.0,
            text: String::new()
        })
        .is_err()
    );
    assert!(s.apply_style(usize::MAX).is_err());
    assert!(s.insert_note(false, "줄\n바꿈").is_err());
    assert!(
        s.delete_object(ObjectAddress {
            section: 999,
            paragraph: 0,
            control: 0
        })
        .is_err()
    );
    assert_eq!(s.info(), before);
    assert_eq!(s.generation, generation);
    assert!(!s.can_undo());
    Ok(())
}

#[test]
fn enter_after_inline_objects_keeps_each_anchor_in_its_paragraph() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.set_equation(
        None,
        EquationOptions {
            script: "a over b".into(),
            size_pt: 12.0,
        },
    )?;
    s.execute(Command::Split)?;
    s.insert_note(false, "각주")?;
    s.insert_note(true, "미주")?;
    s.execute(Command::Split)?;
    s.insert_shape(ShapeOptions {
        kind: ShapeKind::Textbox,
        width_mm: 50.0,
        height_mm: 20.0,
        text: "상자".into(),
    })?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("다음 문단".into()))?;
    assert_eq!(first(&s, "equation")?.paragraph, 0);
    assert_eq!(first(&s, "footnote")?.paragraph, 1);
    assert_eq!(first(&s, "endnote")?.paragraph, 1);
    assert_eq!(first(&s, "textbox")?.paragraph, 2);
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        assert_eq!(first(&r, "equation")?.paragraph, 0, "{ext}");
        assert_eq!(first(&r, "footnote")?.paragraph, 1, "{ext}");
        assert_eq!(first(&r, "endnote")?.paragraph, 1, "{ext}");
        assert_eq!(first(&r, "textbox")?.paragraph, 2, "{ext}");
    }
    Ok(())
}

#[test]
fn split_and_page_break_preserve_text_around_multiple_inline_controls() -> anyhow::Result<()> {
    for page_break in [false, true] {
        let mut s = Session::new()?;
        s.execute(Command::Insert("가😀".into()))?;
        s.insert_note(false, "각주")?;
        s.insert_note(true, "미주")?;
        s.execute(Command::Insert("뒤쪽".into()))?;
        let original = s.paragraph(s.cursor)?.text.clone();
        s.cursor.offset -= 2;
        s.execute(if page_break {
            Command::PageBreak
        } else {
            Command::Split
        })?;
        let paras = &s.core.document().sections[0].paragraphs;
        assert_eq!(format!("{}{}", paras[0].text, paras[1].text), original);
        assert_eq!(paras[1].text, "뒤쪽");
        assert_eq!(first(&s, "footnote")?.paragraph, 0);
        assert_eq!(first(&s, "endnote")?.paragraph, 0);
        s.execute(Command::Undo)?;
        assert_eq!(s.core.document().sections[0].paragraphs[0].text, original);
    }
    Ok(())
}

#[test]
fn reopening_formatted_equations_restores_missing_line_metrics() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.execute(Command::Insert("제목".into()))?;
    s.execute(Command::Character(json!({"fontSize":1800,"bold":true})))?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("수식: ".into()))?;
    s.execute(Command::Character(json!({"fontSize":1100,"bold":false})))?;
    s.set_equation(
        None,
        EquationOptions {
            script: "{a+b} over {c+d}".into(),
            size_pt: 14.0,
        },
    )?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("다음 문단".into()))?;
    s.insert_note(false, "각주")?;
    s.insert_note(true, "미주")?;
    for ext in ["hwp", "hwpx"] {
        let r = reopen(&mut s, ext)?;
        let p = &r.core.document().sections[0].paragraphs[1];
        assert_eq!(p.line_segs.len(), 1, "{ext}");
        let height = p
            .controls
            .iter()
            .find_map(|control| match control {
                hwpcraft_doc::model::control::Control::Equation(eq) => {
                    Some(eq.common.height as i32)
                }
                _ => None,
            })
            .unwrap();
        assert!(p.line_segs[0].line_height >= height, "{ext}");
        assert_eq!(
            r.equation(first(&r, "equation")?)?.script,
            "{a+b} over {c+d}"
        );
    }
    Ok(())
}
