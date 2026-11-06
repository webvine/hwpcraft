//! Owner: core area. Multi-paragraph selection, clipboard and caret motion.
use super::*;
use serde::{Deserialize, Serialize};

/// Caret motions shared by the desktop canvas and automated tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    Up,
    Down,
    PageUp,
    PageDown,
    ParagraphStart,
    ParagraphEnd,
    /// Start of the current word (Hangul MoveWordBegin).
    WordStart,
    /// End of the current word, before the following spaces (MoveWordEnd).
    WordEnd,
    /// Start of the previous paragraph, or of this one when not at its start.
    PrevParagraphStart,
    /// End of the previous paragraph; at the first paragraph, its start.
    PrevParagraphEnd,
    NextParagraphStart,
    PageStart,
    PageEnd,
    /// First/last position of the current list: the table cell, or the body.
    ListStart,
    ListEnd,
    DocumentStart,
    DocumentEnd,
}

/// Paragraphs that can be selected together: the body (over sections, as in
/// Hangul) or one table cell.
fn same_container(a: &Cursor, b: &Cursor) -> bool {
    match (a.cell, b.cell) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            a.section == b.section
                && a.paragraph == b.paragraph
                && x.control == y.control
                && x.index == y.index
        }
        _ => false,
    }
}
fn same_paragraph(a: &Cursor, b: &Cursor) -> bool {
    a.section == b.section && a.paragraph == b.paragraph && a.cell == b.cell
}
/// Paragraph index within the container (body paragraph or cell paragraph).
fn index(c: &Cursor) -> usize {
    c.cell.map(|t| t.paragraph).unwrap_or(c.paragraph)
}
/// Document order of a selection end: section, then paragraph, then offset.
fn order(c: &Cursor) -> (usize, usize, usize) {
    (c.section, index(c), c.offset)
}
fn with_index(c: Cursor, i: usize, offset: usize) -> Cursor {
    match c.cell {
        Some(t) => Cursor {
            cell: Some(Cell { paragraph: i, ..t }),
            offset,
            ..c
        },
        None => Cursor {
            paragraph: i,
            offset,
            ..c
        },
    }
}
pub(crate) fn class(ch: char) -> u8 {
    if ch.is_whitespace() {
        0
    } else if ch.is_alphanumeric() {
        1
    } else {
        2
    }
}

