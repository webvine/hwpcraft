//! 머리말·꼬리말 편집: entering a header or footer from the page, as Hangul's double
//! click on its area does, and the caret inside it. RHWP's header/footer API (made for
//! rhwp-studio's header editing) gives each page's areas, the header or footer a page
//! shows (odd/even pages, one inherited from an earlier section) and caret geometry;
//! this layer keeps which one is edited and on which page. The screen draws the areas
//! and routes input; geometry is in page pixels, as `page_control_layout`.
use super::*;
use hwpcraft_doc::model::header_footer::HeaderFooterApply;
use serde::{Deserialize, Serialize};

/// 양 쪽 / 짝수 쪽 / 홀수 쪽: RHWP's apply codes 0, 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HfApply {
    Both,
    Even,
    Odd,
}
impl HfApply {
    fn code(self) -> u8 {
        match self {
            Self::Both => 0,
            Self::Even => 1,
            Self::Odd => 2,
        }
    }
    fn from_code(code: u64) -> Self {
        match code {
            1 => Self::Even,
            2 => Self::Odd,
            _ => Self::Both,
        }
    }
}

/// One header or footer of a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HfTarget {
    pub section: usize,
    pub footer: bool,
    pub apply_to: HfApply,
}

/// The header or footer a page shows in one of its areas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct HfHit {
    pub target: HfTarget,
    /// The section has it; entering an empty area makes it (`hf_enter`).
    pub exists: bool,
    /// The page hides it (쪽 감추기).
    pub hidden: bool,
}

/// 진하게 · 기울임 · 밑줄 where the edited header or footer would take typing: the
/// start of the selection, or the character before the caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HfCharState {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// A place in the edited header or footer: its paragraph and a scalar offset there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct HfPoint {
    pub paragraph: usize,
    pub offset: usize,
}

#[derive(Clone, Debug)]
struct HfEdit {
    target: HfTarget,
    page: u32,
    caret: HfPoint,
    anchor: Option<HfPoint>,
    /// Column kept across up/down moves, in page pixels.
    preferred_x: Option<f64>,
}

/// The header or footer being edited, if any, and a counter of entering, leaving and
/// page changes (they are not document edits, so `generation` stays).
#[derive(Default)]
pub(crate) struct HfState {
    edit: Option<HfEdit>,
    revision: u64,
}

fn number(v: &Value, key: &str) -> Result<usize> {
    v[key]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| anyhow!("머리말/꼬리말 정보를 읽지 못했습니다: {key}"))
}

impl Session {
    /// The header or footer whose area holds page point (`x`, `y`), as the page shows it.
    pub fn hf_hit(&self, page: u32, x: f64, y: f64) -> Result<Option<HfHit>> {
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(
            &core
                .hit_test_header_footer_native(page, x, y)
                .map_err(err)?,
        )?;
        if v["hit"] != true {
            return Ok(None);
        }
        let target = HfTarget {
            section: number(&v, "sectionIndex")?,
            footer: v["isHeader"] != true,
            apply_to: HfApply::from_code(v["applyTo"].as_u64().unwrap_or(0)),
        };
        self.hf_hit_of(page, target).map(Some)
    }

    /// The header (or footer) `page` shows, without a point: for the ribbon's
    /// 머리말/꼬리말 buttons and 이전·다음.
    pub fn hf_page_target(&self, page: u32, footer: bool) -> Result<HfHit> {
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(
            &core
                .get_header_footer_edit_target_native(page, !footer)
                .map_err(err)?,
        )?;
        let target = HfTarget {
            section: number(&v, "sectionIndex")?,
            footer,
            apply_to: HfApply::from_code(v["applyTo"].as_u64().unwrap_or(0)),
        };
        self.hf_hit_of(page, target)
    }

