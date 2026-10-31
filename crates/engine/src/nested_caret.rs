//! A caret inside a nested table cell or a text box, which the body `Cursor` (one
//! table level) cannot hold. Text edits go through `context.v2`, so protection, undo
//! and generation checks are the same as for the region editor; geometry and up/down
//! movement come from RHWP's path API (0046).
use super::*;
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_doc::model::shape::ShapeObject;
use serde::Serialize;

/// Caret and selection anchor within one paragraph of a nested container.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NestedCaret {
    pub address: ContextAddress,
    pub offset: usize,
    /// Selection anchor in the same paragraph, or `None` without a selection.
    pub anchor: Option<usize>,
    /// Column kept across up/down moves, in page pixels.
    #[serde(skip)]
    preferred_x: Option<f64>,
}

impl NestedCaret {
    fn new(address: ContextAddress, offset: usize) -> Self {
        Self {
            address,
            offset,
            anchor: None,
            preferred_x: None,
        }
    }
    /// A caret at `offset` of the paragraph `address` names, without a selection.
    pub fn at(address: ContextAddress, offset: usize) -> Self {
        Self::new(address, offset)
    }
    /// The selected range, empty when there is no selection.
    pub fn selection(&self) -> std::ops::Range<usize> {
        let anchor = self.anchor.unwrap_or(self.offset);
        anchor.min(self.offset)..anchor.max(self.offset)
    }
}

/// RHWP path entries `(control, cell, cell paragraph)`; a text box is cell 0.
fn triples(address: &ContextAddress) -> Result<Vec<(usize, usize, usize)>> {
    address
        .path
        .iter()
        .map(|step| match step {
            ContextStep::TableCell {
                control,
                cell,
                paragraph,
            } => Ok((*control, *cell, *paragraph)),
            ContextStep::TextBox {
                control,
                shape_path,
                paragraph,
            } if shape_path.is_empty() => Ok((*control, 0, *paragraph)),
            _ => bail!("이 영역은 [편집 > 문서 영역 편집]에서 고칠 수 있습니다."),
        })
        .collect()
}

