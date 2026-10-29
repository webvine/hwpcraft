//! Owner: core area. Hangul-style caret, selection, clipboard, word/line
//! deletion, special characters, auto text, navigation and document statistics
//! as shared engine commands, so the CLI, MCP and desktop behave alike.
//! Behaviour follows what Hangul does for the same actions (e.g. `DeleteWord`
//! removes the rest of the word and the spaces after it).
use super::*;
use hwpcraft_doc::HyperlinkTarget;
use hwpcraft_doc::model::paragraph::Paragraph as DocParagraph;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "caret.move",
        "캐럿 이동 (글자·낱말·줄·문단·쪽·리스트·문서, 선택 확장)",
        "방향키",
    ),
    ("select.all", "모두 선택", "Ctrl+A"),
    ("select.word", "낱말 선택", ""),
    ("select.paragraph", "문단 선택", ""),
    ("select.line", "줄 선택", ""),
    ("select.clear", "선택 해제", "Esc"),
    ("select.get", "선택 영역 조회", ""),
    ("edit.copy", "복사하기", "Ctrl+C"),
    ("edit.cut", "오려 두기", "Ctrl+X"),
    ("edit.paste", "붙이기 (내부 클립보드·글·HTML)", "Ctrl+V"),
    ("edit.clipboard.get", "내부 클립보드 조회", ""),
    (
        "text.delete_word",
        "한 단어 지우기 (캐럿부터 낱말 끝까지)",
        "Ctrl+T",
    ),
    ("text.delete_word_back", "낱말 앞 지우기", "Ctrl+Backspace"),
    ("text.delete_line", "한 줄 지우기", "Ctrl+Y"),
    (
        "text.delete_to_line_end",
        "이후 지우기 (줄 끝까지)",
        "Alt+Y",
    ),
    ("insert.line_break", "줄 나눔 (문단 안)", "Shift+Enter"),
    ("insert.tab", "탭", "Tab"),
    ("insert.nbsp", "묶음 빈 칸", "Ctrl+Alt+Space"),
    ("insert.fixed_width_space", "고정폭 빈 칸", "Alt+Space"),
    ("insert.soft_hyphen", "하이픈", "Ctrl+Shift+-"),
    (
        "insert.auto_text",
        "상용구: 날짜·시각·쪽 번호·전체 쪽수·파일 이름·사용자 이름",
        "",
    ),
    ("edit.overwrite", "삽입/수정 모드", "Insert"),
    ("edit.goto", "이동 (쪽·구역·문단·책갈피)", "Alt+G"),
    ("hyperlink.jump", "캐럿의 하이퍼링크 주소", ""),
    ("hyperlink.next", "하이퍼링크 다음", "Ctrl+Q,R"),
    ("hyperlink.prev", "하이퍼링크 이전", ""),
    ("edit.find_all", "모두 찾기 (일치 위치 목록)", ""),
    (
        "document.statistics",
        "문서 통계 (글자·낱말·줄·문단·쪽·개체 수)",
        "",
    ),
    ("document.outline", "개요 탐색 목록", ""),
];

