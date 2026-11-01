//! Page-number / total-pages fields in table cells (any depth), text boxes and
//! footnote/endnote paragraphs show the number of the page they are drawn on,
//! with their width reserved like body fields, and are saved unchanged.
use hwpcraft_doc::model::control::{AutoNumberType, Control};
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_engine::{Cell, Command, ContextAddress, ContextStep, Cursor, NestedCaret, Session};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Cell,
    NestedCell,
    TextBox,
    /// The field right after the note number, and in a second note paragraph.
    Footnote,
    Endnote,
}

const KINDS: [Kind; 5] = [
    Kind::Cell,
    Kind::NestedCell,
    Kind::TextBox,
    Kind::Footnote,
    Kind::Endnote,
];

/// "앞 <쪽>/<전체> 뒤" as `insert.auto_text` writes it in body text, to copy into a
/// container (a second paragraph, so it holds no section definition).
fn field_paragraph() -> anyhow::Result<Paragraph> {
    let mut s = Session::new()?;
    s.execute(Command::Split)?;
    s.execute(Command::Insert("앞 ".into()))?;
    s.run("insert.auto_text", json!({"kind":"page_of_total"}))?;
    s.execute(Command::Insert(" 뒤".into()))?;
    let mut p = s.core.document().sections[0].paragraphs[1].clone();
    anyhow::ensure!(
        p.controls
            .iter()
            .all(|c| matches!(c, Control::AutoNumber(_))),
        "{:?}",
        p.controls
    );
    p.invalidate_layout_inputs();
    Ok(p)
}

/// `field` behind a note's own number placeholder (the note's first paragraph).
fn after_note_number(note_first: &Paragraph, field: &Paragraph) -> Paragraph {
    let mut p = field.clone();
    p.text = format!(" {}", field.text);
    p.char_offsets = std::iter::once(0)
        .chain(field.char_offsets.iter().map(|o| o + 8))
        .collect();
    p.char_count = field.char_count + 8;
    p.controls.insert(0, note_first.controls[0].clone());
    if !p.ctrl_data_records.is_empty() {
        p.ctrl_data_records.insert(0, None);
    }
    for shape in p.char_shapes.iter_mut().skip(1) {
        shape.start_pos += 8;
    }
    p.invalidate_layout_inputs();
    p
}

/// Two pages starting at page number 123, each with one container of `kind` that
/// holds the field.
fn document(kind: Kind) -> anyhow::Result<Session> {
    let mut s = Session::new()?;
    s.run("page.new_number", json!({"start":123}))?;
    for page in 0..2 {
        s.execute(Command::Insert("본문".into()))?;
        match kind {
            Kind::Footnote => {
                s.run("insert.footnote", json!({"text":"각주"}))?;
            }
            Kind::Endnote => {
                s.run("insert.endnote", json!({"text":"미주"}))?;
            }
            _ => {}
        }
        s.execute(Command::Split)?;
        match kind {
            Kind::Cell | Kind::NestedCell => {
                s.run("table.create", json!({"rows":1,"columns":2}))?;
                let c = s.cursor;
                let len = s.core.document().sections[0].paragraphs[c.paragraph]
                    .text
                    .chars()
                    .count();
                s.select(
                    Cursor {
                        cell: None,
                        offset: len,
                        ..c
                    },
                    false,
                )?;
                s.execute(Command::Split)?;
                if kind == Kind::NestedCell {
                    s.run(
                        "context.table.create",
                        json!({"address":{"section":0,"paragraph":c.paragraph,
                            "path":[{"control":c.cell.unwrap().control,"cell":1,"paragraph":0}]},
                            "rows":1,"columns":1}),
                    )?;
                }
            }
            Kind::TextBox => {
                s.run(
                    "shape.insert",
                    json!({"kind":"textbox","width_mm":60.0,"height_mm":20.0,"paragraphs":["글상자"]}),
                )?;
                for _ in 0..6 {
                    s.execute(Command::Split)?;
                }
            }
            _ => {}
        }
        if page == 0 {
            s.run("insert.page_break", json!({}))?;
        }
    }
    let field = field_paragraph()?;
    for p in &mut s.core.document_mut().sections[0].paragraphs {
        for c in &mut p.controls {
            match (kind, c) {
                (Kind::Cell, Control::Table(t)) => t.cells[0].paragraphs[0] = field.clone(),
                (Kind::NestedCell, Control::Table(t)) => {
                    let Some(Control::Table(inner)) = t.cells[1]
                        .paragraphs
                        .iter_mut()
                        .flat_map(|p| p.controls.iter_mut())
                        .find(|c| matches!(c, Control::Table(_)))
                    else {
                        anyhow::bail!(
                            "nested table: {:?}",
                            t.cells[1]
                                .paragraphs
                                .iter()
                                .map(|p| (&p.text, p.controls.len()))
                                .collect::<Vec<_>>()
                        );
                    };
                    inner.cells[0].paragraphs[0] = field.clone();
                }
                (Kind::TextBox, Control::Shape(shape)) => {
                    let text_box = shape
                        .drawing_mut()
                        .and_then(|d| d.text_box.as_mut())
                        .ok_or_else(|| anyhow::anyhow!("text box"))?;
                    text_box.paragraphs[0] = field.clone();
                }
                (Kind::Footnote, Control::Footnote(note)) => {
                    note.paragraphs[0] = after_note_number(&note.paragraphs[0], &field);
                    note.paragraphs.push(field.clone());
                }
                (Kind::Endnote, Control::Endnote(note)) => note.paragraphs.push(field.clone()),
                _ => {}
            }
        }
    }
    s.core.set_document(s.core.document().clone());
    s.generation += 1;
    Ok(s)
}

