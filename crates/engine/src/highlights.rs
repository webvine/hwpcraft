//! Shared output rectangles for 형광펜 (markpen) ranges.
//!
//! RHWP stores markpen marks (`Paragraph::markpen_marks`) and saves them to
//! HWP/HWPX, but its renderer does not paint them. HwpCraft paints them itself as
//! in screen/PNG/PDF SVGs, using the owning displayed scalar's geometry.
use super::*;
use hwpcraft_doc::model::paragraph::Paragraph;
use presentation_geometry::{Affinity, Boundary, CaretLookup, GeometryCache, Rect};
use serde::Serialize;

/// One highlighted run on one page, in unscaled page pixels (RHWP caret units).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HighlightRect {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// `#rrggbb` as stored by RHWP.
    pub color: String,
}

/// Highlighted scalar ranges of one paragraph: an opening mark (with a colour)
/// up to the next closing mark (without one). Unclosed marks run to the end.
pub fn markpen_ranges(p: &Paragraph) -> Vec<(Range<usize>, String)> {
    let len = p.text.chars().count();
    let mut marks: Vec<_> = p.markpen_marks.iter().collect();
    marks.sort_by_key(|m| (m.char_idx, m.color.is_none()));
    let mut out = Vec::new();
    let mut open: Option<(usize, String)> = None;
    for mark in marks {
        match (&mark.color, open.take()) {
            (Some(color), previous) => {
                if let Some((start, c)) = previous
                    && start < mark.char_idx.min(len)
                {
                    out.push((start..mark.char_idx.min(len), c));
                }
                open = Some((mark.char_idx.min(len), color.clone()));
            }
            (None, Some((start, c))) => {
                if start < mark.char_idx.min(len) {
                    out.push((start..mark.char_idx.min(len), c));
                }
            }
            (None, None) => {}
        }
    }
    if let Some((start, c)) = open
        && start < len
    {
        out.push((start..len, c));
    }
    out
}

/// Pin native stream positions before saving. At paragraph end, char_offsets
/// may have no sentinel; the last offset still includes leading controls.
pub(crate) fn resolve_markpen_positions(p: &mut Paragraph) {
    let chars: Vec<char> = p.text.chars().collect();
    let positions: Vec<_> = p
        .markpen_marks
        .iter()
        .map(|mark| {
            mark.utf16_pos
                .or_else(|| p.char_offsets.get(mark.char_idx).copied())
                .or_else(|| {
                    if mark.char_idx != chars.len() {
                        return None;
                    }
                    let last = chars.len().checked_sub(1)?;
                    let width = if chars[last] == '\t' {
                        8
                    } else {
                        chars[last].len_utf16() as u32
                    };
                    p.char_offsets
                        .get(last)
                        .map(|position| position.saturating_add(width))
                })
        })
        .collect();
    for (mark, position) in p.markpen_marks.iter_mut().zip(positions) {
        mark.utf16_pos = position;
    }
}

/// Characters probed per call; beyond this the rest of the document is skipped.
const MAX_PROBES: usize = 20_000;

fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect {
        x,
        y,
        width: ((a.x + a.width).min(b.x + b.width) - x).max(0.0),
        height: ((a.y + a.height).min(b.y + b.height) - y).max(0.0),
    }
}