    /// Both areas of `page`: `{header: {rect: [x, y, w, h], target, exists, hidden},
    /// footer: {…}}` in page pixels.
    pub fn hf_page_info(&self, page: u32) -> Result<Value> {
        let info = self.page_info(page)?;
        let side = |footer: bool| -> Result<Value> {
            let area = &info[if footer { "footerArea" } else { "headerArea" }];
            let f = |k: &str| area[k].as_f64().unwrap_or(0.0);
            let hit = self.hf_page_target(page, footer)?;
            Ok(json!({
                "rect": [f("x"), f("y"), f("width"), f("height")],
                "target": hit.target,
                "exists": hit.exists,
                "hidden": hit.hidden,
            }))
        };
        Ok(json!({"header": side(false)?, "footer": side(true)?}))
    }

    fn hf_hit_of(&self, page: u32, target: HfTarget) -> Result<HfHit> {
        let exists = self.hf_exists(target)?;
        let hidden = exists && !self.hf_drawn(page, target.footer)?;
        Ok(HfHit {
            target,
            exists,
            hidden,
        })
    }

    fn hf_exists(&self, t: HfTarget) -> Result<bool> {
        let v: Value = serde_json::from_str(
            &self
                .core
                .get_header_footer_native(t.section, !t.footer, t.apply_to.code())
                .map_err(err)?,
        )?;
        Ok(v["exists"] == true)
    }

    /// Whether `page` draws a header (footer): a page that hides it has no such node.
    fn hf_drawn(&self, page: u32, footer: bool) -> Result<bool> {
        fn find(node: &hwpcraft_doc::RenderNode, footer: bool) -> bool {
            use hwpcraft_doc::RenderNodeType;
            match node.node_type {
                RenderNodeType::Header if !footer => true,
                RenderNodeType::Footer if footer => true,
                _ => node.children.iter().any(|c| find(c, footer)),
            }
        }
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let tree = core.build_page_render_tree(page).map_err(err)?;
        Ok(find(&tree.root, footer))
    }

    /// Edit `target` from `page`, with the caret at its start. A target the section
    /// does not have yet is made first (one undo step), as Hangul does.
    pub fn hf_enter(&mut self, target: HfTarget, page: u32) -> Result<()> {
        if page >= self.page_count()? {
            bail!("쪽이 없습니다.");
        }
        if self.core.document().sections.get(target.section).is_none() {
            bail!("구역이 없습니다.");
        }
        if !self.hf_exists(target)? {
            self.guard_protected()?;
            self.transaction(|s| {
                s.core
                    .create_header_footer_native(
                        target.section,
                        !target.footer,
                        target.apply_to.code(),
                    )
                    .map_err(err)?;
                Ok(())
            })?;
        }
        self.clear_nested();
        self.anchor = None;
        self.span_anchor = None;
        self.hf.edit = Some(HfEdit {
            target,
            page,
            caret: HfPoint::default(),
            anchor: None,
            preferred_x: None,
        });
        self.hf.revision += 1;
        Ok(())
    }

    /// [`Session::hf_enter`] with the caret where page point (`x`, `y`) falls.
    pub fn hf_enter_at(&mut self, target: HfTarget, page: u32, x: f64, y: f64) -> Result<()> {
        self.hf_enter(target, page)?;
        self.hf_click(page, x, y, false)?;
        Ok(())
    }

    /// Back to the body.
    pub fn hf_exit(&mut self) {
        if self.hf.edit.take().is_some() {
            self.hf.revision += 1;
        }
    }

    pub fn hf_target(&self) -> Option<HfTarget> {
        self.hf.edit.as_ref().map(|e| e.target)
    }
    /// The page the edited header or footer was entered (or last clicked) on.
    pub fn hf_page(&self) -> Option<u32> {
        self.hf.edit.as_ref().map(|e| e.page)
    }
    /// Changes on entering, leaving and moving to another page.
    pub fn hf_revision(&self) -> u64 {
        self.hf.revision
    }
    /// Caret and selection anchor in the edited header or footer.
    pub fn hf_caret(&self) -> Option<(HfPoint, Option<HfPoint>)> {
        self.hf.edit.as_ref().map(|e| (e.caret, e.anchor))
    }

