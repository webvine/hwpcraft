//! Owner: format area. 상호 참조 (C47): the page of a bookmark, heading, table or
//! picture, or a table's caption number or caption, written as a field that
//! `tools.cross_reference.update` keeps current.
//!
//! The field is Hangul's own form. Its command follows Hancom's public ActionCrossRef
//! table, `?<target>;<object kind>;<content>;<link>;<option>`, where the target is a
//! bookmark name or `#` and an object's instance id. Page references point at a
//! bookmark (headings, tables and pictures get one at their paragraph) and carry the
//! HWPX parameters found in a Hangul-saved document (RefType TARGET_BOOKMARK,
//! RefContentType OBJECT_TYPE_PAGE). Number and caption references point at the
//! table itself; Hangul 2024 shows "1" and "표 1 예산 집행" for them. Their HWPX
//! parameters beyond the command were not observed, so only the command is written.
//!
//! RHWP has no API for a new CROSSREF field, so one is made the way a hyperlink is
//! (same markers and text-range bookkeeping) and then given the CROSSREF identity;
//! an update swaps the identity back for RHWP's tested range-preserving text
//! replacement and restores it.
use super::*;
use hwpcraft_doc::HyperlinkTarget;
use hwpcraft_doc::model::control::{Field, FieldType, Parameter, ParameterList};
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "insert.cross_reference",
        "상호 참조 넣기 (책갈피·개요·표·그림이 있는 쪽 번호, 표 캡션 번호·캡션)",
        "",
    ),
    ("tools.cross_reference.update", "상호 참조 새로 고침", ""),
    ("tools.cross_reference.list", "상호 참조 목록", ""),
    (
        "tools.cross_reference.targets",
        "상호 참조 대상 목록 (개요·표·그림·책갈피)",
        "",
    ),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "insert.cross_reference",
        "본문 캐럿 위치에 대상이 있는 쪽 번호를 상호 참조 필드로 넣습니다. target: {bookmark:\"이름\"} | {heading:\"개요 제목 글자\"} | {table:n} | {picture:n} (n: 문서 순서의 n번째 캡션 달린 표·그림, 1부터). 개요·표·그림은 그 문단에 책갈피(이름 hwpcraft-ref-…)를 달아 가리킵니다. show: page(기본, 쪽 번호) | number(표 캡션 번호, 한글처럼 숫자만: \"표 \"는 앞에 직접 입력) | caption(표 캡션 전체, 예: \"표 1 예산 집행\"); number·caption은 target이 table일 때만이며 표를 고유 개체 id(#id)로 가리킵니다. 넣을 문단에는 표·그림·책갈피 같은 개체가 없어야 합니다. 결과: field_id, text, page 참조는 bookmark·page, number·caption은 table(개체 id).",
        r#"{"target":{"table":1},"show":"number"}"#,
    ),
    (
        "tools.cross_reference.update",
        "매개 변수 없음. 본문의 모든 상호 참조(쪽 번호, 표 캡션 번호·캡션)를 지금 값으로 고칩니다(한 번 실행 취소). 결과: references, updated, missing(대상이 없는 참조의 책갈피 이름 또는 #표 id), skipped(참조가 든 문단에 표·그림·책갈피가 생겨 고치지 못한 field_id).",
        r#"{}"#,
    ),
    (
        "tools.cross_reference.list",
        "매개 변수 없음. items[]: field_id, show(page|number|caption), text(지금 보이는 글), section, paragraph, 책갈피 참조는 bookmark·page(지금 계산한 쪽, 책갈피가 없으면 null), 표 참조는 table(개체 id)·expected(지금 보여야 할 글).",
        r#"{}"#,
    ),
    (
        "tools.cross_reference.targets",
        "매개 변수 없음. insert.cross_reference가 가리킬 수 있는 것: headings[{title, level, page}], tables[{n, caption, page}], pictures[{n, caption, page}](캡션 달린 표·그림, n은 target의 번호), bookmarks[{name, page}](hwpcraft-ref-… 자동 책갈피 제외). page는 1부터 센 물리 쪽입니다.",
        r#"{}"#,
    ),
];

const CROSS_REF_CTRL: u32 = u32::from_be_bytes(*b"%xrf");
/// 문서 요약 field (Hangul's 날짜 코드 `$createtime`, 마지막 저장 날짜 `$modifiedtime`).
pub(crate) const SUMMARY_CTRL: u32 = u32::from_be_bytes(*b"%smr");
/// RHWP accepts only web addresses for its hyperlink scaffolding; never stored.
pub(crate) const SCAFFOLD_URI: &str = "https://hwpcraft.invalid/cross-reference";
const PAGE_KIND: &str = "OBJECT_TYPE_PAGE";
/// Object kinds and contents of the command, from Hancom's public ActionCrossRef table.
const OBJ_TABLE: u8 = 0;
const OBJ_BOOKMARK: u8 = 6;
const SHOW_PAGE: u8 = 0;
const SHOW_NUMBER: u8 = 1;
const SHOW_CONTENT: u8 = 2;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Insert {
    target: Target,
    #[serde(default)]
    show: Show,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
enum Target {
    Bookmark(String),
    Heading(String),
    Table(usize),
    Picture(usize),
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Show {
    #[default]
    Page,
    Number,
    Caption,
}

/// What a reference points at.
#[derive(Clone, PartialEq)]
enum Pointee {
    Bookmark(String),
    /// An object by its instance id.
    Object(u32),
}

/// A cross reference this module can keep current: a bookmark's page, or a
/// table's page, caption number or caption.
struct Reference {
    section: usize,
    paragraph: usize,
    field_id: u32,
    pointee: Pointee,
    content: u8,
    text: String,
}

/// Parse a command `?<target>;<kind>;<content>;…` this module understands.
fn parse_command(field: &Field) -> Option<(Pointee, u8)> {
    if field.field_type != FieldType::CrossRef {
        return None;
    }
    let mut parts = field.command.strip_prefix('?')?.split(';');
    let target = parts.next()?;
    let kind: u8 = parts.next()?.parse().ok()?;
    let content: u8 = parts.next()?.parse().ok()?;
    match (target.strip_prefix('#'), kind, content) {
        (None, OBJ_BOOKMARK, SHOW_PAGE) => Some((Pointee::Bookmark(target.to_owned()), content)),
        (Some(id), OBJ_TABLE, SHOW_PAGE | SHOW_NUMBER | SHOW_CONTENT) => {
            Some((Pointee::Object(id.parse().ok()?), content))
        }
        _ => None,
    }
}

/// The automatic number written in a caption, as Hangul's number reference shows it.
fn caption_number(caption: &hwpcraft_doc::model::shape::Caption) -> Option<String> {
    caption
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::AutoNumber(n) => Some(
                if n.assigned_number > 0 {
                    n.assigned_number
                } else {
                    n.number
                }
                .to_string(),
            ),
            _ => None,
        })
}