/// Independent of the interactive caret, including its soft-wrap affinity.
/// Special inline controls/vertical text retain the prior native fallback.
fn scalar_rect(
    core: &DocumentCore,
    source: &DocumentCore,
    geometry: &GeometryCache,
    generation: u64,
    cursor: Cursor,
    color: &str,
) -> Option<HighlightRect> {
    match geometry
        .caret_from_source(
            core,
            source,
            generation,
            &cursor.into(),
            Affinity::Downstream,
            None,
        )
        .ok()?
    {
        CaretLookup::Text(at) if at.boundary == Boundary::Scalar => {
            if !at.visible {
                return None;
            }
            let mut bounds = Rect {
                x: at.x.min(at.x + at.advance),
                y: at.y,
                width: at.advance.abs().max(2.0),
                height: at.height,
            };
            if let Some(clip) = at.clip {
                bounds = intersect(bounds, clip);
            }
            bounds = intersect(bounds, geometry.page(core, generation, at.page).ok()?.paper);
            if bounds.width <= 0.0 || bounds.height <= 0.0 {
                return None;
            }
            Some(HighlightRect {
                page: at.page,
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
                color: color.to_owned(),
            })
        }
        _ => {
            let native = |c: Cursor| -> Option<Value> {
                let raw = if let Some(cell) = c.cell {
                    core.get_cursor_rect_in_cell_native(
                        c.section,
                        c.paragraph,
                        cell.control,
                        cell.index,
                        cell.paragraph,
                        c.offset,
                    )
                } else {
                    core.get_cursor_rect_native(c.section, c.paragraph, c.offset)
                };
                serde_json::from_str(&raw.ok()?).ok()
            };
            let a = native(cursor)?;
            let b = native(Cursor {
                offset: cursor.offset + 1,
                ..cursor
            })?;
            let page = a["pageIndex"].as_u64()? as u32;
            let x = a["x"].as_f64()?;
            let y = a["y"].as_f64()?;
            let height = a["height"].as_f64().unwrap_or(16.0);
            let same_line = (b["y"].as_f64()? - y).abs() < 1.0
                && b["pageIndex"].as_u64() == Some(u64::from(page));
            let width = if same_line {
                (b["x"].as_f64()? - x).max(2.0)
            } else {
                height * 0.6
            };
            Some(HighlightRect {
                page,
                x,
                y,
                width,
                height,
                color: color.to_owned(),
            })
        }
    }
}

impl Session {
    /// Every markpen run in the document body and first-level table cells, as
    /// page rectangles. Runs on one line are merged; a run across lines yields
    /// one rectangle per line. Geometry failures skip the character, never fail.
    pub fn highlight_rects(&self) -> Vec<HighlightRect> {
        self.highlight_rects_with_limit(MAX_PROBES)
    }

    pub(crate) fn output_highlight_rects(&self) -> Vec<HighlightRect> {
        if let Some((generation, rects)) = self.render_highlights.borrow().as_ref()
            && *generation == self.generation
        {
            return rects.clone();
        }
        let rects = self.highlight_rects_with_limit(usize::MAX);
        *self.render_highlights.borrow_mut() = Some((self.generation, rects.clone()));
        rects
    }

    fn highlight_rects_with_limit(&self, limit: usize) -> Vec<HighlightRect> {
        let mut out = Vec::new();
        let mut probes = 0usize;
        let Ok(layout) = self.page_field_layout() else {
            return out;
        };
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let geometry = layout
            .as_ref()
            .map_or(&self.geometry_cache, |layout| &layout.geometry);
        let generation = layout
            .as_ref()
            .map_or(self.generation, |layout| layout.geometry_revision);
        for cursor in self.cursors() {
            let Ok(paragraph) = self.paragraph(cursor) else {
                continue;
            };
            for (range, color) in markpen_ranges(paragraph) {
                let mut current: Option<HighlightRect> = None;
                for offset in range {
                    probes += 1;
                    if probes > limit {
                        if let Some(r) = current.take() {
                            out.push(r);
                        }
                        return out;
                    }
                    let Some(next) = scalar_rect(
                        core,
                        &self.core,
                        geometry,
                        generation,
                        Cursor { offset, ..cursor },
                        &color,
                    ) else {
                        continue;
                    };
                    match current.as_mut() {
                        Some(r)
                            if r.page == next.page
                                && (r.y - next.y).abs() < 1.0
                                && (r.x + r.width - next.x).abs() < 1.5 =>
                        {
                            r.width = next.x + next.width - r.x;
                            r.height = r.height.max(next.height);
                        }
                        _ => {
                            if let Some(r) = current.take() {
                                out.push(r);
                            }
                            current = Some(next);
                        }
                    }
                }
                if let Some(r) = current.take() {
                    out.push(r);
                }
            }
        }
        out
    }
}