    /// Put the caret where page point (`x`, `y`) falls in the edited header or footer
    /// (`extend` keeps the anchor for a selection). False when the point is not in it.
    pub fn hf_click(&mut self, page: u32, x: f64, y: f64, extend: bool) -> Result<bool> {
        let Some(edit) = self.hf.edit.as_ref() else {
            bail!("머리말/꼬리말을 편집하고 있지 않습니다.");
        };
        let t = edit.target;
        // RHWP answers with the nearest place for any point; a press must be in the
        // area (a drag that extends the selection may leave it).
        if !extend && self.hf_hit(page, x, y)?.map(|h| h.target) != Some(t) {
            return Ok(false);
        }
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(
            &core
                .hit_test_in_header_footer_target_native(
                    page,
                    t.section,
                    !t.footer,
                    t.apply_to.code(),
                    x,
                    y,
                )
                .map_err(err)?,
        )?;
        if v["hit"] != true {
            return Ok(false);
        }
        let point = HfPoint {
            paragraph: number(&v, "paraIndex")?,
            offset: number(&v, "charOffset")?,
        };
        let Some(edit) = self.hf.edit.as_mut() else {
            return Ok(false);
        };
        edit.anchor = if extend {
            Some(edit.anchor.unwrap_or(edit.caret))
        } else {
            None
        };
        edit.caret = point;
        edit.preferred_x = None;
        if edit.page != page {
            edit.page = page;
            self.hf.revision += 1;
        }
        Ok(true)
    }

    /// Caret rectangle `{pageIndex, x, y, height}` on the entered page, in page pixels.
    pub fn hf_caret_rect(&self) -> Result<Value> {
        let Some(edit) = self.hf.edit.as_ref() else {
            bail!("머리말/꼬리말을 편집하고 있지 않습니다.");
        };
        self.hf_rect_at(edit, edit.caret)
    }

    fn hf_rect_at(&self, edit: &HfEdit, at: HfPoint) -> Result<Value> {
        let t = edit.target;
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        Ok(serde_json::from_str(
            &core
                .get_cursor_rect_in_header_footer_native(
                    t.section,
                    !t.footer,
                    t.apply_to.code(),
                    at.paragraph,
                    at.offset,
                    edit.page as i32,
                )
                .map_err(err)?,
        )?)
    }

    /// Line boxes of the selection on `page`, in page pixels; empty without one.
    pub fn hf_selection_rects(&self, page: u32) -> Result<Value> {
        let Some(edit) = self.hf.edit.as_ref() else {
            return Ok(json!([]));
        };
        let Some(anchor) = edit.anchor.filter(|a| *a != edit.caret) else {
            return Ok(json!([]));
        };
        let (start, end) = (anchor.min(edit.caret), anchor.max(edit.caret));
        let t = edit.target;
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        Ok(serde_json::from_str(
            &core
                .get_selection_rects_in_header_footer_native(
                    t.section,
                    !t.footer,
                    t.apply_to.code(),
                    page,
                    start.paragraph,
                    start.offset,
                    end.paragraph,
                    end.offset,
                )
                .map_err(err)?,
        )?)
    }

    /// After undo, redo or another command: leave a header or footer that is gone and
    /// keep the caret inside the one that stays.
    pub fn revalidate_hf(&mut self) {
        let Some(edit) = self.hf.edit.clone() else {
            return;
        };
        if !self.hf_exists(edit.target).unwrap_or(false) {
            self.hf_exit();
            return;
        }
        let paragraphs = self.hf_paragraph_lengths(edit.target);
        let clamp = |p: HfPoint| {
            let paragraph = p.paragraph.min(paragraphs.len().saturating_sub(1));
            HfPoint {
                paragraph,
                offset: p
                    .offset
                    .min(paragraphs.get(paragraph).copied().unwrap_or(0)),
            }
        };
        if let Some(e) = self.hf.edit.as_mut() {
            e.caret = clamp(e.caret);
            e.anchor = e.anchor.map(clamp);
        }
    }

    /// Scalar lengths of the target's paragraphs.
    fn hf_paragraph_lengths(&self, t: HfTarget) -> Vec<usize> {
        self.hf_target_paragraphs(t)
            .map(|ps| ps.iter().map(|p| p.text.chars().count()).collect())
            .unwrap_or_default()
    }

