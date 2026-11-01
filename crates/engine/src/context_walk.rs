//! Owner: format area. Paragraphs outside the body caret's reach — headers, footers,
//! footnotes, endnotes, text boxes and nested table cells — and their caret geometry,
//! for work done over the whole document on save or render (tab widths, highlights).
use super::*;
use hwpcraft_doc::model::document::Document;
use hwpcraft_doc::model::header_footer::HeaderFooterApply;
use hwpcraft_doc::model::shape::ShapeObject;

/// Deepest container nesting walked (as deep as `context.v2` addresses go).
const MAX_DEPTH: usize = 64;

/// The `context.v2` address of every paragraph in a container below the body that
/// `want` accepts, at any depth. First-level table cells are left out: the body
/// `Cursor` reaches them.
pub(crate) fn nested_paragraphs(
    doc: &Document,
    want: &dyn Fn(&Paragraph) -> bool,
) -> Vec<ContextAddress> {
    let mut out = Vec::new();
    for (s, section) in doc.sections.iter().enumerate() {
        for (p, para) in section.paragraphs.iter().enumerate() {
            let at = ContextAddress {
                version: 2,
                section: s,
                paragraph: p,
                path: Vec::new(),
            };
            walk(para, &at, want, &mut out);
        }
    }
    out
}

fn walk(
    para: &Paragraph,
    at: &ContextAddress,
    want: &dyn Fn(&Paragraph) -> bool,
    out: &mut Vec<ContextAddress>,
) {
    if at.path.len() >= MAX_DEPTH {
        return;
    }
    let mut visit = |paragraphs: &[Paragraph], step: &dyn Fn(usize) -> ContextStep| {
        for (k, inner) in paragraphs.iter().enumerate() {
            let mut address = at.clone();
            address.path.push(step(k));
            let body_cell = matches!(address.path.as_slice(), [ContextStep::TableCell { .. }]);
            if !body_cell && want(inner) {
                out.push(address.clone());
            }
            walk(inner, &address, want, out);
        }
    };
    for (c, control) in para.controls.iter().enumerate() {
        match control {
            Control::Table(table) => {
                for (i, cell) in table.cells.iter().enumerate() {
                    visit(&cell.paragraphs, &|k| ContextStep::TableCell {
                        control: c,
                        cell: i,
                        paragraph: k,
                    });
                }
            }
            Control::Header(h) => visit(&h.paragraphs, &|k| ContextStep::Header {
                control: c,
                paragraph: k,
            }),
            Control::Footer(f) => visit(&f.paragraphs, &|k| ContextStep::Footer {
                control: c,
                paragraph: k,
            }),
            Control::Footnote(n) => visit(&n.paragraphs, &|k| ContextStep::Footnote {
                control: c,
                paragraph: k,
            }),
            Control::Endnote(n) => visit(&n.paragraphs, &|k| ContextStep::Endnote {
                control: c,
                paragraph: k,
            }),
            Control::Shape(shape) if !matches!(**shape, ShapeObject::Group(_)) => {
                if let Some(text) = shape.drawing().and_then(|d| d.text_box.as_ref()) {
                    visit(&text.paragraphs, &|k| ContextStep::TextBox {
                        control: c,
                        shape_path: Vec::new(),
                        paragraph: k,
                    });
                }
            }
            _ => {}
        }
    }
}

/// A header or footer: which one and the `apply_to` code RHWP's page lookups use.
fn header_footer(core: &DocumentCore, address: &ContextAddress) -> Option<(bool, u8)> {
    let host = core
        .document()
        .sections
        .get(address.section)?
        .paragraphs
        .get(address.paragraph)?;
    let control = match address.path.as_slice() {
        [ContextStep::Header { control, .. }] | [ContextStep::Footer { control, .. }] => *control,
        _ => return None,
    };
    let (is_header, apply) = match host.controls.get(control)? {
        Control::Header(h) => (true, h.apply_to),
        Control::Footer(f) => (false, f.apply_to),
        _ => return None,
    };
    let apply = match apply {
        HeaderFooterApply::Both => 0,
        HeaderFooterApply::Even => 1,
        HeaderFooterApply::Odd => 2,
    };
    Some((is_header, apply))
}

/// RHWP's caret rectangle `{pageIndex, x, y, height}` before scalar `offset` of the
/// paragraph at `address`, laid out by `core`. A header or footer answers for the
/// first page it is drawn on. `None` for containers RHWP cannot place a caret in (a
/// table inside a header, a text box in a group).
pub(crate) fn caret_rect(
    core: &DocumentCore,
    address: &ContextAddress,
    offset: usize,
) -> Option<Value> {
    let raw = match address.path.as_slice() {
        [ContextStep::Header { paragraph, .. }] | [ContextStep::Footer { paragraph, .. }] => {
            let (is_header, apply) = header_footer(core, address)?;
            core.get_cursor_rect_in_header_footer_native(
                address.section,
                is_header,
                apply,
                *paragraph,
                offset,
                -1,
            )
        }
        [ContextStep::Footnote { control, paragraph }]
        | [ContextStep::Endnote { control, paragraph }] => core.get_cursor_rect_in_note_native(
            address.section,
            address.paragraph,
            *control,
            *paragraph,
            offset,
        ),
        steps => {
            let path: Vec<String> = steps
                .iter()
                .map(|step| match step {
                    ContextStep::TableCell {
                        control,
                        cell,
                        paragraph,
                    } => Some((*control, *cell, *paragraph)),
                    ContextStep::TextBox {
                        control,
                        shape_path,
                        paragraph,
                    } if shape_path.is_empty() => Some((*control, 0, *paragraph)),
                    _ => None,
                })
                .map(|step| {
                    step.map(|(control, cell, paragraph)| {
                        format!(
                            r#"{{"controlIndex":{control},"cellIndex":{cell},"cellParaIndex":{paragraph}}}"#
                        )
                    })
                })
                .collect::<Option<_>>()?;
            core.get_cursor_rect_by_path_native(
                address.section,
                address.paragraph,
                &format!("[{}]", path.join(",")),
                offset,
            )
        }
    };
    serde_json::from_str(&raw.ok()?).ok()
}

/// Every page (0-based) a header or footer is drawn on; empty for other containers.
pub(crate) fn header_footer_pages(core: &DocumentCore, address: &ContextAddress) -> Vec<u32> {
    let Some((is_header, apply)) = header_footer(core, address) else {
        return Vec::new();
    };
    (0..core.page_count())
        .filter(|&page| {
            core.get_header_footer_edit_target_native(page, is_header)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .is_some_and(|v| {
                    v["sectionIndex"].as_u64() == Some(address.section as u64)
                        && v["applyTo"].as_u64() == Some(u64::from(apply))
                })
        })
        .collect()
}
