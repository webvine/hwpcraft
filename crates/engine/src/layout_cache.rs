//! Stored line rows (`line_segs`) as a layout cache in saved files.
//!
//! Rows RHWP computes while editing carry section-cumulative positions; only the
//! ones it marks as its own (`TAG_IMPLEMENTATION_PROPERTY`) are left out of an
//! HWPX package. Editing leaves unmarked rows as well, so a document written in
//! HwpCraft would hand out a mix of stale caches and missing ones, and Hangul
//! and RHWP then paginate the reopened file differently. A document HwpCraft
//! wrote holds no row Hangul laid out, so its HWPX save leaves all rows out and
//! lets the reader lay the document out afresh.
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::document::{Document, Section};
use hwpcraft_doc::model::paragraph::{LineSeg, Paragraph};
use hwpcraft_doc::model::shape::ShapeObject;

const OWN: u32 = LineSeg::TAG_IMPLEMENTATION_PROPERTY;

/// Every paragraph of the document, in a fixed order: body, master pages,
/// table cells and captions, drawing text boxes and captions, notes, headers.
fn visit(document: &mut Document, f: &mut impl FnMut(&mut Paragraph)) {
    for section in &mut document.sections {
        visit_section(section, f);
    }
}

/// Every paragraph of one section, in the order of [`visit`]: its body paragraphs
/// with everything inside them, then its master pages.
pub(crate) fn visit_section(section: &mut Section, f: &mut impl FnMut(&mut Paragraph)) {
    visit_paragraphs(&mut section.paragraphs, f);
    for master in &mut section.section_def.master_pages {
        visit_paragraphs(&mut master.paragraphs, f);
    }
}

/// `list` and every paragraph inside it: table cells and captions, picture
/// captions, drawing text boxes and captions, notes, headers and footers.
pub(crate) fn visit_paragraphs(list: &mut [Paragraph], f: &mut impl FnMut(&mut Paragraph)) {
    fn paragraphs(list: &mut [Paragraph], f: &mut impl FnMut(&mut Paragraph)) {
        for p in list {
            f(p);
            for c in &mut p.controls {
                control(c, f);
            }
        }
    }
    fn shape(s: &mut ShapeObject, f: &mut impl FnMut(&mut Paragraph)) {
        if let ShapeObject::Group(g) = s {
            for child in &mut g.children {
                shape(child, f);
            }
        }
        if let Some(d) = s.drawing_mut() {
            if let Some(t) = d.text_box.as_mut() {
                paragraphs(&mut t.paragraphs, f);
            }
            if let Some(c) = d.caption.as_mut() {
                paragraphs(&mut c.paragraphs, f);
            }
        }
    }
    fn control(c: &mut Control, f: &mut impl FnMut(&mut Paragraph)) {
        match c {
            Control::Table(t) => {
                for cell in &mut t.cells {
                    paragraphs(&mut cell.paragraphs, f);
                }
                if let Some(cap) = t.caption.as_mut() {
                    paragraphs(&mut cap.paragraphs, f);
                }
            }
            Control::Picture(p) => {
                if let Some(cap) = p.caption.as_mut() {
                    paragraphs(&mut cap.paragraphs, f);
                }
            }
            Control::Shape(s) => shape(s, f),
            Control::Header(h) => paragraphs(&mut h.paragraphs, f),
            Control::Footer(h) => paragraphs(&mut h.paragraphs, f),
            Control::Footnote(n) => paragraphs(&mut n.paragraphs, f),
            Control::Endnote(n) => paragraphs(&mut n.paragraphs, f),
            _ => {}
        }
    }
    paragraphs(list, f);
}

/// Does any body row look laid out by the file's author (not by RHWP)? Hangul
/// stores rows for every body paragraph of an HWPX it writes.
pub(crate) fn has_authored_rows(document: &Document) -> bool {
    document
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .any(|p| p.line_segs.iter().any(|l| l.tag & OWN == 0))
}

/// Mark every row as RHWP's own for a save; returns the previous tags.
pub(crate) fn hide(document: &mut Document) -> Vec<u32> {
    let mut tags = Vec::new();
    visit(document, &mut |p| {
        for l in &mut p.line_segs {
            tags.push(l.tag);
            l.tag |= OWN;
        }
    });
    tags
}

/// The document with each body and table-cell paragraph's rows laid out afresh,
/// as reading its HWPX back lays them out. Commands that change sizes (a style
/// set) can leave the stored rows behind the layout on screen; an HWPX file leaves
/// rows to the reader, but an HWP file keeps them, so a document HwpCraft wrote is
/// saved as HWP with these. A paragraph whose text did not come back the same keeps
/// its rows; `None` when the document cannot be laid out afresh.
pub(crate) fn with_fresh_rows(core: &hwpcraft_doc::DocumentCore) -> Option<Document> {
    use hwpcraft_doc::DocumentCore;
    let mut doc = core.document().clone();
    hide(&mut doc);
    let mut export = DocumentCore::new_empty();
    *export.document_mut() = doc;
    let bytes = export.export_hwpx_native().ok()?;
    let mut fresh = DocumentCore::from_bytes(&bytes).ok()?;
    fresh.reflow_linesegs_on_demand();
    /// Copy the fresh rows into `live`; true when any row changed.
    fn take(live: &mut [Paragraph], fresh: &[Paragraph]) -> bool {
        if live.len() != fresh.len() {
            return false;
        }
        let mut changed = false;
        for (p, q) in live.iter_mut().zip(fresh) {
            if p.text != q.text || q.line_segs.is_empty() {
                continue;
            }
            if p.line_segs != q.line_segs {
                p.line_segs = q.line_segs.clone();
                changed = true;
            }
            for (c, d) in p.controls.iter_mut().zip(&q.controls) {
                if let (Control::Table(t), Control::Table(u)) = (c, d)
                    && t.cells.len() == u.cells.len()
                {
                    for (cell, other) in t.cells.iter_mut().zip(&u.cells) {
                        changed |= take(&mut cell.paragraphs, &other.paragraphs);
                    }
                }
            }
        }
        changed
    }
    let mut out = core.document().clone();
    for (section, other) in out.sections.iter_mut().zip(&fresh.document().sections) {
        if take(&mut section.paragraphs, &other.paragraphs) {
            // The rows changed: write the section from the model, not its old bytes.
            section.raw_stream = None;
        }
    }
    Some(out)
}

/// Put back the tags `hide` replaced.
pub(crate) fn restore(document: &mut Document, tags: Vec<u32>) {
    let mut tags = tags.into_iter();
    visit(document, &mut |p| {
        for l in &mut p.line_segs {
            if let Some(tag) = tags.next() {
                l.tag = tag;
            }
        }
    });
}