fn path_json(path: &[(usize, usize, usize)]) -> String {
    let entries: Vec<String> = path
        .iter()
        .map(|(control, cell, paragraph)| {
            format!(
                r#"{{"controlIndex":{control},"cellIndex":{cell},"cellParaIndex":{paragraph}}}"#
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// The paragraphs of the container a path step opens, given its host paragraph.
fn container(host: &Paragraph, control: usize, cell: usize) -> Option<(&[Paragraph], bool)> {
    match host.controls.get(control)? {
        Control::Table(table) => Some((&table.cells.get(cell)?.paragraphs, false)),
        Control::Shape(shape) if !matches!(**shape, ShapeObject::Group(_)) => {
            Some((&shape.drawing()?.text_box.as_ref()?.paragraphs, true))
        }
        _ => None,
    }
}

impl Session {
    pub fn nested_caret(&self) -> Option<&NestedCaret> {
        self.nested.as_ref()
    }
    pub fn clear_nested(&mut self) {
        self.nested = None;
    }

    /// Typed address for RHWP path entries: each step is a table cell or a text box,
    /// told apart by the control it names.
    fn nested_address(
        &self,
        section: usize,
        host: usize,
        path: &[(usize, usize, usize)],
    ) -> Result<ContextAddress> {
        let mut paragraph = self
            .core
            .document()
            .sections
            .get(section)
            .and_then(|s| s.paragraphs.get(host))
            .ok_or_else(|| anyhow!("문단을 찾지 못했습니다."))?;
        let mut steps = Vec::with_capacity(path.len());
        for &(control, cell, index) in path {
            let (paragraphs, text_box) = container(paragraph, control, cell)
                .ok_or_else(|| anyhow!("표 셀이나 글상자가 아닙니다."))?;
            paragraph = paragraphs
                .get(index)
                .ok_or_else(|| anyhow!("문단을 찾지 못했습니다."))?;
            steps.push(if text_box {
                ContextStep::TextBox {
                    control,
                    shape_path: vec![],
                    paragraph: index,
                }
            } else {
                ContextStep::TableCell {
                    control,
                    cell,
                    paragraph: index,
                }
            });
        }
        Ok(ContextAddress {
            version: 2,
            section,
            paragraph: host,
            path: steps,
        })
    }

    /// The paragraph an address names and the number of paragraphs in its container.
    fn nested_paragraph(&self, address: &ContextAddress) -> Result<(&Paragraph, usize)> {
        let path = triples(address)?;
        let mut paragraph = self
            .core
            .document()
            .sections
            .get(address.section)
            .and_then(|s| s.paragraphs.get(address.paragraph))
            .ok_or_else(|| anyhow!("문단을 찾지 못했습니다."))?;
        let mut count = 0;
        for (control, cell, index) in path {
            let (paragraphs, _) = container(paragraph, control, cell)
                .ok_or_else(|| anyhow!("표 셀이나 글상자가 아닙니다."))?;
            count = paragraphs.len();
            paragraph = paragraphs
                .get(index)
                .ok_or_else(|| anyhow!("문단을 찾지 못했습니다."))?;
        }
        Ok((paragraph, count))
    }
    fn nested_len(&self, address: &ContextAddress) -> Result<usize> {
        Ok(self.nested_paragraph(address)?.0.text.chars().count())
    }

    /// Where a click lands when it is inside a nested table cell or a text box:
    /// the places `hit_test` refuses.
    pub fn hit_nested(&self, page: u32, x: f64, y: f64) -> Result<Option<NestedCaret>> {
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(&core.hit_test_native(page, x, y).map_err(err)?)?;
        let Some(entries) = v["cellPath"].as_array() else {
            return Ok(None);
        };
        if entries.len() < 2 && v["isTextBox"] != true {
            return Ok(None);
        }
        let number = |value: &Value, key: &str| {
            value[key]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| anyhow!("RHWP 응답에 {key}가 없습니다: {value}"))
        };
        let path = entries
            .iter()
            .map(|e| {
                Ok((
                    number(e, "controlIndex")?,
                    number(e, "cellIndex")?,
                    number(e, "cellParaIndex")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let address = self.nested_address(
            number(&v, "sectionIndex")?,
            number(&v, "parentParaIndex")?,
            &path,
        )?;
        let offset = number(&v, "charOffset")?.min(self.nested_len(&address)?);
        Ok(Some(NestedCaret::new(address, offset)))
    }

    /// Put the nested caret at `caret`; `extend` keeps the anchor when it is the same
    /// paragraph (a selection does not cross paragraphs here).
    pub fn set_nested(&mut self, mut caret: NestedCaret, extend: bool) -> Result<()> {
        let len = self.nested_len(&caret.address)?;
        caret.offset = caret.offset.min(len);
        caret.anchor = match &self.nested {
            Some(old) if extend && old.address == caret.address => {
                Some(old.anchor.unwrap_or(old.offset))
            }
            _ => None,
        };
        self.clear_selection();
        self.nested = Some(caret);
        Ok(())
    }

    fn nested_or_err(&self) -> Result<NestedCaret> {
        self.nested
            .clone()
            .ok_or_else(|| anyhow!("표 안의 표나 글상자에 캐럿이 없습니다."))
    }

    /// Replace `range` of the caret paragraph by `text` (one paragraph) and place the
    /// caret after it.
    fn nested_replace(&mut self, range: std::ops::Range<usize>, text: &str) -> Result<()> {
        let caret = self.nested_or_err()?;
        let generation = self.generation;
        self.run(
            "context.v2.replace",
            json!({"address":caret.address,"start":range.start,"end":range.end,
                   "text":text,"expected_generation":generation}),
        )?;
        self.nested = Some(NestedCaret::new(
            caret.address,
            range.start + text.chars().count(),
        ));
        Ok(())
    }

    /// Type `text` at the caret, replacing the selection; line breaks split paragraphs.
    pub fn nested_insert(&mut self, text: &str) -> Result<()> {
        let mut lines = text.split('\n');
        let first = lines.next().unwrap_or_default().trim_end_matches('\r');
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        if !range.is_empty() || !first.is_empty() {
            self.nested_replace(range, first)?;
        }
        for line in lines {
            self.nested_enter()?;
            let line = line.trim_end_matches('\r');
            if !line.is_empty() {
                let at = self.nested_or_err()?.offset;
                self.nested_replace(at..at, line)?;
            }
        }
        Ok(())
    }

    /// Backspace: the selection, the character before the caret, or a merge with the
    /// previous paragraph of the same container.
    pub fn nested_backspace(&mut self) -> Result<()> {
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        if !range.is_empty() {
            return self.nested_replace(range, "");
        }
        if caret.offset > 0 {
            return self.nested_replace(caret.offset - 1..caret.offset, "");
        }
        let index = caret.address.path.last().map_or(0, ContextStep::paragraph);
        if index == 0 {
            return Ok(());
        }
        let previous = caret.address.with_index(index - 1);
        let joined_at = self.nested_len(&previous)?;
        let generation = self.generation;
        self.run(
            "context.v2.merge",
            json!({"address":caret.address,"expected_generation":generation}),
        )?;
        self.nested = Some(NestedCaret::new(previous, joined_at));
        Ok(())
    }

    /// Delete: the selection, the character after the caret, or a merge of the next
    /// paragraph of the same container into this one.
    pub fn nested_delete(&mut self) -> Result<()> {
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        if !range.is_empty() {
            return self.nested_replace(range, "");
        }
        let (paragraph, count) = self.nested_paragraph(&caret.address)?;
        let len = paragraph.text.chars().count();
        if caret.offset < len {
            return self.nested_replace(caret.offset..caret.offset + 1, "");
        }
        let index = caret.address.path.last().map_or(0, ContextStep::paragraph);
        if index + 1 >= count {
            return Ok(());
        }
        let generation = self.generation;
        self.run(
            "context.v2.merge",
            json!({"address":caret.address.with_index(index + 1),"expected_generation":generation}),
        )?;
        self.nested = Some(NestedCaret::new(caret.address, caret.offset));
        Ok(())
    }

    /// Enter: delete the selection, then split the paragraph at the caret.
    pub fn nested_enter(&mut self) -> Result<()> {
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        if !range.is_empty() {
            self.nested_replace(range, "")?;
        }
        let caret = self.nested_or_err()?;
        let generation = self.generation;
        let result = self.run(
            "context.v2.split",
            json!({"address":caret.address,"offset":caret.offset,"expected_generation":generation}),
        )?;
        let next = serde_json::from_value::<ContextAddress>(result["address"].clone())
            .unwrap_or_else(|_| {
                let index = caret.address.path.last().map_or(0, ContextStep::paragraph);
                caret.address.with_index(index + 1)
            });
        self.nested = Some(NestedCaret::new(next, 0));
        Ok(())
    }

    /// Move the nested caret; `extend` keeps or starts a selection in the paragraph.
    pub fn nested_move(&mut self, motion: Motion, extend: bool) -> Result<()> {
        let caret = self.nested_or_err()?;
        let (paragraph, count) = self.nested_paragraph(&caret.address)?;
        let len = paragraph.text.chars().count();
        let index = caret.address.path.last().map_or(0, ContextStep::paragraph);
        let mut next = caret.clone();
        match motion {
            Motion::Left if caret.offset > 0 => next.offset -= 1,
            Motion::Left if index > 0 && !extend => {
                next.address = caret.address.with_index(index - 1);
                next.offset = self.nested_len(&next.address)?;
            }
            Motion::Right if caret.offset < len => next.offset += 1,
            Motion::Right if index + 1 < count && !extend => {
                next.address = caret.address.with_index(index + 1);
                next.offset = 0;
            }
            Motion::LineStart | Motion::LineEnd => {
                // The offsets whose caret stands on the caret's row, as the body caret.
                let row = |s: &Self, offset: usize| -> Option<(u64, f64)> {
                    let v = s.nested_rect_at(&caret.address, offset).ok()?;
                    Some((v["pageIndex"].as_u64()?, v["y"].as_f64()?))
                };
                let Some(line) = row(self, caret.offset) else {
                    return Ok(());
                };
                let same = |o: usize| {
                    row(self, o).is_some_and(|(p, y)| p == line.0 && (y - line.1).abs() < 1.0)
                };
                let mut o = caret.offset;
                if matches!(motion, Motion::LineStart) {
                    while o > 0 && same(o - 1) {
                        o -= 1;
                    }
                } else {
                    while o < len && same(o + 1) {
                        o += 1;
                    }
                }
                next.offset = o;
            }
            Motion::ParagraphStart => next.offset = 0,
            Motion::ParagraphEnd => next.offset = len,
            Motion::Up | Motion::Down => {
                let path = triples(&caret.address)?;
                let layout = self.page_field_layout()?;
                let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
                let preferred = caret.preferred_x.unwrap_or(-1.0);
                let v: Value = serde_json::from_str(
                    &core
                        .move_vertical_by_path_native(
                            caret.address.section,
                            caret.address.paragraph,
                            &path_json(&path),
                            caret.offset,
                            if matches!(motion, Motion::Up) { -1 } else { 1 },
                            preferred,
                        )
                        .map_err(err)?,
                )?;
                let moved = hit_path(&v)?;
                let address =
                    self.nested_address(caret.address.section, caret.address.paragraph, &moved)?;
                if extend && address != caret.address {
                    return Ok(());
                }
                next.address = address;
                next.offset = v["charOffset"].as_u64().unwrap_or(0) as usize;
                next.preferred_x = v["preferredX"].as_f64();
            }
            _ => return Ok(()),
        }
        if !matches!(motion, Motion::Up | Motion::Down) {
            next.preferred_x = None;
        }
        next.anchor = if extend {
            Some(caret.anchor.unwrap_or(caret.offset))
        } else {
            None
        };
        if next.address != caret.address {
            next.anchor = None;
        }
        self.nested = Some(next);
        Ok(())
    }

    /// Caret rectangle `{pageIndex, x, y, height}` in page pixels.
    pub fn nested_caret_rect(&self) -> Result<Value> {
        let caret = self.nested_or_err()?;
        self.nested_rect_at(&caret.address, caret.offset)
    }

    /// The caret rectangle at `offset` of the paragraph `address` names.
    fn nested_rect_at(&self, address: &ContextAddress, offset: usize) -> Result<Value> {
        let path = triples(address)?;
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        Ok(serde_json::from_str(
            &core
                .get_cursor_rect_by_path_native(
                    address.section,
                    address.paragraph,
                    &path_json(&path),
                    offset,
                )
                .map_err(err)?,
        )?)
    }

    /// Select the word (or run of spaces or punctuation) around the caret, as a double
    /// click does in the body.
    pub fn nested_select_word(&mut self) -> Result<()> {
        let caret = self.nested_or_err()?;
        let chars: Vec<char> = self
            .nested_paragraph(&caret.address)?
            .0
            .text
            .chars()
            .collect();
        let at = caret.offset.min(chars.len());
        let probe = if at < chars.len() {
            at
        } else {
            at.saturating_sub(1)
        };
        let Some(kind) = chars.get(probe).map(|c| editing::class(*c)) else {
            return Ok(());
        };
        let mut start = probe;
        while start > 0 && editing::class(chars[start - 1]) == kind {
            start -= 1;
        }
        let mut end = probe;
        while end < chars.len() && editing::class(chars[end]) == kind {
            end += 1;
        }
        self.nested = Some(NestedCaret {
            anchor: Some(start),
            ..NestedCaret::new(caret.address, end)
        });
        Ok(())
    }

    /// Select the caret paragraph, as a triple click does in the body.
    pub fn nested_select_paragraph(&mut self) -> Result<()> {
        let caret = self.nested_or_err()?;
        let len = self.nested_len(&caret.address)?;
        self.nested = Some(NestedCaret {
            anchor: Some(0),
            ..NestedCaret::new(caret.address, len)
        });
        Ok(())
    }

    /// Tab / Shift+Tab in a nested table: the start of the next (previous) cell of the
    /// same table. Returns `false` in a text box or at the table's first or last cell.
    pub fn nested_next_cell(&mut self, back: bool) -> Result<bool> {
        let caret = self.nested_or_err()?;
        let Some(ContextStep::TableCell { control, cell, .. }) = caret.address.path.last() else {
            return Ok(false);
        };
        let (control, cell) = (*control, *cell);
        // The paragraph holding the table: the host paragraph or a cell paragraph above.
        let mut host = caret.address.clone();
        host.path.pop();
        let cells = if host.path.is_empty() {
            self.core
                .document()
                .sections
                .get(host.section)
                .and_then(|s| s.paragraphs.get(host.paragraph))
        } else {
            Some(self.nested_paragraph(&host)?.0)
        }
        .and_then(|p| match p.controls.get(control) {
            Some(Control::Table(t)) => Some(t.cells.len()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("표를 찾지 못했습니다."))?;
        let target = if back {
            cell.checked_sub(1)
        } else {
            (cell + 1 < cells).then_some(cell + 1)
        };
        let Some(target) = target else {
            return Ok(false);
        };
        let mut address = caret.address;
        if let Some(last) = address.path.last_mut() {
            *last = ContextStep::TableCell {
                control,
                cell: target,
                paragraph: 0,
            };
        }
        self.nested_len(&address)?;
        self.nested = Some(NestedCaret::new(address, 0));
        Ok(true)
    }

    /// Selection rectangles `[{pageIndex, x, y, width, height}]` in page pixels.
    pub fn nested_selection_rects(&self) -> Result<Vec<Value>> {
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        if range.is_empty() {
            return Ok(vec![]);
        }
        let mut path = triples(&caret.address)?;
        let index = path.last().map_or(0, |step| step.2);
        // The path names the container; the range carries the paragraph.
        if let Some(last) = path.last_mut() {
            last.2 = 0;
        }
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(
            &core
                .get_selection_rects_in_cell_by_path_native(
                    caret.address.section,
                    caret.address.paragraph,
                    &path_json(&path),
                    index,
                    range.start,
                    index,
                    range.end,
                    None,
                )
                .map_err(err)?,
        )?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    /// The selected text of the nested caret paragraph.
    pub fn nested_selected_text(&self) -> Result<String> {
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        let (paragraph, _) = self.nested_paragraph(&caret.address)?;
        Ok(paragraph
            .text
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect())
    }

    /// Character format for the selection, or the whole paragraph without one.
    pub fn nested_character(&mut self, format: Value) -> Result<()> {
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        let (start, end) = if range.is_empty() {
            (0, self.nested_len(&caret.address)?)
        } else {
            (range.start, range.end)
        };
        let generation = self.generation;
        self.run(
            "context.v2.character",
            json!({"address":caret.address,"start":start,"end":end,"format":format,
                   "expected_generation":generation}),
        )?;
        self.nested = Some(caret);
        Ok(())
    }

    /// Paragraph format for the caret paragraph.
    pub fn nested_paragraph_format(&mut self, format: Value) -> Result<()> {
        let caret = self.nested_or_err()?;
        let generation = self.generation;
        self.run(
            "context.v2.paragraph",
            json!({"address":caret.address,"format":format,"expected_generation":generation}),
        )?;
        self.nested = Some(caret);
        Ok(())
    }

    /// After undo/redo or another edit: keep the caret only while its paragraph exists.
    pub fn revalidate_nested(&mut self) {
        let Some(caret) = self.nested.clone() else {
            return;
        };
        match self.nested_len(&caret.address) {
            Ok(len) => {
                let mut caret = caret;
                caret.offset = caret.offset.min(len);
                caret.anchor = caret.anchor.map(|a| a.min(len));
                self.nested = Some(caret);
            }
            Err(_) => self.nested = None,
        }
    }
}

fn hit_path(v: &Value) -> Result<Vec<(usize, usize, usize)>> {
    v["cellPath"]
        .as_array()
        .ok_or_else(|| anyhow!("RHWP 응답에 cellPath가 없습니다: {v}"))?
        .iter()
        .map(|e| {
            let n = |key: &str| {
                e[key]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("RHWP 응답에 {key}가 없습니다: {e}"))
            };
            Ok((n("controlIndex")?, n("cellIndex")?, n("cellParaIndex")?))
        })
        .collect()
}