    /// The target's paragraphs.
    fn hf_target_paragraphs(
        &self,
        t: HfTarget,
    ) -> Option<&[hwpcraft_doc::model::paragraph::Paragraph]> {
        let kind = match t.apply_to {
            HfApply::Both => HeaderFooterApply::Both,
            HfApply::Even => HeaderFooterApply::Even,
            HfApply::Odd => HeaderFooterApply::Odd,
        };
        self.core
            .document()
            .sections
            .get(t.section)
            .into_iter()
            .flat_map(|s| &s.paragraphs)
            .flat_map(|p| &p.controls)
            .find_map(|c| match c {
                Control::Header(h) if !t.footer && h.apply_to == kind => Some(&h.paragraphs),
                Control::Footer(f) if t.footer && f.apply_to == kind => Some(&f.paragraphs),
                _ => None,
            })
            .map(Vec::as_slice)
    }
}

/// Text editing in the entered header or footer. The text goes through `context.v2`
/// like the nested caret's (protection, undo and generation checks are the same), so
/// these run the nested caret's edits on a caret standing in the header for the call.
impl Session {
    /// The context address of paragraph `paragraph` of `t`.
    fn hf_address(&self, t: HfTarget, paragraph: usize) -> Result<ContextAddress> {
        let kind = match t.apply_to {
            HfApply::Both => HeaderFooterApply::Both,
            HfApply::Even => HeaderFooterApply::Even,
            HfApply::Odd => HeaderFooterApply::Odd,
        };
        let section = self
            .core
            .document()
            .sections
            .get(t.section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let (host, control) = section
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(pi, p)| {
                p.controls
                    .iter()
                    .position(|c| match c {
                        Control::Header(h) => !t.footer && h.apply_to == kind,
                        Control::Footer(f) => t.footer && f.apply_to == kind,
                        _ => false,
                    })
                    .map(|ci| (pi, ci))
            })
            .ok_or_else(|| anyhow!("머리말/꼬리말이 없습니다."))?;
        let step = if t.footer {
            ContextStep::Footer { control, paragraph }
        } else {
            ContextStep::Header { control, paragraph }
        };
        Ok(ContextAddress {
            version: 2,
            section: t.section,
            paragraph: host,
            path: vec![step],
        })
    }

    /// Run `f` with the nested caret standing at the header caret, then take its place
    /// back as the header caret.
    fn with_hf_caret<R>(&mut self, f: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
        let Some(edit) = self.hf.edit.clone() else {
            bail!("머리말/꼬리말을 편집하고 있지 않습니다.");
        };
        let address = self.hf_address(edit.target, edit.caret.paragraph)?;
        let mut caret = NestedCaret::at(address, edit.caret.offset);
        caret.anchor = edit.anchor.map(|a| crate::nested_caret::NestedPoint {
            paragraph: a.paragraph,
            offset: a.offset,
        });
        let saved = self.nested.replace(caret);
        let result = f(self);
        let after = std::mem::replace(&mut self.nested, saved);
        if let (Some(after), Some(e)) = (after, self.hf.edit.as_mut()) {
            e.caret = HfPoint {
                paragraph: after.address.path.last().map_or(0, ContextStep::paragraph),
                offset: after.offset,
            };
            e.anchor = after.anchor.map(|a| HfPoint {
                paragraph: a.paragraph,
                offset: a.offset,
            });
            e.preferred_x = None;
        }
        result
    }

    /// The character format at the header caret (see [`HfCharState`]); `None` when no
    /// header or footer is edited.
    pub fn hf_char_state(&self) -> Option<HfCharState> {
        let edit = self.hf.edit.as_ref()?;
        let (at, offset) = match edit.anchor.filter(|a| *a != edit.caret) {
            Some(anchor) => {
                let start = anchor.min(edit.caret);
                (start, start.offset)
            }
            None => (edit.caret, edit.caret.offset.saturating_sub(1)),
        };
        let paragraph = self.hf_target_paragraphs(edit.target)?.get(at.paragraph)?;
        let id = paragraph.char_shape_id_at(offset).unwrap_or(0);
        let shape = self
            .core
            .document()
            .doc_info
            .char_shapes
            .get(usize::try_from(id).ok()?)?;
        Some(HfCharState {
            bold: shape.bold,
            italic: shape.italic,
            underline: shape.underline_type != hwpcraft_doc::model::style::UnderlineType::None,
        })
    }

