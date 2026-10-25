//! Owner: core area. Tab widths for saved files.
//!
//! HWP/HWPX store every inline tab with its laid-out width. Tabs typed in HwpCraft
//! have no stored width, so RHWP writes a zero-width marker and Hangul then draws
//! the tab with no width. Before saving, fill those tabs the way Hangul places them;
//! after saving, put the live model back so later edits keep re-flowing tabs.
use super::*;
use hwpcraft_doc::model::paragraph::{TAB_EXT_PLACEHOLDER, tab_ext_is_placeholder};

/// Where a tabbed paragraph lives: body paragraph, or a first-level table cell paragraph.
#[derive(Clone, Copy)]
struct Slot {
    section: usize,
    paragraph: usize,
    cell: Option<Cell>,
}

type Saved = Vec<(Slot, Vec<[u16; 7]>)>;

impl Session {
    fn tab_paragraph_mut(&mut self, at: Slot) -> Option<&mut Paragraph> {
        let p = self
            .core
            .document_mut()
            .sections
            .get_mut(at.section)?
            .paragraphs
            .get_mut(at.paragraph)?;
        match at.cell {
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
                    out.push(Slot {
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
                                    out.push(Slot {
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
        out
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
        at: Slot,
        para: &Paragraph,
        offset: usize,
        existing: Option<[u16; 7]>,
        line: &mut (f64, f64),
    ) -> Option<[u16; 7]> {
        let caret = |offset| -> Option<(u64, f64, f64)> {
            let v = self
                .caret_at(Cursor {
                    section: at.section,
                    paragraph: at.paragraph,
                    offset,
                    cell: at.cell,
                })
                .ok()?;
            Some((v["pageIndex"].as_u64()?, v["x"].as_f64()?, v["y"].as_f64()?))
        };
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
        // caret position (table cells); positions are stored at twice HWPUNIT.
        let base = match at.cell {
            Some(_) => caret(0)?.1,
            None => {
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
    /// Fill missing tab widths; returns the original records for `restore_tabs`.
    fn materialize_tabs(&mut self) -> Saved {
        let mut saved = Vec::new();
        for at in self.tab_slots() {
            let Ok(para) = self.paragraph(Cursor {
                section: at.section,
                paragraph: at.paragraph,
                offset: 0,
                cell: at.cell,
            }) else {
                continue;
            };
            let para = para.clone();
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
            for (n, offset) in tabs.into_iter().enumerate() {
                let existing = records
                    .get(n)
                    .copied()
                    .filter(|r| !tab_ext_is_placeholder(r));
                let Some(record) = self.tab_record(at, &para, offset, existing, &mut line) else {
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
            if changed && let Some(target) = self.tab_paragraph_mut(at) {
                saved.push((at, std::mem::replace(&mut target.tab_extended, records)));
            }
        }
        saved
    }
    fn restore_tabs(&mut self, saved: Saved) {
        for (at, records) in saved {
            if let Some(target) = self.tab_paragraph_mut(at) {
                target.tab_extended = records;
            }
        }
    }
    /// Save with laid-out tab widths, keeping the live model unchanged.
    pub(crate) fn save_with_tab_widths(&mut self, path: &Path) -> Result<()> {
        let saved = self.materialize_tabs();
        *self.page_field_cache.borrow_mut() = None;
        *self.render_highlights.borrow_mut() = None;
        let result = (|| match self.password.as_deref() {
            Some(password) if is_hwp(path) => {
                hwpcraft_formats::save_with_password(&self.core, path, password)
            }
            _ if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) =>
            {
                self.render_pdf(&(0..self.page_count()?).collect::<Vec<_>>())
                    .and_then(|bytes| hwpcraft_formats::atomic_write(path, &bytes))
            }
            _ => hwpcraft_formats::save(&self.core, path),
        })();
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