/// (kind, value, x, width) of every page-field overlay of an SVG page.
fn overlays(svg: &str) -> Vec<(String, String, f64, f64)> {
    let pattern = regex::Regex::new(
        r#"data-hwpcraft-page-field="([a-z]+)" data-value="([^"]*)" data-x="([^"]+)" data-width="([^"]+)""#,
    )
    .unwrap();
    pattern
        .captures_iter(svg)
        .map(|c| {
            (
                c[1].to_owned(),
                c[2].to_owned(),
                c[3].parse().unwrap(),
                c[4].parse().unwrap(),
            )
        })
        .collect()
}

/// Glyphs drawn outside the page-field overlays.
fn native_glyphs(svg: &str) -> Vec<String> {
    let groups = regex::Regex::new(r#"(?s)<g data-hwpcraft-page-field=.*?</g>"#).unwrap();
    let text = regex::Regex::new(r#"<text[^>]*>([^<]*)</text>"#).unwrap();
    let rest = groups.replace_all(svg, "");
    text.captures_iter(&rest).map(|c| c[1].to_owned()).collect()
}

/// The container paragraphs of the field, as saved: (text, controls with positions).
fn saved_form(s: &Session, kind: Kind) -> Vec<(String, Vec<(String, usize)>)> {
    let describe = |p: &Paragraph| {
        let controls = p
            .controls
            .iter()
            .zip(p.control_text_positions())
            .map(|(c, at)| match c {
                Control::AutoNumber(n) => (format!("{:?}", n.number_type), at),
                other => (format!("{:?}", std::mem::discriminant(other)), at),
            })
            .collect();
        (p.text.clone(), controls)
    };
    let mut out = Vec::new();
    for p in &s.core.document().sections[0].paragraphs {
        for c in &p.controls {
            match (kind, c) {
                (Kind::Cell, Control::Table(t)) => out.push(describe(&t.cells[0].paragraphs[0])),
                (Kind::NestedCell, Control::Table(t)) => {
                    if let Some(Control::Table(inner)) = t.cells[1]
                        .paragraphs
                        .iter()
                        .flat_map(|p| p.controls.iter())
                        .find(|c| matches!(c, Control::Table(_)))
                    {
                        out.push(describe(&inner.cells[0].paragraphs[0]));
                    }
                }
                (Kind::TextBox, Control::Shape(shape)) => {
                    if let Some(tb) = shape.drawing().and_then(|d| d.text_box.as_ref()) {
                        out.extend(tb.paragraphs.iter().map(describe));
                    }
                }
                (Kind::Footnote, Control::Footnote(note)) => {
                    out.extend(note.paragraphs.iter().map(describe))
                }
                (Kind::Endnote, Control::Endnote(note)) => {
                    out.extend(note.paragraphs.iter().map(describe))
                }
                _ => {}
            }
        }
    }
    out
}

/// Every page shows its own number in each field drawn on it, the document total in
/// each total field, and no second number RHWP drew itself.
fn assert_page_values(s: &Session, kind: Kind) -> anyhow::Result<()> {
    let pages = s.page_count()?;
    let total = pages.to_string();
    let mut page_fields = 0;
    for page in 0..pages {
        let svg = s.render_page_svg(page)?;
        let value = (123 + page).to_string();
        for (field, shown, _, width) in overlays(&svg) {
            if field == "page" {
                page_fields += 1;
                assert_eq!(shown, value, "{kind:?}: page {page}");
                assert!(width > 15.0, "{kind:?}: three digits keep their width");
            } else {
                assert_eq!(shown, total, "{kind:?}: total on page {page}");
            }
        }
        // 123/124 contain the only 3 or 4 of these pages.
        let last = value.chars().last().unwrap().to_string();
        assert!(
            !native_glyphs(&svg).contains(&last),
            "{kind:?}: page {page} draws a second number"
        );
    }
    let per_container = if kind == Kind::Footnote { 2 } else { 1 };
    assert_eq!(
        page_fields,
        2 * per_container,
        "{kind:?}: every field drawn"
    );
    Ok(())
}

#[test]
fn container_fields_show_the_page_they_are_drawn_on() -> anyhow::Result<()> {
    for kind in KINDS {
        let s = document(kind)?;
        assert_eq!(s.page_count()?, 2, "{kind:?}");
        assert_page_values(&s, kind)?;
        // Cells and text boxes: page 1 and page 2 each hold one.
        if !matches!(kind, Kind::Endnote) {
            for page in 0..2 {
                let svg = s.render_page_svg(page)?;
                let value = (123 + page).to_string();
                assert!(
                    overlays(&svg)
                        .iter()
                        .any(|(f, v, _, _)| f == "page" && *v == value),
                    "{kind:?}: page {page}"
                );
            }
        }
        assert!(s.render_pdf(&[0, 1])?.starts_with(b"%PDF-"));
    }
    Ok(())
}

#[test]
fn container_fields_are_saved_unchanged_and_shown_after_reopening() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for kind in KINDS {
        let mut s = document(kind)?;
        let before = saved_form(&s, kind);
        assert!(!before.is_empty(), "{kind:?}");
        s.render_page_svg(0)?;
        s.render_page_svg(1)?;
        assert_eq!(
            saved_form(&s, kind),
            before,
            "{kind:?}: rendering edits nothing"
        );
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("{kind:?}.{extension}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(saved_form(&reopened, kind), before, "{kind:?} {extension}");
            assert_page_values(&reopened, kind)?;
        }
    }
    Ok(())
}

/// Caret x at `offset` of the first field paragraph of `kind`, and the field's width.
fn caret_step(s: &mut Session, kind: Kind) -> anyhow::Result<(f64, f64)> {
    let svg = s.render_page_svg(0)?;
    let (_, _, _, width) = overlays(&svg)
        .into_iter()
        .find(|(f, ..)| f == "page")
        .ok_or_else(|| anyhow::anyhow!("no field"))?;
    // "앞 " then the page field at offset 2.
    let (host, control) = s.core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Table(_) | Control::Shape(_)))
                .map(|c| (p, c))
        })
        .ok_or_else(|| anyhow::anyhow!("no container"))?;
    let x = |v: Value| v["x"].as_f64().unwrap_or(f64::NAN);
    let (a, b) = match kind {
        Kind::Cell => {
            let at = |offset| Cursor {
                section: 0,
                paragraph: host,
                offset,
                cell: Some(Cell {
                    control,
                    index: 0,
                    paragraph: 0,
                }),
            };
            (x(s.caret_at(at(2))?), x(s.caret_at(at(3))?))
        }
        Kind::NestedCell | Kind::TextBox => {
            let mut path = vec![];
            if kind == Kind::NestedCell {
                let Control::Table(outer) =
                    &s.core.document().sections[0].paragraphs[host].controls[control]
                else {
                    anyhow::bail!("outer table");
                };
                let (paragraph, inner) = outer.cells[1]
                    .paragraphs
                    .iter()
                    .enumerate()
                    .find_map(|(k, p)| {
                        p.controls
                            .iter()
                            .position(|c| matches!(c, Control::Table(_)))
                            .map(|c| (k, c))
                    })
                    .ok_or_else(|| anyhow::anyhow!("nested table"))?;
                path.push(ContextStep::TableCell {
                    control,
                    cell: 1,
                    paragraph,
                });
                path.push(ContextStep::TableCell {
                    control: inner,
                    cell: 0,
                    paragraph: 0,
                });
            } else {
                path.push(ContextStep::TextBox {
                    control,
                    shape_path: vec![],
                    paragraph: 0,
                });
            }
            let address = ContextAddress {
                version: 2,
                section: 0,
                paragraph: host,
                path,
            };
            s.set_nested(NestedCaret::at(address.clone(), 2), false)?;
            let a = x(s.nested_caret_rect()?);
            s.set_nested(NestedCaret::at(address, 3), false)?;
            (a, x(s.nested_caret_rect()?))
        }
        _ => unreachable!(),
    };
    Ok((b - a, width))
}

