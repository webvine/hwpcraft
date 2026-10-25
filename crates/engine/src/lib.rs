#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use anyhow::{Result, anyhow, bail};
use hwpcraft_doc::model::{control::Control, paragraph::Paragraph};
pub use hwpcraft_doc::{Cell, Cursor, DocumentCore, HyperlinkInfo, HyperlinkTarget, VERSION};
use serde_json::{Value, json};
mod ext_format;
mod ext_object;
mod ext_page;
mod ext_table;
mod extensions;
mod table_drawing;
pub use autotext::set_config_dir;
pub use ext_format::COLOR_PRESETS;
pub use extensions::{command_help, command_help_entries, extension_commands};
mod autotext;
mod caret;
mod editing;
mod hanja;
mod review;
pub use editing::Motion;
mod hwp_actions;
pub use hwp_actions::{HWP_ACTIONS, hwp_action};
mod features;
mod find;
pub use find::{FindDirection, FindOptions};
mod links;
pub use links::HyperlinkEdit;
mod highlights;
pub use highlights::{HighlightRect, markpen_ranges};
mod symbols;
pub use symbols::{
    SpecialChar, SymbolGroup, SymbolInsert, SymbolSearch, SymbolSet, block_of, ks_code,
    special_chars, symbol_chars, symbol_search, symbol_sets,
};
mod formatting;
pub use formatting::validate_format;
mod objects;
pub use objects::{EquationOptions, ObjectAddress, ShapeKind, ShapeOptions};
mod automation;
mod page;
mod protocol;
mod tabs;
mod tools;
pub use features::TableEdit;
pub use page::PageSetup;
use std::{
    ops::Range,
    path::{Path, PathBuf},
};
pub use tools::{MarkKind, SortKey, SortOptions};

pub fn blank() -> Result<DocumentCore> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(core)
}

