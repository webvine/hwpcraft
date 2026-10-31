//! Owner: shell/page area. Putting a control into a paragraph or taking it out, together with
//! its 8-unit slot in the paragraph's UTF-16 stream and everything that points into the
//! paragraph by stream position or control index: char offsets, character-shape runs, line
//! starts, range tags, highlighter marks, CTRL_DATA records and field ranges. RHWP's own
//! shift helpers are crate-private. Moving the controls alone leaves a field range on the
//! wrong control (the field or hyperlink is dropped on save) or a later object at the
//! paragraph's end.
//!
//! Field ranges count text characters, so only their control index moves. A field's end
//! marker has a slot but no control; `remove_field` takes both markers out.
use crate::ext_format::open_control_slot;
use hwpcraft_doc::model::control::Control;
use hwpcraft_doc::model::paragraph::Paragraph;

const CTRL_UNITS: u32 = 8;

/// Insert `control` (with its CTRL_DATA record) in front of character `offset`, behind the
/// controls already there. Returns its control index.
pub(crate) fn insert_control(
    para: &mut Paragraph,
    offset: usize,
    control: Control,
    record: Option<Vec<u8>>,
) -> usize {
    let ci = open_control_slot(para, offset);
    put(para, ci, control, record);
    ci
}

/// Insert `control` as control `index` at the paragraph's head, where section and column
/// definitions go: its slot is the `index`-th one of the stream, in front of every
/// character and every other control. `controls[..index]` must be head controls too (a
/// column definition goes behind its section definition).
pub(crate) fn insert_head_control(
    para: &mut Paragraph,
    index: usize,
    control: Control,
    record: Option<Vec<u8>>,
) {
    let slot = index as u32 * CTRL_UNITS;
    para.char_count += CTRL_UNITS;
    for o in para.char_offsets.iter_mut().filter(|o| **o >= slot) {
        *o += CTRL_UNITS;
    }
    // The first character-shape run and line stay at the paragraph's start (RHWP's rule).
    let moves = |p: u32| p > slot || (p == slot && slot > 0);
    for cs in &mut para.char_shapes {
        if moves(cs.start_pos) {
            cs.start_pos += CTRL_UNITS;
        }
    }
    for ls in &mut para.line_segs {
        if moves(ls.text_start) {
            ls.text_start += CTRL_UNITS;
        }
    }
    for rt in &mut para.range_tags {
        if rt.start >= slot {
            rt.start += CTRL_UNITS;
        }
        if rt.end >= slot {
            rt.end += CTRL_UNITS;
        }
    }
    for m in &mut para.markpen_marks {
        if let Some(p) = &mut m.utf16_pos
            && *p >= slot
        {
            *p += CTRL_UNITS;
        }
    }
    put(para, index, control, record);
}

fn put(para: &mut Paragraph, ci: usize, control: Control, record: Option<Vec<u8>>) {
    para.align_ctrl_data_records();
    para.controls.insert(ci, control);
    para.ctrl_data_records.insert(ci, record);
    for range in &mut para.field_ranges {
        if range.control_idx >= ci {
            range.control_idx += 1;
        }
    }
    para.has_para_text = true;
    para.invalidate_layout_inputs();
}

/// Take control `ci` out of `para` and close its slot (the inverse of `insert_control`).
/// Returns the control and its CTRL_DATA record. A field's begin control goes through
/// `remove_field` instead, which also takes its end marker and range.
pub(crate) fn remove_control(para: &mut Paragraph, ci: usize) -> (Control, Option<Vec<u8>>) {
    let pos = para.control_text_positions().get(ci).copied().unwrap_or(0);
    para.align_ctrl_data_records();
    let control = para.controls.remove(ci);
    let record = para.ctrl_data_records.remove(ci);
    for range in &mut para.field_ranges {
        if range.control_idx > ci {
            range.control_idx -= 1;
        }
    }
    close_slot_before(para, pos);
    para.invalidate_layout_inputs();
    (control, record)
}

/// Take field range `range` out of `para` with both of its markers (the begin control and
/// the end slot); the text inside stays. Returns the field's control and CTRL_DATA record.
pub(crate) fn remove_field(para: &mut Paragraph, range: usize) -> (Control, Option<Vec<u8>>) {
    let field = para.field_ranges.remove(range);
    // The end marker sits in front of the character after the field. Closing it first
    // keeps the begin control's text position where it was.
    close_slot_before(para, field.end_char_idx);
    let pos = para
        .control_text_positions()
        .get(field.control_idx)
        .copied()
        .unwrap_or(field.start_char_idx);
    para.align_ctrl_data_records();
    let control = para.controls.remove(field.control_idx);
    let record = para.ctrl_data_records.remove(field.control_idx);
    for r in &mut para.field_ranges {
        if r.control_idx > field.control_idx {
            r.control_idx -= 1;
        }
    }
    close_slot_before(para, pos);
    para.invalidate_layout_inputs();
    (control, record)
}

/// Close one 8-unit slot in the gap in front of character `pos` (at the paragraph's end
/// nothing follows it and only the count changes).
fn close_slot_before(para: &mut Paragraph, pos: usize) {
    para.char_count = para.char_count.saturating_sub(CTRL_UNITS);
    let Some(&end) = para.char_offsets.get(pos) else {
        return;
    };
    // The slot ended where the following character starts.
    for o in &mut para.char_offsets[pos..] {
        *o = o.saturating_sub(CTRL_UNITS);
    }
    let moves = |p: u32| p >= end;
    for cs in &mut para.char_shapes {
        if moves(cs.start_pos) {
            cs.start_pos -= CTRL_UNITS;
        }
    }
    for ls in &mut para.line_segs {
        if moves(ls.text_start) {
            ls.text_start -= CTRL_UNITS;
        }
    }
    for rt in &mut para.range_tags {
        if moves(rt.start) {
            rt.start -= CTRL_UNITS;
        }
        if moves(rt.end) {
            rt.end -= CTRL_UNITS;
        }
    }
    for m in &mut para.markpen_marks {
        if let Some(p) = &mut m.utf16_pos
            && *p >= end
        {
            *p -= CTRL_UNITS;
        }
    }
}
