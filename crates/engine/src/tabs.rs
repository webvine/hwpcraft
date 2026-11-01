//! Owner: core area. Tab widths for saved files.
//!
//! HWP/HWPX store every inline tab with its laid-out width. Tabs typed in HwpCraft
//! have no stored width, so RHWP writes a zero-width marker and Hangul then draws
//! the tab with no width. Before saving, fill those tabs the way Hangul places them;
//! after saving, put the live model back so later edits keep re-flowing tabs. Body
//! paragraphs and table cells, headers, footers, notes and text boxes at any depth
//! are measured from RHWP's caret positions.
use super::*;
use hwpcraft_doc::model::paragraph::{TAB_EXT_PLACEHOLDER, tab_ext_is_placeholder};

/// Where a tabbed paragraph lives.
#[derive(Clone)]
enum Slot {
    /// A body paragraph or a first-level table cell paragraph: the body caret reaches it.
    Body {
        section: usize,
        paragraph: usize,
        cell: Option<Cell>,
    },
    /// A paragraph of a header, footer, footnote, endnote, text box or nested table cell.
    Context(ContextAddress),
}

type Saved = Vec<(Slot, Vec<[u16; 7]>)>;

impl Session {
    fn tab_paragraph_mut(&mut self, at: &Slot) -> Option<&mut Paragraph> {
        let (section, paragraph, cell) = match at {
            Slot::Body {
                section,
                paragraph,
                cell,
            } => (*section, *paragraph, *cell),
            Slot::Context(address) => {
                return self.context_v2_paragraph_mut_keeping_layout(address).ok();
            }
        };
        // Only the save fills these records, and it puts the old ones back.
        let p = self
            .core
            .document_mut_keeping_layout()
            .sections
            .get_mut(section)?
            .paragraphs
            .get_mut(paragraph)?;
        match cell {
            None => Some(p),
            Some(t) => match p.controls.get_mut(t.control)? {
                Control::Table(table) => table
                    .cells
                    .get_mut(t.index)?
                    .paragraphs
                    .get_mut(t.paragraph),
                _ => None,
            },
        }
    }
    fn tab_slots(&self) -> Vec<Slot> {
        let mut out = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, para) in section.paragraphs.iter().enumerate() {
                if para.text.contains('\t') {
                    out.push(Slot::Body {
                        section: s,
                        paragraph: p,
                        cell: None,
                    });
                }
                for (c, control) in para.controls.iter().enumerate() {
                    if let Control::Table(table) = control {
                        for (i, cell) in table.cells.iter().enumerate() {
                            for (k, cp) in cell.paragraphs.iter().enumerate() {
                                if cp.text.contains('\t') {
                                    out.push(Slot::Body {
                                        section: s,
                                        paragraph: p,
                                        cell: Some(Cell {
                                            control: c,
                                            index: i,
                                            paragraph: k,
                                        }),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        let nested =
            context_walk::nested_paragraphs(self.core.document(), &|p| p.text.contains('\t'));
        out.extend(nested.into_iter().map(Slot::Context));
        out
    }
    /// The paragraph a slot names, as it is now.
    fn tab_paragraph(&self, at: &Slot) -> Option<Paragraph> {
        match at {
            Slot::Body {
                section,
                paragraph,
                cell,
            } => self
                .paragraph(Cursor {
                    section: *section,
                    paragraph: *paragraph,
                    offset: 0,
                    cell: *cell,
                })
                .ok()
                .cloned(),
            Slot::Context(address) => self.context_v2_paragraph(address).ok().cloned(),
        }
    }
    /// Caret (page index, x, y) before scalar `offset` of a slot's paragraph, in page
    /// pixels.
    fn tab_caret(&self, at: &Slot, offset: usize) -> Option<(u64, f64, f64)> {
        let v: Value = match at {
            Slot::Body {
                section,
                paragraph,
                cell,
            } => self
                .caret_at(Cursor {
                    section: *section,
                    paragraph: *paragraph,
                    offset,
                    cell: *cell,
                })
                .ok()?,
            Slot::Context(address) => {
                let layout = self.page_field_layout().ok().flatten();
                let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
                context_walk::caret_rect(core, address, offset)?
            }
        };
        Some((v["pageIndex"].as_u64()?, v["x"].as_f64()?, v["y"].as_f64()?))
    }
    /// Inline tab record for the tab at scalar `offset`: width in HWPUNIT plus the
    /// kind and fill of the tab stop it reaches. `None` when layout cannot tell (line wrap).
    ///
    /// The width follows Hangul's placement using relative widths only, so it is right
    /// even where RHWP's own right/centre/decimal placement differs: the text after the
    /// tab (up to the next tab or the line end) is aligned at the next stop.
    ///
    /// `line` carries (line y, shift) between the tabs of one line: how far Hangul's
    /// position after the previous tab is from RHWP's, so later tabs start where Hangul
    /// will actually draw them. `existing` keeps a record Hangul already wrote.
    fn tab_record(
        &self,
        at: &Slot,
        para: &Paragraph,
        offset: usize,
        existing: Option<[u16; 7]>,
        line: &mut (f64, f64),
    ) -> Option<[u16; 7]> {
        let caret = |offset| self.tab_caret(at, offset);
        let (page, x_rhwp, y0) = caret(offset)?;
        let (page1, x1, y1) = caret(offset + 1)?;
        if page != page1 || (y0 - y1).abs() > 0.5 {
            return None;
        }
        if (y0 - line.0).abs() > 0.5 {
            *line = (y0, 0.0);
        }
        let x0 = x_rhwp + line.1;
        if let Some(r) = existing {
            let width = f64::from(u32::from(r[0]) | (u32::from(r[1]) << 16)) * 96.0 / 7200.0;
            line.1 = x0 + width - x1;
            return Some(r);
        }
        // Stops are measured from the column start (body) or the paragraph's first
        // caret position (table cells, headers, notes, text boxes); positions are
        // stored at twice HWPUNIT.
        let base = match at {
            Slot::Body { cell: None, .. } => {
                let info = self.page_info(page as u32).ok()?;
                info["columns"]
                    .as_array()
                    .and_then(|cols| {
                        cols.iter()
                            .filter_map(|c| Some((c["x"].as_f64()?, c["width"].as_f64()?)))
                            .find(|(x, w)| x_rhwp >= *x - 1.0 && x_rhwp <= x + w + 1.0)
                            .map(|(x, _)| x)
                    })
                    .unwrap_or(0.0)
            }
            _ => caret(0)?.1,
        };
        let doc = self.core.document();
        let stop = doc
            .doc_info
            .para_shapes
            .get(para.para_shape_id as usize)
            .and_then(|shape| doc.doc_info.tab_defs.get(shape.tab_def_id as usize))
            .and_then(|def| {
                let mut tabs: Vec<_> = def.tabs.iter().collect();
                tabs.sort_by_key(|t| t.position);
                tabs.into_iter()
                    .map(|t| (base + f64::from(t.position) * 96.0 / 7200.0 / 2.0, t))
                    .find(|(x, _)| *x > x0 + 0.5)
            });
        let Some((stop_x, stop)) = stop else {
            // Automatic tab grid: keep the laid-out width of a left tab.
            return Some(record((x1 - x_rhwp).max(0.0), 1, 0));
        };
        // The aligned segment: after the tab up to the next tab or the end of this line.
        let chars: Vec<char> = para.text.chars().collect();
        let mut end = offset + 1;
        while end < chars.len() && chars[end] != '\t' {
            match caret(end + 1) {
                Some((p, _, y)) if p == page && (y - y0).abs() <= 0.5 => end += 1,
                _ => break,
            }
        }
        let x_at = |o: usize| caret(o).map(|(_, x, _)| x);
        // RHWP reports the caret right after an unsized tab at its own tab-stop guess
        // while the glyphs still flow from the tab start, so carets from the second
        // segment character on are reliable but the one at `offset + 1` may not be.
        // Recover the segment start from the second character and the first advance.
        let segment_start = if end >= offset + 2 {
            let second = x_at(offset + 2)?;
            let advance = if second > x1 && second - x1 < 40.0 {
                second - x1
            } else if end >= offset + 3 {
                x_at(offset + 3)? - second
            } else {
                second - x_rhwp
            };
            second - advance.max(0.0)
        } else {
            x1
        };
        let segment_end = if end >= offset + 2 {
            x_at(end)?
        } else {
            segment_start
        };
        let x_of = |o: usize| {
            if o <= offset + 1 {
                Some(segment_start)
            } else {
                x_at(o)
            }
        };
        let aligned = match stop.tab_type {
            1 => segment_end - segment_start,
            2 => (segment_end - segment_start) / 2.0,
            3 => match chars[offset + 1..end].iter().position(|c| *c == '.') {
                Some(dot) => x_of(offset + 1 + dot)? - segment_start,
                None => segment_end - segment_start,
            },
            _ => 0.0,
        };
        let width = (stop_x - aligned - x0).max(0.0);
        line.1 = x0 + width - segment_start;
        Some(record(
            width,
            u16::from(stop.tab_type) + 1,
            u16::from(stop.fill_type),
        ))
    }
    /// Some tab has no stored width yet, so an HWP save fills it in
    /// (`materialize_tabs`) and writes that paragraph's section again.
    pub(crate) fn tab_widths_missing(&self) -> bool {
        self.tab_slots().iter().any(|at| {
            self.tab_paragraph(at).is_some_and(|para| {
                let tabs = para.text.chars().filter(|c| *c == '\t').count();
                para.tab_extended.len() < tabs
                    || para.tab_extended[..tabs].iter().any(tab_ext_is_placeholder)
            })
        })
    }
    /// Fill missing tab widths; returns the original records for `restore_tabs`.
    fn materialize_tabs(&mut self) -> Saved {
        let mut saved = Vec::new();
        for at in self.tab_slots() {
            let Some(para) = self.tab_paragraph(&at) else {
                continue;
            };
            let mut records = para.tab_extended.clone();
            let mut changed = false;
            let mut line = (f64::INFINITY, 0.0);
            let tabs: Vec<usize> = para
                .text
                .chars()
                .enumerate()
                .filter(|(_, c)| *c == '\t')
                .map(|(offset, _)| offset)
                .collect();
            // Every tab already stored: nothing to fill, so skip the caret layout
            // queries (most of the save time on long documents).
            if records.len() >= tabs.len()
                && records[..tabs.len()]
                    .iter()
                    .all(|r| !tab_ext_is_placeholder(r))
            {
                continue;
            }
            for (n, offset) in tabs.into_iter().enumerate() {
                let existing = records
                    .get(n)
                    .copied()
                    .filter(|r| !tab_ext_is_placeholder(r));
                let Some(record) = self.tab_record(&at, &para, offset, existing, &mut line) else {
                    continue;
                };
                if existing.is_some() {
                    continue;
                }
                if records.len() <= n {
                    records.resize(n + 1, TAB_EXT_PLACEHOLDER);
                }
                records[n] = record;
                changed = true;
            }
            if changed && let Some(target) = self.tab_paragraph_mut(&at) {
                saved.push((at, std::mem::replace(&mut target.tab_extended, records)));
            }
        }
        saved
    }
    fn restore_tabs(&mut self, saved: Saved) {
        for (at, records) in saved {
            if let Some(target) = self.tab_paragraph_mut(&at) {
                target.tab_extended = records;
            }
        }
    }
    /// Save with laid-out tab widths, keeping the live model unchanged.
    pub(crate) fn save_with_tab_widths(&mut self, path: &Path) -> Result<()> {
        self.save_with_tab_widths_for(path, false)
    }
    /// `copy`: a file only HwpCraft reads back (자동 복구, 문서마당). It keeps the live
    /// rows instead of laying the document out afresh for an HWP file (about 180 ms on
    /// a 44-page report, which a periodic recovery save would spend on the UI thread).
    pub(crate) fn save_with_tab_widths_for(&mut self, path: &Path, copy: bool) -> Result<()> {
        self.guard_distribution()?;
        if !copy {
            self.refuse_unkept_hwp_save(path)?;
            // HWP has no field for combo box items; see `form_properties`.
            self.refuse_combo_item_loss(path)?;
        }
        // Every model change below is undone before this returns: they go through
        // `document_mut_keeping_layout`, which keeps undo and typeset reuse on. Only
        // the HWP and HWPX writers read tab widths, so a PDF lays out the live model.
        let saved = if is_hwp(path) {
            self.materialize_tabs()
        } else {
            Vec::new()
        };
        // See `layout_cache`: a document HwpCraft wrote hands out no row caches.
        let hidden = (self.authored
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("hwpx")))
        .then(|| crate::layout_cache::hide(self.core.document_mut_keeping_layout()));
        // An HWP file keeps rows: a document HwpCraft wrote gets the fresh ones its
        // HWPX copy is laid out with (`layout_cache::with_fresh_rows`).
        let fresh = (self.authored
            && !copy
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("hwp")))
        .then(|| crate::layout_cache::with_fresh_rows(&self.core))
        .flatten()
        // Swapped into this core, so the writers still see its source format.
        .map(|doc| std::mem::replace(self.core.document_mut_keeping_layout(), doc));
        let core = &self.core;
        *self.page_field_cache.borrow_mut() = None;
        *self.render_highlights.borrow_mut() = None;
        let result = (|| match self.password.as_deref() {
            Some(password) if is_hwp(path) => {
                hwpcraft_formats::save_with_password(core, path, password)
            }
            _ if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) =>
            {
                self.render_pdf(&(0..self.page_count()?).collect::<Vec<_>>())
                    .and_then(|bytes| hwpcraft_formats::atomic_write(path, &bytes))
            }
            // A changed tracked document: BodyText accepted, ViewText marked.
            _ if self.rewrites_tracked_hwp(path) => self.save_tracked_hwp(path),
            _ => hwpcraft_formats::save(core, path),
        })();
        if let Some(tags) = hidden {
            crate::layout_cache::restore(self.core.document_mut_keeping_layout(), tags);
        }
        if let Some(live) = fresh {
            *self.core.document_mut_keeping_layout() = live;
        }
        self.restore_tabs(saved);
        *self.page_field_cache.borrow_mut() = None;
        *self.render_highlights.borrow_mut() = None;
        result
    }
}

fn is_hwp(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|x| x.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("hwp" | "hwpx")
    )
}

/// HWP inline tab record: width (HWPUNIT, low/high word), (kind << 8) | fill, marker.
fn record(width_px: f64, kind: u16, fill: u16) -> [u16; 7] {
    let width = (width_px * 7200.0 / 96.0).round() as u32;
    [
        (width & 0xFFFF) as u16,
        (width >> 16) as u16,
        (kind << 8) | (fill & 0xFF),
        0,
        0,
        0,
        0x0009,
    ]
}
