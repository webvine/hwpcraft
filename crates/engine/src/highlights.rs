//! Owner: core area. Screen rectangles for 형광펜 (markpen) ranges.
//!
//! RHWP stores markpen marks (`Paragraph::markpen_marks`) and saves them to
//! HWP/HWPX, but its renderer does not paint them. HwpCraft paints them itself as
//! an overlay, using the same caret geometry the selection highlight uses.
use super::*;
use hwpcraft_doc::model::paragraph::Paragraph;
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

/// Characters probed per call; beyond this the rest of the document is skipped.
const MAX_PROBES: usize = 20_000;

impl Session {
    /// Every markpen run in the document body and first-level table cells, as
    /// page rectangles. Runs on one line are merged; a run across lines yields
    /// one rectangle per line. Geometry failures skip the character, never fail.
    pub fn highlight_rects(&self) -> Vec<HighlightRect> {
        let mut out = Vec::new();
        let mut probes = 0usize;
        for cursor in self.cursors() {
            let Ok(paragraph) = self.paragraph(cursor) else {
                continue;
            };
            for (range, color) in markpen_ranges(paragraph) {
                let mut current: Option<HighlightRect> = None;
                for offset in range {
                    probes += 1;
                    if probes > MAX_PROBES {
                        if let Some(r) = current.take() {
                            out.push(r);
                        }
                        return out;
                    }
                    let a = self.caret_at(Cursor { offset, ..cursor });
                    let b = self.caret_at(Cursor {
                        offset: offset + 1,
                        ..cursor
                    });
                    let (Ok(a), Ok(b)) = (a, b) else {
                        continue;
                    };
                    let page = a["pageIndex"].as_u64().unwrap_or(0) as u32;
                    let x = a["x"].as_f64().unwrap_or(0.0);
                    let y = a["y"].as_f64().unwrap_or(0.0);
                    let h = a["height"].as_f64().unwrap_or(16.0);
                    let same_line = (b["y"].as_f64().unwrap_or(y) - y).abs() < 1.0
                        && b["pageIndex"].as_u64() == Some(u64::from(page));
                    let w = if same_line {
                        (b["x"].as_f64().unwrap_or(x) - x).max(2.0)
                    } else {
                        h * 0.6
                    };
                    match current.as_mut() {
                        Some(r)
                            if r.page == page
                                && (r.y - y).abs() < 1.0
                                && (r.x + r.width - x).abs() < 1.5 =>
                        {
                            r.width = x + w - r.x;
                            r.height = r.height.max(h);
                        }
                        _ => {
                            if let Some(r) = current.take() {
                                out.push(r);
                            }
                            current = Some(HighlightRect {
                                page,
                                x,
                                y,
                                width: w,
                                height: h,
                                color: color.clone(),
                            });
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