fn err(e: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{e}")
}
/// Shared by the desktop and CLI; text offsets count Unicode scalar values.
pub enum Command {
    Insert(String),
    Backspace,
    Delete,
    Split,
    PageBreak,
    PageSetup(PageSetup),
    Undo,
    Redo,
    Character(Value),
    Paragraph(Value),
    Table { rows: u16, columns: u16 },
}
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("document.text", "본문·표 셀 텍스트 조회", ""),
    ("document.inspect", "문서 구조 조회", ""),
    ("select.text", "문자열 선택 (n번째 일치)", ""),
    ("format.character.get", "글자 모양 조회", ""),
    ("format.paragraph.get", "문단 모양 조회", ""),
    ("format.paragraph.apply", "문단 모양·머리 모양 적용", ""),
    ("format.outline.shape", "개요 번호 모양 설정", ""),
    ("insert.equation", "수식 넣기", ""),
    ("equation.set", "수식 고치기", ""),
    ("insert.footnote", "각주 넣기", ""),
    ("insert.endnote", "미주 넣기", ""),
    ("note.set", "각주·미주 텍스트 고치기", ""),
    ("insert.shape", "도형·글상자 넣기", ""),
    ("shape.resize", "도형·글상자 크기", ""),
    ("textbox.set", "글상자 텍스트 고치기", ""),
    ("object.list", "개체 목록", ""),
    ("object.delete", "개체 지우기", ""),
    ("style.list", "스타일 목록", ""),
    ("style.apply", "기존 스타일 적용", ""),
    ("file.new", "새 문서", "Ctrl+N"),
    ("file.open", "열기", "Ctrl+O"),
    ("file.save", "저장", "Ctrl+S"),
    ("file.save_as", "다른 이름으로 저장", "Ctrl+Shift+S"),
    ("file.pdf", "PDF 내보내기", ""),
    ("edit.undo", "실행 취소", "Ctrl+Z"),
    ("edit.redo", "다시 실행", "Ctrl+Shift+Z"),
    ("edit.find", "찾기 / 바꾸기", "Ctrl+F"),
    ("edit.replace", "바꾸기 (선택한 찾은 내용)", "Ctrl+H"),
    ("edit.replace_all", "모두 바꾸기", ""),
    ("edit.sort", "정렬 (선택한 문단)", ""),
    ("edit.delete_ctrl_marks", "조판 부호 지우기", ""),
    ("page.setup", "편집 용지", "F7"),
    ("insert.page_break", "쪽 나누기", "Ctrl+Enter"),
    ("format.character", "글자 모양", "Ctrl+B / I / U"),
    ("format.paragraph", "문단 모양", ""),
    ("insert.table", "표 만들기", ""),
    ("text.insert", "텍스트 입력", ""),
    ("text.backspace", "앞 글자 지우기", "Backspace"),
    ("text.delete", "뒤 글자 지우기", "Delete"),
    ("paragraph.split", "문단 나누기", "Enter"),
    ("selection.set", "문단 / 표 셀 선택", ""),
    ("table.edit", "표 줄·칸 편집 / 본문 폭에 맞춤", ""),
    ("insert.hyperlink", "하이퍼링크", "Ctrl+K"),
    ("insert.picture", "PNG 그림 삽입", ""),
    ("bookmark.add", "책갈피 추가", ""),
    ("bookmark.list", "책갈피 목록", ""),
    ("bookmark.goto", "책갈피로 이동", ""),
    ("bookmark.delete", "책갈피 삭제", ""),
    ("page.columns", "다단 설정", ""),
    ("insert.column_break", "단 나누기", ""),
    ("page.header_footer", "머리말 / 꼬리말 텍스트", ""),
    ("insert.page_number", "꼬리말에 가운데 쪽 번호", ""),
    ("hyperlink.list", "현재 문단의 하이퍼링크 목록", ""),
    ("hyperlink.update", "하이퍼링크 주소·표시 문자열 고치기", ""),
    ("hyperlink.remove", "하이퍼링크 지우기 (글자 유지)", ""),
    ("edit.paste_html", "HTML 서식 붙이기", ""),
    ("file.password", "문서 암호 설정·해제 (저장 시 적용)", ""),
    (
        "insert.symbol",
        "문자표: 문자 넣기 (글꼴 지정 가능)",
        "Ctrl+F10",
    ),
    (
        "symbol.sets",
        "문자표 영역 목록 (HNC·KS·유니코드·이모지·특수 문자)",
        "",
    ),
    ("symbol.list", "문자표 영역의 문자 목록", ""),
    ("symbol.search", "문자표 검색 (코드·이름·키워드)", ""),
    ("symbol.special", "이름 있는 특수 문자 목록", ""),
];
#[derive(Clone)]
struct Snapshot {
    id: u32,
    cursor: Cursor,
    anchor: Option<usize>,
    span_anchor: Option<Cursor>,
    revision: u64,
}
/// See [`Session::checkpoint`].
pub struct Checkpoint {
    id: u32,
    cursor: Cursor,
    anchor: Option<usize>,
    span_anchor: Option<Cursor>,
    revision: u64,
    undo_len: usize,
    overwrite: bool,
}
pub struct Session {
    pub core: DocumentCore,
    pub cursor: Cursor,
    /// Selection is confined to the current paragraph, including table cells.
    pub anchor: Option<usize>,
    /// Anchor in another paragraph of the same body/cell; see `editing.rs`.
    pub span_anchor: Option<Cursor>,
    /// Overwrite (수정) mode replaces following characters while typing.
    pub overwrite: bool,
    pub path: Option<PathBuf>,
    /// Document password applied to HWP/HWPX saves; set when a protected file was
    /// opened or by `set_password`. Never written to logs or command results.
    password: Option<String>,
    /// Caret after the last coalesced typing step; consecutive typing shares one undo.
    typing: Option<Cursor>,
    typing_chars: usize,
    clipboard_text: Option<String>,
    pub generation: u64,
    revision: u64,
    next_revision: u64,
    saved_revision: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Session {
    pub fn new() -> Result<Self> {
        Ok(Self::with_core(blank()?, None))
    }
    fn with_core(core: DocumentCore, path: Option<PathBuf>) -> Self {
        Self {
            core,
            path,
            password: None,
            cursor: Cursor::default(),
            anchor: None,
            span_anchor: None,
            overwrite: false,
            typing: None,
            typing_chars: 0,
            clipboard_text: None,
            generation: 0,
            revision: 0,
            next_revision: 1,
            saved_revision: 0,
            undo: vec![],
            redo: vec![],
        }
    }
    pub fn open(&mut self, path: &Path) -> Result<()> {
        let core = hwpcraft_formats::open(path)?;
        let generation = self.generation + 1;
        *self = Self::with_core(core, Some(path.to_owned()));
        self.generation = generation;
        Ok(())
    }
    /// Open a password-protected document. The password is kept for later saves.
    pub fn open_with_password(&mut self, path: &Path, password: &str) -> Result<()> {
        let core = hwpcraft_formats::open_with_password(path, password)?;
        let generation = self.generation + 1;
        *self = Self::with_core(core, Some(path.to_owned()));
        self.generation = generation;
        self.password = Some(password.to_owned());
        Ok(())
    }
    /// Set (`Some`) or clear (`None`) the password the next HWP/HWPX save uses.
    /// Changing it marks the document dirty so the user is asked to save.
    pub fn set_password(&mut self, password: Option<String>) -> Result<()> {
        if let Some(p) = &password {
            if p.is_empty() {
                bail!("문서 암호를 입력하세요.");
            }
            if p.chars().count() > 64 || p.chars().any(char::is_control) {
                bail!("문서 암호는 제어 문자 없이 64자 이하로 입력하세요.");
            }
        }
        if self.password != password {
            self.password = password;
            self.revision = self.next_revision;
            self.next_revision += 1;
        }
        Ok(())
    }
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }
    pub fn reset(&mut self) -> Result<()> {
        let generation = self.generation + 1;
        *self = Self::new()?;
        self.generation = generation;
        Ok(())
    }
    pub fn dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.save_with_tab_widths(path)?;
        if matches!(
            path.extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("hwp" | "hwpx")
        ) {
            self.path = Some(path.to_owned());
            self.saved_revision = self.revision;
        }
        Ok(())
    }
    pub fn paragraph(&self, c: Cursor) -> Result<&Paragraph> {
        let p = self
            .core
            .document()
            .sections
            .get(c.section)
            .and_then(|s| s.paragraphs.get(c.paragraph))
            .ok_or_else(|| anyhow!("문단이 없습니다."))?;
        if let Some(cell) = c.cell {
            if let Some(Control::Table(t)) = p.controls.get(cell.control) {
                return t
                    .cells
                    .get(cell.index)
                    .and_then(|v| v.paragraphs.get(cell.paragraph))
                    .ok_or_else(|| anyhow!("표의 셀 문단이 없습니다."));
            }
            bail!("이 개체의 편집은 아직 지원하지 않습니다.");
        }
        Ok(p)
    }
    pub fn text(&self) -> &str {
        self.paragraph(self.cursor)
            .map(|p| p.text.as_str())
            .unwrap_or("")
    }
    pub fn len(&self) -> usize {
        self.text().chars().count()
    }
    pub fn is_empty(&self) -> bool {
        self.text().is_empty()
    }
    /// Selected scalar range inside the caret paragraph. For a multi-paragraph
    /// span this is the caret paragraph's part; use `span()` for the whole range.
    pub fn selection(&self) -> Range<usize> {
        if let Some(a) = self.span_anchor {
            let len = self.len();
            let offset = self.cursor.offset.min(len);
            let anchor = a.cell.map(|t| t.paragraph).unwrap_or(a.paragraph);
            let caret = self
                .cursor
                .cell
                .map(|t| t.paragraph)
                .unwrap_or(self.cursor.paragraph);
            return if anchor < caret {
                0..offset
            } else {
                offset..len
            };
        }
        let end = self.cursor.offset.min(self.len());
        let start = self.anchor.unwrap_or(end).min(self.len());
        start.min(end)..start.max(end)
    }
    pub fn selected_text(&self) -> String {
        if self.span_anchor.is_some() {
            return self.span_text().unwrap_or_default();
        }
        let r = self.selection();
        self.text().chars().skip(r.start).take(r.len()).collect()
    }
    pub fn hit_test(&self, page: u32, x: f64, y: f64) -> Result<Cursor> {
        let v: Value = serde_json::from_str(&self.core.hit_test_native(page, x, y).map_err(err)?)?;
        if v["cellPath"].as_array().is_some_and(|p| p.len() > 1)
            || v["isTextBox"].as_bool() == Some(true)
        {
            bail!(
                "중첩 표 / 글상자는 열람과 보존을 지원합니다. 직접 편집은 아직 지원하지 않습니다."
            );
        }
        let n = |k: &str| v[k].as_u64().unwrap_or(0) as usize;
        let cell = v.get("cellIndex").map(|_| Cell {
            control: n("controlIndex"),
            index: n("cellIndex"),
            paragraph: n("cellParaIndex"),
        });
        Ok(Cursor {
            section: n("sectionIndex"),
            paragraph: if cell.is_some() {
                n("parentParaIndex")
            } else {
                n("paragraphIndex")
            },
            offset: n("charOffset"),
            cell,
        })
    }
    pub fn caret(&self) -> Result<Value> {
        self.caret_at(self.cursor)
    }
    pub fn caret_at(&self, c: Cursor) -> Result<Value> {
        let r = if let Some(t) = c.cell {
            self.core.get_cursor_rect_in_cell_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
                c.offset,
            )
        } else {
            self.core
                .get_cursor_rect_native(c.section, c.paragraph, c.offset)
        };
        Ok(serde_json::from_str(&r.map_err(err)?)?)
    }
    /// Every editable paragraph address in document order: body paragraphs and
    /// first-level table-cell paragraphs. No text is copied, so commands that
    /// only need addresses (search, navigation, counts) stay cheap on long documents.
    pub fn cursors(&self) -> Vec<Cursor> {
        let mut out = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, para) in section.paragraphs.iter().enumerate() {
                out.push(Cursor {
                    section: s,
                    paragraph: p,
                    ..Default::default()
                });
                for (i, ctrl) in para.controls.iter().enumerate() {
                    if let Control::Table(t) = ctrl {
                        for (j, cell) in t.cells.iter().enumerate() {
                            for k in 0..cell.paragraphs.len() {
                                out.push(Cursor {
                                    section: s,
                                    paragraph: p,
                                    offset: 0,
                                    cell: Some(Cell {
                                        control: i,
                                        index: j,
                                        paragraph: k,
                                    }),
                                });
                            }
                        }
                    }
                }
            }
        }
        out
    }
    /// Number of editable paragraphs (body and first-level cells).
    pub fn paragraph_total(&self) -> usize {
        self.core
            .document()
            .sections
            .iter()
            .flat_map(|s| s.paragraphs.iter())
            .map(|para| {
                1 + para
                    .controls
                    .iter()
                    .filter_map(|c| match c {
                        Control::Table(t) => {
                            Some(t.cells.iter().map(|c| c.paragraphs.len()).sum::<usize>())
                        }
                        _ => None,
                    })
                    .sum::<usize>()
            })
            .sum()
    }
    /// `cursors()` with a short text preview per paragraph, for list views.
    pub fn entries(&self) -> Vec<(Cursor, String)> {
        self.cursors()
            .into_iter()
            .map(|c| {
                let preview = match (self.paragraph(c), c.cell) {
                    (Ok(p), None) => p.text.chars().take(64).collect(),
                    (Ok(p), Some(cell)) => {
                        let (row, col) = self
                            .core
                            .document()
                            .sections
                            .get(c.section)
                            .and_then(|s| s.paragraphs.get(c.paragraph))
                            .and_then(|para| match para.controls.get(cell.control) {
                                Some(Control::Table(t)) => t.cells.get(cell.index),
                                _ => None,
                            })
                            .map(|cell| (cell.row + 1, cell.col + 1))
                            .unwrap_or((0, 0));
                        format!(
                            "[{row},{col}] {}",
                            p.text.chars().take(48).collect::<String>()
                        )
                    }
                    (Err(_), _) => String::new(),
                };
                (c, preview)
            })
            .collect()
    }
    fn snapshot(&mut self) -> Snapshot {
        Snapshot {
            id: self.core.save_snapshot_native(),
            cursor: self.cursor,
            anchor: self.anchor,
            span_anchor: self.span_anchor,
            revision: self.revision,
        }
    }
    /// Mark a point several commands can be rolled back to as one unit, for an
    /// atomic MCP batch. Each command still records its own undo step; `rollback`
    /// drops those steps again. The redo stack is not restored.
    pub fn checkpoint(&mut self) -> Checkpoint {
        let snapshot = self.snapshot();
        Checkpoint {
            id: snapshot.id,
            cursor: snapshot.cursor,
            anchor: snapshot.anchor,
            span_anchor: snapshot.span_anchor,
            revision: snapshot.revision,
            undo_len: self.undo.len(),
            overwrite: self.overwrite,
        }
    }
    /// Restore the document and caret to a checkpoint and forget the undo steps
    /// recorded since. The checkpoint is consumed.
    pub fn rollback(&mut self, checkpoint: Checkpoint) -> Result<()> {
        self.core
            .restore_snapshot_native(checkpoint.id)
            .map_err(err)?;
        self.core.discard_snapshot_native(checkpoint.id);
        while self.undo.len() > checkpoint.undo_len {
            if let Some(s) = self.undo.pop() {
                self.core.discard_snapshot_native(s.id);
            }
        }
        self.cursor = checkpoint.cursor;
        self.anchor = checkpoint.anchor;
        self.span_anchor = checkpoint.span_anchor;
        self.revision = checkpoint.revision;
        self.overwrite = checkpoint.overwrite;
        self.typing = None;
        self.generation += 1;
        Ok(())
    }
    /// Keep the work done since a checkpoint and free its snapshot.
    pub fn release(&mut self, checkpoint: Checkpoint) {
        self.core.discard_snapshot_native(checkpoint.id);
    }
    fn transaction(&mut self, f: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let snapshot = self.snapshot();
        self.typing = None;
        if let Err(e) = f(self) {
            self.core
                .restore_snapshot_native(snapshot.id)
                .map_err(err)?;
            self.core.discard_snapshot_native(snapshot.id);
            self.cursor = snapshot.cursor;
            self.anchor = snapshot.anchor;
            self.span_anchor = snapshot.span_anchor;
            return Err(e);
        }
        self.normalize_selection();
        for s in self.redo.drain(..) {
            self.core.discard_snapshot_native(s.id);
        }
        self.undo.push(snapshot);
        if self.undo.len() > 40 {
            let old = self.undo.remove(0);
            self.core.discard_snapshot_native(old.id);
        }
        self.revision = self.next_revision;
        self.next_revision += 1;
        self.generation += 1;
        Ok(())
    }
    fn history(&mut self, redo: bool) -> Result<()> {
        let target = if redo {
            self.redo.last()
        } else {
            self.undo.last()
        }
        .cloned();
        if let Some(target) = target {
            let current = self.snapshot();
            if let Err(e) = self.core.restore_snapshot_native(target.id) {
                self.core.discard_snapshot_native(current.id);
                return Err(err(e));
            }
            if redo {
                self.redo.pop();
                self.undo.push(current);
            } else {
                self.undo.pop();
                self.redo.push(current);
            }
            self.core.discard_snapshot_native(target.id);
            self.cursor = target.cursor;
            self.revision = target.revision;
            self.anchor = target.anchor;
            self.span_anchor = target.span_anchor;
            self.typing = None;
            self.generation += 1;
        }
        Ok(())
    }
    fn delete_range(&mut self, r: Range<usize>) -> Result<()> {
        if r.is_empty() {
            return Ok(());
        }
        let c = self.cursor;
        let result = if let Some(t) = c.cell {
            self.core.delete_text_in_cell_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
                r.start,
                r.len(),
            )
        } else {
            self.core
                .delete_text_native(c.section, c.paragraph, r.start, r.len())
        };
        result.map_err(err)?;
        self.cursor.offset = r.start;
        self.clear_selection();
        Ok(())
    }
    // RHWP split APIs count inline controls as logical characters; text editing
    // and our public cursor use scalar text offsets. Convert only at this boundary.
    fn split_offset(&self, c: Cursor) -> Result<usize> {
        let p = self.paragraph(c)?;
        let positions = p.control_text_positions();
        let before = p
            .controls
            .iter()
            .zip(positions)
            .filter(|(ctrl, pos)| {
                *pos < c.offset
                    && matches!(
                        ctrl,
                        Control::Shape(_)
                            | Control::Table(_)
                            | Control::Picture(_)
                            | Control::Equation(_)
                            | Control::Footnote(_)
                            | Control::Endnote(_)
                            | Control::AutoNumber(_)
                            | Control::CharOverlap(_)
                    )
            })
            .count();
        Ok(c.offset + before)
    }
    fn split(&mut self) -> Result<()> {
        let c = self.cursor;
        let offset = self.split_offset(c)?;
        if let Some(t) = c.cell {
            self.core
                .split_paragraph_in_cell_native(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    offset,
                    None,
                )
                .map_err(err)?;
            if let Some(cell) = self.cursor.cell.as_mut() {
                cell.paragraph += 1;
            }
        } else {
            self.core
                .split_paragraph_native(c.section, c.paragraph, offset, None)
                .map_err(err)?;
            self.cursor.paragraph += 1;
        }
        self.cursor.offset = 0;
        self.anchor = None;
        Ok(())
    }
    fn insert(&mut self, text: &str) -> Result<()> {
        if text.len() > 1_000_000 {
            bail!("한 번에 입력할 수 있는 텍스트는 1 MB 이하입니다.");
        }
        let replace = self.overwrite && !self.has_selection();
        self.delete_selection()?;
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if replace {
            // 수정 mode overwrites at most the rest of the caret paragraph.
            let first = text.split('\n').next().unwrap_or("").chars().count();
            let offset = self.cursor.offset;
            self.delete_range(offset..(offset + first).min(self.len()))?;
        }
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.split()?;
            }
            if line.is_empty() {
                continue;
            }
            let c = self.cursor;
            let old_len = self.len();
            let r = if let Some(t) = c.cell {
                self.core.insert_text_in_cell_native(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    c.offset,
                    line,
                )
            } else {
                self.core
                    .insert_text_native(c.section, c.paragraph, c.offset, line)
            };
            r.map_err(err)?;
            self.repair_trailing_number_stride(c, old_len);
            self.cursor.offset += line.chars().count();
        }
        self.anchor = None;
        Ok(())
    }
    fn delete(&mut self, back: bool) -> Result<()> {
        if self.has_selection() {
            return self.delete_selection();
        }
        let c = self.cursor;
        let length = self.len();
        if back && c.offset > 0 {
            return self.delete_range(c.offset - 1..c.offset);
        }
        if !back && c.offset < length {
            return self.delete_range(c.offset..c.offset + 1);
        }
        let n = if back {
            c.cell
                .map(|t| t.paragraph)
                .unwrap_or(c.paragraph)
                .checked_sub(1)
        } else {
            Some(c.cell.map(|t| t.paragraph).unwrap_or(c.paragraph))
        };
        if let Some(n) = n {
            let prev = if let Some(t) = c.cell {
                Cursor {
                    cell: Some(Cell { paragraph: n, ..t }),
                    ..c
                }
            } else {
                Cursor { paragraph: n, ..c }
            };
            let next = if let Some(t) = c.cell {
                Cursor {
                    cell: Some(Cell {
                        paragraph: n + 1,
                        ..t
                    }),
                    ..c
                }
            } else {
                Cursor {
                    paragraph: n + 1,
                    ..c
                }
            };
            let len = self.paragraph(prev)?.text.chars().count();
            if self.paragraph(next).is_ok() {
                if let Some(t) = c.cell {
                    self.core
                        .merge_paragraph_in_cell_native(
                            c.section,
                            c.paragraph,
                            t.control,
                            t.index,
                            n + 1,
                        )
                        .map_err(err)?;
                } else {
                    self.core
                        .merge_paragraph_native(c.section, n + 1)
                        .map_err(err)?;
                }
                self.cursor = Cursor {
                    offset: len,
                    ..prev
                };
            }
        }
        Ok(())
    }
    /// Hangul refuses edits inside a protected table cell (셀 보호).
    pub(crate) fn guard_protected(&self) -> Result<()> {
        if self.cell_protected(self.cursor) {
            bail!("보호된 셀은 고칠 수 없습니다.");
        }
        Ok(())
    }
    pub fn execute(&mut self, command: Command) -> Result<()> {
        if !matches!(
            command,
            Command::Undo | Command::Redo | Command::PageSetup(_)
        ) {
            self.guard_protected()?;
        }
        if let Command::Insert(text) = &command
            && self.coalesce_typing(text)
        {
            // Continue the previous undo step without cloning the document again.
            self.insert(text)?;
            self.typing = Some(self.cursor);
            self.typing_chars += text.chars().count();
            self.revision = self.next_revision;
            self.next_revision += 1;
            self.generation += 1;
            return Ok(());
        }
        let typed = match &command {
            Command::Insert(t) if !t.contains(['\n', '\r']) && !self.has_selection() => {
                Some(t.chars().count())
            }
            _ => None,
        };
        let result = match command {
            Command::Undo => self.history(false),
            Command::Redo => self.history(true),
            command => self.transaction(|s| {
                s.paragraph(s.cursor)?;
                if s.cursor.offset > s.len() {
                    bail!("커서가 문단 범위를 벗어났습니다.");
                }
                let c = s.cursor;
                match command {
                    Command::Insert(t) => s.insert(&t)?,
                    Command::Backspace => s.delete(true)?,
                    Command::Delete => s.delete(false)?,
                    Command::Split => {
                        s.delete_selection()?;
                        s.split()?;
                    }
                    Command::PageBreak => {
                        if c.cell.is_some() {
                            bail!("쪽 나누기는 본문에서만 사용할 수 있습니다.");
                        }
                        s.delete_selection()?;
                        let c = s.cursor;
                        let offset = s.split_offset(c)?;
                        s.core
                            .insert_page_break_native(c.section, c.paragraph, offset)
                            .map_err(err)?;
                        s.cursor.paragraph += 1;
                        s.cursor.offset = 0;
                        s.anchor = None;
                    }
                    Command::PageSetup(setup) => {
                        setup.validate()?;
                        s.core
                            .set_page_def_native(c.section, &setup.properties().to_string())
                            .map_err(err)?;
                    }
                    Command::Character(v) => {
                        let props = s.character_parameters(v)?.to_string();
                        let parts = s.selected_paragraphs()?;
                        // No selection formats the whole caret paragraph, as before.
                        let whole = parts.len() == 1 && parts[0].1.is_empty();
                        for (c, r) in parts {
                            let r = if whole {
                                0..s.paragraph(c)?.text.chars().count()
                            } else {
                                r
                            };
                            let result = if let Some(t) = c.cell {
                                s.core.apply_char_format_in_cell_native(
                                    c.section,
                                    c.paragraph,
                                    t.control,
                                    t.index,
                                    t.paragraph,
                                    r.start,
                                    r.end,
                                    &props,
                                )
                            } else {
                                s.core.apply_char_format_native(
                                    c.section,
                                    c.paragraph,
                                    r.start,
                                    r.end,
                                    &props,
                                )
                            };
                            result.map_err(err)?;
                        }
                    }
                    Command::Paragraph(v) => {
                        let props = s.paragraph_parameters(v)?.to_string();
                        for (c, _) in s.selected_paragraphs()? {
                            let result = if let Some(t) = c.cell {
                                s.core.apply_para_format_in_cell_native(
                                    c.section,
                                    c.paragraph,
                                    t.control,
                                    t.index,
                                    t.paragraph,
                                    &props,
                                )
                            } else {
                                s.core
                                    .apply_para_format_native(c.section, c.paragraph, &props)
                            };
                            result.map_err(err)?;
                        }
                    }
                    Command::Table { rows, columns } => {
                        if c.cell.is_some() {
                            bail!("중첩 표 만들기는 아직 지원하지 않습니다.");
                        }
                        if rows == 0 || columns == 0 || rows > 30 || columns > 12 {
                            bail!("표는 1–30행, 1–12열로 만들 수 있습니다.");
                        }
                        s.core
                            .create_table_native(c.section, c.paragraph, c.offset, rows, columns)
                            .map_err(err)?;
                    }
                    Command::Undo | Command::Redo => {}
                }
                Ok(())
            }),
        };
        if result.is_ok()
            && let Some(n) = typed
        {
            self.typing = Some(self.cursor);
            self.typing_chars = n;
        }
        result
    }
    /// Typing continues one undo step while the caret stays where the last typing
    /// left it, up to a word-sized group, like Hangul's per-word undo.
    fn coalesce_typing(&self, text: &str) -> bool {
        self.typing == Some(self.cursor)
            && !self.has_selection()
            && !self.overwrite
            && !text.is_empty()
            && !text.contains(['\n', '\r'])
            && text.chars().count() <= 8
            && self.typing_chars < 40
            && !self.undo.is_empty()
            && !text.starts_with(char::is_whitespace)
    }
    /// A minimal scalar-range diff preserves unaffected style runs and inline controls.
    pub fn replace_paragraph_text(&mut self, new: &str) -> Result<()> {
        let old: Vec<char> = self.text().chars().collect();
        let new: Vec<char> = new.chars().collect();
        if old == new {
            return Ok(());
        }
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let insert: String = new[prefix..new.len() - suffix].iter().collect();
        self.guard_protected()?;
        self.transaction(|s| {
            s.clear_selection();
            s.delete_range(prefix..old.len() - suffix)?;
            s.cursor.offset = prefix;
            s.insert(&insert)
        })
    }
    pub fn find_next(&mut self, query: &str) -> Result<bool> {
        if query.is_empty() {
            return Ok(false);
        }
        let entries = self.cursors();
        let current = entries
            .iter()
            .position(|c| {
                c.section == self.cursor.section
                    && c.paragraph == self.cursor.paragraph
                    && c.cell == self.cursor.cell
            })
            .unwrap_or(0);
        for step in 0..=entries.len() {
            let Some(c) = entries.get((current + step) % entries.len().max(1)) else {
                break;
            };
            let text = &self.paragraph(*c)?.text;
            let skip = if step == 0 { self.cursor.offset } else { 0 };
            let tail: String = text.chars().skip(skip).collect();
            if let Some(byte) = tail.find(query) {
                let start = skip + tail[..byte].chars().count();
                self.clear_selection();
                self.cursor = Cursor {
                    offset: start + query.chars().count(),
                    ..*c
                };
                self.anchor = Some(start);
                self.typing = None;
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn page_setup(&self) -> Result<PageSetup> {
        let section = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        Ok(PageSetup::from_page(&section.section_def.page_def))
    }

    pub fn character_properties(&self) -> Result<Value> {
        let c = self.cursor;
        let offset = self.selection().start.min(self.len().saturating_sub(1));
        let raw = if let Some(t) = c.cell {
            self.core.get_cell_char_properties_at_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
                offset,
            )
        } else {
            self.core
                .get_char_properties_at_native(c.section, c.paragraph, offset)
        };
        Ok(serde_json::from_str(&raw.map_err(err)?)?)
    }

    pub fn paragraph_properties(&self) -> Result<Value> {
        let c = self.cursor;
        let raw = if let Some(t) = c.cell {
            self.core.get_cell_para_properties_at_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
            )
        } else {
            self.core
                .get_para_properties_at_native(c.section, c.paragraph)
        };
        Ok(serde_json::from_str(&raw.map_err(err)?)?)
    }

    /// Literal, non-overlapping replacements in body and first-level table paragraphs.
    /// Plan first; apply backwards so later addresses remain valid. One undo restores all edits.
    pub fn replace_all(&mut self, query: &str, replacement: &str) -> Result<usize> {
        if query.is_empty() {
            bail!("찾을 내용을 입력하세요.");
        }
        if query.contains(['\r', '\n']) || replacement.contains(['\r', '\n']) {
            bail!("모두 바꾸기는 문단 안의 한 줄 텍스트만 지원합니다.");
        }
        if query.len() > 1_000_000 || replacement.len() > 1_000_000 {
            bail!("찾기와 바꾸기 텍스트는 각각 1 MB 이하입니다.");
        }
        if query == replacement {
            return Ok(0);
        }
        let mut matches = Vec::new();
        let mut work_bytes = 0usize;
        let length = query.chars().count();
        for cursor in self.cursors() {
            if self.cell_protected(cursor) {
                continue;
            }
            let text = &self.paragraph(cursor)?.text;
            let mut previous_byte = 0;
            let mut offset = 0;
            for (byte, _) in text.match_indices(query) {
                offset += text[previous_byte..byte].chars().count();
                previous_byte = byte;
                matches.push((cursor, offset));
                work_bytes = work_bytes
                    .saturating_add(query.len())
                    .saturating_add(replacement.len());
                if matches.len() > 10_000 || work_bytes > 8_000_000 {
                    bail!("한 번의 모두 바꾸기는 10,000건 / 처리 텍스트 8 MB 이하입니다.");
                }
            }
        }
        let count = matches.len();
        if count == 0 {
            return Ok(0);
        }
        self.transaction(|s| {
            for (cursor, offset) in matches.into_iter().rev() {
                s.clear_selection();
                s.cursor = Cursor {
                    offset: offset + length,
                    ..cursor
                };
                s.anchor = Some(offset);
                s.insert(replacement)?;
            }
            Ok(())
        })?;
        Ok(count)
    }
    pub fn info(&self) -> Value {
        json!({"pages":self.core.page_count(),"sections":self.core.document().sections.len(),"paragraphs":self.paragraph_total(),"dirty":self.dirty(),"path":self.path})
    }
}