    /// The object address (with `header_footer`, see `hf_objects`) of a
    /// `page_control_layout` entry inside the edited header or footer; `None` for an
    /// entry anywhere else.
    pub fn hf_layout_address(&self, entry: &Value) -> Option<Value> {
        let t = self.hf_target()?;
        let hf = entry.get("headerFooter")?;
        let host = self.hf_address(t, 0).ok()?;
        let control = match host.path.first()? {
            ContextStep::Header { control, .. } | ContextStep::Footer { control, .. } => *control,
            _ => return None,
        };
        let same = entry["secIdx"].as_u64() == Some(t.section as u64)
            && hf["outerParaIdx"].as_u64() == Some(host.paragraph as u64)
            && hf["outerControlIdx"].as_u64() == Some(control as u64)
            && entry.get("cellIdx").is_none();
        same.then(|| {
            json!({"section": t.section, "paragraph": entry["paraIdx"], "control": entry["controlIdx"],
                "header_footer": {"paragraph": host.paragraph, "control": control}})
        })
    }

    /// Select the whole header or footer (Ctrl+A), as Hangul does there.
    pub fn hf_select_all(&mut self) {
        let Some(t) = self.hf_target() else {
            return;
        };
        let lengths = self.hf_paragraph_lengths(t);
        let last = lengths.len().saturating_sub(1);
        if let Some(e) = self.hf.edit.as_mut() {
            e.anchor = Some(HfPoint::default());
            e.caret = HfPoint {
                paragraph: last,
                offset: lengths.get(last).copied().unwrap_or(0),
            };
            e.preferred_x = None;
        }
    }

    /// Type `text` at the caret, replacing the selection; line breaks split paragraphs.
    pub fn hf_insert(&mut self, text: &str) -> Result<()> {
        self.with_hf_caret(|s| s.nested_insert(text))
    }
    /// Backspace: the selection, the character before the caret, or a merge with the
    /// previous paragraph.
    pub fn hf_backspace(&mut self) -> Result<()> {
        self.with_hf_caret(Self::nested_backspace)
    }
    /// Delete: the selection, the character after the caret, or a merge of the next
    /// paragraph into this one.
    pub fn hf_delete(&mut self) -> Result<()> {
        self.with_hf_caret(Self::nested_delete)
    }
    /// Enter: delete the selection and split the paragraph at the caret.
    pub fn hf_enter_key(&mut self) -> Result<()> {
        self.with_hf_caret(Self::nested_enter)
    }
    pub fn hf_select_word(&mut self) -> Result<()> {
        self.with_hf_caret(Self::nested_select_word)
    }
    pub fn hf_select_paragraph(&mut self) -> Result<()> {
        self.with_hf_caret(Self::nested_select_paragraph)
    }
    pub fn hf_selected_text(&mut self) -> Result<String> {
        self.with_hf_caret(|s| s.nested_selected_text())
    }
    /// Character format for the selection, or the whole caret paragraph without one.
    pub fn hf_character(&mut self, format: Value) -> Result<()> {
        self.with_hf_caret(|s| s.nested_character(format))
    }
    /// Paragraph format for the paragraphs of the selection, or the caret paragraph.
    pub fn hf_paragraph_format(&mut self, format: Value) -> Result<()> {
        self.with_hf_caret(|s| s.nested_paragraph_format(format))
    }

    /// Insert a field at the caret, replacing the selection: page_number, total_pages,
    /// file_name, or (in the first paragraph) date_created / date_saved. One undo step.
    pub fn hf_insert_field(&mut self, kind: &str) -> Result<()> {
        self.check_hf_field(kind)?;
        self.transaction(|s| {
            s.hf_insert("")?;
            let Some(edit) = s.hf.edit.clone() else {
                bail!("머리말/꼬리말을 편집하고 있지 않습니다.");
            };
            let (t, at) = (edit.target, edit.caret);
            let added = s.insert_hf_field_at(t, at.paragraph, at.offset, kind)?;
            // The caret goes after what the field put in the text.
            if let Some(e) = s.hf.edit.as_mut() {
                e.caret.offset = at.offset + added;
                e.anchor = None;
            }
            Ok(())
        })
    }