/// (command id, parameter help, JSON example) for `list_commands` and docs.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "caret.move",
        "motion: left|right|word_left|word_right|word_start|word_end|line_start|line_end|up|down|page_up|page_down|page_start|page_end|paragraph_start|paragraph_end|prev_paragraph_start|prev_paragraph_end|next_paragraph_start|list_start|list_end|document_start|document_end; extend keeps the anchor (Shift). Covers Hangul Move*/MoveSel*.",
        r#"{"motion":"word_right","extend":false}"#,
    ),
    (
        "select.all",
        "No parameters; whole cell or the caret section's body.",
        "{}",
    ),
    (
        "select.word",
        "No parameters; the word (or run of spaces/punctuation) at the caret.",
        "{}",
    ),
    ("select.paragraph", "No parameters.", "{}"),
    (
        "select.line",
        "No parameters; the laid-out line under the caret.",
        "{}",
    ),
    ("select.clear", "No parameters.", "{}"),
    (
        "select.get",
        "No parameters; cursor, anchor, spanAnchor, hasSelection, text.",
        "{}",
    ),
    (
        "edit.copy",
        "No parameters; copies the selection to the internal clipboard (formats, tables, objects) and returns its text.",
        "{}",
    ),
    (
        "edit.cut",
        "No parameters; copy plus one undoable deletion.",
        "{}",
    ),
    (
        "edit.paste",
        "Without arguments pastes the internal clipboard. text pastes plain text (or, when it equals the last copy, the rich internal copy); html pastes with formatting; mode text_only forces plain text.",
        r#"{"text":"붙일 글"}"#,
    ),
    (
        "edit.clipboard.get",
        "No parameters; text, hasControl (table/picture/shape), internal.",
        "{}",
    ),
    (
        "text.delete_word",
        "No parameters; deletes from the caret to the next word start (Hangul Ctrl+T).",
        "{}",
    ),
    (
        "text.delete_word_back",
        "No parameters; deletes back to the word start (Ctrl+Backspace).",
        "{}",
    ),
    (
        "text.delete_line",
        "No parameters; deletes the whole caret line, the paragraph when it is one line (Ctrl+Y).",
        "{}",
    ),
    (
        "text.delete_to_line_end",
        "No parameters; deletes from the caret to the end of its line (Alt+Y).",
        "{}",
    ),
    (
        "insert.line_break",
        "No parameters; a line break inside the paragraph (saved as HWPX lineBreak).",
        "{}",
    ),
    ("insert.tab", "No parameters.", "{}"),
    ("insert.nbsp", "No parameters; U+00A0 묶음 빈 칸.", "{}"),
    (
        "insert.fixed_width_space",
        "No parameters; U+2007 고정폭 빈 칸.",
        "{}",
    ),
    ("insert.soft_hyphen", "No parameters; U+00AD.", "{}"),
    (
        "insert.auto_text",
        "kind: date|time|page_number|total_pages|page_of_total|file_name|user_name. date/time accept an optional strftime format (default 2026년 10월 25일 토요일 / 오후 3:04). Page fields are live auto-number controls in body text.",
        r#"{"kind":"date","format":"%Y-%m-%d"}"#,
    ),
    (
        "edit.overwrite",
        "Optional value boolean; omit to toggle. Returns overwrite.",
        r#"{"value":true}"#,
    ),
    (
        "edit.goto",
        "Exactly one of page (1-based), section (0-based), paragraph (0-based body paragraph of the caret section), bookmark (name).",
        r#"{"page":2}"#,
    ),
    (
        "hyperlink.jump",
        "No parameters; uri/text/fieldId of the link under the caret, for the caller to open.",
        "{}",
    ),
    (
        "hyperlink.next",
        "No parameters; selects the next link after the caret (no wrap). found false when none.",
        "{}",
    ),
    (
        "hyperlink.prev",
        "No parameters; selects the previous link before the caret.",
        "{}",
    ),
    (
        "edit.find_all",
        "query (one line), case_sensitive (default false), include_cells (default true), limit 1–10000 (default 1000). Returns matches with cursor/length; nested cells carry cellPath only.",
        r#"{"query":"한글","case_sensitive":false}"#,
    ),
    (
        "document.statistics",
        "No parameters; characters, charactersNoSpaces, words, lines, paragraphs, pages, sections, tables, pictures, shapes, equations, footnotes, endnotes, memos.",
        "{}",
    ),
    (
        "document.outline",
        "No parameters; outline paragraphs with level, number, title, page, section, paragraph.",
        "{}",
    ),
];

