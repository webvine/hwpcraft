//! A caret inside a nested table cell or a text box, which the body `Cursor` (one
//! table level) cannot hold. Text edits go through `context.v2`, so protection, undo
//! and generation checks are the same as for the region editor; geometry and up/down
//! movement come from RHWP's path API (0046).
use super::*;
use hwpcraft_doc::model::paragraph::Paragraph;
use hwpcraft_doc::model::shape::ShapeObject;
use serde::Serialize;

/// A cell block of a table inside a table cell, picked by dragging (or Shift+click)
/// from one of its cells to another: the table is `control` of the cell paragraph
/// `host` names, and `range` is the block in its grid.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NestedBlock {
    pub host: DocumentAddress,
    pub control: usize,
    pub range: crate::ext_table::CellRange,
}

/// Cached page boxes of a nested block: (generation, page, block) and the boxes.
pub(crate) type NestedBlockRects = ((u64, u32, NestedBlock), Vec<[f64; 4]>);

/// A position in a nested container: the paragraph's index in the container (the
/// last path step's paragraph) and a scalar offset in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NestedPoint {
    pub paragraph: usize,
    pub offset: usize,
}

/// Caret and selection anchor in one nested container (a table cell or text box).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NestedCaret {
    pub address: ContextAddress,
    pub offset: usize,
    /// Selection anchor in the same container, or `None` without a selection.
    pub anchor: Option<NestedPoint>,
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
    /// The caret as a point of its container.
    pub fn point(&self) -> NestedPoint {
        NestedPoint {
            paragraph: index(&self.address),
            offset: self.offset,
        }
    }
    /// The selection as ordered (start, end) points; `None` when nothing is selected.
    pub fn span(&self) -> Option<(NestedPoint, NestedPoint)> {
        let (a, b) = (self.anchor?, self.point());
        (a != b).then(|| (a.min(b), a.max(b)))
    }
    /// The selected range when it lies in the caret paragraph, else empty.
    pub fn selection(&self) -> std::ops::Range<usize> {
        match self.span() {
            Some((a, b)) if a.paragraph == b.paragraph => a.offset..b.offset,
            _ => self.offset..self.offset,
        }
    }
}

/// The paragraph index of the last path step (the caret paragraph in its container).
fn index(address: &ContextAddress) -> usize {
    address.path.last().map_or(0, ContextStep::paragraph)
}