#[test]
fn the_caret_crosses_a_container_field_by_its_displayed_width() -> anyhow::Result<()> {
    for kind in [Kind::Cell, Kind::NestedCell, Kind::TextBox] {
        let mut s = document(kind)?;
        let (step, width) = caret_step(&mut s, kind)?;
        assert!(
            (step - width).abs() < 0.6,
            "{kind:?}: caret step {step}, field width {width}"
        );
    }
    Ok(())
}

/// A footer page number RHWP draws itself is not projected or drawn twice.
#[test]
fn header_and_footer_numbers_are_left_to_rhwp() -> anyhow::Result<()> {
    let mut s = document(Kind::Cell)?;
    s.run("page.footer", json!({"apply_to":"both","template":2}))?;
    for page in 0..2 {
        let svg = s.render_page_svg(page)?;
        let fields = overlays(&svg);
        assert_eq!(fields.len(), 2, "only the cell's page and total fields");
        let last = (123 + page).to_string().chars().last().unwrap().to_string();
        assert_eq!(
            native_glyphs(&svg).iter().filter(|g| **g == last).count(),
            1,
            "page {page}: the footer number once"
        );
    }
    Ok(())
}

#[test]
fn container_number_formats_follow_the_field() -> anyhow::Result<()> {
    let mut s = document(Kind::Cell)?;
    for p in &mut s.core.document_mut().sections[0].paragraphs {
        for c in &mut p.controls {
            if let Control::Table(t) = c {
                for c in &mut t.cells[0].paragraphs[0].controls {
                    if let Control::AutoNumber(n) = c
                        && n.number_type == AutoNumberType::Page
                    {
                        n.format = 2; // upper-case Roman
                    }
                }
            }
        }
    }
    s.generation += 1;
    let svg = s.render_page_svg(0)?;
    assert!(
        overlays(&svg)
            .iter()
            .any(|(f, v, ..)| f == "page" && v == "CXXIII"),
        "{:?}",
        overlays(&svg)
    );
    Ok(())
}

