//! Owner: caret area. The caret behind the one inline object that ends its paragraph
//! without a character of its own.
//!
//! An object HwpCraft inserts gets a placeholder character, so the places before and after
//! it are two text offsets. Hangul stores none: a picture treated as a character alone in
//! its paragraph (a centred logo), or after the paragraph's text, leaves one text offset on
//! both sides, and RHWP answers a click behind it with an offset past the text. Here the
//! text offset stays the public cursor and "after the object" is a flag beside it: the caret
//! is drawn at the object's right edge, typing goes after the object, the arrows and End
//! step over it, Backspace after it and Delete before it remove it, and Enter there splits
//! after it. A click left of the object's middle goes in front of it. Only body
//! paragraphs with that one object: elsewhere the caret keeps the offsets RHWP and the
//! drawn caret already share (an object followed by text, several objects at the end, an
//! object HwpCraft inserted with its placeholder, table cells). Shift+arrows do not yet
//! select the object alone.

use crate::{Cursor, ObjectAddress, Result, Session, control_slots, err};
use hwpcraft_doc::model::paragraph::Paragraph;
use serde_json::Value;

/// A paragraph with one object treated as a character, standing at its end behind the text
/// (a picture alone, or after the text): there RHWP's offsets and the text offsets agree up
/// to the text's end, and only the place behind the object lies past it. With several
/// objects at the end the place a click means is left as it was.
fn objects_at_end(para: &Paragraph) -> bool {
    let len = para.text.chars().count();
    let mut ends = para
        .controls
        .iter()
        .zip(para.control_text_positions())
        .filter(|(control, _)| control.is_treat_as_char_object());
    matches!((ends.next(), ends.next()), (Some((_, position)), None) if position == len)
}

/// The object at the end of such a paragraph, when the caret offset `offset` is that end.
fn bare_at(para: &Paragraph, offset: usize) -> Option<usize> {
    let len = para.text.chars().count();
    if offset != len || !objects_at_end(para) {
        return None;
    }
    para.controls
        .iter()
        .zip(para.control_text_positions())
        .enumerate()
        .filter(|(_, (control, position))| *position == len && control.is_treat_as_char_object())
        .map(|(ci, _)| ci)
        .next_back()
}

impl Session {
    fn body_paragraph(&self, c: Cursor) -> Option<&Paragraph> {
        if c.cell.is_some() {
            return None;
        }
        self.core
            .document()
            .sections
            .get(c.section)?
            .paragraphs
            .get(c.paragraph)
    }

    /// The control the caret stands right behind (see the module notes).
    pub(crate) fn after_object_control(&self) -> Option<usize> {
        let c = self.after_object.get().filter(|c| *c == self.cursor)?;
        bare_at(self.body_paragraph(c)?, c.offset)
    }

    /// The caret stands behind an inline object that ends its paragraph (module notes).
    pub fn caret_after_object(&self) -> bool {
        self.after_object_control().is_some()
    }

    pub(crate) fn set_after_object(&self, on: bool) {
        self.after_object.set(on.then_some(self.cursor));
    }

    /// The pending click lands after an object: `select` takes it up with its cursor.
    pub(crate) fn accept_after_hit(&self, c: Cursor) {
        self.after_object
            .set(self.hit_after.take().filter(|h| *h == c));
    }

    /// In a paragraph whose one object ends it, RHWP answers a click on either side of the
    /// object past the text (also left of it, at the line's start): right of the object's
    /// middle it is the place behind the object, else the place in front of it.
    pub(crate) fn place_hit(&self, page: u32, x: f64, mut cursor: Cursor) -> Cursor {
        self.hit_after.set(None);
        let Some(para) = self.body_paragraph(cursor) else {
            return cursor;
        };
        if !objects_at_end(para) {
            return cursor;
        }
        let len = para.text.chars().count();
        if cursor.offset > len {
            cursor.offset = len;
            // Behind the object only right of its middle, as for a character: RHWP also
            // answers a click just inside a narrow equation's left edge past the text.
            let behind = bare_at(para, len).is_some_and(|ci| {
                self.bare_object_box(page, cursor, ci)
                    .is_none_or(|(bx, _, bw, _)| x >= bx + bw / 2.0)
            });
            if behind {
                self.hit_after.set(Some(cursor));
            }
        }
        cursor
    }

    /// The page box (x, y, w, h) of body control `ci` of the cursor's paragraph.
    fn bare_object_box(&self, page: u32, c: Cursor, ci: usize) -> Option<(f64, f64, f64, f64)> {
        let layout: Value = serde_json::from_str(&self.page_control_layout(page).ok()?).ok()?;
        let n = |v: &Value, k: &str| v[k].as_f64();
        layout["controls"].as_array()?.iter().find_map(|v| {
            (v["secIdx"].as_u64() == Some(c.section as u64)
                && v["paraIdx"].as_u64() == Some(c.paragraph as u64)
                && v["controlIdx"].as_u64() == Some(ci as u64)
                && v.get("cellIdx").is_none())
            .then(|| Some((n(v, "x")?, n(v, "y")?, n(v, "w")?, n(v, "h")?)))
            .flatten()
        })
    }