/// Whether two addresses name paragraphs of the same container.
fn same_container(a: &ContextAddress, b: &ContextAddress) -> bool {
    a.section == b.section
        && a.paragraph == b.paragraph
        && a.path.len() == b.path.len()
        && !a.path.is_empty()
        && a.with_index(0) == b.with_index(0)
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
        self.nested_block = None;
    }

    /// The nested-table cell block, if one is picked.
    pub fn nested_block(&self) -> Option<&NestedBlock> {
        self.nested_block.as_ref()
    }
    pub fn clear_nested_block(&mut self) {
        self.nested_block = None;
    }

    /// The nested table holding the caret's cell and the cell's index in it: the
    /// paragraph that holds the table, the table's control index and the cell.
    pub fn nested_table_cell(&self) -> Option<(DocumentAddress, usize, usize)> {
        let caret = self.nested.as_ref()?;
        table_cell_of(&caret.address)
    }

    /// (row, col, row span, col span) of `cell` in the nested table.
    fn nested_cell_box(
        &self,
        host: &DocumentAddress,
        control: usize,
        cell: usize,
    ) -> Option<(u16, u16, u16, u16)> {
        let v = self.nested_table_info(host, control).ok()?;
        let c = v["cells"].as_array()?.iter().find(|c| c["index"] == cell)?;
        let n = |k: &str| c[k].as_u64().map(|v| v as u16);
        Some((
            n("row")?,
            n("col")?,
            n("rowSpan")?.max(1),
            n("colSpan")?.max(1),
        ))
    }

    /// Page boxes `[x, y, width, height]` of the block's cells on `page`.
    pub fn nested_block_rects(&self, page: u32) -> Result<Vec<[f64; 4]>> {
        let Some(block) = &self.nested_block else {
            return Ok(vec![]);
        };
        let key = (self.generation, page, block.clone());
        if let Some((k, rects)) = self.nested_block_rects.borrow().as_ref()
            && *k == key
        {
            return Ok(rects.clone());
        }
        fn visit(
            node: &hwpcraft_doc::RenderNode,
            block: &NestedBlock,
            inside: bool,
            out: &mut Vec<[f64; 4]>,
        ) {
            use hwpcraft_doc::RenderNodeType;
            let inside = match &node.node_type {
                RenderNodeType::Table(t) => {
                    t.section_index == Some(block.host.section)
                        && t.control_index == Some(block.control)
                        && t.cell_context.as_ref().is_some_and(|c| {
                            c.parent_para_index == block.host.paragraph
                            // The path runs through the host cells and ends at this
                            // table's own entry.
                            && c.path.len() == block.host.path.len() + 1
                            && c.path.last().is_some_and(|e| e.control_index == block.control)
                            && c.path.iter().zip(&block.host.path).all(|(e, s)| {
                                e.control_index == s.control
                                    && e.cell_index == s.cell
                                    && e.cell_para_index == s.paragraph
                            })
                        })
                }
                RenderNodeType::TableCell(c) if inside => {
                    let r = &block.range;
                    if (r.start_row..=r.end_row).contains(&c.row)
                        && (r.start_col..=r.end_col).contains(&c.col)
                    {
                        let b = &node.bbox;
                        out.push([b.x, b.y, b.width, b.height]);
                    }
                    false
                }
                _ => inside,
            };
            for child in &node.children {
                visit(child, block, inside, out);
            }
        }
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let tree = core.build_page_render_tree(page).map_err(err)?;
        let mut out = Vec::new();
        visit(&tree.root, block, false, &mut out);
        *self.nested_block_rects.borrow_mut() = Some((key, out.clone()));
        Ok(out)
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

    /// Put the nested caret at `caret`; `extend` keeps the anchor when it is in the
    /// same container, so a selection may run over its paragraphs.
    pub fn set_nested(&mut self, mut caret: NestedCaret, extend: bool) -> Result<()> {
        let len = self.nested_len(&caret.address)?;
        caret.offset = caret.offset.min(len);
        // Extending from one cell of a nested table into another picks a cell block,
        // as dragging over a top-level table's cells does; the caret stays.
        if extend
            && let Some(old) = &self.nested
            && let (Some(from), Some(to)) =
                (table_cell_of(&old.address), table_cell_of(&caret.address))
            && from.0 == to.0
            && from.1 == to.1
            && from.2 != to.2
        {
            let anchor = self
                .nested_block
                .as_ref()
                .filter(|b| b.host == from.0 && b.control == from.1)
                .map(|_| from.2)
                .unwrap_or(from.2);
            if let (Some(a), Some(b)) = (
                self.nested_cell_box(&from.0, from.1, anchor),
                self.nested_cell_box(&to.0, to.1, to.2),
            ) {
                self.nested_block = Some(NestedBlock {
                    host: from.0,
                    control: from.1,
                    range: crate::ext_table::CellRange {
                        start_row: a.0.min(b.0),
                        start_col: a.1.min(b.1),
                        end_row: (a.0 + a.2 - 1).max(b.0 + b.2 - 1),
                        end_col: (a.1 + a.3 - 1).max(b.1 + b.3 - 1),
                    },
                });
                return Ok(());
            }
        }
        self.nested_block = None;
        caret.anchor = match &self.nested {
            Some(old) if extend && same_container(&old.address, &caret.address) => {
                Some(old.anchor.unwrap_or(old.point()))
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

    /// Delete a selection that runs over paragraphs: the tail of the first, the
    /// paragraphs between and the head of the last, joined into the first, as one
    /// undo step. The caret goes to the start of the selection.
    fn nested_delete_span(&mut self, start: NestedPoint, end: NestedPoint) -> Result<()> {
        let caret = self.nested_or_err()?;
        let first = caret.address.with_index(start.paragraph);
        self.transaction(|s| {
            let edit = |s: &mut Self, command: &str, mut params: Value| -> Result<Value> {
                params["expected_generation"] = json!(s.generation);
                s.run(command, params)
            };
            let clear = |s: &mut Self, address: &ContextAddress, from: usize, to: usize| {
                if from < to {
                    edit(
                        s,
                        "context.v2.replace",
                        json!({"address":address,"start":from,"end":to,"text":""}),
                    )?;
                }
                Ok::<_, anyhow::Error>(())
            };
            // Back to front, so earlier offsets stay valid: the head of the last
            // paragraph, the paragraphs between, then the tail of the first.
            clear(s, &caret.address.with_index(end.paragraph), 0, end.offset)?;
            for k in (start.paragraph + 1..end.paragraph).rev() {
                let address = caret.address.with_index(k);
                let len = s.nested_len(&address)?;
                clear(s, &address, 0, len)?;
            }
            let len = s.nested_len(&first)?;
            clear(s, &first, start.offset, len)?;
            for _ in start.paragraph..end.paragraph {
                let next = caret.address.with_index(start.paragraph + 1);
                edit(s, "context.v2.merge", json!({ "address": next }))?;
            }
            Ok(())
        })?;
        self.nested = Some(NestedCaret::new(first, start.offset));
        Ok(())
    }

    /// Delete the selection if there is one; `true` when something was selected.
    fn nested_delete_selection(&mut self) -> Result<bool> {
        let caret = self.nested_or_err()?;
        match caret.span() {
            None => Ok(false),
            Some((a, b)) if a.paragraph == b.paragraph => {
                let address = caret.address.with_index(a.paragraph);
                self.nested = Some(NestedCaret::new(address, a.offset));
                self.nested_replace(a.offset..b.offset, "")?;
                Ok(true)
            }
            Some((a, b)) => {
                self.nested_delete_span(a, b)?;
                Ok(true)
            }
        }
    }

    /// Type `text` at the caret, replacing the selection; line breaks split paragraphs.
    pub fn nested_insert(&mut self, text: &str) -> Result<()> {
        let mut lines = text.split('\n');
        let first = lines.next().unwrap_or_default().trim_end_matches('\r');
        let caret = self.nested_or_err()?;
        let range = caret.selection();
        if caret
            .span()
            .is_some_and(|(a, b)| a.paragraph != b.paragraph)
        {
            self.nested_delete_selection()?;
            if !first.is_empty() {
                let at = self.nested_or_err()?.offset;
                self.nested_replace(at..at, first)?;
            }
        } else if !range.is_empty() || !first.is_empty() {
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
        if self.nested_delete_selection()? {
            return Ok(());
        }
        let caret = self.nested_or_err()?;
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
        if self.nested_delete_selection()? {
            return Ok(());
        }
        let caret = self.nested_or_err()?;
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
        self.nested_delete_selection()?;
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
            Motion::Left if index > 0 => {
                next.address = caret.address.with_index(index - 1);
                next.offset = self.nested_len(&next.address)?;
            }
            Motion::Right if caret.offset < len => next.offset += 1,
            Motion::Right if index + 1 < count => {
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
                if extend && !same_container(&address, &caret.address) {
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
        next.anchor = if extend && same_container(&next.address, &caret.address) {
            Some(caret.anchor.unwrap_or(caret.point()))
        } else {
            None
        };
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
        let paragraph = index(&caret.address);
        self.nested = Some(NestedCaret {
            anchor: Some(NestedPoint {
                paragraph,
                offset: start,
            }),
            ..NestedCaret::new(caret.address, end)
        });
        Ok(())
    }

    /// Select the caret paragraph, as a triple click does in the body.
    pub fn nested_select_paragraph(&mut self) -> Result<()> {
        let caret = self.nested_or_err()?;
        let len = self.nested_len(&caret.address)?;
        let paragraph = index(&caret.address);
        self.nested = Some(NestedCaret {
            anchor: Some(NestedPoint {
                paragraph,
                offset: 0,
            }),
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
        let Some((start, end)) = caret.span() else {
            return Ok(vec![]);
        };
        let mut path = triples(&caret.address)?;
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
                    start.paragraph,
                    start.offset,
                    end.paragraph,
                    end.offset,
                    None,
                )
                .map_err(err)?,
        )?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    /// The selected text, paragraphs joined by line breaks.
    pub fn nested_selected_text(&self) -> Result<String> {
        let mut lines = Vec::new();
        for (address, from, to) in self.nested_ranges(false)? {
            let text = &self.nested_paragraph(&address)?.0.text;
            lines.push(text.chars().skip(from).take(to - from).collect::<String>());
        }
        Ok(lines.join("\n"))
    }

    /// The paragraphs of the selection with the character range in each. Without a
    /// selection: the whole caret paragraph when `whole`, else nothing.
    fn nested_ranges(&self, whole: bool) -> Result<Vec<(ContextAddress, usize, usize)>> {
        let caret = self.nested_or_err()?;
        let Some((start, end)) = caret.span() else {
            if !whole {
                return Ok(vec![]);
            }
            let len = self.nested_len(&caret.address)?;
            return Ok(vec![(caret.address, 0, len)]);
        };
        (start.paragraph..=end.paragraph)
            .map(|k| {
                let address = caret.address.with_index(k);
                let len = self.nested_len(&address)?;
                let from = if k == start.paragraph {
                    start.offset
                } else {
                    0
                };
                let to = if k == end.paragraph { end.offset } else { len };
                Ok((address, from.min(len), to.min(len).max(from.min(len))))
            })
            .collect()
    }

    /// Character format for the selection, or the whole paragraph without one.
    pub fn nested_character(&mut self, format: Value) -> Result<()> {
        let caret = self.nested_or_err()?;
        let ranges = self.nested_ranges(true)?;
        self.transaction(|s| {
            for (address, start, end) in ranges {
                if start == end && s.nested_len(&address)? > 0 {
                    continue;
                }
                let generation = s.generation;
                s.run(
                    "context.v2.character",
                    json!({"address":address,"start":start,"end":end,"format":format,
                           "expected_generation":generation}),
                )?;
            }
            Ok(())
        })?;
        self.nested = Some(caret);
        Ok(())
    }

    /// Paragraph format for the caret paragraph.
    pub fn nested_paragraph_format(&mut self, format: Value) -> Result<()> {
        let caret = self.nested_or_err()?;
        let ranges = self.nested_ranges(true)?;
        self.transaction(|s| {
            for (address, ..) in ranges {
                let generation = s.generation;
                s.run(
                    "context.v2.paragraph",
                    json!({"address":address,"format":format,"expected_generation":generation}),
                )?;
            }
            Ok(())
        })?;
        self.nested = Some(caret);
        Ok(())
    }

    /// The character shape at the nested caret, for the format queries the dialogs
    /// read: the start of the selection, or the character before the caret (the
    /// first one at a paragraph start). `None` without a nested caret.
    pub(crate) fn nested_char_shape_id(&self) -> Result<Option<u16>> {
        let Some(caret) = &self.nested else {
            return Ok(None);
        };
        let (address, offset) = match caret.span() {
            Some((start, _)) => (caret.address.with_index(start.paragraph), start.offset),
            None => (caret.address.clone(), caret.offset.saturating_sub(1)),
        };
        let paragraph = self.nested_paragraph(&address)?.0;
        let id = paragraph.char_shape_id_at(offset).unwrap_or(0);
        Ok(Some(u16::try_from(id).unwrap_or(0)))
    }

    /// The section and paragraph shape of the nested caret paragraph; `None` without
    /// a nested caret.
    pub(crate) fn nested_para_shape_id(&self) -> Result<Option<(usize, u16)>> {
        let Some(caret) = &self.nested else {
            return Ok(None);
        };
        let paragraph = self.nested_paragraph(&caret.address)?.0;
        Ok(Some((caret.address.section, paragraph.para_shape_id)))
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
                caret.anchor = caret.anchor.and_then(|a| {
                    let len = self
                        .nested_len(&caret.address.with_index(a.paragraph))
                        .ok()?;
                    Some(NestedPoint {
                        offset: a.offset.min(len),
                        ..a
                    })
                });
                self.nested = Some(caret);
            }
            Err(_) => {
                self.nested = None;
                self.nested_block = None;
            }
        }
    }
}

/// The cell paragraph holding the table (as a `DocumentAddress`), the table's
/// control index and the cell index, when `address` names a nested table cell.
fn table_cell_of(address: &ContextAddress) -> Option<(DocumentAddress, usize, usize)> {
    let (last, before) = address.path.split_last()?;
    let ContextStep::TableCell { control, cell, .. } = last else {
        return None;
    };
    if before.is_empty() {
        return None;
    }
    let path = before
        .iter()
        .map(|step| match step {
            ContextStep::TableCell {
                control,
                cell,
                paragraph,
            } => Some(crate::contexts::CellStep {
                control: *control,
                cell: *cell,
                paragraph: *paragraph,
            }),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((
        DocumentAddress {
            section: address.section,
            paragraph: address.paragraph,
            path,
        },
        *control,
        *cell,
    ))
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
