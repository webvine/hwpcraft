//! Repair table row/column counts that are smaller than the cells they hold.
//!
//! A damaged file can declare `rowCnt="0"` while still carrying addressed
//! cells. RHWP parses and paginates such a table, then panics while drawing
//! its borders, which takes the desktop app or the MCP server down with it.
//! Raising the declared counts to the addressed extent makes the model
//! consistent before anything renders it; valid tables are left untouched.
use hwpcraft_doc::DocumentCore;
use hwpcraft_doc::model::{
    control::Control,
    paragraph::Paragraph,
    shape::{Caption, ShapeObject},
    table::Table,
};

const MAX_DEPTH: usize = 64;

/// Repair every table in the document. Returns how many tables changed; the
/// derived layout is rebuilt only when at least one did.
pub(crate) fn repair(core: &mut DocumentCore) -> usize {
    let mut repaired = 0;
    for section in &mut core.document_mut().sections {
        paragraphs(&mut section.paragraphs, 0, &mut repaired);
        for master in &mut section.section_def.master_pages {
            paragraphs(&mut master.paragraphs, 0, &mut repaired);
        }
    }
    if repaired > 0 {
        let doc = core.document().clone();
        core.set_document(doc);
    }
    repaired
}

fn table(value: &mut Table, depth: usize, repaired: &mut usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let extent = |start: u16, span: u16| start.saturating_add(span.max(1));
    let rows = value.cells.iter().map(|c| extent(c.row, c.row_span)).max();
    let cols = value.cells.iter().map(|c| extent(c.col, c.col_span)).max();
    if let (Some(rows), Some(cols)) = (rows, cols)
        && (value.row_count < rows || value.col_count < cols)
    {
        value.row_count = value.row_count.max(rows);
        value.col_count = value.col_count.max(cols);
        value.rebuild_grid();
        *repaired += 1;
    }
    caption(value.caption.as_mut(), depth, repaired);
    for cell in &mut value.cells {
        paragraphs(&mut cell.paragraphs, depth + 1, repaired);
    }
}

fn paragraphs(paras: &mut [Paragraph], depth: usize, repaired: &mut usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for control in paras.iter_mut().flat_map(|para| &mut para.controls) {
        match control {
            Control::Table(value) => table(value, depth + 1, repaired),
            Control::Shape(value) => shape(value, depth + 1, repaired),
            Control::Picture(picture) => caption(picture.caption.as_mut(), depth, repaired),
            Control::Header(header) => paragraphs(&mut header.paragraphs, depth + 1, repaired),
            Control::Footer(footer) => paragraphs(&mut footer.paragraphs, depth + 1, repaired),
            Control::Footnote(note) => paragraphs(&mut note.paragraphs, depth + 1, repaired),
            Control::Endnote(note) => paragraphs(&mut note.paragraphs, depth + 1, repaired),
            Control::HiddenComment(comment) => {
                paragraphs(&mut comment.paragraphs, depth + 1, repaired)
            }
            _ => {}
        }
    }
}

fn caption(value: Option<&mut Caption>, depth: usize, repaired: &mut usize) {
    if let Some(value) = value {
        paragraphs(&mut value.paragraphs, depth + 1, repaired);
    }
}

fn shape(value: &mut ShapeObject, depth: usize, repaired: &mut usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(drawing) = value.drawing_mut() {
        if let Some(text) = drawing.text_box.as_mut() {
            paragraphs(&mut text.paragraphs, depth + 1, repaired);
        }
        caption(drawing.caption.as_mut(), depth, repaired);
    }
    match value {
        ShapeObject::Group(group) => {
            caption(group.caption.as_mut(), depth, repaired);
            for child in &mut group.children {
                shape(child, depth + 1, repaired);
            }
        }
        ShapeObject::Picture(picture) => caption(picture.caption.as_mut(), depth, repaired),
        ShapeObject::Chart(chart) => caption(chart.caption.as_mut(), depth, repaired),
        ShapeObject::Ole(ole) => caption(ole.caption.as_mut(), depth, repaired),
        _ => {}
    }
}