/// A caption as shown: its text with the automatic numbers written in.
pub(crate) fn caption_text(caption: &hwpcraft_doc::model::shape::Caption) -> String {
    let mut out = Vec::new();
    for para in &caption.paragraphs {
        let positions = para.control_text_positions();
        let mut line = String::new();
        let mut chars = para.text.chars().enumerate().peekable();
        let mut controls = para.controls.iter().zip(positions).peekable();
        loop {
            let at = chars.peek().map_or(usize::MAX, |(i, _)| *i);
            if let Some((control, pos)) = controls.peek()
                && *pos <= at
            {
                if let Control::AutoNumber(n) = control {
                    let n = if n.assigned_number > 0 {
                        n.assigned_number
                    } else {
                        n.number
                    };
                    line.push_str(&n.to_string());
                }
                controls.next();
                continue;
            }
            match chars.next() {
                Some((_, c)) => line.push(if c.is_control() { ' ' } else { c }),
                None => break,
            }
        }
        out.push(line.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    out.join(" ").trim().to_owned()
}

pub(crate) fn string(name: &str, value: &str) -> Parameter {
    Parameter::String {
        name: Some(name.to_owned()),
        value: value.to_owned(),
        preserve_space: false,
    }
}

fn boolean(name: &str, value: bool, lexical: &str) -> Parameter {
    Parameter::Boolean {
        name: Some(name.to_owned()),
        value,
        lexical: Some(lexical.to_owned()),
    }
}

/// HWPX parameters of a table number or caption reference: only what was observed.
fn object_reference_parameters(command: &str) -> ParameterList {
    ParameterList {
        name: Some(String::new()),
        items: vec![
            boolean("Fiexde", true, "1"),
            Parameter::Integer {
                name: Some("Prop".into()),
                value: 0,
            },
            string("Command", command),
        ],
    }
}

/// Hangul's HWPX parameters of a bookmark page reference, in its order.
fn page_reference_parameters(bookmark: &str, command: &str) -> ParameterList {
    ParameterList {
        name: Some(String::new()),
        items: vec![
            boolean("Fiexde", true, "1"),
            Parameter::Integer {
                name: Some("Prop".into()),
                value: 0,
            },
            string("Command", command),
            string("RefPath", &format!("?{bookmark};")),
            string("RefType", "TARGET_BOOKMARK"),
            string("RefContentType", PAGE_KIND),
            boolean("RefHyperLink", false, "false"),
            string("RefOpenType", "HWPHYPERLINK_JUMP_CURRENTTAB"),
        ],
    }
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_cross_ref_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        match id {
            "insert.cross_reference" => Some(
                serde_json::from_value::<Insert>(params.clone())
                    .map_err(|e| anyhow!("insert.cross_reference: 매개 변수 오류 ({e})"))
                    .and_then(|p| self.insert_cross_reference(p)),
            ),
            "tools.cross_reference.update"
            | "tools.cross_reference.list"
            | "tools.cross_reference.targets" => {
                Some(if params.as_object().is_some_and(|p| !p.is_empty()) {
                    Err(anyhow!("{id}에는 매개 변수가 없습니다."))
                } else if id.ends_with("update") {
                    self.update_cross_references()
                } else if id.ends_with("targets") {
                    self.cross_reference_targets()
                } else {
                    Ok(json!({ "items": self.cross_references() }))
                })
            }
            _ => None,
        }
    }

    /// One-based physical page of a body position, as the table of contents counts.
    pub(crate) fn physical_page(
        &self,
        section: usize,
        paragraph: usize,
        offset: usize,
    ) -> Option<usize> {
        self.page_of(Cursor {
            section,
            paragraph,
            offset,
            cell: None,
        })
        .map(|p| p + 1)
    }

    /// Follow a link inside the document: `uri` is a link's address as listed
    /// (`?<bookmark>` for Hangul's bookmark links). Moves the caret to the bookmark
    /// and returns true; false for any other address.
    pub fn follow_internal_link(&mut self, uri: &str) -> Result<bool> {
        let Some(name) = uri
            .strip_prefix('?')
            .filter(|n| !n.is_empty() && !n.starts_with('#'))
        else {
            return Ok(false);
        };
        self.goto_bookmark(name)?;
        Ok(true)
    }

    /// Every bookmark by name: (section, paragraph, character), for link destinations.
    pub(crate) fn bookmark_positions(
        &self,
    ) -> std::collections::HashMap<String, (usize, usize, usize)> {
        self.bookmarks()
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|b| {
                let n = |k: &str| b[k].as_u64().map(|v| v as usize);
                Some((
                    b["name"].as_str()?.to_owned(),
                    (n("sec")?, n("para")?, n("charPos")?),
                ))
            })
            .collect()
    }

    /// Where a link inside the document leads in `core`'s layout: the 0-based page
    /// and the top left of the target's line in CSS px. `command` is a bookmark
    /// link's address (`?name`) or a cross reference's command (`?name;…` for a
    /// bookmark, `?#id;…` for a table by instance id).
    pub(crate) fn internal_link_destination(
        &self,
        core: &hwpcraft_doc::DocumentCore,
        bookmarks: &std::collections::HashMap<String, (usize, usize, usize)>,
        command: &str,
    ) -> Option<(u32, f64, f64)> {
        let address = hwpcraft_doc::model::hyperlink::command_uri(command);
        let target = address.strip_prefix('?')?;
        let (section, paragraph, offset) = match target.strip_prefix('#') {
            Some(id) => {
                let (s, p, _) = self.table_by_instance(id.parse().ok()?)?;
                (s, p, 0)
            }
            None => *bookmarks.get(target)?,
        };
        let rect: Value = serde_json::from_str(
            &core
                .get_cursor_rect_native(section, paragraph, offset)
                .ok()?,
        )
        .ok()?;
        let (x, y) = (rect["x"].as_f64()?, rect["y"].as_f64()?);
        (x.is_finite() && y.is_finite()).then_some((rect["pageIndex"].as_u64()? as u32, x, y))
    }

    /// Does the body or a table cell hold a bookmark link or a cross reference?
    pub(crate) fn has_internal_links(&self) -> bool {
        fn any(paragraphs: &[hwpcraft_doc::model::paragraph::Paragraph]) -> bool {
            paragraphs.iter().any(|p| {
                p.controls.iter().any(|c| match c {
                    Control::Field(f) => {
                        f.field_type == FieldType::CrossRef
                            || (f.field_type == FieldType::Hyperlink
                                && hwpcraft_doc::model::hyperlink::command_uri(&f.command)
                                    .starts_with('?'))
                    }
                    Control::Table(t) => t.cells.iter().any(|cell| any(&cell.paragraphs)),
                    _ => false,
                })
            })
        }
        self.core
            .document()
            .sections
            .iter()
            .any(|s| any(&s.paragraphs))
    }

    /// Why a cross reference cannot go at the caret, or `None` when it can.
    pub fn cross_reference_blocked(&self) -> Option<&'static str> {
        if self.cursor.cell.is_some() {
            return Some("상호 참조는 본문 문단에 넣습니다. 표 셀 밖에 커서를 놓으세요.");
        }
        if self.has_selection() {
            return Some("선택을 해제하고 상호 참조를 넣을 위치에 커서를 놓으세요.");
        }
        // RHWP edits field ranges only in paragraphs whose controls are fields.
        let here = self.paragraph(self.cursor).ok()?;
        here.controls
            .iter()
            .any(|c| {
                !matches!(
                    c,
                    Control::SectionDef(_) | Control::ColumnDef(_) | Control::Field(_)
                )
            })
            .then_some(
                "상호 참조는 표·그림·책갈피 같은 개체가 없는 본문 문단에 넣습니다. 새 문단에 넣어 주세요.",
            )
    }

    /// Everything `insert.cross_reference` can point at, for a picker.
    pub fn cross_reference_targets(&self) -> Result<Value> {
        let headings: Vec<Value> = self
            .outline_headings(10)?
            .iter()
            .map(
                |h| json!({"title": super::toc::clean(&h.title), "level": h.level, "page": h.page}),
            )
            .collect();
        let mut tables = Vec::new();
        let mut pictures = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, para) in section.paragraphs.iter().enumerate() {
                let page = self.physical_page(s, p, 0);
                // Counted per paragraph, the way a `{table: n}` target is found.
                if let Some(caption) = para.controls.iter().find_map(|c| match c {
                    Control::Table(t) => t.caption.as_ref(),
                    _ => None,
                }) {
                    tables.push(json!({"n": tables.len() + 1, "caption": caption_text(caption), "page": page}));
                }
                if let Some(caption) = para.controls.iter().find_map(|c| match c {
                    Control::Picture(pic) => pic.caption.as_ref(),
                    _ => None,
                }) {
                    pictures.push(json!({"n": pictures.len() + 1, "caption": caption_text(caption), "page": page}));
                }
            }
        }
        let pages = self.bookmark_pages()?;
        let bookmarks: Vec<Value> = self
            .bookmarks()?
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| b["name"].as_str())
            .filter(|name| !name.starts_with("hwpcraft-ref-"))
            .map(|name| json!({"name": name, "page": pages.get(name)}))
            .collect();
        Ok(
            json!({"headings": headings, "tables": tables, "pictures": pictures, "bookmarks": bookmarks}),
        )
    }

    /// (section, paragraph) of the target, or of the bookmark when it already exists.
    fn cross_reference_target(&self, target: &Target) -> Result<(String, usize, usize, bool)> {
        let doc = self.core.document();
        let body = || {
            doc.sections.iter().enumerate().flat_map(|(s, section)| {
                section
                    .paragraphs
                    .iter()
                    .enumerate()
                    .map(move |(p, para)| (s, p, para))
            })
        };
        let nth = |kind: &str, n: usize, matches: &dyn Fn(&Control) -> bool| {
            if n == 0 {
                bail!("{kind} 번호는 1부터 셉니다.");
            }
            body()
                .filter(|(_, _, para)| para.controls.iter().any(matches))
                .nth(n - 1)
                .map(|(s, p, _)| (s, p))
                .ok_or_else(|| anyhow!("캡션 달린 {kind} {n}을(를) 찾지 못했습니다."))
        };
        let (name, section, paragraph) = match target {
            Target::Bookmark(name) => {
                let found = self.bookmarks()?;
                let b = found
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|b| b["name"] == name.as_str())
                    .ok_or_else(|| anyhow!("책갈피 '{name}'이(가) 없습니다."))?;
                let n = |k: &str| b[k].as_u64().unwrap_or(0) as usize;
                return Ok((name.clone(), n("sec"), n("para"), true));
            }
            Target::Heading(title) => {
                let title = title.trim();
                if title.is_empty() {
                    bail!("가리킬 개요 제목 글자를 적으세요.");
                }
                let (s, p, _) = body()
                    .find(|(_, _, para)| para.text.trim() == title)
                    .or_else(|| body().find(|(_, _, para)| para.text.contains(title)))
                    .ok_or_else(|| anyhow!("제목 '{title}'을(를) 본문에서 찾지 못했습니다."))?;
                (format!("hwpcraft-ref-heading-{s}-{p}"), s, p)
            }
            Target::Table(n) => {
                let (s, p) = nth(
                    "표",
                    *n,
                    &|c| matches!(c, Control::Table(t) if t.caption.is_some()),
                )?;
                (format!("hwpcraft-ref-table-{n}"), s, p)
            }
            Target::Picture(n) => {
                let (s, p) = nth(
                    "그림",
                    *n,
                    &|c| matches!(c, Control::Picture(pic) if pic.caption.is_some()),
                )?;
                (format!("hwpcraft-ref-picture-{n}"), s, p)
            }
        };
        let exists = self
            .bookmarks()?
            .as_array()
            .into_iter()
            .flatten()
            .any(|b| b["name"] == name.as_str());
        Ok((name, section, paragraph, exists))
    }

    /// (section, paragraph, control) of the n-th captioned table, as `{table: n}` counts.
    fn captioned_table(&self, n: usize) -> Result<(usize, usize, usize)> {
        if n == 0 {
            bail!("표 번호는 1부터 셉니다.");
        }
        self.core
            .document()
            .sections
            .iter()
            .enumerate()
            .flat_map(|(s, section)| {
                section
                    .paragraphs
                    .iter()
                    .enumerate()
                    .filter_map(move |(p, para)| {
                        para.controls
                            .iter()
                            .position(|c| matches!(c, Control::Table(t) if t.caption.is_some()))
                            .map(|c| (s, p, c))
                    })
            })
            .nth(n - 1)
            .ok_or_else(|| anyhow!("캡션 달린 표 {n}을(를) 찾지 못했습니다."))
    }

    /// The table with this instance id: (section, paragraph, its caption).
    fn table_by_instance(
        &self,
        id: u32,
    ) -> Option<(usize, usize, &hwpcraft_doc::model::shape::Caption)> {
        self.core
            .document()
            .sections
            .iter()
            .enumerate()
            .find_map(|(s, section)| {
                section.paragraphs.iter().enumerate().find_map(|(p, para)| {
                    para.controls.iter().find_map(|c| match c {
                        Control::Table(t) if t.common.instance_id == id => {
                            t.caption.as_ref().map(|caption| (s, p, caption))
                        }
                        _ => None,
                    })
                })
            })
    }

    /// Give the table a nonzero instance id no other object in the document has,
    /// keeping the one it has when it already is.
    fn table_instance_id(&mut self, section: usize, paragraph: usize, control: usize) -> u32 {
        let mut used = std::collections::HashMap::<u32, usize>::new();
        for para in self
            .core
            .document()
            .sections
            .iter()
            .flat_map(|s| &s.paragraphs)
        {
            for c in &para.controls {
                let id = match c {
                    Control::Table(t) => t.common.instance_id,
                    Control::Picture(p) => p.common.instance_id,
                    Control::Equation(e) => e.common.instance_id,
                    _ => continue,
                };
                *used.entry(id).or_default() += 1;
            }
        }
        let doc = self.core.document_mut();
        let Some(Control::Table(table)) = doc.sections[section].paragraphs[paragraph]
            .controls
            .get_mut(control)
        else {
            return 0;
        };
        let current = table.common.instance_id;
        if current != 0 && used.get(&current) == Some(&1) {
            return current;
        }
        let id = used.keys().max().map_or(1, |m| m.saturating_add(1)).max(1);
        table.common.instance_id = id;
        // HWP writes a table's common attributes from these bytes when it has them.
        if table.raw_ctrl_data.len() >= 36 {
            table.raw_ctrl_data[32..36].copy_from_slice(&id.to_le_bytes());
        }
        doc.sections[section].raw_stream = None;
        id
    }

    fn insert_cross_reference(&mut self, p: Insert) -> Result<Value> {
        self.guard_protected()?;
        if let Some(why) = self.cross_reference_blocked() {
            bail!(why);
        }
        if p.show != Show::Page {
            let Target::Table(n) = p.target else {
                bail!(
                    "번호·캡션 참조는 지금 표만 가리킵니다. 그림·개요·책갈피는 쪽 번호(show: page)로 넣으세요."
                );
            };
            return self.insert_table_reference(n, p.show);
        }
        let (bookmark, section, paragraph, exists) = self.cross_reference_target(&p.target)?;
        let mut out = Value::Null;
        self.transaction(|s| {
            if !exists {
                let added: Value = serde_json::from_str(
                    &s.core
                        .add_bookmark_native(section, paragraph, 0, &bookmark)
                        .map_err(err)?,
                )?;
                if added["ok"] == false {
                    bail!(
                        "{}",
                        added["error"]
                            .as_str()
                            .unwrap_or("책갈피를 달지 못했습니다.")
                    );
                }
                s.refresh_layout()?;
            }
            let page = s
                .physical_page(section, paragraph, 0)
                .ok_or_else(|| anyhow!("대상의 쪽을 찾지 못했습니다."))?;
            let text = page.to_string();
            let command = format!("?{bookmark};{OBJ_BOOKMARK};{SHOW_PAGE};0;0;");
            let parameters = page_reference_parameters(&bookmark, &command);
            let field_id = s.write_cross_reference(&text, &command, parameters)?;
            out = json!({"field_id": field_id, "bookmark": bookmark, "page": page, "text": text});
            Ok(())
        })?;
        Ok(out)
    }

    /// A table's caption number or caption at the caret, pointing at the table itself.
    fn insert_table_reference(&mut self, n: usize, show: Show) -> Result<Value> {
        let (section, paragraph, control) = self.captioned_table(n)?;
        let mut out = Value::Null;
        self.transaction(|s| {
            let id = s.table_instance_id(section, paragraph, control);
            let (_, _, caption) = s
                .table_by_instance(id)
                .ok_or_else(|| anyhow!("표 {n}의 캡션을 찾지 못했습니다."))?;
            let (content, text) = if show == Show::Number {
                let number = caption_number(caption)
                    .ok_or_else(|| anyhow!("표 {n}의 캡션에 번호가 없습니다."))?;
                (SHOW_NUMBER, number)
            } else {
                (SHOW_CONTENT, caption_text(caption))
            };
            if text.is_empty() {
                bail!("표 {n}의 캡션이 비어 있습니다.");
            }
            let command = format!("?#{id};{OBJ_TABLE};{content};0;0");
            let field_id =
                s.write_cross_reference(&text, &command, object_reference_parameters(&command))?;
            out = json!({"field_id": field_id, "table": id, "text": text});
            Ok(())
        })?;
        Ok(out)
    }

    /// Insert `text` at the caret as a CROSSREF field with this command; the caret ends
    /// after it. Runs inside the caller's transaction.
    fn write_cross_reference(
        &mut self,
        text: &str,
        command: &str,
        parameters: ParameterList,
    ) -> Result<u32> {
        self.write_text_field(text, |f| {
            f.field_type = FieldType::CrossRef;
            f.ctrl_id = CROSS_REF_CTRL;
            f.parameters = parameters;
            f.command = command.to_owned();
        })
    }

    /// Insert `text` at the caret as a field that `identity` turns into its kind
    /// (cross reference, date field); the caret ends after it. RHWP has no API for
    /// these fields, so the range is made the way a hyperlink is and then given its
    /// identity. Runs inside the caller's transaction.
    pub(crate) fn write_text_field(
        &mut self,
        text: &str,
        identity: impl FnOnce(&mut Field),
    ) -> Result<u32> {
        let c = self.cursor;
        // As links, neighbouring generated fields keep the new text out of their range.
        let saved = self.text_fields_as_links(c.section, c.paragraph);
        let inserted = self
            .core
            .insert_text_native(c.section, c.paragraph, c.offset, text)
            .map_err(err);
        let target = HyperlinkTarget::body(c.section, c.paragraph);
        let len = text.chars().count();
        let field_id = inserted.and_then(|_| {
            self.core
                .insert_hyperlink_native(&target, c.offset, c.offset + len, SCAFFOLD_URI)
                .map_err(err)
        });
        self.restore_field_types(c.section, c.paragraph, &saved);
        let field_id = field_id?;
        self.set_field_identity(c.section, c.paragraph, field_id, |f| {
            identity(f);
            f.hyperlink_format = None;
            f.raw_parameters_xml = None;
        })?;
        self.cursor.offset = c.offset + len;
        Ok(field_id)
    }

    /// Change one body field in place (RHWP has no API for it) and mark the section.
    pub(crate) fn set_field_identity(
        &mut self,
        section: usize,
        paragraph: usize,
        field_id: u32,
        change: impl FnOnce(&mut Field),
    ) -> Result<()> {
        let doc = self.core.document_mut();
        let field = doc.sections[section].paragraphs[paragraph]
            .controls
            .iter_mut()
            .find_map(|c| match c {
                Control::Field(f) if f.field_id == field_id => Some(f),
                _ => None,
            })
            .ok_or_else(|| anyhow!("필드 {field_id}을(를) 찾지 못했습니다."))?;
        change(field);
        doc.sections[section].raw_stream = None;
        Ok(())
    }

    /// Switch the paragraph's cross references between their own type and the
    /// hyperlink type RHWP's link editing understands.
    /// Turn the paragraph's generated text fields (cross references, date fields) into
    /// the hyperlink type RHWP's link editing understands; returns what to restore.
    pub(crate) fn text_fields_as_links(
        &mut self,
        section: usize,
        paragraph: usize,
    ) -> Vec<(u32, FieldType)> {
        let doc = self.core.document_mut();
        let mut saved = Vec::new();
        for c in &mut doc.sections[section].paragraphs[paragraph].controls {
            if let Control::Field(f) = c
                && (f.ctrl_id == CROSS_REF_CTRL || f.ctrl_id == SUMMARY_CTRL)
            {
                saved.push((f.field_id, f.field_type));
                f.field_type = FieldType::Hyperlink;
            }
        }
        doc.sections[section].raw_stream = None;
        saved
    }

    pub(crate) fn restore_field_types(
        &mut self,
        section: usize,
        paragraph: usize,
        saved: &[(u32, FieldType)],
    ) {
        let doc = self.core.document_mut();
        for c in &mut doc.sections[section].paragraphs[paragraph].controls {
            if let Control::Field(f) = c
                && let Some((_, kind)) = saved.iter().find(|(id, _)| *id == f.field_id)
            {
                f.field_type = *kind;
            }
        }
        doc.sections[section].raw_stream = None;
    }

    /// Body cross references this module understands, in document order.
    fn page_references(&self) -> Vec<Reference> {
        let mut out = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, para) in section.paragraphs.iter().enumerate() {
                for range in &para.field_ranges {
                    let Some(Control::Field(field)) = para.controls.get(range.control_idx) else {
                        continue;
                    };
                    let Some((pointee, content)) = parse_command(field) else {
                        continue;
                    };
                    let text: String = para
                        .text
                        .chars()
                        .skip(range.start_char_idx)
                        .take(range.end_char_idx.saturating_sub(range.start_char_idx))
                        .collect();
                    out.push(Reference {
                        section: s,
                        paragraph: p,
                        field_id: field.field_id,
                        pointee,
                        content,
                        text,
                    });
                }
            }
        }
        out
    }

    /// What the reference should show now, or `None` when its target is gone.
    fn reference_value(
        &self,
        r: &Reference,
        pages: &std::collections::HashMap<String, usize>,
    ) -> Option<String> {
        match &r.pointee {
            Pointee::Bookmark(name) => pages.get(name).map(ToString::to_string),
            Pointee::Object(id) => {
                let (s, p, caption) = self.table_by_instance(*id)?;
                match r.content {
                    SHOW_NUMBER => caption_number(caption),
                    SHOW_CONTENT => Some(caption_text(caption)).filter(|t| !t.is_empty()),
                    _ => self.physical_page(s, p, 0).map(|n| n.to_string()),
                }
            }
        }
    }

    fn bookmark_pages(&self) -> Result<std::collections::HashMap<String, usize>> {
        let mut pages = std::collections::HashMap::new();
        for b in self.bookmarks()?.as_array().into_iter().flatten() {
            let n = |k: &str| b[k].as_u64().unwrap_or(0) as usize;
            if let (Some(name), Some(page)) = (
                b["name"].as_str(),
                self.physical_page(n("sec"), n("para"), n("charPos")),
            ) {
                pages.insert(name.to_owned(), page);
            }
        }
        Ok(pages)
    }

    pub fn cross_references(&self) -> Vec<Value> {
        let pages = self.bookmark_pages().unwrap_or_default();
        self.page_references()
            .iter()
            .map(|r| {
                let show = ["page", "number", "caption"][usize::from(r.content).min(2)];
                let mut item = json!({"field_id": r.field_id, "text": r.text, "show": show,
                       "section": r.section, "paragraph": r.paragraph});
                match &r.pointee {
                    Pointee::Bookmark(name) => {
                        item["bookmark"] = json!(name);
                        item["page"] = json!(pages.get(name));
                    }
                    Pointee::Object(id) => {
                        item["table"] = json!(id);
                        item["expected"] = json!(self.reference_value(r, &pages));
                    }
                }
                item
            })
            .collect()
    }

    fn update_cross_references(&mut self) -> Result<Value> {
        self.guard_protected()?;
        let references = self.page_references();
        let total = references.len();
        let mut updated = 0;
        let mut missing = Vec::new();
        let mut skipped = Vec::new();
        self.transaction(|s| {
            // A changed number can move later pages; repeat until the numbers settle.
            for _ in 0..3 {
                let pages = s.bookmark_pages()?;
                let mut changed = false;
                for r in s.page_references() {
                    let Some(value) = s.reference_value(&r, &pages) else {
                        let name = match &r.pointee {
                            Pointee::Bookmark(name) => name.clone(),
                            Pointee::Object(id) => format!("#{id}"),
                        };
                        if !missing.contains(&name) {
                            missing.push(name);
                        }
                        continue;
                    };
                    if value == r.text {
                        continue;
                    }
                    let (sec, para, id) = (r.section, r.paragraph, r.field_id);
                    s.set_field_identity(sec, para, id, |f| f.field_type = FieldType::Hyperlink)?;
                    let target = HyperlinkTarget::body(sec, para);
                    let result = s.core.replace_hyperlink_text_native(&target, id, &value);
                    s.set_field_identity(sec, para, id, |f| f.field_type = FieldType::CrossRef)?;
                    // A paragraph that gained an object since cannot be edited this way.
                    if result.is_err() {
                        if !skipped.contains(&id) {
                            skipped.push(id);
                        }
                        continue;
                    }
                    updated += 1;
                    changed = true;
                }
                if !changed {
                    break;
                }
            }
            s.cursor.offset = s.cursor.offset.min(s.len());
            Ok(())
        })?;
        Ok(json!({"references": total, "updated": updated, "missing": missing, "skipped": skipped}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Result<Session> {
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text": "참조 문단: 표는 쪽에 있습니다.\n"}),
        )?;
        s.run("insert.page_break", json!({}))?;
        s.run("text.insert", json!({"text": "제2장 결과\n"}))?;
        s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
        let (p, c) = s.core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(p, para)| {
                para.controls
                    .iter()
                    .position(|c| matches!(c, Control::Table(_)))
                    .map(|c| (p, c))
            })
            .ok_or_else(|| anyhow!("table"))?;
        s.cursor = Cursor {
            section: 0,
            paragraph: p,
            offset: 0,
            cell: Some(Cell {
                control: c,
                index: 0,
                paragraph: 0,
            }),
        };
        s.run("table.properties", json!({"props": {"hasCaption": true}}))?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 6,
            cell: None,
        };
        Ok(s)
    }

    #[test]
    fn page_reference_to_a_table_follows_it_and_survives_both_formats() -> Result<()> {
        let mut s = report()?;
        let made = s.run("insert.cross_reference", json!({"target": {"table": 1}}))?;
        assert_eq!(made["page"], 2, "{made}");
        assert_eq!(made["text"], "2");
        let targets = s.run("tools.cross_reference.targets", json!({}))?;
        assert_eq!(targets["tables"][0]["n"], 1, "{targets}");
        assert_eq!(targets["tables"][0]["page"], 2, "{targets}");
        assert_eq!(targets["tables"][0]["caption"], "표 1", "{targets}");
        assert_eq!(targets["bookmarks"], json!([]), "{targets}");
        let list = s.run("tools.cross_reference.list", json!({}))?;
        assert_eq!(list["items"][0]["bookmark"], "hwpcraft-ref-table-1");
        assert_eq!(list["items"][0]["text"], "2");
        // Push the table one page further and refresh.
        s.cursor = Cursor {
            section: 0,
            paragraph: 1,
            offset: 0,
            cell: None,
        };
        s.run("insert.page_break", json!({}))?;
        let refreshed = s.run("tools.cross_reference.update", json!({}))?;
        assert_eq!(refreshed["updated"], 1, "{refreshed}");
        assert_eq!(s.cross_references()[0]["text"], "3");
        assert_eq!(s.cross_references()[0]["page"], 3);
        let dir = tempfile::tempdir()?;
        for ext in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("xref.{ext}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let items = reopened.cross_references();
            assert_eq!(items.len(), 1, "{ext}: {items:?}");
            assert_eq!(items[0]["bookmark"], "hwpcraft-ref-table-1", "{ext}");
            assert_eq!(items[0]["text"], "3", "{ext}");
            assert_eq!(items[0]["page"], 3, "{ext}");
        }
        // One undo removes the whole update.
        s.run("edit.undo", json!({}))?;
        assert_eq!(s.cross_references()[0]["text"], "2");
        Ok(())
    }

    #[test]
    fn heading_and_bookmark_targets_and_refusals() -> Result<()> {
        let mut s = report()?;
        let made = s.run(
            "insert.cross_reference",
            json!({"target": {"heading": "제2장 결과"}}),
        )?;
        assert_eq!(made["page"], 2);
        // A second reference in the same paragraph, to a bookmark set elsewhere.
        let at = s.cursor;
        let elsewhere = |paragraph| Cursor {
            section: 0,
            paragraph,
            offset: 0,
            cell: None,
        };
        let heading = s.core.document().sections[0]
            .paragraphs
            .iter()
            .position(|p| p.text.starts_with("제2장"))
            .ok_or_else(|| anyhow!("heading"))?;
        s.cursor = elsewhere(heading);
        s.run("bookmark.add", json!({"name": "요약"}))?;
        s.cursor = at;
        let made = s.run(
            "insert.cross_reference",
            json!({"target": {"bookmark": "요약"}}),
        )?;
        assert_eq!(made["page"], 2);
        assert_eq!(s.cross_references().len(), 2);
        // Not into a paragraph that holds an object (here the bookmark).
        s.cursor = elsewhere(heading);
        assert!(
            s.run("insert.cross_reference", json!({"target": {"table": 1}}))
                .is_err()
        );
        s.cursor = at;
        assert!(
            s.run(
                "insert.cross_reference",
                json!({"target": {"bookmark": "없음"}})
            )
            .is_err()
        );
        assert!(
            s.run("insert.cross_reference", json!({"target": {"picture": 1}}))
                .is_err()
        );
        assert!(
            s.run("insert.cross_reference", json!({"target": {"table": 0}}))
                .is_err()
        );
        assert!(
            s.run("tools.cross_reference.update", json!({"x": 1}))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn text_typed_right_after_a_reference_stays_outside_it() -> Result<()> {
        let mut s = report()?;
        s.run("insert.cross_reference", json!({"target": {"table": 1}}))?;
        s.run("text.insert", json!({"text": "쪽"}))?;
        let refs = s.cross_references();
        assert_eq!(refs[0]["text"], "2", "{refs:?}");
        let dir = tempfile::tempdir()?;
        for ext in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("typed.{ext}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let refs = reopened.cross_references();
            assert_eq!(refs.len(), 1, "{ext}: {refs:?}");
            assert_eq!(refs[0]["text"], "2", "{ext}: {refs:?}");
            assert!(
                reopened.core.document().sections[0].paragraphs[0]
                    .text
                    .contains("2쪽")
            );
        }
        // Updating keeps the typed text.
        s.run("tools.cross_reference.update", json!({}))?;
        assert!(
            s.core.document().sections[0].paragraphs[0]
                .text
                .contains("2쪽")
        );
        Ok(())
    }

    fn table_ids(s: &Session) -> Vec<u32> {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter_map(|c| match c {
                Control::Table(t) => Some(t.common.instance_id),
                _ => None,
            })
            .collect()
    }

    fn set_caption(s: &mut Session, text: &str) -> Result<()> {
        let at = s.cursor;
        let (p, c) = s.core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(p, para)| {
                para.controls
                    .iter()
                    .position(|c| matches!(c, Control::Table(t) if t.caption.is_some()))
                    .map(|c| (p, c))
            })
            .ok_or_else(|| anyhow!("table"))?;
        s.cursor = Cursor {
            section: 0,
            paragraph: p,
            offset: 0,
            cell: Some(Cell {
                control: c,
                index: 0,
                paragraph: 0,
            }),
        };
        s.run("table.caption_text", json!({ "text": text }))?;
        s.cursor = at;
        Ok(())
    }

    /// Number and caption references as Hangul 2024 writes and shows them: the
    /// command `?#<table id>;0;1;0;0` shows "1", `?#<table id>;0;2;0;0` the caption.
    #[test]
    fn table_number_and_caption_references_follow_the_table() -> Result<()> {
        let mut s = report()?;
        set_caption(&mut s, "예산 집행")?;
        // A second, uncaptioned table holding the same instance id as the first.
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 0,
            cell: None,
        };
        s.run("caret.move", json!({"motion": "document_end"}))?;
        s.run("text.insert", json!({"text": "\n"}))?;
        s.run("insert.table", json!({"rows": 1, "columns": 1}))?;
        let doc = s.core.document_mut();
        for c in doc.sections[0]
            .paragraphs
            .iter_mut()
            .flat_map(|p| &mut p.controls)
        {
            if let Control::Table(t) = c {
                t.common.instance_id = 5;
            }
        }
        doc.sections[0].raw_stream = None;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 6,
            cell: None,
        };
        let before = table_ids(&s);
        s.run("text.insert", json!({"text": "표 "}))?;
        let number = s.run(
            "insert.cross_reference",
            json!({"target": {"table": 1}, "show": "number"}),
        )?;
        assert_eq!(number["text"], "1", "{number}");
        let id = number["table"]
            .as_u64()
            .ok_or_else(|| anyhow!("{number}"))? as u32;
        assert!(id != 0 && id != 5, "{number}");
        let ids = table_ids(&s);
        assert_eq!(ids.iter().filter(|i| **i == id).count(), 1, "{ids:?}");
        s.run("text.insert", json!({"text": " 참조, "}))?;
        let caption = s.run(
            "insert.cross_reference",
            json!({"target": {"table": 1}, "show": "caption"}),
        )?;
        assert_eq!(caption["text"], "표 1 예산 집행", "{caption}");
        assert_eq!(caption["table"], id);
        let commands = |s: &Session| -> Vec<(String, String)> {
            let para = &s.core.document().sections[0].paragraphs[0];
            para.field_ranges
                .iter()
                .filter_map(|r| match para.controls.get(r.control_idx) {
                    Some(Control::Field(f)) if f.field_type == FieldType::CrossRef => Some((
                        f.command.clone(),
                        para.text
                            .chars()
                            .skip(r.start_char_idx)
                            .take(r.end_char_idx - r.start_char_idx)
                            .collect(),
                    )),
                    _ => None,
                })
                .collect()
        };
        let expected = vec![
            (format!("?#{id};0;1;0;0"), "1".to_owned()),
            (format!("?#{id};0;2;0;0"), "표 1 예산 집행".to_owned()),
        ];
        assert_eq!(commands(&s), expected);
        assert!(
            s.core.document().sections[0].paragraphs[0]
                .text
                .contains("표 1 참조, 표 1 예산 집행")
        );
        // Both formats keep the commands, the shown text and the table they point at.
        let dir = tempfile::tempdir()?;
        for ext in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("number.{ext}"));
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            assert_eq!(commands(&reopened), expected, "{ext}");
            let items = reopened.cross_references();
            assert_eq!(items.len(), 2, "{ext}: {items:?}");
            for item in &items {
                assert_eq!(item["table"], id, "{ext}: {item}");
                assert_eq!(item["expected"], item["text"], "{ext}: {item}");
            }
        }
        // A new caption reaches the caption reference on update; the number stays.
        set_caption(&mut s, "결산")?;
        let refreshed = s.run("tools.cross_reference.update", json!({}))?;
        assert_eq!(refreshed["updated"], 1, "{refreshed}");
        assert_eq!(
            commands(&s)[1],
            (format!("?#{id};0;2;0;0"), "표 1 결산".to_owned())
        );
        s.run("edit.undo", json!({}))?;
        s.run("edit.undo", json!({}))?;
        // One undo takes the caption reference away, one more the text before it.
        s.run("edit.undo", json!({}))?;
        s.run("edit.undo", json!({}))?;
        assert_eq!(commands(&s).len(), 1);
        // The number reference and the id it gave the table go together.
        s.run("edit.undo", json!({}))?;
        assert!(commands(&s).is_empty());
        assert_eq!(table_ids(&s), before);
        // Only tables, and only captioned ones.
        assert!(
            s.run(
                "insert.cross_reference",
                json!({"target": {"picture": 1}, "show": "number"})
            )
            .is_err()
        );
        assert!(
            s.run(
                "insert.cross_reference",
                json!({"target": {"table": 2}, "show": "caption"})
            )
            .is_err()
        );
        Ok(())
    }
}