    /// A field kind header and footer fields take, and a saved date for date_saved.
    fn check_hf_field(&self, kind: &str) -> Result<()> {
        let field = crate::ext_page::hf_field(kind)?;
        if let Err(command) = field
            && self.date_field_text(command).is_none()
        {
            bail!("저장한 적 없는 문서라 마지막 저장 날짜가 없습니다. 먼저 저장하세요.");
        }
        Ok(())
    }

    /// Put field `kind` at character `offset` of paragraph `paragraph` of header or
    /// footer `t` (no caret needed, as an MCP client does). How many characters the
    /// field added to that paragraph's text.
    pub(crate) fn insert_hf_field_at(
        &mut self,
        t: HfTarget,
        paragraph: usize,
        offset: usize,
        kind: &str,
    ) -> Result<usize> {
        let field = crate::ext_page::hf_field(kind)?;
        let Some(before) = self.hf_paragraph_lengths(t).get(paragraph).copied() else {
            bail!("그 머리말/꼬리말에는 문단 {paragraph}이(가) 없습니다.");
        };
        if offset > before {
            bail!("offset이 문단 길이({before}자)를 넘습니다.");
        }
        match field {
            Ok(code) => {
                self.core
                    .insert_field_in_hf_native(
                        t.section,
                        !t.footer,
                        t.apply_to.code(),
                        paragraph,
                        offset,
                        code,
                    )
                    .map_err(err)?;
            }
            Err(_) if paragraph != 0 => {
                bail!("날짜 필드는 머리말/꼬리말의 첫 문단에만 넣을 수 있습니다.")
            }
            Err(command) => {
                self.insert_hf_date_field(t.section, !t.footer, t.apply_to.code(), offset, command)?
            }
        }
        self.refresh_layout()?;
        let after = self
            .hf_paragraph_lengths(t)
            .get(paragraph)
            .copied()
            .unwrap_or(before);
        Ok(after.saturating_sub(before))
    }