    /// The caret after the object: RHWP's caret at the logical place behind it (the object
    /// is the paragraph's only one, so that is the text end plus one), read from the copy
    /// with field values when there is one, like the other carets and the hit test.
    pub(crate) fn after_object_caret(&self, c: Cursor) -> Result<Option<Value>> {
        if c != self.cursor || self.after_object_control().is_none() {
            return Ok(None);
        }
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let rect = core
            .get_cursor_rect_native(c.section, c.paragraph, c.offset + 1)
            .map_err(err)?;
        Ok(Some(serde_json::from_str(&rect)?))
    }

    /// Enter after a bare object splits behind it.
    pub(crate) fn split_after_object(&self) -> usize {
        usize::from(self.after_object_control().is_some())
    }

    /// Typing after a bare object: the text went in front of it (RHWP inserts in front of
    /// controls at the same place), so the object moves back in front of the text.
    pub(crate) fn keep_object_in_front(&mut self, c: Cursor, ci: usize) -> Result<()> {
        let para = self
            .core
            .document_mut()
            .sections
            .get_mut(c.section)
            .and_then(|s| s.paragraphs.get_mut(c.paragraph))
            .ok_or_else(|| anyhow::anyhow!("문단을 찾지 못했습니다."))?;
        let (control, record) = control_slots::remove_control(para, ci);
        control_slots::insert_control(para, c.offset, control, record);
        self.core
            .relayout_paragraph_native(c.section, c.paragraph, None)
            .map_err(err)?;
        Ok(())
    }

    /// Backspace after, or Delete in front of, a bare object removes it. `None` when the
    /// key is not at one.
    pub(crate) fn delete_bare_object(&mut self, back: bool) -> Option<Result<()>> {
        let c = self.cursor;
        let para = self.body_paragraph(c)?;
        let ci = if back {
            self.after_object_control()?
        } else if self.after_object_control().is_none() {
            bare_at(para, c.offset)?
        } else {
            return None;
        };
        let at = ObjectAddress {
            section: c.section,
            paragraph: c.paragraph,
            control: ci,
        };
        let r = self.delete_object(at);
        self.cursor = c;
        self.set_after_object(false);
        Some(r)
    }

    /// After a horizontal caret move from `from`: → in front of a bare object first goes
    /// behind it, ← from behind it goes in front, and a ← that crossed the character behind
    /// an object (or reached the end of the previous paragraph) stands behind it.
    /// `true` when the step was taken here.
    pub(crate) fn step_over_object(&mut self, right: bool) -> bool {
        let c = self.cursor;
        let Some(para) = self.body_paragraph(c) else {
            return false;
        };
        match (right, self.after_object_control()) {
            (true, None) if bare_at(para, c.offset).is_some() => {
                self.set_after_object(true);
                true
            }
            (false, Some(_)) => {
                self.set_after_object(false);
                true
            }
            _ => false,
        }
    }

    /// After a ← that landed on `to`: behind the object when the step came from the next
    /// paragraph onto the object that ends this one.
    pub(crate) fn landed_left(&self, from: Cursor) {
        let to = self.cursor;
        let crossed = to.paragraph != from.paragraph || to.offset < from.offset;
        if crossed
            && let Some(para) = self.body_paragraph(to)
            && bare_at(para, to.offset).is_some()
        {
            self.set_after_object(true);
        }
    }

    /// Before Backspace at `c`: the object stands in front of the character it deletes, so
    /// once that character is gone the caret is behind the object ("[그림]가").
    pub(crate) fn object_before_deleted(&self, c: Cursor) -> bool {
        c.offset > 0
            && self.body_paragraph(c).is_some_and(|para| {
                para.controls.iter().zip(para.control_text_positions()).any(
                    |(control, position)| {
                        control.is_treat_as_char_object() && position == c.offset - 1
                    },
                )
            })
    }

    /// After that Backspace: behind the object when it now ends the paragraph.
    pub(crate) fn behind_after_backspace(&self, object_before: bool) {
        let c = self.cursor;
        if object_before
            && let Some(para) = self.body_paragraph(c)
            && bare_at(para, c.offset).is_some()
        {
            self.set_after_object(true);
        }
    }

    /// End at a bare object that ends the paragraph stands behind it.
    pub(crate) fn landed_line_end(&self) {
        let c = self.cursor;
        if let Some(para) = self.body_paragraph(c)
            && c.offset == para.text.chars().count()
            && bare_at(para, c.offset).is_some()
        {
            self.set_after_object(true);
        }
    }
}