/// A title row RHWP repeats on every page of a long table shows each page its own
/// number (RHWP draws none in the continued table).
#[test]
fn repeated_title_rows_show_the_number_of_each_page() -> anyhow::Result<()> {
    let mut s = Session::new()?;
    s.run("page.new_number", json!({"start":123}))?;
    s.run("table.create", json!({"rows":80,"columns":2}))?;
    let field = field_paragraph()?;
    for p in &mut s.core.document_mut().sections[0].paragraphs {
        for c in &mut p.controls {
            if let Control::Table(t) = c {
                t.repeat_header = true;
                for cell in t.cells.iter_mut().filter(|cell| cell.row == 0) {
                    cell.is_header = true;
                }
                t.cells[0].paragraphs[0] = field.clone();
            }
        }
    }
    s.core.set_document(s.core.document().clone());
    s.generation += 1;
    let pages = s.page_count()?;
    assert!(pages >= 2);
    for page in 0..pages {
        let svg = s.render_page_svg(page)?;
        let shown: Vec<_> = overlays(&svg)
            .into_iter()
            .map(|(kind, value, ..)| (kind, value))
            .collect();
        assert_eq!(
            shown,
            [
                ("page".to_owned(), (123 + page).to_string()),
                ("total".to_owned(), pages.to_string())
            ],
            "page {page}"
        );
    }
    Ok(())
}