impl Session {
    /// The selection anchor, whether it lies in the caret paragraph or another one.
    pub fn anchor_cursor(&self) -> Option<Cursor> {
        self.span_anchor.or_else(|| {
            self.anchor.map(|offset| Cursor {
                offset,
                ..self.cursor
            })
        })
    }
    /// Ordered ends of a selection that spans paragraphs, or `None`.
    pub fn span(&self) -> Option<(Cursor, Cursor)> {
        let a = self.span_anchor?;
        let b = self.cursor;
        if order(&a) <= order(&b) {
            Some((a, b))
        } else {
            Some((b, a))
        }
    }
    pub fn has_selection(&self) -> bool {
        self.span_anchor.is_some() || !self.selection().is_empty()
    }
    pub fn clear_selection(&mut self) {
        self.anchor = None;
        self.span_anchor = None;
    }
    /// Every paragraph in the selection with its selected scalar range.
    /// Without a selection this is the caret paragraph's `selection()` range.
    pub fn selected_paragraphs(&self) -> Result<Vec<(Cursor, Range<usize>)>> {
        let Some((start, end)) = self.span() else {
            return Ok(vec![(self.cursor, self.selection())]);
        };
        let mut out = Vec::new();
        for section in start.section..=end.section {
            let first = if section == start.section {
                index(&start)
            } else {
                0
            };
            let last = if section == end.section {
                index(&end)
            } else {
                self.core.document().sections[section]
                    .paragraphs
                    .len()
                    .saturating_sub(1)
            };
            for i in first..=last {
                let c = with_index(Cursor { section, ..start }, i, 0);
                let len = self.paragraph(c)?.text.chars().count();
                let from = if section == start.section && i == index(&start) {
                    start.offset.min(len)
                } else {
                    0
                };
                let to = if section == end.section && i == index(&end) {
                    end.offset.min(len)
                } else {
                    len
                };
                out.push((c, from..to.max(from)));
            }
        }
        Ok(out)
    }
    pub(crate) fn span_text(&self) -> Result<String> {
        let mut parts = Vec::new();
        for (c, r) in self.selected_paragraphs()? {
            let text = &self.paragraph(c)?.text;
            parts.push(text.chars().skip(r.start).take(r.len()).collect::<String>());
        }
        Ok(parts.join("\n"))
    }
    /// Drop a stale span after commands that changed paragraph structure.
    pub(crate) fn normalize_selection(&mut self) {
        if let Some(a) = self.span_anchor {
            let valid = same_container(&a, &self.cursor)
                && !same_paragraph(&a, &self.cursor)
                && self
                    .paragraph(a)
                    .is_ok_and(|p| a.offset <= p.text.chars().count());
            if !valid {
                self.span_anchor = None;
            }
        }
        let len = self.len();
        if self.anchor.is_some_and(|a| a > len) {
            self.anchor = None;
        }
        self.cursor.offset = self.cursor.offset.min(len);
    }
    /// Move the caret, extending the selection across paragraphs of one container.
    pub fn select(&mut self, cursor: Cursor, extend: bool) -> Result<()> {
        self.paragraph(cursor)?;
        self.insertion_shape.explicit_selection();
        let anchor = self.anchor_cursor().unwrap_or(self.cursor);
        self.clear_selection();
        if extend && same_container(&anchor, &cursor) {
            if same_paragraph(&anchor, &cursor) {
                self.anchor = Some(anchor.offset);
            } else {
                self.span_anchor = Some(anchor);
            }
        }
        self.cursor = cursor;
        self.cursor.offset = self.cursor.offset.min(self.len());
        self.accept_visual_hit(self.cursor);
        self.accept_after_hit(self.cursor);
        self.typing = None;
        Ok(())
    }
    /// Delete the current selection, including a multi-paragraph span.
    pub(crate) fn delete_selection(&mut self) -> Result<()> {
        if self.track_recording() && self.has_selection() {
            return self.track_delete_selection();
        }
        let Some((start, mut end)) = self.span() else {
            return self.delete_range(self.selection());
        };
        if end.section > start.section {
            // Over section boundaries: the later sections join the first one (as
            // deleting a section break does), then the joined range goes.
            let joined: usize = self.core.document().sections[start.section..end.section]
                .iter()
                .map(|s| s.paragraphs.len())
                .sum();
            for section in (start.section + 1..=end.section).rev() {
                self.join_sections(section, false)?;
            }
            end = Cursor {
                section: start.section,
                paragraph: joined + end.paragraph,
                ..end
            };
        }
        let cell = start.cell.map(|t| (start.paragraph, t.control, t.index));
        self.core
            .delete_range_native(
                start.section,
                index(&start),
                start.offset,
                index(&end),
                end.offset,
                cell,
            )
            .map_err(err)?;
        self.cursor = start;
        self.clear_selection();
        Ok(())
    }
    /// Select the whole cell, or the whole body of the document (every section).
    pub fn select_all(&mut self) -> Result<()> {
        if self.cursor.cell.is_none() {
            let sections = &self.core.document().sections;
            let last = sections.len().saturating_sub(1);
            let paragraph = sections
                .get(last)
                .map_or(0, |s| s.paragraphs.len().saturating_sub(1));
            let end = Cursor {
                section: last,
                paragraph,
                offset: 0,
                cell: None,
            };
            let len = self.paragraph(end)?.text.chars().count();
            self.select(Cursor::default(), false)?;
            return self.select(Cursor { offset: len, ..end }, true);
        }
        let count = match self.cursor.cell {
            Some(t) => match self
                .core
                .document()
                .sections
                .get(self.cursor.section)
                .and_then(|s| s.paragraphs.get(self.cursor.paragraph))
                .and_then(|p| p.controls.get(t.control))
            {
                Some(Control::Table(table)) => table
                    .cells
                    .get(t.index)
                    .map(|c| c.paragraphs.len())
                    .unwrap_or(0),
                _ => 0,
            },
            None => self
                .core
                .document()
                .sections
                .get(self.cursor.section)
                .map(|s| s.paragraphs.len())
                .unwrap_or(0),
        };
        let last = count
            .checked_sub(1)
            .ok_or_else(|| anyhow!("선택할 문단이 없습니다."))?;
        let first = with_index(self.cursor, 0, 0);
        let len = self
            .paragraph(with_index(first, last, 0))?
            .text
            .chars()
            .count();
        self.select(first, false)?;
        self.select(with_index(first, last, len), true)
    }
    /// Select the word (or run of spaces/punctuation) around the caret.
    pub fn select_word(&mut self) -> Result<()> {
        let chars: Vec<char> = self.text().chars().collect();
        let at = self.cursor.offset.min(chars.len());
        let probe = if at < chars.len() {
            at
        } else {
            at.saturating_sub(1)
        };
        let Some(kind) = chars.get(probe).map(|c| class(*c)) else {
            return Ok(());
        };
        let mut start = probe;
        while start > 0 && class(chars[start - 1]) == kind {
            start -= 1;
        }
        let mut end = probe;
        while end < chars.len() && class(chars[end]) == kind {
            end += 1;
        }
        self.clear_selection();
        self.anchor = Some(start);
        self.cursor.offset = end;
        Ok(())
    }
    pub fn select_paragraph(&mut self) {
        self.clear_selection();
        self.anchor = Some(0);
        self.cursor.offset = self.len();
    }
    fn paragraph_count(&self, c: &Cursor) -> usize {
        let Some(para) = self
            .core
            .document()
            .sections
            .get(c.section)
            .and_then(|s| s.paragraphs.get(c.paragraph))
        else {
            return 0;
        };
        match c.cell {
            None => self
                .core
                .document()
                .sections
                .get(c.section)
                .map(|s| s.paragraphs.len())
                .unwrap_or(0),
            Some(t) => match para.controls.get(t.control) {
                Some(Control::Table(table)) => table
                    .cells
                    .get(t.index)
                    .map(|c| c.paragraphs.len())
                    .unwrap_or(0),
                _ => 0,
            },
        }
    }
    /// The caret's line: its page and the line's cross position (y, or x in a vertical
    /// section, where a line is a column).
    fn caret_line(&self, c: Cursor) -> Option<(u64, f64)> {
        let v = self.caret_at(c).ok()?;
        let along = if v["vertical"] == true { "x" } else { "y" };
        Some((v["pageIndex"].as_u64()?, v[along].as_f64()?))
    }
    /// ↓/↑ inside a paragraph: from the stored line (`line_segs`) the caret is shown on
    /// to the one after (`up`: before) it, in the order the paragraph runs through
    /// columns and pages, keeping the caret's distance from the start of its line.
    /// `caret` is the caret as shown (page, x, y). `None` on the paragraph's last
    /// (`up`: first) line, or when the result would not leave the caret's line.
    fn row_neighbour(&self, c: Cursor, caret: (u64, f64, f64), h: f64, up: bool) -> Option<Cursor> {
        let p = self.paragraph(c).ok()?;
        // Positions count an object treated as a character as one; the text does not.
        let len = crate::contexts::logical_split_offset(p, p.text.chars().count());
        // Each line's first character, as RHWP finds it (0104's `row_text_start`).
        let starts: Vec<usize> = p
            .line_segs
            .iter()
            .map(|l| {
                let text = p
                    .char_offsets
                    .iter()
                    .position(|&start| start >= l.text_start)
                    .unwrap_or(p.char_offsets.len());
                crate::contexts::logical_split_offset(p, text)
            })
            .collect();
        let end = |k: usize| starts.get(k + 1).copied().unwrap_or(len);
        let at = |offset: usize| Cursor { offset, ..c };
        // A line's place: its second position, as the first is shown at the end of
        // the line before.
        let place = |k: usize| {
            let v = self.caret_at(at((starts[k] + 1).min(end(k)))).ok()?;
            Some((v["pageIndex"].as_u64()?, v["x"].as_f64()?, v["y"].as_f64()?))
        };
        let same = |a: (u64, f64, f64), b: (u64, f64, f64)| a.0 == b.0 && (a.2 - b.2).abs() < 0.5;
        let mut r = starts.iter().rposition(|&s| s <= c.offset)?;
        let mut here = place(r)?;
        // A line's first position may be shown at the end of the line before.
        if r > 0 && c.offset == starts[r] && !same(here, caret) {
            r -= 1;
            here = place(r)?;
        }
        if !same(here, caret) {
            return None;
        }
        let target = if up { r.checked_sub(1)? } else { r + 1 };
        if target >= starts.len() {
            return None;
        }
        let there = place(target)?;
        let t = self
            .hit_test(
                there.0 as u32,
                caret.1 - here.1 + there.1,
                there.2 + h * 0.5,
            )
            .ok()
            .filter(|t| {
                t.section == c.section
                    && t.cell == c.cell
                    && index(t) == index(&c)
                    && (starts[target]..=end(target)).contains(&t.offset)
            })
            .unwrap_or(at((starts[target] + 1).min(end(target))));
        (t != c).then_some(t)
    }
    /// In a vertical section (세로쓰기) the arrows follow the columns: ↑↓ move through
    /// the column's characters, ← goes on to the next column (to the left), → back.
    fn vertical_motion(&self, motion: Motion) -> Motion {
        if !self.caret().is_ok_and(|v| v["vertical"] == true) {
            return motion;
        }
        match motion {
            Motion::Up => Motion::Left,
            Motion::Down => Motion::Right,
            Motion::Left => Motion::Down,
            Motion::Right => Motion::Up,
            other => other,
        }
    }
    /// Zero-based global page of a position (body paragraph of a cell caret).
    pub fn page_of(&self, c: Cursor) -> Option<usize> {
        self.caret_at(c).ok()?["pageIndex"]
            .as_u64()
            .map(|p| p as usize)
    }
    /// First body position of a zero-based global page; `None` past the last page.
    pub fn page_first_position(&self, page: usize) -> Option<Cursor> {
        let layout = self.page_field_layout().ok()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let raw = core.get_position_of_page_native(page).ok()?;
        let v: Value = serde_json::from_str(&raw).ok()?;
        let n = |k: &str| v[k].as_u64().map(|x| x as usize);
        let c = Cursor {
            section: n("sec")?,
            paragraph: n("para")?,
            offset: n("charOffset").unwrap_or(0),
            cell: None,
        };
        // Native partial-paragraph page queries return offset zero. Keep that
        // address when it is really on this page (including leading empty
        // paragraphs), otherwise use the rendered body's actual first scalar.
        if self.caret_at(c).ok()?["pageIndex"].as_u64() != Some(page as u64) {
            let first = if let Some(layout) = layout.as_ref() {
                layout.first_position(page as u32).ok()?
            } else {
                self.geometry_cache
                    .first_body_position(core, self.generation, page as u32)
                    .ok()?
                    .and_then(|position| position.legacy_cursor())
            };
            if let Some(first) = first {
                return self.paragraph(first).ok().map(|_| first);
            }
        }
        self.paragraph(c).ok().map(|_| c)
    }
    /// The last position before a body paragraph start (end of the previous paragraph).
    fn position_before(&self, c: Cursor) -> Result<Cursor> {
        if c.offset > 0 {
            return Ok(Cursor {
                offset: c.offset - 1,
                ..c
            });
        }
        let prev = if c.paragraph > 0 {
            Cursor {
                paragraph: c.paragraph - 1,
                offset: 0,
                cell: None,
                ..c
            }
        } else if c.section > 0 {
            let s = c.section - 1;
            let p = self
                .core
                .document()
                .sections
                .get(s)
                .map(|x| x.paragraphs.len().saturating_sub(1))
                .unwrap_or(0);
            Cursor {
                section: s,
                paragraph: p,
                offset: 0,
                cell: None,
            }
        } else {
            return Ok(c);
        };
        Ok(Cursor {
            offset: self.paragraph(prev)?.text.chars().count(),
            ..prev
        })
    }
    /// The caret position a motion lands on, without changing the session.
    pub fn motion_target(&self, motion: Motion) -> Result<Cursor> {
        let c = self.cursor;
        let len = self.len();
        let chars: Vec<char> = self.text().chars().collect();
        let count = self.paragraph_count(&c);
        let i = index(&c);
        let previous_end = || -> Result<Cursor> {
            let p = with_index(c, i - 1, 0);
            Ok(with_index(
                p,
                i - 1,
                self.paragraph(p)?.text.chars().count(),
            ))
        };
        Ok(match motion {
            Motion::Left if c.offset > 0 => Cursor {
                offset: c.offset - 1,
                ..c
            },
            Motion::Left if i > 0 => previous_end()?,
            Motion::Right if c.offset < len => Cursor {
                offset: c.offset + 1,
                ..c
            },
            Motion::Right if i + 1 < count => with_index(c, i + 1, 0),
            Motion::Left | Motion::Right => c,
            Motion::WordLeft => {
                if c.offset == 0 {
                    return if i > 0 { previous_end() } else { Ok(c) };
                }
                let mut o = c.offset;
                while o > 0 && class(chars[o - 1]) == 0 {
                    o -= 1;
                }
                if o > 0 {
                    let kind = class(chars[o - 1]);
                    while o > 0 && class(chars[o - 1]) == kind {
                        o -= 1;
                    }
                }
                Cursor { offset: o, ..c }
            }
            Motion::WordRight => {
                if c.offset >= len {
                    return Ok(if i + 1 < count {
                        with_index(c, i + 1, 0)
                    } else {
                        c
                    });
                }
                let mut o = c.offset;
                let kind = class(chars[o]);
                while o < len && class(chars[o]) == kind {
                    o += 1;
                }
                while o < len && class(chars[o]) == 0 {
                    o += 1;
                }
                Cursor { offset: o, ..c }
            }
            Motion::LineStart | Motion::LineEnd => {
                let Some(line) = self.caret_line(c) else {
                    return Ok(c);
                };
                let same = |o: usize| {
                    self.caret_line(Cursor { offset: o, ..c })
                        .is_some_and(|(p, y)| p == line.0 && (y - line.1).abs() < 1.0)
                };
                let mut o = c.offset;
                if motion == Motion::LineStart {
                    while o > 0 && same(o - 1) {
                        o -= 1;
                    }
                } else {
                    while o < len && same(o + 1) {
                        o += 1;
                    }
                }
                Cursor { offset: o, ..c }
            }
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown => {
                let up = matches!(motion, Motion::Up | Motion::PageUp);
                // A paragraph that holds only objects (a table) may have no caret
                // position of its own; only those are asked, a caret costs a layout.
                let caretable = |t: Cursor| {
                    self.paragraph(t)
                        .is_ok_and(|p| !p.text.is_empty() || p.controls.is_empty())
                        || self.caret_at(t).is_ok()
                };
                // The nearest paragraph in the motion's direction with a caret, in the
                // paragraph list `base` belongs to; past a cell's first or last
                // paragraph, the body paragraphs around the table.
                let neighbour = |base: Cursor| -> Cursor {
                    let mut base = base;
                    loop {
                        let count = self.paragraph_count(&base);
                        let mut j = index(&base);
                        loop {
                            let next = if up { j.checked_sub(1) } else { Some(j + 1) };
                            match next {
                                Some(n) if n < count => {
                                    let t = with_index(base, n, 0);
                                    if caretable(t) {
                                        return t;
                                    }
                                    j = n;
                                }
                                _ => break,
                            }
                        }
                        if base.cell.is_none() {
                            return c;
                        }
                        base = Cursor {
                            offset: 0,
                            cell: None,
                            ..base
                        };
                    }
                };
                let Ok(v) = self.caret() else {
                    return Ok(neighbour(c));
                };
                let page = v["pageIndex"].as_u64().unwrap_or(0) as u32;
                let x = v["x"].as_f64().unwrap_or(0.0);
                let y = v["y"].as_f64().unwrap_or(0.0);
                let h = v["height"].as_f64().unwrap_or(16.0).max(4.0);
                // A vertical section's lines are columns, the next one to the left.
                let w = v["width"].as_f64().unwrap_or(0.0);
                let vertical = v["vertical"] == true;
                let target = match motion {
                    Motion::Up if vertical => self.hit_test(page, x + w * 2.1, y),
                    Motion::Down if vertical => self.hit_test(page, x - w * 1.1, y),
                    // A line made tall by an object treated as a character holds the
                    // caret low in it: probe on, a caret height at a time, until the hit
                    // leaves the caret's line.
                    // Inside a paragraph, the next or previous stored line.
                    Motion::Up | Motion::Down
                        if let Some(t) = self.row_neighbour(c, (u64::from(page), x, y), h, up) =>
                    {
                        Ok(t)
                    }
                    Motion::Up | Motion::Down => {
                        let line = self.caret_line(c);
                        let probe = |k: f64| {
                            let dy = if up { -h * (0.7 + k) } else { h * (1.7 + k) };
                            self.hit_test(page, x, y + dy)
                        };
                        // No further than the paragraph's tallest line (HWPUNIT, 75 a px).
                        let tallest = self.paragraph(c).map_or(0.0, |p| {
                            p.line_segs
                                .iter()
                                .map(|l| f64::from(l.line_height.max(0)) / 75.0)
                                .fold(0.0, f64::max)
                        });
                        let steps = ((tallest / h).ceil() as u32).min(40);
                        let mut hit = probe(0.0);
                        for k in 1..=steps {
                            match &hit {
                                Ok(t)
                                    if *t == c
                                        || (t.section == c.section
                                            && t.cell == c.cell
                                            && index(t) == i
                                            && self.caret_line(*t) == line) =>
                                {
                                    hit = probe(f64::from(k));
                                }
                                _ => break,
                            }
                        }
                        // Still on the line the caret is shown on (the first or last line
                        // of the document or page): no line to go to there.
                        if let Ok(t) = &hit
                            && t.section == c.section
                            && t.cell == c.cell
                            && index(t) == i
                            && self
                                .caret_line(*t)
                                .is_some_and(|(p, ty)| p == u64::from(page) && (ty - y).abs() < 0.5)
                        {
                            hit = Ok(c);
                        }
                        hit
                    }
                    Motion::PageUp if page > 0 => self.hit_test(page - 1, x, y),
                    Motion::PageDown if page + 1 < self.page_count()? => {
                        self.hit_test(page + 1, x, y)
                    }
                    _ => Ok(c),
                };
                // A paragraph reached by stepping (one on another page, or past a
                // table): its last line (↑) or first line (↓) at the caret's x.
                let aligned = |n: Cursor| -> Cursor {
                    if n == c || vertical || !matches!(motion, Motion::Up | Motion::Down) {
                        return n;
                    }
                    let Ok(p) = self.paragraph(n) else {
                        return n;
                    };
                    let edge = if up {
                        crate::contexts::logical_split_offset(p, p.text.chars().count())
                    } else {
                        0
                    };
                    let Some((at_page, at_y)) = self.caret_line(Cursor { offset: edge, ..n })
                    else {
                        return n;
                    };
                    self.hit_test(at_page as u32, x, at_y + h * 0.5)
                        .ok()
                        .filter(|t| {
                            t.section == n.section && t.cell == n.cell && index(t) == index(&n)
                        })
                        .unwrap_or(n)
                };
                match target {
                    Ok(t) if t != c && self.paragraph(t).is_ok() && caretable(t) => t,
                    // A table-only paragraph has no caret: go on to the next one with
                    // a caret beyond it.
                    Ok(t) if t != c && self.paragraph(t).is_ok() => aligned(neighbour(t)),
                    // Fall back to the neighbouring paragraph when hit testing lands
                    // in a margin, a nested object or the same place.
                    _ => aligned(neighbour(c)),
                }
            }
            Motion::ParagraphStart => Cursor { offset: 0, ..c },
            Motion::ParagraphEnd => Cursor { offset: len, ..c },
            Motion::WordStart => {
                // Inside or at the end of a word: its start. Otherwise the previous word.
                let o = c.offset.min(len);
                if o > 0
                    && class(chars[o - 1]) != 0
                    && (o == len || class(chars[o]) == class(chars[o - 1]))
                {
                    let kind = class(chars[o - 1]);
                    let mut start = o;
                    while start > 0 && class(chars[start - 1]) == kind {
                        start -= 1;
                    }
                    Cursor { offset: start, ..c }
                } else {
                    self.motion_target(Motion::WordLeft)?
                }
            }
            Motion::WordEnd => {
                let mut o = c.offset.min(len);
                // Skip spaces first, then run to the end of the word they precede.
                while o < len && class(chars[o]) == 0 {
                    o += 1;
                }
                if o >= len {
                    return Ok(if i + 1 < count {
                        with_index(c, i + 1, 0)
                    } else {
                        Cursor { offset: len, ..c }
                    });
                }
                let kind = class(chars[o]);
                while o < len && class(chars[o]) == kind {
                    o += 1;
                }
                Cursor { offset: o, ..c }
            }
            Motion::PrevParagraphStart => {
                if c.offset > 0 || i == 0 {
                    Cursor { offset: 0, ..c }
                } else {
                    with_index(c, i - 1, 0)
                }
            }
            Motion::PrevParagraphEnd => {
                if i > 0 {
                    previous_end()?
                } else {
                    Cursor { offset: 0, ..c }
                }
            }
            Motion::NextParagraphStart => {
                if i + 1 < count {
                    with_index(c, i + 1, 0)
                } else {
                    Cursor { offset: len, ..c }
                }
            }
            Motion::PageStart => {
                let page = self.page_of(c);
                page.and_then(|p| self.page_first_position(p))
                    .unwrap_or(Cursor { offset: 0, ..c })
            }
            Motion::PageEnd => match self
                .page_of(c)
                .and_then(|p| self.page_first_position(p + 1))
            {
                Some(next) if next.offset > 0 => next,
                Some(next) => self.position_before(next)?,
                None => self.motion_target(Motion::DocumentEnd)?,
            },
            Motion::ListStart => {
                if c.cell.is_some() {
                    with_index(c, 0, 0)
                } else {
                    Cursor::default()
                }
            }
            Motion::ListEnd => {
                if c.cell.is_some() {
                    let last = count.saturating_sub(1);
                    let end = with_index(c, last, 0);
                    with_index(c, last, self.paragraph(end)?.text.chars().count())
                } else {
                    self.motion_target(Motion::DocumentEnd)?
                }
            }
            Motion::DocumentStart => Cursor::default(),
            Motion::DocumentEnd => {
                let sections = &self.core.document().sections;
                let s = sections.len().saturating_sub(1);
                let p = sections
                    .get(s)
                    .map(|x| x.paragraphs.len())
                    .unwrap_or(1)
                    .saturating_sub(1);
                let end = Cursor {
                    section: s,
                    paragraph: p,
                    offset: 0,
                    cell: None,
                };
                Cursor {
                    offset: self.paragraph(end)?.text.chars().count(),
                    ..end
                }
            }
        })
    }
    pub fn move_caret(&mut self, motion: Motion, extend: bool) -> Result<()> {
        self.recorded(
            || {
                Some(crate::MacroStep {
                    command: "caret.move".into(),
                    params: json!({ "motion": motion, "extend": extend }),
                })
            },
            |s| s.move_caret_unrecorded(motion, extend),
        )
    }
    fn move_caret_unrecorded(&mut self, motion: Motion, extend: bool) -> Result<()> {
        self.hit_hint.set(None);
        let motion = self.vertical_motion(motion);
        // An inline object without a character of its own is one step (`bare_objects`).
        if !extend
            && !self.has_selection()
            && matches!(motion, Motion::Left | Motion::Right)
            && self.step_over_object(motion == Motion::Right)
        {
            return Ok(());
        }
        if !extend && self.has_selection() && matches!(motion, Motion::Left | Motion::Right) {
            // Like Hangul, an arrow collapses a selection to its matching edge.
            let edges = self.selected_paragraphs()?;
            let target = if motion == Motion::Left {
                edges.first().map(|(c, r)| Cursor {
                    offset: r.start,
                    ..*c
                })
            } else {
                edges.last().map(|(c, r)| Cursor {
                    offset: r.end,
                    ..*c
                })
            };
            if let Some(t) = target {
                return self.select(t, false);
            }
        }
        let visual_page = if matches!(
            motion,
            Motion::LineStart | Motion::LineEnd | Motion::PageStart | Motion::PageEnd
        ) {
            self.caret()
                .ok()
                .and_then(|v| v["pageIndex"].as_u64())
                .map(|p| p as u32)
        } else {
            None
        };
        let target = self.motion_target(motion)?;
        let previous = self.insertion_shape.clone();
        let from = self.cursor;
        self.select(target, extend)?;
        self.insertion_shape = previous;
        self.insertion_shape
            .motion(from, self.cursor, motion, extend);
        if let Some(page) = visual_page {
            self.set_visual_hint(
                self.cursor,
                page,
                if matches!(motion, Motion::LineEnd | Motion::PageEnd) {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                },
            );
        }
        if !extend {
            match motion {
                Motion::Left => self.landed_left(from),
                Motion::LineEnd => self.landed_line_end(),
                _ => {}
            }
        }
        Ok(())
    }
    /// Copy to RHWP's internal clipboard (formats, tables and objects) and return plain text.
    pub fn copy(&mut self) -> Result<String> {
        self.guard_distribution()?;
        if !self.has_selection() {
            return Ok(String::new());
        }
        let text = self.span_text()?;
        // RHWP's rich copy covers one section; over sections only the text goes out.
        if self.span().is_some_and(|(a, b)| a.section != b.section) {
            self.clipboard_text = None;
            return Ok(text);
        }
        let replay = clipboard_history::ClipboardReplay::prepare(self)?;
        let result = replay.copy_into(&mut self.core);
        if result.is_ok() {
            self.clipboard_replay = Some(std::rc::Rc::new(replay));
        }
        // Plain text still reaches the system clipboard if rich copy is unsupported here.
        self.clipboard_text = result.ok().map(|_| text.clone());
        Ok(text)
    }
    /// Cut is copy plus one undoable deletion.
    pub fn cut(&mut self) -> Result<String> {
        self.guard_protected()?;
        let text = self.copy()?;
        if self.has_selection() {
            self.track_scope(|s| s.transaction(|s| s.delete_selection()))?;
        }
        Ok(text)
    }
    /// HTML for the system clipboard (body selections only), so other programs
    /// receive formatting. `None` inside cells or without a selection.
    pub fn copy_html(&self) -> Result<Option<String>> {
        self.guard_distribution()?;
        if !self.has_selection() {
            return Ok(None);
        }
        let parts = self.selected_paragraphs()?;
        let (Some((first, r1)), Some((last, r2))) = (parts.first(), parts.last()) else {
            return Ok(None);
        };
        if first.cell.is_some() || first.section != last.section {
            return Ok(None);
        }
        Ok(self
            .core
            .export_selection_html_native(
                first.section,
                first.paragraph,
                r1.start,
                last.paragraph,
                r2.end,
            )
            .ok()
            .filter(|h| !h.trim().is_empty()))
    }
    /// Paste from the system clipboard: RHWP's internal clipboard when the text is
    /// our last copy, otherwise the HTML flavour (formatting from other programs),
    /// otherwise plain text.
    pub fn paste_with_html(&mut self, text: &str, html: Option<&str>) -> Result<()> {
        let normalize = |s: &str| s.replace("\r\n", "\n").replace('\r', "\n");
        let internal = self.clipboard_text.as_deref().map(normalize) == Some(normalize(text))
            && self.core.has_internal_clipboard_native();
        match html {
            Some(html) if !internal && !html.trim().is_empty() => {
                // A failed HTML import leaves the document untouched; fall back to text.
                match self.paste_html(html) {
                    Ok(()) => Ok(()),
                    Err(_) => self.paste(text),
                }
            }
            _ => self.paste(text),
        }
    }
    /// Insert HTML (from the clipboard or a command) at the caret with its formatting.
    pub fn paste_html(&mut self, html: &str) -> Result<()> {
        if html.len() > 4 * 1024 * 1024 {
            bail!("HTML 붙이기는 4 MB 이하만 지원합니다.");
        }
        if html.trim().is_empty() {
            bail!("붙일 HTML이 없습니다.");
        }
        self.guard_protected()?;
        self.transaction(|s| {
            s.delete_selection()?;
            let c = s.cursor;
            if s.split_offset(c)? != c.offset {
                bail!("개체가 있는 자리에는 HTML을 붙일 수 없습니다.");
            }
            let raw = match c.cell {
                Some(t) => s.core.paste_html_in_cell_native(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    c.offset,
                    html,
                ),
                None => s
                    .core
                    .paste_html_native(c.section, c.paragraph, c.offset, html),
            }
            .map_err(err)?;
            let v: Value = serde_json::from_str(&raw)?;
            if v["ok"] != true {
                bail!(
                    "HTML 붙이기에 실패했습니다: {}",
                    v["error"].as_str().unwrap_or("")
                );
            }
            let n = |k: &str| v[k].as_u64().map(|x| x as usize);
            let para = n("cellParaIdx")
                .or_else(|| n("paraIdx"))
                .unwrap_or(index(&c));
            s.cursor = with_index(c, para, n("charOffset").unwrap_or(c.offset));
            s.clear_selection();
            s.normalize_selection();
            Ok(())
        })
    }
    /// Paste keeps RHWP formatting when the system clipboard still holds our last copy.
    pub fn paste(&mut self, text: &str) -> Result<()> {
        let normalize = |s: &str| s.replace("\r\n", "\n").replace('\r', "\n");
        let internal = self.clipboard_text.as_deref().map(normalize) == Some(normalize(text))
            && self.core.has_internal_clipboard_native();
        if !internal {
            return self.execute(Command::Insert(text.to_owned()));
        }
        self.guard_protected()?;
        self.transaction(|s| {
            s.delete_selection()?;
            let c = s.cursor;
            // RHWP splits multi-paragraph pastes at logical offsets; with inline
            // objects before the caret, plain text is the safe fallback.
            if s.split_offset(c)? != c.offset {
                return s.insert(text);
            }
            let raw = match c.cell {
                Some(t) => s.core.paste_internal_in_cell_native(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    c.offset,
                ),
                None => s
                    .core
                    .paste_internal_native(c.section, c.paragraph, c.offset),
            }
            .map_err(err)?;
            let v: Value = serde_json::from_str(&raw)?;
            if v["ok"] != true {
                bail!(
                    "붙이기에 실패했습니다: {}",
                    v["error"].as_str().unwrap_or("")
                );
            }
            let n = |k: &str| v[k].as_u64().map(|x| x as usize);
            let para = n("cellParaIdx")
                .or_else(|| n("paraIdx"))
                .unwrap_or(index(&c));
            s.cursor = with_index(c, para, n("charOffset").unwrap_or(c.offset));
            s.clear_selection();
            s.normalize_selection();
            Ok(())
        })
    }
}
