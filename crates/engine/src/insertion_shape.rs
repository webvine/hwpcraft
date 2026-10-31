//! Pending caret formatting, separated from formatting existing selected text.
//!
//! Body behavior follows isolated public Hancom probes. First-level cell model
//! operations use the same native primitives, but native cell/IME/field-boundary
//! parity has not yet been measured. Root owns Session/history integration.
use super::*;
use hwpcraft_doc::model::{
    document::{DocInfo, Document},
    paragraph::CharShapeRef,
    style::{BorderFill, CharShape, Font},
};

#[derive(Clone, Default)]
pub(crate) struct InsertionShapeState {
    pending: Option<(Cursor, NativeInsertionShape)>,
    stamp: u64,
}

/// Native attributes and exact resources, not a lossy UI-JSON history payload.
#[derive(Clone)]
pub(crate) struct NativeInsertionShape {
    shape: CharShape,
    fonts: [Option<Font>; 7],
    border: Option<BorderFill>,
    properties: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Ephemeral,
    Empty,
    Terminal,
    Leading(u32),
}

pub(crate) struct PreparedFormat {
    parts: Vec<(Cursor, Range<usize>)>,
    patch: Value,
    collapsed: Option<(Cursor, Boundary, NativeInsertionShape)>,
    durable_write: bool,
    /// The caller already defers pagination (an atomic batch or replace-all).
    layout_deferred: bool,
}

#[derive(Clone)]
pub(crate) struct InsertTicket {
    style: NativeInsertionShape,
    apply: bool,
}

pub(crate) struct SplitTicket {
    carry: Option<NativeInsertionShape>,
}

fn paragraph(doc: &Document, at: Cursor) -> Result<&Paragraph> {
    let root = doc
        .sections
        .get(at.section)
        .and_then(|s| s.paragraphs.get(at.paragraph))
        .ok_or_else(|| anyhow!("입력 서식의 본문 문단이 없습니다."))?;
    if let Some(cell) = at.cell {
        let Some(Control::Table(table)) = root.controls.get(cell.control) else {
            bail!("입력 서식의 표가 없습니다.");
        };
        table
            .cells
            .get(cell.index)
            .and_then(|c| c.paragraphs.get(cell.paragraph))
            .ok_or_else(|| anyhow!("입력 서식의 셀 문단이 없습니다."))
    } else {
        Ok(root)
    }
}

fn paragraph_mut(doc: &mut Document, at: Cursor) -> Result<&mut Paragraph> {
    let root = doc
        .sections
        .get_mut(at.section)
        .and_then(|s| s.paragraphs.get_mut(at.paragraph))
        .ok_or_else(|| anyhow!("입력 서식의 본문 문단이 없습니다."))?;
    if let Some(cell) = at.cell {
        let Some(Control::Table(table)) = root.controls.get_mut(cell.control) else {
            bail!("입력 서식의 표가 없습니다.");
        };
        table
            .cells
            .get_mut(cell.index)
            .and_then(|c| c.paragraphs.get_mut(cell.paragraph))
            .ok_or_else(|| anyhow!("입력 서식의 셀 문단이 없습니다."))
    } else {
        Ok(root)
    }
}

fn checked(doc: &Document, at: Cursor, range: &Range<usize>) -> Result<()> {
    let p = paragraph(doc, at)?;
    if range.start > range.end || range.end > p.text.chars().count() {
        bail!("입력 서식 범위가 문단을 벗어났습니다.");
    }
    if let Some(cell) = at.cell {
        let root = &doc.sections[at.section].paragraphs[at.paragraph];
        if let Some(Control::Table(table)) = root.controls.get(cell.control)
            && (table.common.locked || table.cells[cell.index].cell_protect())
        {
            bail!("보호된 셀은 고칠 수 없습니다.");
        }
    }
    Ok(())
}