fn no_params(id: &str, v: &Value) -> Result<()> {
    if v.as_object().is_some_and(|o| !o.is_empty()) {
        bail!("{id} 명령에는 매개 변수가 없습니다.");
    }
    Ok(())
}
fn parse<T: serde::de::DeserializeOwned>(id: &str, v: &Value) -> Result<T> {
    serde_json::from_value(v.clone()).map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    motion: Motion,
    #[serde(default)]
    extend: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Paste {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Toggle {
    #[serde(default)]
    value: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoText {
    kind: String,
    #[serde(default)]
    format: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Goto {
    #[serde(default)]
    page: Option<usize>,
    #[serde(default)]
    section: Option<usize>,
    #[serde(default)]
    paragraph: Option<usize>,
    #[serde(default)]
    bookmark: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindAll {
    query: String,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default = "yes")]
    include_cells: bool,
    #[serde(default = "thousand")]
    limit: usize,
}
fn yes() -> bool {
    true
}
fn thousand() -> usize {
    1000
}

/// Counts for `document.statistics`.
#[derive(Default)]
struct Stats {
    characters: usize,
    characters_no_spaces: usize,
    words: usize,
    lines: usize,
    paragraphs: usize,
    tables: usize,
    pictures: usize,
    shapes: usize,
    equations: usize,
    footnotes: usize,
    endnotes: usize,
    memos: usize,
}
impl Stats {
    fn paragraph(&mut self, p: &DocParagraph) {
        self.paragraphs += 1;
        self.lines += p.line_segs.len().max(1);
        for ch in p.text.chars() {
            if ch == '\n' {
                continue;
            }
            self.characters += 1;
            if !ch.is_whitespace() {
                self.characters_no_spaces += 1;
            }
        }
        self.words += p.text.split_whitespace().count();
        for control in &p.controls {
            match control {
                Control::Table(t) => {
                    self.tables += 1;
                    for cell in &t.cells {
                        for cp in &cell.paragraphs {
                            self.paragraph(cp);
                        }
                    }
                }
                Control::Picture(_) => self.pictures += 1,
                Control::Shape(_) => self.shapes += 1,
                Control::Equation(_) => self.equations += 1,
                Control::Footnote(_) => self.footnotes += 1,
                Control::Endnote(_) => self.endnotes += 1,
                Control::HiddenComment(_) => self.memos += 1,
                _ => {}
            }
        }
    }
}

impl Session {
    pub(crate) fn run_caret_ext(&mut self, id: &str, p: &Value) -> Option<Result<Value>> {
        Some(match id {
            "caret.move" => parse::<Move>(id, p).and_then(|m| {
                let before = self.cursor;
                self.move_caret(m.motion, m.extend)?;
                Ok(json!({"cursor":self.cursor,"anchor":self.anchor,"spanAnchor":self.span_anchor,"moved":self.cursor != before}))
            }),
            "select.all" | "select.word" | "select.paragraph" | "select.line" | "select.clear"
            | "select.get" => no_params(id, p).and_then(|()| {
                match id {
                    "select.all" => self.select_all()?,
                    "select.word" => self.select_word()?,
                    "select.paragraph" => self.select_paragraph(),
                    "select.line" => self.select_line()?,
                    "select.clear" => self.clear_selection(),
                    _ => {}
                }
                Ok(self.selection_json())
            }),
            "edit.copy" => no_params(id, p).and_then(|()| Ok(json!({"text": self.copy()?}))),
            "edit.cut" => no_params(id, p).and_then(|()| Ok(json!({"text": self.cut()?}))),
            "edit.paste" => parse::<Paste>(id, p).and_then(|a| {
                self.paste_command(a)?;
                Ok(self.info())
            }),
            "edit.clipboard.get" => no_params(id, p).map(|()| {
                json!({"text": self.clipboard_text.clone().unwrap_or_default(),
                    "hasControl": self.core.clipboard_has_control_native(),
                    "internal": self.core.has_internal_clipboard_native()})
            }),
            "text.delete_word" | "text.delete_word_back" | "text.delete_line"
            | "text.delete_to_line_end" => no_params(id, p).and_then(|()| {
                match id {
                    "text.delete_word" => self.delete_word(false)?,
                    "text.delete_word_back" => self.delete_word(true)?,
                    "text.delete_line" => self.delete_line(false)?,
                    _ => self.delete_line(true)?,
                }
                Ok(self.info())
            }),
            "insert.line_break" | "insert.tab" | "insert.nbsp" | "insert.fixed_width_space"
            | "insert.soft_hyphen" => no_params(id, p).and_then(|()| {
                let ch = match id {
                    "insert.line_break" => '\n',
                    "insert.tab" => '\t',
                    "insert.nbsp" => '\u{00A0}',
                    "insert.fixed_width_space" => '\u{2007}',
                    _ => '\u{00AD}',
                };
                self.insert_special(ch)?;
                Ok(self.info())
            }),
            "insert.auto_text" => parse::<AutoText>(id, p).and_then(|a| {
                let inserted = self.insert_auto_text(&a.kind, a.format.as_deref())?;
                Ok(json!({"inserted": inserted, "cursor": self.cursor}))
            }),
            "edit.overwrite" => parse::<Toggle>(id, p).map(|t| {
                self.overwrite = t.value.unwrap_or(!self.overwrite);
                json!({"overwrite": self.overwrite})
            }),
            "edit.goto" => parse::<Goto>(id, p).and_then(|g| {
                self.goto(g)?;
                Ok(json!({"cursor": self.cursor, "page": self.page_of(self.cursor)}))
            }),
            "hyperlink.jump" => no_params(id, p).and_then(|()| {
                let link = self
                    .hyperlink_at_caret()?
                    .ok_or_else(|| anyhow!("캐럿 위치에 하이퍼링크가 없습니다."))?;
                Ok(json!({"uri": link.uri, "text": link.text, "fieldId": link.field_id}))
            }),
            "hyperlink.next" | "hyperlink.prev" => {
                no_params(id, p).and_then(|()| self.hyperlink_step(id == "hyperlink.next"))
            }
            "edit.find_all" => parse::<FindAll>(id, p).and_then(|f| self.find_all(f)),
            "document.statistics" => no_params(id, p).map(|()| self.statistics()),
            "document.outline" => no_params(id, p).and_then(|()| {
                let raw = self.core.get_outline_navigation_native().map_err(err)?;
                Ok(serde_json::from_str(&raw)?)
            }),
            _ => return None,
        })
    }

    pub fn selection_json(&self) -> Value {
        json!({"cursor":self.cursor,"anchor":self.anchor,"spanAnchor":self.span_anchor,
            "hasSelection":self.has_selection(),"text":self.selected_text()})
    }
    /// Select the laid-out line under the caret.
    pub fn select_line(&mut self) -> Result<()> {
        let start = self.motion_target(Motion::LineStart)?;
        let end = self.motion_target(Motion::LineEnd)?;
        self.clear_selection();
        self.anchor = Some(start.offset);
        self.cursor.offset = end.offset;
        Ok(())
    }
    fn paste_command(&mut self, a: Paste) -> Result<()> {
        let text_only = match a.mode.as_deref() {
            None | Some("default") => false,
            Some("text_only") => true,
            Some(other) => bail!("알 수 없는 붙이기 방식: {other}"),
        };
        if text_only {
            let text = match a.text {
                Some(t) => t,
                None => self
                    .clipboard_text
                    .clone()
                    .ok_or_else(|| anyhow!("클립보드가 비어 있습니다."))?,
            };
            return self.execute(Command::Insert(text));
        }
        match (a.text, a.html) {
            (text, Some(html)) => self.paste_with_html(&text.unwrap_or_default(), Some(&html)),
            (Some(text), None) => self.paste(&text),
            (None, None) => {
                let text = self
                    .clipboard_text
                    .clone()
                    .ok_or_else(|| anyhow!("클립보드가 비어 있습니다."))?;
                self.paste(&text)
            }
        }
    }
    /// Hangul `DeleteWord` / `DeleteWordBack`: a selection is deleted as is;
    /// otherwise the text up to the next (or back to the current) word start goes.
    fn delete_word(&mut self, back: bool) -> Result<()> {
        self.guard_protected()?;
        if self.has_selection() {
            return self.transaction(|s| s.delete_selection());
        }
        let c = self.cursor;
        let target = self.motion_target(if back {
            Motion::WordLeft
        } else {
            Motion::WordRight
        })?;
        let same =
            target.section == c.section && target.paragraph == c.paragraph && target.cell == c.cell;
        if !same || target.offset == c.offset {
            // At a paragraph edge the word deletion merges like Backspace/Delete.
            return self.execute(if back {
                Command::Backspace
            } else {
                Command::Delete
            });
        }
        let range = target.offset.min(c.offset)..target.offset.max(c.offset);
        self.transaction(|s| s.delete_range(range))
    }
    /// Hangul `DeleteLine` (whole caret line, the paragraph when it is one line,
    /// including its break) and `DeleteLineCurEnd` (caret to end of line).
    fn delete_line(&mut self, to_end: bool) -> Result<()> {
        self.guard_protected()?;
        let c = self.cursor;
        let len = self.len();
        let start = self.motion_target(Motion::LineStart)?.offset;
        let end = self.motion_target(Motion::LineEnd)?.offset;
        if to_end {
            if c.offset >= end {
                return Ok(());
            }
            return self.transaction(|s| s.delete_range(c.offset..end));
        }
        if start == 0 && end >= len {
            let next = self.motion_target(Motion::NextParagraphStart)?;
            let count_after = next != c || next.offset != len;
            return self.transaction(|s| {
                s.clear_selection();
                if count_after && next.offset == 0 {
                    // Remove the paragraph by joining it onto the next one.
                    s.cursor = Cursor { offset: 0, ..c };
                    s.span_anchor = None;
                    s.select(Cursor { offset: 0, ..c }, false)?;
                    s.select(next, true)?;
                    s.delete_selection()
                } else if len == 0 {
                    // Last, empty paragraph: nothing but the break, handled by Backspace.
                    s.delete(true)
                } else {
                    s.delete_range(0..len)?;
                    // Last paragraph of its list: drop the break before it too.
                    s.delete(true)
                }
            });
        }
        self.transaction(|s| s.delete_range(start..end))
    }
    /// Insert one special character (line break, tab, spaces, soft hyphen) at the
    /// caret through RHWP, replacing the selection; one undo step.
    fn insert_special(&mut self, ch: char) -> Result<()> {
        self.guard_protected()?;
        self.transaction(|s| {
            s.delete_selection()?;
            let ticket = s.insertion_shape.capture_insert(s)?;
            let c = s.cursor;
            let text = ch.to_string();
            if let Some(t) = c.cell {
                s.core.insert_text_in_cell_native(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    c.offset,
                    &text,
                )
            } else {
                s.core
                    .insert_text_native(c.section, c.paragraph, c.offset, &text)
            }
            .map_err(err)?;
            ticket.apply_range(&mut s.core, c, c.offset..c.offset + 1)?;
            s.cursor.offset += 1;
            s.anchor = None;
            ticket.finish(&mut s.insertion_shape, s.cursor);
            Ok(())
        })
    }
    /// Hangul 상용구/자동 글: literal date, time, file and user names; live page
    /// number / total page controls in body text.
    fn insert_auto_text(&mut self, kind: &str, format: Option<&str>) -> Result<String> {
        let literal = match kind {
            "date" => Some(now_text(format.unwrap_or("%Y년 %m월 %d일 %A"))),
            "time" => Some(now_text(format.unwrap_or("%p %I:%M"))),
            "file_name" => Some(
                self.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "제목 없음".to_owned()),
            ),
            "user_name" => Some(
                std::env::var("USERNAME")
                    .or_else(|_| std::env::var("USER"))
                    .unwrap_or_default(),
            ),
            "page_number" | "total_pages" | "page_of_total" => None,
            other => bail!("알 수 없는 상용구 종류: {other}"),
        };
        if let Some(text) = literal {
            if text.is_empty() {
                bail!("넣을 내용이 없습니다.");
            }
            self.execute(Command::Insert(text.clone()))?;
            return Ok(text);
        }
        if self.cursor.cell.is_some() {
            bail!("쪽 번호 필드는 본문에서만 넣을 수 있습니다.");
        }
        self.guard_protected()?;
        let kinds: &[&str] = match kind {
            "page_number" => &["page"],
            "total_pages" => &["total"],
            _ => &["page", "/", "total"],
        };
        self.transaction(|s| {
            s.delete_selection()?;
            for k in kinds {
                if *k == "/" {
                    s.insert("/")?;
                    continue;
                }
                let c = s.cursor;
                s.insert_native_auto_number(c, k)?;
                s.cursor.offset += 1;
                s.anchor = None;
            }
            s.relayout_after_marker()?;
            Ok(())
        })?;
        Ok(kind.to_owned())
    }
    /// Insert one saved native control while keeping the source control order.
    /// Native trailing-number positions fall back to text.len(), even though
    /// their placeholder is the preceding scalar. Resolve that boundary before
    /// computing the new index, then repair both old and new slot strides.
    pub(crate) fn insert_native_auto_number(&mut self, c: Cursor, kind: &str) -> Result<usize> {
        let before = self.paragraph(c)?.clone();
        let positions = before.control_text_positions();
        let len = before.text.chars().count();
        let actual: Vec<_> = before
            .controls
            .iter()
            .zip(&positions)
            .enumerate()
            .map(|(ci, (ctrl, pos))| {
                if matches!(ctrl, Control::Field(_)) {
                    before
                        .field_ranges
                        .iter()
                        .find(|r| r.control_idx == ci)
                        .map_or(*pos, |r| r.start_char_idx)
                } else if matches!(ctrl, Control::AutoNumber(_))
                    && *pos == len
                    && len > 0
                    && before.text.ends_with(' ')
                {
                    len - 1
                } else {
                    *pos
                }
            })
            .collect();
        let native_index = positions.iter().filter(|p| **p < c.offset).count();
        let index = before
            .controls
            .iter()
            .zip(&actual)
            .filter(|(ctrl, pos)| {
                **pos < c.offset
                    || **pos == c.offset
                        && matches!(ctrl, Control::SectionDef(_) | Control::ColumnDef(_))
            })
            .count();
        let (list, stream) = self.root_stream_position(c)?;
        let response = self
            .core
            .insert_auto_number_at_cursor(0, list, stream, kind)
            .map_err(err)?;
        let result: Value = serde_json::from_str(&response)?;
        if result["ok"] == false {
            bail!(
                "쪽 번호를 넣지 못했습니다: {}",
                result["reason"].as_str().unwrap_or("")
            );
        }
        let mut expected: Vec<char> = before.text.chars().collect();
        expected.insert(c.offset, ' ');
        let para = &mut self.core.document_mut().sections[c.section].paragraphs[c.paragraph];
        if para.text != expected.into_iter().collect::<String>()
            || para.controls.len() != before.controls.len() + 1
            || !matches!(
                para.controls.get(native_index),
                Some(Control::AutoNumber(_))
            )
        {
            bail!("자동 번호가 요청한 본문 위치에 삽입되지 않았습니다.");
        }
        let inserted = para.controls.remove(native_index);
        para.controls.insert(index, inserted);
        let mut data = before.ctrl_data_records.clone();
        if data.len() < before.controls.len() {
            data.resize(before.controls.len(), None);
        }
        data.insert(index, None);
        para.ctrl_data_records = data;
        if para.field_ranges.len() != before.field_ranges.len() {
            bail!("번호 삽입 중 필드 범위의 개수가 바뀌었습니다.");
        }
        // Text insertion updates scalar ranges, but the native control insert
        // does not update their control indices. Derive indices from the saved
        // source association so this is never a second shift of a native index.
        for (range, old) in para.field_ranges.iter_mut().zip(&before.field_ranges) {
            range.control_idx = old.control_idx + usize::from(old.control_idx >= index);
        }
        for (ctrl, pos) in before.controls.iter().zip(actual) {
            if matches!(ctrl, Control::AutoNumber(_)) {
                self.fix_number_stride(Cursor {
                    offset: pos + usize::from(pos >= c.offset),
                    ..c
                })?;
            }
        }
        self.fix_number_stride(c)?;
        self.relayout_after_marker()?;
        Ok(index)
    }
    /// RHWP leaves a plain space as the placeholder of a freshly inserted auto
    /// number but records a one-unit stride for it in `char_offsets`, while its
    /// position lookup and serializers expect the eight units of a control slot.
    /// Without the stride the control is only found by the end-of-paragraph
    /// fallback, so saves and later insertions move it. Restore the stride here.
    fn fix_number_stride(&mut self, c: Cursor) -> Result<()> {
        let para = self
            .core
            .document_mut()
            .sections
            .get_mut(c.section)
            .and_then(|s| s.paragraphs.get_mut(c.paragraph))
            .ok_or_else(|| anyhow!("문단이 없습니다."))?;
        let idx = c.offset;
        if para.text.chars().nth(idx) != Some(' ') {
            bail!("쪽 번호 자리표를 찾지 못했습니다.");
        }
        let offsets = &mut para.char_offsets;
        if idx + 1 < offsets.len() {
            let stride = offsets[idx + 1].saturating_sub(offsets[idx]);
            if stride < 8 {
                let add = 8 - stride;
                let boundary = offsets[idx + 1];
                for o in offsets.iter_mut().skip(idx + 1) {
                    *o += add;
                }
                for shape in &mut para.char_shapes {
                    if shape.start_pos >= boundary {
                        shape.start_pos += add;
                    }
                }
                for tag in &mut para.range_tags {
                    if tag.start >= boundary {
                        tag.start += add;
                    }
                    if tag.end >= boundary {
                        tag.end += add;
                    }
                }
                for mark in &mut para.markpen_marks {
                    if let Some(pos) = &mut mark.utf16_pos
                        && *pos >= boundary
                    {
                        *pos += add;
                    }
                }
                for line in &mut para.line_segs {
                    if line.text_start >= boundary {
                        line.text_start += add;
                    }
                }
                para.stored_text_partition_dirty = true;
            }
        }
        Ok(())
    }
    /// Text typed right after a trailing auto-number placeholder inherits a
    /// one-unit stride from RHWP's insertion; give the placeholder its slot back
    /// so the number stays before the new text. `old_len` is the paragraph length
    /// before the insertion.
    pub(crate) fn repair_trailing_number_stride(&mut self, c: Cursor, old_len: usize) {
        if c.offset != old_len || old_len == 0 || c.cell.is_some() {
            return;
        }
        let Some(para) = self
            .core
            .document()
            .sections
            .get(c.section)
            .and_then(|s| s.paragraphs.get(c.paragraph))
        else {
            return;
        };
        let number_controls = para
            .controls
            .iter()
            .filter(|k| matches!(k, Control::AutoNumber(_) | Control::NewNumber(_)))
            .count();
        if number_controls == 0 || para.text.chars().nth(old_len - 1) != Some(' ') {
            return;
        }
        let trailing = para
            .controls
            .iter()
            .zip(para.control_text_positions())
            .any(|(k, pos)| {
                matches!(k, Control::AutoNumber(_) | Control::NewNumber(_)) && pos >= old_len
            });
        if trailing {
            let _ = self.fix_number_stride(Cursor {
                offset: old_len - 1,
                ..c
            });
        }
    }
    /// Rebuild RHWP's layout caches after a direct model edit (snapshot round trip,
    /// like the format area does).
    fn relayout_after_marker(&mut self) -> Result<()> {
        let id = self.save_history_snapshot();
        let r = self.core.restore_snapshot_native(id).map_err(err);
        self.retire_history_snapshot(id);
        r.map(|_| ())
    }
    /// RHWP root-list paragraph index and stream position of a body caret, counted
    /// the way RHWP's field queries do: a tab is 8 units, an extended control 8
    /// (an auto number 7 beside its placeholder character), a field 8 per end.
    fn root_stream_position(&self, c: Cursor) -> Result<(usize, usize)> {
        let sections = &self.core.document().sections;
        let before: usize = sections
            .iter()
            .take(c.section)
            .map(|s| s.paragraphs.len())
            .sum();
        let p = self.paragraph(c)?;
        let idx = c.offset.min(p.text.chars().count());
        let positions = p.control_text_positions();
        let mut units = idx + p.text.chars().take(idx).filter(|ch| *ch == '\t').count() * 7;
        for (ci, ctrl) in p.controls.iter().enumerate() {
            if matches!(ctrl, Control::Field(_)) {
                continue;
            }
            if positions.get(ci).is_some_and(|pos| *pos <= idx) {
                units += match ctrl {
                    Control::AutoNumber(_) | Control::NewNumber(_) => 7,
                    _ => 8,
                };
            }
        }
        for range in &p.field_ranges {
            if range.start_char_idx <= idx {
                units += 8;
            }
            if range.end_char_idx <= idx {
                units += 8;
                if range.start_char_idx == range.end_char_idx {
                    units += match p.controls.get(range.control_idx) {
                        Some(Control::Field(f)) => {
                            f.guide_text().map(|g| g.chars().count()).unwrap_or(0)
                        }
                        _ => 0,
                    };
                }
            }
        }
        Ok((before + c.paragraph, units))
    }
    fn goto(&mut self, g: Goto) -> Result<()> {
        let given = [
            g.page.is_some(),
            g.section.is_some(),
            g.paragraph.is_some(),
            g.bookmark.is_some(),
        ]
        .iter()
        .filter(|x| **x)
        .count();
        if given != 1 {
            bail!("page, section, paragraph, bookmark 중 하나만 지정하세요.");
        }
        if let Some(name) = g.bookmark {
            return self.goto_bookmark(&name);
        }
        let target = if let Some(page) = g.page {
            let pages = self.page_count()?;
            if page == 0 || page > pages as usize {
                bail!("쪽 번호는 1–{pages} 사이여야 합니다.");
            }
            self.page_first_position(page - 1)
                .ok_or_else(|| anyhow!("{page}쪽의 위치를 찾지 못했습니다."))?
        } else if let Some(section) = g.section {
            Cursor {
                section,
                ..Default::default()
            }
        } else {
            Cursor {
                section: self.cursor.section,
                paragraph: g.paragraph.unwrap_or(0),
                offset: 0,
                cell: None,
            }
        };
        self.select(target, false)?;
        if let Some(page) = g.page {
            self.set_visual_hint(self.cursor, (page - 1) as u32, Affinity::Downstream);
        }
        Ok(())
    }
    /// Links of any body/cell paragraph, in text order.
    fn hyperlinks_at(&self, c: Cursor) -> Vec<hwpcraft_doc::HyperlinkInfo> {
        let target = HyperlinkTarget {
            section: c.section,
            para: c.paragraph,
            cell_path: c
                .cell
                .map(|t| vec![(t.control, t.index, t.paragraph)])
                .unwrap_or_default(),
        };
        self.core.hyperlinks_native(&target).unwrap_or_default()
    }
    /// Select the next/previous link relative to the caret, without wrapping.
    fn hyperlink_step(&mut self, forward: bool) -> Result<Value> {
        let entries = self.cursors();
        let here = entries
            .iter()
            .position(|c| {
                c.section == self.cursor.section
                    && c.paragraph == self.cursor.paragraph
                    && c.cell == self.cursor.cell
            })
            .unwrap_or(0);
        let caret = self.cursor.offset;
        let r = self.selection();
        let order: Vec<usize> = if forward {
            (here..entries.len()).collect()
        } else {
            (0..=here).rev().collect()
        };
        for i in order {
            let c = entries[i];
            let mut links = self.hyperlinks_at(c);
            if !forward {
                links.reverse();
            }
            for link in links {
                let ok = if i != here {
                    true
                } else if forward {
                    link.start >= r.end.max(caret) && link.start > r.start
                } else {
                    link.end <= r.start.min(caret) && link.end < r.end.max(caret)
                };
                if ok {
                    self.select(
                        Cursor {
                            offset: link.start,
                            ..c
                        },
                        false,
                    )?;
                    self.select(
                        Cursor {
                            offset: link.end,
                            ..c
                        },
                        true,
                    )?;
                    return Ok(
                        json!({"found": true, "cursor": self.cursor, "anchor": self.anchor,
                        "uri": link.uri, "text": link.text, "fieldId": link.field_id}),
                    );
                }
            }
        }
        Ok(json!({"found": false, "cursor": self.cursor}))
    }
    fn find_all(&self, f: FindAll) -> Result<Value> {
        if f.query.is_empty() || f.query.contains(['\r', '\n']) {
            bail!("찾을 내용은 비어 있지 않은 한 줄 문자열이어야 합니다.");
        }
        if f.query.len() > 1_000_000 {
            bail!("찾을 내용은 1 MB 이하입니다.");
        }
        if !(1..=10_000).contains(&f.limit) {
            bail!("limit은 1–10000입니다.");
        }
        let raw = self
            .core
            .search_all_text_native(&f.query, f.case_sensitive, f.include_cells)
            .map_err(err)?;
        let hits: Vec<Value> = serde_json::from_str(&raw)?;
        let total = hits.len();
        let length = f.query.chars().count();
        let matches: Vec<Value> = hits
            .into_iter()
            .take(f.limit)
            .map(|h| {
                let n = |v: &Value, k: &str| v[k].as_u64().map(|x| x as usize);
                let (section, paragraph, offset) = (
                    n(&h, "sec").unwrap_or(0),
                    n(&h, "para").unwrap_or(0),
                    n(&h, "charOffset").unwrap_or(0),
                );
                let ctx = &h["cellContext"];
                let cursor = if ctx.is_null() {
                    Some(Cursor {
                        section,
                        paragraph,
                        offset,
                        cell: None,
                    })
                } else if ctx.get("ctrlIdx").is_some() {
                    Some(Cursor {
                        section,
                        paragraph: n(ctx, "parentPara").unwrap_or(paragraph),
                        offset,
                        cell: Some(Cell {
                            control: n(ctx, "ctrlIdx").unwrap_or(0),
                            index: n(ctx, "cellIdx").unwrap_or(0),
                            paragraph: n(ctx, "cellPara").unwrap_or(0),
                        }),
                    })
                } else {
                    None
                };
                let mut m = json!({"length": n(&h, "length").unwrap_or(length)});
                match cursor {
                    Some(c) => m["cursor"] = json!(c),
                    None => {
                        m["nested"] = json!(true);
                        m["cellPath"] = ctx.clone();
                        m["section"] = json!(section);
                        m["paragraph"] = json!(paragraph);
                        m["offset"] = json!(offset);
                    }
                }
                m
            })
            .collect();
        Ok(json!({"total": total, "returned": matches.len(), "matches": matches}))
    }
    /// Hangul 문서 정보 → 문서 통계. Text counts include table cells at any depth.
    pub fn statistics(&self) -> Value {
        let pages = self.page_count();
        let mut st = Stats::default();
        let doc = self.core.document();
        let mut body_paragraphs = 0usize;
        for section in &doc.sections {
            for p in &section.paragraphs {
                body_paragraphs += 1;
                st.paragraph(p);
            }
        }
        json!({
            "characters": st.characters, "charactersNoSpaces": st.characters_no_spaces,
            "words": st.words, "lines": st.lines, "paragraphs": st.paragraphs,
            "bodyParagraphs": body_paragraphs, "pages": pages.as_ref().ok(),
            "layoutError": pages.as_ref().err().map(ToString::to_string),
            "sections": doc.sections.len(), "tables": st.tables, "pictures": st.pictures,
            "shapes": st.shapes, "equations": st.equations, "footnotes": st.footnotes,
            "endnotes": st.endnotes, "memos": st.memos,
        })
    }
}

/// Local date/time text with Korean weekday and AM/PM names.
fn now_text(format: &str) -> String {
    let now = chrono::Local::now();
    let weekday = [
        "월요일",
        "화요일",
        "수요일",
        "목요일",
        "금요일",
        "토요일",
        "일요일",
    ][now
        .format("%u")
        .to_string()
        .parse::<usize>()
        .unwrap_or(1)
        .clamp(1, 7)
        - 1];
    let ampm = if now.format("%p").to_string() == "AM" {
        "오전"
    } else {
        "오후"
    };
    // %A and %p are replaced before chrono formats the rest, so they stay Korean.
    let format = format.replace("%A", weekday).replace("%p", ampm);
    now.format(&format).to_string()
}