    /// `page.header_footer.field.insert`: a field at a place in a header or footer.
    pub(crate) fn insert_hf_field_command(&mut self, params: Value) -> Result<Value> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Field {
            #[serde(default)]
            section: Option<usize>,
            #[serde(default = "on_header")]
            header: bool,
            #[serde(default = "both")]
            apply_to: HfApply,
            #[serde(default)]
            paragraph: usize,
            #[serde(default)]
            offset: Option<usize>,
            kind: String,
        }
        fn on_header() -> bool {
            true
        }
        fn both() -> HfApply {
            HfApply::Both
        }
        let p: Field = serde_json::from_value(params)?;
        let t = HfTarget {
            section: p.section.unwrap_or(self.cursor.section),
            footer: !p.header,
            apply_to: p.apply_to,
        };
        let lengths = self.hf_paragraph_lengths(t);
        if lengths.is_empty() {
            bail!(
                "그 구역에 그런 머리말/꼬리말이 없습니다. page.header 또는 page.footer로 먼저 만드세요."
            );
        }
        let offset = p
            .offset
            .unwrap_or_else(|| lengths.get(p.paragraph).copied().unwrap_or(0));
        self.check_hf_field(&p.kind)?;
        let mut added = 0;
        self.transaction(|s| {
            added = s.insert_hf_field_at(t, p.paragraph, offset, &p.kind)?;
            Ok(())
        })?;
        // The same header being edited on screen: its caret and anchor keep their text.
        if let Some(e) = self.hf.edit.as_mut().filter(|e| e.target == t) {
            for point in std::iter::once(&mut e.caret).chain(e.anchor.as_mut()) {
                if point.paragraph == p.paragraph && offset <= point.offset {
                    point.offset += added;
                }
            }
        }
        Ok(json!({"paragraph": p.paragraph, "offset": offset + added}))
    }

    /// Move the caret; `extend` keeps or starts a selection. Left/right cross into
    /// the neighbouring paragraph; up/down and line start/end follow the laid-out lines.
    pub fn hf_move(&mut self, motion: Motion, extend: bool) -> Result<()> {
        match motion {
            Motion::Left | Motion::Right | Motion::ParagraphStart | Motion::ParagraphEnd => {
                self.with_hf_caret(|s| s.nested_move(motion, extend))
            }
            Motion::LineStart | Motion::LineEnd => self.hf_move_in_line(motion, extend),
            Motion::Up | Motion::Down => self.hf_move_vertically(motion, extend),
            _ => Ok(()),
        }
    }

    fn hf_place(&mut self, to: HfPoint, extend: bool, preferred_x: Option<f64>) {
        if let Some(e) = self.hf.edit.as_mut() {
            e.anchor = if extend {
                Some(e.anchor.unwrap_or(e.caret))
            } else {
                None
            };
            e.caret = to;
            e.preferred_x = preferred_x;
        }
    }

    fn hf_move_in_line(&mut self, motion: Motion, extend: bool) -> Result<()> {
        let Some(edit) = self.hf.edit.clone() else {
            bail!("머리말/꼬리말을 편집하고 있지 않습니다.");
        };
        let len = self
            .hf_paragraph_lengths(edit.target)
            .get(edit.caret.paragraph)
            .copied()
            .unwrap_or(0);
        let row = |s: &Self, offset: usize| -> Option<f64> {
            let at = HfPoint {
                paragraph: edit.caret.paragraph,
                offset,
            };
            s.hf_rect_at(&edit, at).ok()?["y"].as_f64()
        };
        let Some(line) = row(self, edit.caret.offset) else {
            return Ok(());
        };
        let same = |o: usize| row(self, o).is_some_and(|y| (y - line).abs() < 1.0);
        let mut o = edit.caret.offset.min(len);
        if matches!(motion, Motion::LineStart) {
            while o > 0 && same(o - 1) {
                o -= 1;
            }
        } else {
            while o < len && same(o + 1) {
                o += 1;
            }
        }
        let to = HfPoint {
            paragraph: edit.caret.paragraph,
            offset: o,
        };
        self.hf_place(to, extend, None);
        Ok(())
    }

    fn hf_move_vertically(&mut self, motion: Motion, extend: bool) -> Result<()> {
        let Some(edit) = self.hf.edit.clone() else {
            bail!("머리말/꼬리말을 편집하고 있지 않습니다.");
        };
        let rect = self.hf_rect_at(&edit, edit.caret)?;
        let f = |k: &str| rect[k].as_f64().unwrap_or(0.0);
        let x = edit.preferred_x.unwrap_or(f("x"));
        let t = edit.target;
        // Further and further from the caret line until a point lands on another line
        // (line spacing makes the line box taller than the caret).
        for step in [0.15, 0.5, 1.0, 1.5, 2.0, 3.0] {
            let reach = 2.0 + f("height") * step;
            let y = if matches!(motion, Motion::Up) {
                f("y") - reach
            } else {
                f("y") + f("height") + reach
            };
            let layout = self.page_field_layout()?;
            let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
            let v: Value = serde_json::from_str(
                &core
                    .hit_test_in_header_footer_target_native(
                        edit.page,
                        t.section,
                        !t.footer,
                        t.apply_to.code(),
                        x,
                        y,
                    )
                    .map_err(err)?,
            )?;
            if v["hit"] != true {
                return Ok(());
            }
            let to = HfPoint {
                paragraph: number(&v, "paraIndex")?,
                offset: number(&v, "charOffset")?,
            };
            // Past the first or last line the nearest place stays on the caret line.
            let landed = self.hf_rect_at(&edit, to)?["y"].as_f64().unwrap_or(f("y"));
            if (landed - f("y")).abs() >= 1.0 {
                self.hf_place(to, extend, Some(x));
                return Ok(());
            }
        }
        Ok(())
    }
}