fn boundary(doc: &Document, at: Cursor) -> Result<Boundary> {
    let p = paragraph(doc, at)?;
    let len = p.text.chars().count();
    if len == 0 {
        return Ok(Boundary::Empty);
    }
    if at.offset == len {
        return Ok(Boundary::Terminal);
    }
    if at.offset == 0
        && at.cell.is_none()
        && p.controls
            .first()
            .is_some_and(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
        && let Some(&first) = p.char_offsets.first()
        && first > 0
    {
        return Ok(Boundary::Leading(first));
    }
    Ok(Boundary::Ephemeral)
}

fn native_id(doc: &Document, at: Cursor) -> Result<u32> {
    let p = paragraph(doc, at)?;
    checked(doc, at, &(at.offset..at.offset))?;
    if p.text.is_empty() || matches!(boundary(doc, at)?, Boundary::Leading(_)) {
        return Ok(p.char_shapes.first().map_or(0, |r| r.char_shape_id));
    }
    Ok(p.char_shape_id_at(at.offset.saturating_sub(1)).unwrap_or(0))
}

fn resource_index<T: serde::Serialize>(items: &[T], wanted: &T) -> Result<Option<usize>> {
    let bytes = serde_json::to_vec(wanted)?;
    for (index, item) in items.iter().enumerate() {
        if serde_json::to_vec(item)? == bytes {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

impl NativeInsertionShape {
    fn capture(session: &Session, id: u32) -> Result<Self> {
        let mut style = Self::capture_document(session.core.document(), id)?;
        style.properties = session.char_shape_json(id);
        Ok(style)
    }
    fn capture_document(document: &Document, id: u32) -> Result<Self> {
        let info = &document.doc_info;
        let shape = info
            .char_shapes
            .get(id as usize)
            .cloned()
            .ok_or_else(|| anyhow!("입력 글자 모양이 없습니다: {id}"))?;
        let fonts = std::array::from_fn(|language| {
            info.font_faces
                .get(language)
                .and_then(|faces| faces.get(usize::from(shape.font_ids[language])))
                .cloned()
        });
        for (language, font) in fonts.iter().enumerate() {
            if font.is_none()
                && (shape.font_ids[language] != 0
                    || info.font_faces.get(language).is_some_and(|f| !f.is_empty()))
            {
                bail!("언어 {language}의 글꼴 참조가 잘못되었습니다.");
            }
        }
        let border = shape
            .border_fill_id
            .checked_sub(1)
            .and_then(|id| info.border_fills.get(usize::from(id)))
            .cloned();
        if shape.border_fill_id != 0 && border.is_none() {
            bail!("글자 테두리 참조가 잘못되었습니다.");
        }
        let properties = Value::Null;
        Ok(Self {
            shape,
            fonts,
            border,
            properties,
        })
    }

    /// Registration happens only in a detached document being committed.
    pub(crate) fn register(&self, doc: &mut Document) -> Result<u32> {
        self.register_info(&mut doc.doc_info)
    }
    fn register_info(&self, info: &mut DocInfo) -> Result<u32> {
        let mut shape = self.shape.clone();
        for language in 0..7 {
            if let Some(font) = &self.fonts[language] {
                let faces = info
                    .font_faces
                    .get_mut(language)
                    .ok_or_else(|| anyhow!("언어별 글꼴 목록이 없습니다."))?;
                let existing = resource_index(faces, font)?;
                let index = match existing {
                    Some(index) => index,
                    None => {
                        u16::try_from(faces.len())
                            .map_err(|_| anyhow!("글꼴 ID 공간이 소진되었습니다."))?;
                        faces.push(font.clone());
                        faces.len() - 1
                    }
                };
                let index =
                    u16::try_from(index).map_err(|_| anyhow!("글꼴 ID 공간이 소진되었습니다."))?;
                if shape.font_ids[language] != index {
                    shape.raw_data = None;
                    shape.font_ids[language] = index;
                }
            }
        }
        if let Some(border) = &self.border {
            let existing = resource_index(&info.border_fills, border)?;
            let index = match existing {
                Some(index) => index,
                None => {
                    u16::try_from(info.border_fills.len() + 1)
                        .map_err(|_| anyhow!("테두리 ID 공간이 소진되었습니다."))?;
                    info.border_fills.push(border.clone());
                    info.border_fills.len() - 1
                }
            };
            let id = u16::try_from(index + 1)
                .map_err(|_| anyhow!("테두리 ID 공간이 소진되었습니다."))?;
            if shape.border_fill_id != id {
                shape.raw_data = None;
                shape.border_fill_id = id;
            }
        }
        let id = if let Some(id) = info.char_shapes.iter().position(|old| *old == shape) {
            id
        } else {
            u32::try_from(info.char_shapes.len())
                .map_err(|_| anyhow!("글자 모양 ID 공간이 소진되었습니다."))?;
            info.char_shapes.push(shape);
            info.char_shapes.len() - 1
        };
        info.raw_stream_dirty = true;
        u32::try_from(id).map_err(|_| anyhow!("글자 모양 ID 공간이 소진되었습니다."))
    }

    fn modified(&self, session: &Session, patch: &Value) -> Result<Self> {
        // Preview through the real native formatter on one detached character.
        // Fonts, borders and native attr bits are handled by existing code.
        let mut doc = session.core.document().clone();
        // Laying out the other sections cost about 0.25 s on a 380-page document.
        doc.sections.truncate(1);
        let id = self.register(&mut doc)?;
        let mut p = Paragraph::new_empty();
        p.insert_text_at(0, "x");
        p.set_single_char_shape(id);
        let first = doc
            .sections
            .first_mut()
            .ok_or_else(|| anyhow!("입력 서식 구역이 없습니다."))?;
        first.paragraphs = vec![p];
        first.raw_stream = None;
        let mut core = DocumentCore::new_empty();
        core.set_document(doc);
        let mut formatter = Session::with_core(core, None);
        let props =
            formatter.character_parameters_from(patch.clone(), Some(self.properties.clone()))?;
        formatter
            .core
            .apply_char_format_native(0, 0, 0, 1, &props.to_string())
            .map_err(err)?;
        validate_resources(formatter.core.document())?;
        let id = formatter.core.document().sections[0].paragraphs[0]
            .char_shape_id_at(0)
            .ok_or_else(|| anyhow!("고친 입력 글자 모양이 없습니다."))?;
        Self::capture(&formatter, id)
    }

    fn equivalent(&self, other: &Self) -> Result<bool> {
        let mut a = self.shape.clone();
        let mut b = other.shape.clone();
        a.font_ids = [0; 7];
        b.font_ids = [0; 7];
        a.border_fill_id = 0;
        b.border_fill_id = 0;
        Ok(a == b
            && serde_json::to_vec(&self.fonts)? == serde_json::to_vec(&other.fonts)?
            && serde_json::to_vec(&self.border)? == serde_json::to_vec(&other.border)?)
    }
}

pub(crate) fn validate_resources(doc: &Document) -> Result<()> {
    if doc
        .doc_info
        .font_faces
        .iter()
        .any(|faces| faces.len() > usize::from(u16::MAX) + 1)
        || doc.doc_info.border_fills.len() > usize::from(u16::MAX)
    {
        bail!("글꼴 또는 테두리 ID 공간이 소진되었습니다.");
    }
    Ok(())
}

impl InsertionShapeState {
    pub(crate) fn clear(&mut self) {
        self.pending = None;
        self.stamp = self.stamp.wrapping_add(1);
    }
    pub(crate) fn revision(&self) -> u64 {
        self.stamp
    }
    fn set(&mut self, at: Cursor, style: NativeInsertionShape) {
        self.pending = Some((at, style));
        self.stamp = self.stamp.wrapping_add(1);
    }
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn effective(&self, session: &Session, at: Cursor) -> Result<NativeInsertionShape> {
        if let Some((cursor, style)) = &self.pending
            && *cursor == at
        {
            return Ok(style.clone());
        }
        NativeInsertionShape::capture(session, native_id(session.core.document(), at)?)
    }

    pub(crate) fn properties(&self, session: &Session) -> Result<Value> {
        Ok(self.effective(session, session.cursor)?.properties)
    }

    /// Plan first, so root can choose an undo transaction only for a real edit.
    pub(crate) fn prepare(&self, session: &Session, patch: Value) -> Result<PreparedFormat> {
        validate_format(&patch, true)?;
        session.guard_protected()?;
        let parts = session.selected_paragraphs()?;
        for (at, range) in &parts {
            checked(session.core.document(), *at, range)?;
        }
        let collapsed = if session.has_selection() {
            None
        } else {
            let at = session.cursor;
            Some((
                at,
                boundary(session.core.document(), at)?,
                self.effective(session, at)?.modified(session, &patch)?,
            ))
        };
        let durable_write = match &collapsed {
            Some((at, kind, style)) if *kind != Boundary::Ephemeral => {
                let p = paragraph(session.core.document(), *at)?;
                let existing = match kind {
                    Boundary::Terminal => {
                        let position = (p.char_count & 0x7fff_ffff).saturating_sub(1);
                        p.char_shapes
                            .iter()
                            .rev()
                            .find(|r| r.start_pos <= position)
                            .map_or(0, |r| r.char_shape_id)
                    }
                    _ => p.char_shapes.first().map_or(0, |r| r.char_shape_id),
                };
                !NativeInsertionShape::capture(session, existing)?.equivalent(style)?
            }
            _ => false,
        };
        Ok(PreparedFormat {
            parts,
            patch,
            collapsed,
            durable_write,
            layout_deferred: session.layout_deferred(),
        })
    }

    pub(crate) fn capture_insert(&self, session: &Session) -> Result<InsertTicket> {
        Ok(InsertTicket {
            style: self.effective(session, session.cursor)?,
            apply: self.has_pending() || !paragraph(session.core.document(),session.cursor)?.field_ranges.iter().any(|r| {
                (r.start_char_idx==session.cursor.offset || r.end_char_idx==session.cursor.offset)
                    && matches!(paragraph(session.core.document(),session.cursor).ok().and_then(|p|p.controls.get(r.control_idx)),Some(Control::Field(f)) if f.field_type==hwpcraft_doc::model::control::FieldType::Hyperlink)
            }),
        })
    }

    pub(crate) fn capture_split(&self, session: &Session) -> Result<SplitTicket> {
        let at = session.cursor;
        let end = paragraph(session.core.document(), at)?.text.chars().count();
        Ok(SplitTicket {
            carry: (at.offset == end)
                .then(|| self.effective(session, at))
                .transpose()?,
        })
    }

    /// Explicit SetPos/selection resets pending even when the location is equal.
    pub(crate) fn explicit_selection(&mut self) {
        self.clear();
    }

    pub(crate) fn motion(&mut self, from: Cursor, to: Cursor, motion: Motion, extend: bool) {
        if extend || from != to {
            self.clear();
            return;
        }
        // Verified no-op movements. End-right explicitly re-resolves left shape.
        if !matches!(motion, Motion::Up | Motion::Left) {
            self.clear();
        }
    }

    /// Undo/redo of a real edit resolves native shape from restored DOM.
    /// Empty history is a no-op; checkpoint/failure rollback restores a clone.
    pub(crate) fn history(&mut self, target_existed: bool) {
        if target_existed {
            self.clear();
        }
    }
}

impl PreparedFormat {
    pub(crate) fn document_edit(&self) -> bool {
        self.collapsed.is_none() || self.durable_write
    }

    /// Both forms edit the live core inside the caller's undo transaction, whose
    /// snapshot restores a paragraph that fails part-way.
    pub(crate) fn apply(
        self,
        core: &mut DocumentCore,
        state: &mut InsertionShapeState,
    ) -> Result<()> {
        if let Some((at, kind, style)) = self.collapsed {
            checked(core.document(), at, &(at.offset..at.offset))?;
            if self.durable_write {
                write_boundary(core, at, kind, &style)?;
            }
            state.set(at, style);
            return Ok(());
        }
        // A staged copy cost two full re-layouts (copy in, copy back), about 0.5 s on
        // a 380-page document whatever the selection size. Pagination runs once at
        // the end instead of after every paragraph.
        let mut formatter =
            Session::with_core(std::mem::replace(core, DocumentCore::new_empty()), None);
        let result = if self.layout_deferred {
            format_selection(&mut formatter, self.parts, &self.patch)
        } else {
            formatter.deferred_layout(|f| format_selection(f, self.parts, &self.patch))
        };
        *core = formatter.core;
        result?;
        state.clear();
        Ok(())
    }
}

fn format_selection(
    formatter: &mut Session,
    parts: Vec<(Cursor, Range<usize>)>,
    patch: &Value,
) -> Result<()> {
    for (at, range) in parts {
        checked(formatter.core.document(), at, &range)?;
        formatter.cursor = at;
        let id = paragraph(formatter.core.document(), at)?
            .char_shape_id_at(range.start)
            .unwrap_or(0);
        let current = formatter.char_shape_json(id);
        let props = formatter
            .character_parameters_from(patch.clone(), Some(current))?
            .to_string();
        if let Some(cell) = at.cell {
            formatter.core.apply_char_format_in_cell_native(
                at.section,
                at.paragraph,
                cell.control,
                cell.index,
                cell.paragraph,
                range.start,
                range.end,
                &props,
            )
        } else {
            formatter.core.apply_char_format_native(
                at.section,
                at.paragraph,
                range.start,
                range.end,
                &props,
            )
        }
        .map_err(err)?;
        if range.start == 0 {
            align_selected_leading(&mut formatter.core, at)?;
        }
        validate_resources(formatter.core.document())?;
    }
    Ok(())
}

/// Register `style` and point the boundary run at it in the live document, then lay
/// out that paragraph again. A copied document plus `set_document` re-laid out the
/// whole document (about 0.25 s per Enter at a paragraph end on 380 pages).
fn write_boundary(
    core: &mut DocumentCore,
    at: Cursor,
    kind: Boundary,
    style: &NativeInsertionShape,
) -> Result<()> {
    let id = register_live(core, style)?;
    apply_boundary(core.document_mut(), at, kind, id)?;
    core.relayout_paragraph_native(at.section, at.paragraph, at.cell.map(|c| c.control))
        .map_err(err)?;
    Ok(())
}

/// Resources go in as one doc_info swap, so a failed registration changes nothing.
fn register_live(core: &mut DocumentCore, style: &NativeInsertionShape) -> Result<u32> {
    let mut info = core.document().doc_info.clone();
    let id = style.register_info(&mut info)?;
    core.document_mut().doc_info = info;
    Ok(id)
}

/// Native selection formatting starting at the first text character also sets
/// the preceding section/column definition run. RHWP formats only text units.
/// Only a section's first paragraph has such a run; others are left untouched,
/// so RHWP's layout caches stay trusted for undo.
pub(crate) fn align_selected_leading(core: &mut DocumentCore, mut at: Cursor) -> Result<()> {
    at.offset = 0;
    let Boundary::Leading(first) = boundary(core.document(), at)? else {
        return Ok(());
    };
    let id = paragraph(core.document(), at)?
        .char_shape_id_at(0)
        .unwrap_or(0);
    apply_boundary(core.document_mut(), at, Boundary::Leading(first), id)?;
    // Tells RHWP only this paragraph changed, so undo keeps the other sections.
    core.relayout_paragraph_native(at.section, at.paragraph, at.cell.map(|c| c.control))
        .map_err(err)?;
    Ok(())
}

fn apply_boundary(doc: &mut Document, at: Cursor, kind: Boundary, id: u32) -> Result<()> {
    let p = paragraph_mut(doc, at)?;
    match kind {
        Boundary::Empty => p.set_single_char_shape(id),
        Boundary::Terminal => {
            let position = (p.char_count & 0x7fff_ffff).saturating_sub(1);
            p.char_shapes.retain(|r| r.start_pos != position);
            p.char_shapes.push(CharShapeRef {
                start_pos: position,
                char_shape_id: id,
            });
            p.char_shapes.sort_by_key(|r| r.start_pos);
        }
        Boundary::Leading(first) => {
            let text_id = p.char_shape_id_at(0).unwrap_or(0);
            p.char_shapes.retain(|r| r.start_pos >= first);
            if !p.char_shapes.iter().any(|r| r.start_pos == first) {
                p.char_shapes.insert(
                    0,
                    CharShapeRef {
                        start_pos: first,
                        char_shape_id: text_id,
                    },
                );
            }
            p.char_shapes.insert(
                0,
                CharShapeRef {
                    start_pos: 0,
                    char_shape_id: id,
                },
            );
        }
        Boundary::Ephemeral => {}
    }
    doc.sections[at.section].raw_stream = None;
    Ok(())
}

impl InsertTicket {
    pub(crate) fn multiline(mut self) -> Self {
        self.apply = true;
        self
    }
    /// Apply to NEW text only. Caller inserts first and wraps the full operation
    /// in one transaction, reusing this ticket across multiline internal splits.
    pub(crate) fn apply_range(
        &self,
        core: &mut DocumentCore,
        at: Cursor,
        range: Range<usize>,
    ) -> Result<()> {
        checked(core.document(), at, &range)?;
        if range.is_empty() || !self.apply {
            return Ok(());
        }
        // Most input already inherits the exact desired style. Avoid cloning
        // and rebuilding the whole document in that common case.
        let p = paragraph(core.document(), at)?;
        let start = *p
            .char_offsets
            .get(range.start)
            .ok_or_else(|| anyhow!("입력 문자 좌표가 없습니다."))?;
        let end = p
            .char_offsets
            .get(range.end)
            .copied()
            .unwrap_or_else(|| (p.char_count & 0x7fff_ffff).saturating_sub(1));
        let mut ids =
            std::collections::BTreeSet::from([p.char_shape_id_at(range.start).unwrap_or(0)]);
        ids.extend(
            p.char_shapes
                .iter()
                .filter(|r| r.start_pos >= start && r.start_pos < end)
                .map(|r| r.char_shape_id),
        );
        let mut exact = true;
        for id in ids {
            let view = NativeInsertionShape::capture_document(core.document(), id)?;
            if !self.style.equivalent(&view)? {
                exact = false;
                break;
            }
        }
        if exact {
            return Ok(());
        }
        let id = register_live(core, &self.style)?;
        if let Some(cell) = at.cell {
            core.set_char_shape_id_in_cell_native(
                at.section,
                at.paragraph,
                cell.control,
                cell.index,
                cell.paragraph,
                range.start,
                range.end,
                id,
            )
        } else {
            core.set_char_shape_id_native(at.section, at.paragraph, range.start, range.end, id)
        }
        .map_err(err)?;
        Ok(())
    }

    pub(crate) fn finish(&self, state: &mut InsertionShapeState, caret: Cursor) {
        let _ = caret;
        // The inserted left character now carries the effective native style.
        state.clear();
    }
}

impl SplitTicket {
    pub(crate) fn finish(
        self,
        core: &mut DocumentCore,
        state: &mut InsertionShapeState,
        caret: Cursor,
    ) -> Result<()> {
        if let Some(style) = self.carry {
            if !paragraph(core.document(), caret)?.text.is_empty() {
                bail!("문단 끝 나누기의 대상 문단이 비어 있지 않습니다.");
            }
            checked(core.document(), caret, &(0..0))?;
            write_boundary(core, caret, Boundary::Empty, &style)?;
            state.set(caret, style);
        } else {
            state.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(offset: usize) -> Cursor {
        Cursor {
            offset,
            ..Default::default()
        }
    }
    fn document(text: &str) -> Result<Session> {
        let mut s = Session::new()?;
        s.execute(Command::Insert(text.into()))?;
        Ok(s)
    }
    fn bold(s: &Session, offset: usize) -> Result<bool> {
        let id = paragraph(s.core.document(), at(offset))?
            .char_shape_id_at(offset)
            .unwrap_or(0);
        Ok(s.core.document().doc_info.char_shapes[id as usize].bold)
    }
    fn format(s: &mut Session, state: &mut InsertionShapeState, patch: Value) -> Result<bool> {
        let p = state.prepare(s, patch)?;
        let edits = p.document_edit();
        p.apply(&mut s.core, state)?;
        Ok(edits)
    }
    fn insert(s: &mut Session, state: &mut InsertionShapeState, text: &str) -> Result<()> {
        let ticket = state.capture_insert(s)?;
        let c = s.cursor;
        s.core
            .insert_text_native(c.section, c.paragraph, c.offset, text)
            .map_err(err)?;
        ticket.apply_range(&mut s.core, c, c.offset..c.offset + text.chars().count())?;
        s.cursor.offset += text.chars().count();
        ticket.finish(state, s.cursor);
        Ok(())
    }

    #[test]
    fn middle_override_is_pure_and_only_new_unicode_text_changes() -> Result<()> {
        let mut s = document("ab😀cd")?;
        s.cursor = at(2);
        let mut state = InsertionShapeState::default();
        let before = serde_json::to_vec(&s.core.document().doc_info.char_shapes)?;
        assert!(!format(
            &mut s,
            &mut state,
            json!({"bold":true,"fontSize":1600})
        )?);
        assert_eq!(
            serde_json::to_vec(&s.core.document().doc_info.char_shapes)?,
            before
        );
        assert!(!bold(&s, 1)?);
        assert!(!bold(&s, 2)?);
        insert(&mut s, &mut state, "X🦀")?;
        assert_eq!(s.text(), "abX🦀😀cd");
        assert!(!bold(&s, 1)?);
        assert!(bold(&s, 2)?);
        assert!(bold(&s, 3)?);
        assert!(!bold(&s, 4)?);
        Ok(())
    }

    #[test]
    fn mixed_boundary_queries_left_and_selection_preserves_other_attributes() -> Result<()> {
        let mut s = document("abcdef")?;
        let mut state = InsertionShapeState::default();
        s.select(at(0), false)?;
        s.select(at(3), true)?;
        assert!(format(&mut s, &mut state, json!({"bold":true}))?);
        s.select(at(3), false)?;
        assert_eq!(state.properties(&s)?["bold"], true);
        s.select(at(1), false)?;
        s.select(at(5), true)?;
        format(&mut s, &mut state, json!({"italic":true}))?;
        assert!(bold(&s, 2)?);
        assert!(!bold(&s, 3)?);
        assert!(!bold(&s, 5)?);
        Ok(())
    }

    #[test]
    fn empty_terminal_and_leading_shapes_do_not_reformat_existing_text() -> Result<()> {
        for text in ["", "abc"] {
            let mut s = document(text)?;
            let mut state = InsertionShapeState::default();
            s.cursor = at(text.len());
            assert!(format(&mut s, &mut state, json!({"bold":true}))?);
            if !text.is_empty() {
                assert!(!bold(&s, 2)?);
            }
            let p = paragraph(s.core.document(), s.cursor)?;
            let terminal = (p.char_count & 0x7fff_ffff).saturating_sub(1);
            assert!(p.char_shapes.iter().any(|r| r.start_pos <= terminal
                && s.core.document().doc_info.char_shapes[r.char_shape_id as usize].bold));
            insert(&mut s, &mut state, "X")?;
            assert!(bold(&s, text.len())?);
        }
        let mut s = document("abc")?;
        s.cursor = at(0);
        let mut state = InsertionShapeState::default();
        let first = paragraph(s.core.document(), s.cursor)?.char_offsets[0];
        assert!(first > 0);
        assert!(format(&mut s, &mut state, json!({"bold":true}))?);
        assert!(!bold(&s, 0)?);
        insert(&mut s, &mut state, "X")?;
        assert!(bold(&s, 0)?);
        assert!(!bold(&s, 1)?);
        Ok(())
    }

    #[test]
    fn font_registration_is_deferred_until_real_input() -> Result<()> {
        let mut s = document("abcd")?;
        s.cursor = at(2);
        let mut state = InsertionShapeState::default();
        let before = serde_json::to_vec(&s.core.document().doc_info.font_faces)?;
        format(
            &mut s,
            &mut state,
            json!({"fontFamily":"HwpCraft Insertion Test Font"}),
        )?;
        assert_eq!(
            serde_json::to_vec(&s.core.document().doc_info.font_faces)?,
            before
        );
        assert_eq!(
            state.properties(&s)?["fontFamily"],
            "HwpCraft Insertion Test Font"
        );
        insert(&mut s, &mut state, "X")?;
        assert!(s.core.document().doc_info.font_faces.iter().all(|faces| {
            faces
                .iter()
                .any(|f| f.name == "HwpCraft Insertion Test Font")
        }));
        Ok(())
    }

    #[test]
    fn movements_history_and_cloned_rollback_have_distinct_rules() -> Result<()> {
        let mut s = document("abcd")?;
        s.cursor = at(2);
        let mut state = InsertionShapeState::default();
        format(&mut s, &mut state, json!({"bold":true}))?;
        let checkpoint = state.clone();
        state.motion(at(2), at(2), Motion::Up, false);
        assert_eq!(state.properties(&s)?["bold"], true);
        state.history(false);
        assert_eq!(state.properties(&s)?["bold"], true);
        state.explicit_selection();
        assert_eq!(state.properties(&s)?["bold"], false);
        state = checkpoint.clone();
        state.motion(at(2), at(3), Motion::Right, false);
        assert!(state.pending.is_none());
        state = checkpoint.clone();
        state.motion(at(2), at(2), Motion::Right, false);
        assert!(state.pending.is_none());
        state = checkpoint;
        state.history(true);
        assert_eq!(state.properties(&s)?["bold"], false);
        Ok(())
    }

    #[test]
    fn invalid_format_does_not_change_pending_or_document() -> Result<()> {
        let mut s = document("abcd")?;
        s.cursor = at(2);
        let mut state = InsertionShapeState::default();
        format(&mut s, &mut state, json!({"bold":true}))?;
        let before = state.properties(&s)?;
        let doc = serde_json::to_vec(&s.core.document().sections[0].paragraphs)?;
        assert!(state.prepare(&s, json!({"fontSize":-1})).is_err());
        assert_eq!(state.properties(&s)?, before);
        assert_eq!(
            serde_json::to_vec(&s.core.document().sections[0].paragraphs)?,
            doc
        );
        Ok(())
    }

    #[test]
    fn standalone_split_resets_middle_but_multiline_ticket_keeps_shape() -> Result<()> {
        for multiline in [false, true] {
            let mut s = document("abcd")?;
            s.cursor = at(2);
            let mut state = InsertionShapeState::default();
            format(&mut s, &mut state, json!({"bold":true}))?;
            let input = state.capture_insert(&s)?;
            let split = state.capture_split(&s)?;
            s.core
                .split_paragraph_native(0, 0, s.split_offset(s.cursor)?, None)
                .map_err(err)?;
            s.cursor = Cursor {
                paragraph: 1,
                ..Default::default()
            };
            split.finish(&mut s.core, &mut state, s.cursor)?;
            s.core.insert_text_native(0, 1, 0, "X").map_err(err)?;
            if multiline {
                input.apply_range(&mut s.core, s.cursor, 0..1)?;
            }
            let p = paragraph(s.core.document(), s.cursor)?;
            let id = p.char_shape_id_at(0).unwrap_or(0);
            assert_eq!(
                s.core.document().doc_info.char_shapes[id as usize].bold,
                multiline
            );
        }
        let mut s = document("abc")?;
        let mut state = InsertionShapeState::default();
        format(&mut s, &mut state, json!({"bold":true}))?;
        let split = state.capture_split(&s)?;
        s.core
            .split_paragraph_native(0, 0, s.split_offset(s.cursor)?, None)
            .map_err(err)?;
        s.cursor = Cursor {
            paragraph: 1,
            ..Default::default()
        };
        split.finish(&mut s.core, &mut state, s.cursor)?;
        assert_eq!(state.properties(&s)?["bold"], true);
        Ok(())
    }

    #[test]
    fn durable_boundary_runs_survive_both_formats_without_reformatting_text() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for text in ["", "abc"] {
            let mut s = document(text)?;
            let mut state = InsertionShapeState::default();
            format(&mut s, &mut state, json!({"bold":true,"fontSize":1600}))?;
            for extension in ["hwp", "hwpx"] {
                let path = dir.path().join(format!("{}.{extension}", text.len()));
                hwpcraft_formats::save(&s.core, &path)?;
                let loaded = hwpcraft_formats::open(&path)?;
                let p = &loaded.document().sections[0].paragraphs[0];
                assert_eq!(p.text, text);
                assert!(
                    p.char_shapes.iter().any(|r| {
                        let style =
                            &loaded.document().doc_info.char_shapes[r.char_shape_id as usize];
                        style.bold && style.base_size == 1600
                    }),
                    "{extension}: durable empty/terminal shape was lost"
                );
                if !text.is_empty() {
                    let id = p.char_shape_id_at(0).unwrap_or(0);
                    assert!(!loaded.document().doc_info.char_shapes[id as usize].bold);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn native_snapshot_rollback_restores_pending_but_user_undo_resolves_dom() -> Result<()> {
        let mut s = document("abcd")?;
        s.cursor = at(2);
        let mut state = InsertionShapeState::default();
        format(&mut s, &mut state, json!({"bold":true}))?;
        let checkpoint = state.clone();
        let before = s.core.save_snapshot_native();
        insert(&mut s, &mut state, "X")?;
        let after = s.core.save_snapshot_native();
        s.core.restore_snapshot_native(before).map_err(err)?;
        s.cursor = at(2);
        state = checkpoint.clone();
        assert_eq!(state.properties(&s)?["bold"], true);
        state.history(true);
        assert_eq!(state.properties(&s)?["bold"], false);
        s.core.restore_snapshot_native(after).map_err(err)?;
        s.cursor = at(3);
        state.history(true);
        assert_eq!(state.properties(&s)?["bold"], true);
        s.core.discard_snapshot_native(before);
        s.core.discard_snapshot_native(after);
        Ok(())
    }

    #[test]
    fn actual_session_typing_inherits_left_shape_at_both_mixed_boundaries() -> Result<()> {
        for bold_first in [false, true] {
            let mut s = document("abcdef")?;
            s.select(at(if bold_first { 0 } else { 3 }), false)?;
            s.select(at(if bold_first { 3 } else { 6 }), true)?;
            s.execute(Command::Character(json!({"bold":true})))?;
            s.select(at(3), false)?;
            s.execute(Command::Insert("X".into()))?;
            assert_eq!(s.text(), "abcXdef");
            assert_eq!(bold(&s, 3)?, bold_first);
            assert_eq!(bold(&s, 4)?, !bold_first);
            s.execute(Command::Insert("Y".into()))?;
            assert_eq!(bold(&s, 4)?, bold_first);
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "abcdef");
        }
        Ok(())
    }

    #[test]
    fn integrated_pending_input_history_and_no_fake_generation() -> Result<()> {
        let mut s = document("abcd")?;
        s.select(at(2), false)?;
        let generation = s.generation;
        s.execute_hwp_action(
            "CharShape",
            json!({}),
            Some(json!({"Bold":1,"Height":1600})),
        )?;
        assert_eq!(s.generation, generation);
        assert!(!bold(&s, 1)?);
        assert!(!bold(&s, 2)?);
        s.execute(Command::Insert("X".into()))?;
        s.execute(Command::Insert("Y".into()))?;
        assert_eq!(s.text(), "abXYcd");
        assert!(bold(&s, 2)?);
        assert!(!bold(&s, 4)?);
        s.execute(Command::Undo)?;
        assert_eq!(s.text(), "abcd");
        assert_eq!(s.character_properties()?["bold"], false);
        s.execute(Command::Redo)?;
        assert_eq!(s.text(), "abXYcd");
        assert_eq!(s.character_properties()?["bold"], true);
        s.select(at(2), false)?;
        s.execute(Command::Character(json!({"italic":true})))?;
        let checkpoint = s.checkpoint();
        let pending = s.character_properties()?;
        s.execute(Command::Insert("z".into()))?;
        s.rollback(checkpoint)?;
        assert_eq!(s.character_properties()?, pending);
        assert_eq!(s.text(), "abXYcd");
        Ok(())
    }

    #[test]
    fn integrated_enter_and_multiline_text_follow_different_native_rules() -> Result<()> {
        for multiline in [false, true] {
            let mut s = document("abcd")?;
            s.select(at(2), false)?;
            s.execute(Command::Character(json!({"bold":true})))?;
            if multiline {
                s.execute(Command::Insert("X\nY".into()))?;
            } else {
                s.execute(Command::Split)?;
                s.execute(Command::Insert("Y".into()))?;
            }
            let p = &s.core.document().sections[0].paragraphs[1];
            let id = p.char_shape_id_at(0).unwrap_or(0);
            assert_eq!(
                s.core.document().doc_info.char_shapes[id as usize].bold,
                multiline
            );
            assert_eq!(p.text, "Ycd");
        }
        Ok(())
    }

    #[test]
    fn zero_missing_language_fonts_are_defaults_but_nonzero_refs_are_errors() -> Result<()> {
        let mut s = document("abcd")?;
        s.select(at(2), false)?;
        for faces in &mut s.core.document_mut().doc_info.font_faces {
            faces.clear();
        }
        for shape in &mut s.core.document_mut().doc_info.char_shapes {
            shape.font_ids = [0; 7];
        }
        let state = InsertionShapeState::default();
        assert_eq!(state.properties(&s)?["bold"], false);
        assert!(state.prepare(&s, json!({"bold":true})).is_ok());
        let id = paragraph(s.core.document(), s.cursor)?
            .char_shape_id_at(1)
            .unwrap_or(0);
        s.core.document_mut().doc_info.char_shapes[id as usize].font_ids[3] = 1;
        assert!(state.properties(&s).is_err());
        s.core.document_mut().doc_info.char_shapes[id as usize].font_ids[3] = 0;
        s.core.document_mut().doc_info.char_shapes[id as usize].border_fill_id = u16::MAX;
        assert!(state.properties(&s).is_err());
        Ok(())
    }

    #[test]
    fn full_border_table_cannot_wrap_a_new_border_to_zero() -> Result<()> {
        let mut s = document("abcd")?;
        s.select(at(2), false)?;
        let fill = s.core.document().doc_info.border_fills[0].clone();
        s.core
            .document_mut()
            .doc_info
            .border_fills
            .resize(usize::from(u16::MAX), fill);
        let before = s.generation;
        let count = s.core.document().doc_info.char_shapes.len();
        assert!(
            s.execute(Command::Character(
                json!({"borderLeft":{"type":1,"width":7,"color":"#123abc"}})
            ))
            .is_err()
        );
        assert_eq!(s.generation, before);
        assert_eq!(
            s.core.document().doc_info.border_fills.len(),
            usize::from(u16::MAX)
        );
        assert_eq!(s.core.document().doc_info.char_shapes.len(), count);
        assert_eq!(s.text(), "abcd");
        Ok(())
    }

    #[test]
    fn pending_only_checkpoint_rollback_does_not_invalidate_document_generation() -> Result<()> {
        let mut s = document("abcd")?;
        s.select(at(2), false)?;
        let generation = s.generation;
        let checkpoint = s.checkpoint();
        s.execute(Command::Character(json!({"bold":true})))?;
        s.rollback(checkpoint)?;
        assert_eq!(s.generation, generation);
        assert_eq!(s.character_properties()?["bold"], false);
        Ok(())
    }

    #[test]
    fn a_new_font_at_u16_capacity_fails_before_any_live_resource_edit() -> Result<()> {
        let mut s = document("abcd")?;
        s.select(at(2), false)?;
        let font = s.core.document().doc_info.font_faces[0][0].clone();
        s.core.document_mut().doc_info.font_faces[0].resize(usize::from(u16::MAX) + 1, font);
        let counts = s
            .core
            .document()
            .doc_info
            .font_faces
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>();
        let generation = s.generation;
        assert!(
            s.execute(Command::Character(
                json!({"fontFamily":"capacity-only-new-font"})
            ))
            .is_err()
        );
        assert_eq!(s.generation, generation);
        assert_eq!(
            s.core
                .document()
                .doc_info
                .font_faces
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            counts
        );
        assert_eq!(s.text(), "abcd");
        Ok(())
    }

    #[test]
    fn char_shape_queries_accept_native_u32_ids_and_noop_remapped_fonts_are_not_edits() -> Result<()>
    {
        let mut s = document("abcd")?;
        s.select(at(2), false)?;
        let shape = s.core.document().doc_info.char_shapes[0].clone();
        s.core
            .document_mut()
            .doc_info
            .char_shapes
            .resize(usize::from(u16::MAX) + 2, shape);
        s.core.document_mut().sections[0].paragraphs[0]
            .set_single_char_shape(u32::from(u16::MAX) + 1);
        assert_eq!(s.character_properties()?["bold"], false);
        // Use an ordinary document for the detached-resource equivalence path.
        let mut s = document("abc")?;
        s.execute(Command::Character(
            json!({"fontFamily":"new-shared-language-font"}),
        ))?;
        let generation = s.generation;
        s.execute(Command::Character(
            json!({"fontFamily":"new-shared-language-font"}),
        ))?;
        assert_eq!(s.generation, generation);
        Ok(())
    }

    #[test]
    fn shipped_self_created_sample_has_valid_native_font_resources() -> Result<()> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/HwpCraft-시작하기.hwp");
        let mut s = Session::new()?;
        s.open(&path)?;
        for (cursor, _) in s.entries() {
            let p = paragraph(s.core.document(), cursor)?;
            for shape in &p.char_shapes {
                NativeInsertionShape::capture(&s, shape.char_shape_id)?;
            }
        }
        Ok(())
    }

    fn special_fixture(offset: usize) -> Result<Session> {
        let mut s = document("abCDxy")?;
        s.select(at(2), false)?;
        s.select(at(4), true)?;
        s.execute(Command::Character(
            json!({"bold":true,"fontSize":1600,"textColor":"#0000ff"}),
        ))?;
        s.select(at(offset), false)?;
        s.execute(Command::Character(
            json!({"bold":false,"italic":true,"fontSize":1800,"textColor":"#ff0000"}),
        ))?;
        Ok(s)
    }
    fn shape_at(s: &Session, offset: usize) -> &CharShape {
        let p = &s.core.document().sections[0].paragraphs[0];
        &s.core.document().doc_info.char_shapes[p.char_shape_id_at(offset).unwrap_or(0) as usize]
    }
    fn check_special_neighbors(s: &Session, special: char, follow: bool) {
        let added = if follow { 2 } else { 1 };
        assert_eq!(s.core.document().sections[0].paragraphs.len(), 1);
        assert_eq!(
            s.text(),
            format!("ab{special}{}CDxy", if follow { "X" } else { "" })
        );
        assert!(!shape_at(s, 1).italic);
        assert_eq!(shape_at(s, 1).base_size, 1000);
        for offset in 2..2 + added {
            assert!(shape_at(s, offset).italic);
            assert!(!shape_at(s, offset).bold);
            assert_eq!(shape_at(s, offset).base_size, 1800);
        }
        assert!(shape_at(s, 2 + added).bold);
        assert_eq!(shape_at(s, 2 + added).base_size, 1600);
        assert!(!shape_at(s, 4 + added).bold);
        assert!(!shape_at(s, 4 + added).italic);
    }

    #[test]
    fn pending_tab_and_inline_break_preserve_neighbors_and_snapshot_redo_styles() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for (command, ch) in [("insert.tab", '\t'), ("insert.line_break", '\n')] {
            let mut s = special_fixture(2)?;
            s.run(command, json!({}))?;
            check_special_neighbors(&s, ch, false);
            s.execute(Command::Insert("X".into()))?;
            check_special_neighbors(&s, ch, true);
            s.execute(Command::Undo)?;
            check_special_neighbors(&s, ch, false);
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "abCDxy");
            assert!(shape_at(&s, 2).bold);
            assert!(!shape_at(&s, 1).italic);
            // Native public-action redo has a style-loss quirk for these
            // controls. HwpCraft deliberately retains its exact snapshots.
            s.execute(Command::Redo)?;
            check_special_neighbors(&s, ch, false);
            s.execute(Command::Redo)?;
            check_special_neighbors(&s, ch, true);
            for extension in ["hwp", "hwpx"] {
                let path = dir.path().join(format!("{command}.{extension}"));
                s.save(&path)?;
                let mut reopened = Session::new()?;
                reopened.open(&path)?;
                check_special_neighbors(&reopened, ch, true);
            }
        }
        Ok(())
    }

    #[test]
    fn delete_directions_resolve_surviving_left_shape_without_reformatting_neighbors() -> Result<()>
    {
        for (offset, back, remaining, expected_bold) in [
            (2, false, "abDxy", false),
            (2, true, "aCDxy", false),
            (3, false, "abCxy", true),
            (3, true, "abDxy", false),
        ] {
            let mut s = special_fixture(offset)?;
            s.execute(if back {
                Command::Backspace
            } else {
                Command::Delete
            })?;
            assert_eq!(s.text(), remaining);
            assert_eq!(s.character_properties()?["italic"], false);
            assert_eq!(s.character_properties()?["bold"], expected_bold);
            s.execute(Command::Undo)?;
            assert_eq!(s.text(), "abCDxy");
            assert!(shape_at(&s, 2).bold);
            assert!(shape_at(&s, 3).bold);
            assert!(!shape_at(&s, 1).italic);
            s.execute(Command::Redo)?;
            assert_eq!(s.text(), remaining);
            let caret = s.cursor.offset;
            s.execute(Command::Insert("X".into()))?;
            assert_eq!(shape_at(&s, caret).bold, expected_bold);
            assert!(!shape_at(&s, caret).italic);
            let surviving_bold = if !back && offset == 3 {
                caret - 1
            } else {
                caret + 1
            };
            assert!(shape_at(&s, surviving_bold).bold);
        }
        Ok(())
    }

    #[test]
    fn pending_end_delete_and_special_backspace_recover_native_left_context() -> Result<()> {
        // Native Delete at the document end resets pending insertion style
        // even though it removes no character. The terminal style stays stored.
        let mut s = special_fixture(6)?;
        s.execute(Command::Delete)?;
        assert_eq!(s.text(), "abCDxy");
        assert_eq!(s.character_properties()?["italic"], false);
        for (command, ch) in [("insert.tab", '\t'), ("insert.line_break", '\n')] {
            let mut s = special_fixture(2)?;
            s.run(command, json!({}))?;
            check_special_neighbors(&s, ch, false);
            s.execute(Command::Backspace)?;
            assert_eq!(s.text(), "abCDxy");
            assert_eq!(s.character_properties()?["italic"], false);
            assert!(shape_at(&s, 2).bold);
            assert!(!shape_at(&s, 1).italic);
            s.execute(Command::Insert("X".into()))?;
            assert_eq!(s.text(), "abXCDxy");
            assert!(!shape_at(&s, 2).italic);
            assert!(shape_at(&s, 3).bold);
        }
        Ok(())
    }
}
