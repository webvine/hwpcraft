//! Native range comments. The field and its content remain in the HWP/HWPX DOM.
//!
//! Memo anchors use the public range-field editing primitive; no URL or private
//! application sidecar is written. HWP5 MEMO_LIST content is restored at load
//! because the underlying reader currently leaves these lists in raw records.
use super::*;
use hwpcraft_doc::model::control::{Field, FieldType, Parameter, ParameterList};
use hwpcraft_doc::model::document::{DocInfo, Document};
use hwpcraft_doc::{HwpRecord, HyperlinkTarget, parse_hwp_paragraph_list};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("memo.list", "메모 목록", ""),
    ("memo.get", "메모 조회", ""),
    ("memo.create", "메모 만들기", ""),
    ("memo.update", "메모 내용·작성자 고치기", ""),
    ("memo.reply", "메모에 댓글 달기", ""),
    ("memo.delete", "메모 지우기 (본문 유지)", ""),
];

pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "memo.list",
        "매개 변수 없음. 메모 필드마다 id, author, content, anchor. 표 셀 메모도 나열합니다.",
        "{}",
    ),
    (
        "memo.get",
        "id: memo.create/list가 돌려준 필드 id. 선택은 바뀌지 않습니다.",
        r#"{"id":1}"#,
    ),
    (
        "memo.create",
        "text(1 MiB까지), author?(기본 사용자; 데스크톱의 개인 정보 이름은 여기 쓰이지 않음), expected_generation?. 지금 본문 한 문단 안의 선택, 또는 빈 캐럿 범위에 메모를 답니다. 범위 글은 바꾸지 않습니다. expected_generation이 낡았으면 바꾸기 전에 실패합니다.",
        r#"{"text":"내용을 확인해 주세요.","author":"검토자"}"#,
    ),
    (
        "memo.update",
        "id; text?와 author?로 메모 내용·작성자를 바꿉니다(하나는 있어야 함). expected_generation?은 memo.get/list가 준 값이며 낡은 초안은 바꾸지 않고 실패합니다. 한글 메모에는 해결 상태가 없어 제공하지 않습니다.",
        r#"{"id":1,"text":"수정한 메모"}"#,
    ),
    (
        "memo.reply",
        "id(메모 또는 그 스레드의 아무 댓글), text, author?(기본 사용자), expected_generation?. 한글이 저장하는 방식대로 댓글을 답니다: 같은 글에 단 또 하나의 메모를 스레드 안쪽에 중첩. memo.list의 댓글에는 replyTo(스레드 첫 메모)와 바깥에서 안쪽 순서의 thread가 있습니다. 빈 캐럿 범위의 메모에는 댓글을 달 수 없습니다. 한 번 실행 취소.",
        r#"{"id":1,"text":"반영했습니다.","author":"작성자"}"#,
    ),
    (
        "memo.delete",
        "id, expected_generation?. 메모 필드와 내용을 지우며, 스레드 첫 메모면 댓글도 모두 지웁니다(결과: removedIds). 범위 글과 서식은 그대로입니다. 한 번 실행 취소. 낡은 generation은 바꾸기 전에 실패합니다.",
        r#"{"id":1}"#,
    ),
];

const MEMO_SHAPE: u16 = 16 + 76;
const MEMO_LIST: u16 = 16 + 77;
const PARA_HEADER: u16 = 16 + 50;
const MAX_TEXT_BYTES: usize = 1_048_576;
const MAX_PARAGRAPHS: usize = 1_000;
const NO_SHAPE: u16 = u16::MAX;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    text: String,
    #[serde(default = "default_author")]
    author: String,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    id: u32,
    text: String,
    #[serde(default = "default_author")]
    author: String,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: u32,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Delete {
    id: u32,
    #[serde(default)]
    expected_generation: Option<u64>,
}

fn check_generation(session: &Session, expected: Option<u64>) -> Result<()> {
    if expected.is_some_and(|expected| expected != session.generation) {
        bail!("문서가 메모를 불러온 뒤 변경되었습니다. 메모를 다시 불러오세요.");
    }
    Ok(())
}

/// The creation-time tokens of a new memo's command (`MEMO/shape/n/<low>/<high>/…`),
/// as Hangul 2024 writes them: the FILETIME of now. Existing values are kept as read.
fn filetime_tokens() -> (u32, u32) {
    let since_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() / 100);
    let filetime = (since_unix + 116_444_736_000_000_000) as u64;
    (filetime as u32, (filetime >> 32) as u32)
}

fn default_author() -> String {
    "사용자".into()
}
fn is_memo(field: &Field) -> bool {
    field.field_type == FieldType::Memo || field.command.starts_with("MEMO/")
}
fn memo_number(field: &Field) -> u32 {
    if field.memo_index != 0 {
        field.memo_index
    } else {
        field
            .command
            .split('/')
            .nth(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}
fn author(field: &Field) -> String {
    field
        .parameters
        .items
        .iter()
        .find_map(|p| match p {
            Parameter::String { name, value, .. } if name.as_deref() == Some("Author") => {
                Some(value.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| field.command.split('/').nth(5).unwrap_or("").to_owned())
}
fn validate_author(author: &str) -> Result<()> {
    if author.is_empty()
        || author.chars().count() > 256
        || author
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        bail!("메모 작성자는 제어 문자와 /, \\ 없이 1–256자로 입력하세요.");
    }
    Ok(())
}
fn validate_text(text: &str) -> Result<Vec<&str>> {
    if text.len() > MAX_TEXT_BYTES
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        bail!("메모는 제어 문자 없이 1 MiB 이하로 입력하세요.");
    }
    let lines: Vec<_> = text.split('\n').collect();
    if lines.len() > MAX_PARAGRAPHS {
        bail!("메모는 1,000개 문단 이하로 입력하세요.");
    }
    Ok(lines)
}
fn body_paragraphs(text: &str) -> Result<Vec<Paragraph>> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines = validate_text(&normalized)?;
    Ok(lines
        .into_iter()
        .map(|line| {
            let mut paragraph = Paragraph::new_empty();
            paragraph.insert_text_at(0, line);
            paragraph
        })
        .collect())
}
fn parameters(command: &str, index: u32, author: &str, shape: u16) -> ParameterList {
    let string = |name: &str, value: String| Parameter::String {
        name: Some(name.into()),
        value,
        preserve_space: true,
    };
    ParameterList {
        name: Some(String::new()),
        items: vec![
            Parameter::Integer {
                name: Some("Prop".into()),
                value: 0,
            },
            string("Command", command.into()),
            string("ID", format!("memo{index}")),
            Parameter::Integer {
                name: Some("Number".into()),
                value: i64::from(index),
            },
            string("Author", author.into()),
            string("MemoShapeIDRef", shape.to_string()),
        ],
    }
}
fn update_field(field: &mut Field, text: Option<Vec<Paragraph>>, by: Option<&str>) {
    let index = memo_number(field);
    let shape = field
        .command
        .split('/')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(NO_SHAPE);
    let author = by.map(str::to_owned).unwrap_or_else(|| author(field));
    field.field_type = FieldType::Memo;
    field.memo_index = index;
    if by.is_some() || !field.command.starts_with("MEMO/") {
        let mut tokens: Vec<_> = field.command.split('/').map(str::to_owned).collect();
        if tokens.len() >= 7 && tokens.first().is_some_and(|s| s == "MEMO") {
            tokens[5] = author.clone();
            field.command = tokens.join("/");
        } else {
            field.command = format!(
                "MEMO/{shape}/{index}/{}/{}/{author}/\\;;",
                field.field_id, field.field_id
            );
        }
    }
    field.raw_type = None;
    // The field header uses %unk, but PARA_TEXT must carry the %%me begin
    // marker. A zero marker passes our parser yet loses the native anchor.
    field.ctrl_id = 0x2525_6d65;
    field.properties |= 0x8001; // memo + editable, matching public native output
    field.instance_id.get_or_insert(field.field_id);
    field.raw_parameters_xml = None;
    let generated = parameters(&field.command, index, &author, shape);
    for item in generated.items {
        let name = parameter_name(&item);
        if let Some(old) = field
            .parameters
            .items
            .iter_mut()
            .find(|old| parameter_name(old) == name)
        {
            *old = item;
        } else {
            field.parameters.items.push(item);
        }
    }
    field.hyperlink_format = None;
    field.guide_residue = None;
    field
        .memo_text_direction
        .get_or_insert_with(|| "HORIZONTAL".into());
    if let Some(paragraphs) = text {
        field.memo_paragraphs = paragraphs;
    }
}
/// Re-lay out the memo's anchor paragraph and rebuild only its section after a
/// field-metadata edit. Rebuilding the whole document here (clone + set_document)
/// recomposed and repaginated every section: ~2 s per memo edit on a 381-page file.
/// Lay out the memo's body paragraph again after its field was edited in the
/// model. Only that paragraph changed, so RHWP's undo keeps the other sections.
fn relayout_paragraph(s: &mut Session, at: Cursor) -> Result<()> {
    s.core
        .document()
        .sections
        .get(at.section)
        .and_then(|x| x.paragraphs.get(at.paragraph))
        .ok_or_else(|| anyhow!("메모 문단을 찾지 못했습니다."))?;
    s.core
        .relayout_paragraph_native(at.section, at.paragraph, at.cell.map(|c| c.control))
        .map_err(err)?;
    Ok(())
}
fn parameter_name(parameter: &Parameter) -> Option<&str> {
    match parameter {
        Parameter::Boolean { name, .. }
        | Parameter::Integer { name, .. }
        | Parameter::Float { name, .. }
        | Parameter::String { name, .. } => name.as_deref(),
        Parameter::List(list) => list.name.as_deref(),
    }
}

#[derive(Clone)]
struct Location {
    cursor: Cursor,
    control: usize,
}
/// The memo with this id in a body paragraph or a paragraph of a table cell
/// (one table level, as the caret addresses cells).
fn find_body(doc: &Document, id: u32) -> Result<Location> {
    let mut found = None;
    let mut visit = |paragraph: &Paragraph, cursor: Cursor| -> Result<()> {
        for (control, c) in paragraph.controls.iter().enumerate() {
            if matches!(c, Control::Field(f) if is_memo(f) && f.field_id == id) {
                if found.is_some() {
                    bail!("중복 메모 ID는 편집할 수 없습니다: {id}");
                }
                found = Some(Location { cursor, control });
            }
        }
        Ok(())
    };
    for (section, s) in doc.sections.iter().enumerate() {
        for (paragraph, p) in s.paragraphs.iter().enumerate() {
            let body = Cursor {
                section,
                paragraph,
                ..Default::default()
            };
            visit(p, body)?;
            for (control, c) in p.controls.iter().enumerate() {
                let Control::Table(table) = c else { continue };
                for (index, value) in table.cells.iter().enumerate() {
                    for (cell_paragraph, cp) in value.paragraphs.iter().enumerate() {
                        let cell = Some(Cell {
                            control,
                            index,
                            paragraph: cell_paragraph,
                        });
                        visit(cp, Cursor { cell, ..body })?;
                    }
                }
            }
        }
    }
    found.ok_or_else(|| {
        anyhow!(
            "메모 ID를 찾을 수 없습니다: {id}. 본문과 표 셀(한 단계)의 메모만 편집할 수 있습니다."
        )
    })
}
/// The paragraph a cursor addresses: a body paragraph or a cell paragraph.
fn paragraph_mut(doc: &mut Document, at: Cursor) -> Option<&mut Paragraph> {
    let body = doc
        .sections
        .get_mut(at.section)?
        .paragraphs
        .get_mut(at.paragraph)?;
    match at.cell {
        None => Some(body),
        Some(cell) => match body.controls.get_mut(cell.control)? {
            Control::Table(table) => table
                .cells
                .get_mut(cell.index)?
                .paragraphs
                .get_mut(cell.paragraph),
            _ => None,
        },
    }
}
fn field_mut<'a>(doc: &'a mut Document, at: &Location) -> Result<&'a mut Field> {
    match paragraph_mut(doc, at.cursor).and_then(|p| p.controls.get_mut(at.control)) {
        Some(Control::Field(field)) if is_memo(field) => Ok(field),
        _ => bail!("메모 주소가 더 이상 유효하지 않습니다."),
    }
}
/// RHWP's range-field target for a body or cell paragraph.
fn field_target(at: Cursor) -> HyperlinkTarget {
    HyperlinkTarget {
        section: at.section,
        para: at.paragraph,
        cell_path: at
            .cell
            .map(|c| vec![(c.control, c.index, c.paragraph)])
            .unwrap_or_default(),
    }
}
fn collect_fields<'a>(paragraphs: &'a [Paragraph], fields: &mut Vec<&'a Field>) {
    for p in paragraphs {
        for control in &p.controls {
            match control {
                Control::Field(field) => {
                    fields.push(field);
                    collect_fields(&field.memo_paragraphs, fields);
                }
                Control::Table(table) => {
                    for cell in &table.cells {
                        collect_fields(&cell.paragraphs, fields);
                    }
                }
                Control::Footnote(note) => collect_fields(&note.paragraphs, fields),
                Control::Endnote(note) => collect_fields(&note.paragraphs, fields),
                _ => {}
            }
        }
    }
}

// Hancom's default memo style is the sentinel 65535, without a DocInfo
// MemoShape record. Keep native custom styles when present. Do not synthesize
// an unnecessary style: DocInfo requires MemoShape before ForbiddenChar, and
// appending a shape after that record makes native Hancom discard the body.
fn memo_shape(doc: &Document, section: usize) -> u16 {
    if doc.doc_info.memo_shape_count > 0 {
        doc.sections[section].section_def.memo_shape_id
    } else {
        NO_SHAPE
    }
}

fn restore_shape_xml(info: &mut DocInfo) -> Result<bool> {
    if info.memo_properties_xml.is_some() {
        return Ok(false);
    }
    let shapes: Vec<_> = info
        .extra_records
        .iter()
        .filter(|record| record.tag_id == MEMO_SHAPE)
        .collect();
    if shapes.is_empty() {
        return Ok(false);
    }
    if usize::try_from(info.memo_shape_count).ok() != Some(shapes.len()) {
        bail!("메모 모양 개수와 실제 레코드가 다릅니다.");
    }
    let mut xml = format!("<hh:memoProperties itemCnt=\"{}\">", shapes.len());
    for (index, record) in shapes.into_iter().enumerate() {
        if record.data.len() < 22 {
            bail!("메모 모양 레코드가 잘렸습니다.");
        }
        let u32_at = |start: usize| {
            u32::from_le_bytes([
                record.data[start],
                record.data[start + 1],
                record.data[start + 2],
                record.data[start + 3],
            ])
        };
        let color = |at| {
            let bgr = u32_at(at);
            format!(
                "#{:02X}{:02X}{:02X}",
                bgr & 255,
                (bgr >> 8) & 255,
                (bgr >> 16) & 255
            )
        };
        let line = match record.data[4] {
            0 => "NONE",
            1 => "SOLID",
            2 => "DASH_DOT",
            3 => "DASH",
            4 => "DASH_DOT_DOT",
            5 => "LONG_DASH",
            6 => "CIRCLE",
            7 => "DOUBLE_SLIM",
            8 => "SLIM_THICK",
            9 => "THICK_SLIM",
            10 => "SLIM_THICK_SLIM",
            11 => "WAVE",
            12 => "DOUBLE_WAVE",
            13 => "DOT",
            _ => bail!("지원하지 않는 메모 테두리 모양입니다."),
        };
        // Native schema spelling is NOMAL (not NORMAL), including its typo.
        // https://github.com/hancom-io/hwpx-owpml-model/blob/master/OWPML/Class/enumdef.h
        let kind = match u32_at(18) {
            0 => "NOMAL",
            1 => "USER_INSERT",
            2 => "USER_DELETE",
            3 => "USER_UPDATE",
            _ => bail!("지원하지 않는 메모 모양 종류입니다."),
        };
        xml.push_str(&format!("<hh:memoPr id=\"{}\" width=\"{}\" lineType=\"{line}\" lineWidth=\"{}\" lineColor=\"{}\" fillColor=\"{}\" activeColor=\"{}\" memoType=\"{kind}\"/>",index+1,u32_at(0),record.data[5],color(6),color(10),color(14)));
    }
    xml.push_str("</hh:memoProperties>");
    info.memo_properties_xml = Some(xml);
    Ok(true)
}

impl Session {
    pub(crate) fn run_comments_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(command, _, _)| *command == id) {
            return None;
        }
        Some(match id {
            "memo.list" => {
                if params.as_object().is_some_and(|o| o.is_empty()) {
                    Ok(self.memo_details())
                } else {
                    Err(anyhow!("memo.list에는 매개 변수가 없습니다."))
                }
            }
            "memo.get" => serde_json::from_value::<Id>(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|p| self.memo_get(p.id)),
            "memo.create" => serde_json::from_value::<Create>(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|p| self.create_memo(p)),
            "memo.reply" => serde_json::from_value::<Reply>(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|p| self.reply_memo(p)),
            "memo.update" => serde_json::from_value::<Update>(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|p| self.update_memo(p)),
            "memo.delete" => serde_json::from_value::<Delete>(params.clone())
                .map_err(anyhow::Error::from)
                .and_then(|p| self.delete_memo(p.id, p.expected_generation)),
            _ => Err(anyhow!("알 수 없는 메모 명령: {id}")),
        })
    }
    pub fn memo_details(&self) -> Value {
        fn scan(paragraph: &Paragraph, address: Value, out: &mut Vec<Value>) {
            for (index, control) in paragraph.controls.iter().enumerate() {
                match control {
                    Control::Field(field) if is_memo(field) => {
                        let range = paragraph
                            .field_ranges
                            .iter()
                            .find(|r| r.control_idx == index);
                        let anchor: String = range
                            .map(|r| {
                                paragraph
                                    .text
                                    .chars()
                                    .skip(r.start_char_idx)
                                    .take(r.end_char_idx.saturating_sub(r.start_char_idx))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let mut at = address.clone();
                        at["control"] = json!(index);
                        let paragraphs: Vec<_> = field
                            .memo_paragraphs
                            .iter()
                            .map(|p| p.text.as_str())
                            .collect();
                        out.push(json!({"id":field.field_id,"index":memo_number(field),"at":at,"author":author(field),"anchor":anchor,"start":range.map(|r|r.start_char_idx),"end":range.map(|r|r.end_char_idx),"paragraphs":paragraphs,"text":paragraphs.join("\n"),"contentAvailable":!field.memo_paragraphs.is_empty(),"editable":true,"threadingSupported":true,"resolvedStateSupported":false}));
                    }
                    Control::Table(table) => {
                        for (cell, value) in table.cells.iter().enumerate() {
                            for (p, paragraph) in value.paragraphs.iter().enumerate() {
                                let mut at = address.clone();
                                at["cell"] = json!({"control":index,"index":cell,"paragraph":p});
                                scan(paragraph, at, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for (section, s) in self.core.document().sections.iter().enumerate() {
            for (paragraph, p) in s.paragraphs.iter().enumerate() {
                scan(
                    p,
                    json!({"section":section,"paragraph":paragraph}),
                    &mut out,
                );
            }
        }
        // Memos on the same text in one paragraph form a thread (Hangul's 댓글), first
        // memo outermost; later ones reply to it, numbered from outside in.
        let mut first: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for item in &mut out {
            item["generation"] = json!(self.generation);
            let mut at = item["at"].clone();
            at.as_object_mut().map(|o| o.remove("control"));
            let key = format!("{at}:{}:{}", item["start"], item["end"]);
            let empty = item["start"] == item["end"];
            match first.get_mut(&key) {
                Some((root, count)) if !empty => {
                    *count += 1;
                    item["replyTo"] = json!(root);
                    item["thread"] = json!(*count);
                }
                _ => {
                    first.insert(key, (item["id"].as_u64().unwrap_or(0), 0));
                    item["replyTo"] = Value::Null;
                    item["thread"] = json!(0);
                }
            }
        }
        json!(out)
    }
    fn memo_get(&self, id: u32) -> Result<Value> {
        let details = self.memo_details();
        let items: Vec<_> = details
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["id"] == id)
            .collect();
        if items.len() != 1 {
            bail!("메모 ID가 없거나 중복되었습니다: {id}");
        }
        Ok(items[0].clone())
    }
    fn create_memo(&mut self, request: Create) -> Result<Value> {
        check_generation(self, request.expected_generation)?;
        validate_author(&request.author)?;
        let content = body_paragraphs(&request.text)?;
        self.guard_protected()?;
        let cursor = self.cursor;
        if self.span_anchor.is_some() {
            bail!("여러 문단에 걸친 메모는 아직 지원하지 않습니다.");
        }
        let range = self.selection();
        let start = range.start;
        let end = range.end;
        if end > self.paragraph(cursor)?.text.chars().count() {
            bail!("메모 범위가 문단을 벗어났습니다.");
        }
        let mut fields = Vec::new();
        for section in &self.core.document().sections {
            collect_fields(&section.paragraphs, &mut fields);
        }
        let index = fields
            .iter()
            .filter(|f| is_memo(f))
            .map(|f| memo_number(f))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|x| *x <= u32::from(u16::MAX))
            .ok_or_else(|| anyhow!("메모 번호 공간이 소진되었습니다."))?;
        let mut created_id = 0;
        self.transaction(|s| {
            created_id = s.place_memo(cursor, start, end, index, &request.author, content)?;
            Ok(())
        })?;
        self.memo_get(created_id)
    }

    /// The next memo number in the document.
    fn next_memo_number(&self) -> Result<u32> {
        let mut fields = Vec::new();
        for section in &self.core.document().sections {
            collect_fields(&section.paragraphs, &mut fields);
        }
        fields
            .iter()
            .filter(|f| is_memo(f))
            .map(|f| memo_number(f))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|x| *x <= u32::from(u16::MAX))
            .ok_or_else(|| anyhow!("메모 번호 공간이 소진되었습니다."))
    }

    /// A memo field over `start..end` of the paragraph at `cursor` (a caret slot when
    /// empty), inside the caller's transaction. Returns its field id.
    fn place_memo(
        &mut self,
        cursor: Cursor,
        start: usize,
        end: usize,
        index: u32,
        author: &str,
        content: Vec<Paragraph>,
    ) -> Result<u32> {
        let s = self;
        {
            let id = if start < end {
                s.core
                    .insert_hyperlink_native(
                        &field_target(cursor),
                        start,
                        end,
                        "https://invalid.invalid/field-anchor",
                    )
                    .map_err(err)?
            } else {
                let raw = match cursor.cell {
                    None => s.core.insert_click_here_field_at(
                        cursor.section,
                        cursor.paragraph,
                        start,
                        "",
                        "",
                        "",
                        true,
                    ),
                    Some(cell) => s.core.insert_click_here_field_at_in_cell(
                        cursor.section,
                        cursor.paragraph,
                        cell.control,
                        cell.index,
                        cell.paragraph,
                        start,
                        false,
                        "",
                        "",
                        "",
                        true,
                    ),
                }
                .map_err(err)?;
                let result: Value = serde_json::from_str(&raw)?;
                result["fieldId"]
                    .as_u64()
                    .and_then(|id| u32::try_from(id).ok())
                    .ok_or_else(|| anyhow!("메모 필드 생성 응답에 ID가 없습니다."))?
            };
            let shape = memo_shape(s.core.document(), cursor.section);
            let paragraph = paragraph_mut(s.core.document_mut(), cursor)
                .ok_or_else(|| anyhow!("메모 문단을 찾지 못했습니다."))?;
            let field = paragraph
                .controls
                .iter_mut()
                .find_map(|c| match c {
                    Control::Field(f) if f.field_id == id => Some(f),
                    _ => None,
                })
                .ok_or_else(|| anyhow!("생성한 메모 필드를 찾지 못했습니다."))?;
            let (low, high) = filetime_tokens();
            field.memo_index = index;
            field.command = format!("MEMO/{shape}/{index}/{low}/{high}/{author}/\\;;");
            update_field(field, Some(content), Some(author));
            s.core.document_mut().sections[cursor.section].raw_stream = None;
            relayout_paragraph(s, cursor)?;
            Ok(id)
        }
    }

    fn reply_memo(&mut self, request: Reply) -> Result<Value> {
        check_generation(self, request.expected_generation)?;
        validate_author(&request.author)?;
        let content = body_paragraphs(&request.text)?;
        self.guard_protected()?;
        let at = find_body(self.core.document(), request.id)?;
        let (start, end) = self
            .paragraph(at.cursor)?
            .field_ranges
            .iter()
            .find(|r| r.control_idx == at.control)
            .map(|r| (r.start_char_idx, r.end_char_idx))
            .ok_or_else(|| anyhow!("메모 범위를 찾지 못했습니다."))?;
        if start >= end {
            bail!("글자 범위 없이 캐럿 자리에 단 메모에는 댓글을 달 수 없습니다.");
        }
        let index = self.next_memo_number()?;
        let mut created_id = 0;
        self.transaction(|s| {
            created_id = s.place_memo(at.cursor, start, end, index, &request.author, content)?;
            Ok(())
        })?;
        self.memo_get(created_id)
    }
    fn update_memo(&mut self, request: Update) -> Result<Value> {
        check_generation(self, request.expected_generation)?;
        if request.text.is_none() && request.author.is_none() {
            bail!("고칠 메모 내용 또는 작성자를 지정하세요.");
        }
        if let Some(author) = &request.author {
            validate_author(author)?;
        }
        let content = request.text.as_deref().map(body_paragraphs).transpose()?;
        self.guard_protected()?;
        let at = find_body(self.core.document(), request.id)?;
        self.transaction(|s| {
            update_field(
                field_mut(s.core.document_mut(), &at)?,
                content,
                request.author.as_deref(),
            );
            s.core.document_mut().sections[at.cursor.section].raw_stream = None;
            relayout_paragraph(s, at.cursor)?;
            Ok(())
        })?;
        self.memo_get(request.id)
    }
    fn delete_memo(&mut self, id: u32, expected_generation: Option<u64>) -> Result<Value> {
        check_generation(self, expected_generation)?;
        self.guard_protected()?;
        find_body(self.core.document(), id)?;
        // Deleting a thread's first memo takes its replies along, innermost first.
        let details = self.memo_details();
        let mut ids: Vec<u32> = details
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["replyTo"] == id)
            .filter_map(|m| m["id"].as_u64().and_then(|v| u32::try_from(v).ok()))
            .collect();
        ids.reverse();
        ids.push(id);
        self.transaction(|s| {
            for id in &ids {
                let at = find_body(s.core.document(), *id)?;
                // Removing a range field uses the same native marker-axis primitive
                // as hyperlinks; keep all body text and its character formatting.
                field_mut(s.core.document_mut(), &at)?.field_type = FieldType::Hyperlink;
                s.core
                    .remove_hyperlink_native(&field_target(at.cursor), *id)
                    .map_err(err)?;
            }
            Ok(())
        })?;
        Ok(json!({"id":id,"removed":true,"removedIds":ids,"generation":self.generation}))
    }
}

/// Restore HWP5 memo bodies from preserved section records before a new session
/// replaces the old one. All parsing completes on a clone before committing.
pub fn restore_memo_content(core: &mut DocumentCore) -> Result<()> {
    let mut fields = Vec::new();
    for section in &core.document().sections {
        collect_fields(&section.paragraphs, &mut fields);
    }
    if !fields.iter().any(|f| is_memo(f)) {
        return Ok(());
    }
    let mut doc = core.document().clone();
    let mut changed = restore_shape_xml(&mut doc.doc_info)?;
    for section in &mut doc.sections {
        let Some(raw) = &section.raw_stream else {
            continue;
        };
        let records =
            HwpRecord::read_all(raw).map_err(|e| anyhow!("메모 레코드를 읽지 못했습니다: {e}"))?;
        let mut contents = BTreeMap::new();
        let mut first_memo = None;
        for (i, record) in records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.tag_id == MEMO_LIST)
        {
            first_memo.get_or_insert(i);
            let bytes: [u8; 4] = record
                .data
                .get(..4)
                .ok_or_else(|| anyhow!("메모 목록 번호가 잘렸습니다."))?
                .try_into()
                .map_err(|_| anyhow!("메모 목록 번호 오류"))?;
            let index = u32::from_le_bytes(bytes);
            let end = records
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, r)| r.tag_id == MEMO_LIST || r.level < record.level)
                .map_or(records.len(), |(j, _)| j);
            let start = records
                .iter()
                .enumerate()
                .take(end)
                .skip(i + 1)
                .find(|(_, r)| r.tag_id == PARA_HEADER)
                .map(|(j, _)| j);
            if let Some(start) = start {
                let paragraphs = parse_hwp_paragraph_list(&records[start..end]);
                let expected = records[start..end]
                    .iter()
                    .filter(|r| r.tag_id == PARA_HEADER && r.level == records[start].level)
                    .count();
                if paragraphs.is_empty() || paragraphs.len() != expected {
                    bail!("메모 {index}의 본문 문단을 읽지 못했습니다.");
                }
                if contents.insert(index, paragraphs).is_some() {
                    bail!("중복 메모 목록 번호: {index}");
                }
            }
        }
        // Memos sit in body paragraphs and in table cells (any depth).
        fn fill(
            paragraphs: &mut [Paragraph],
            contents: &BTreeMap<u32, Vec<Paragraph>>,
            changed: &mut bool,
        ) {
            for paragraph in paragraphs {
                for control in &mut paragraph.controls {
                    match control {
                        Control::Field(field)
                            if is_memo(field) && field.memo_paragraphs.is_empty() =>
                        {
                            if let Some(body) = contents.get(&memo_number(field)) {
                                let author = author(field);
                                update_field(field, Some(body.clone()), Some(&author));
                                *changed = true;
                            }
                        }
                        Control::Table(table) => {
                            for cell in &mut table.cells {
                                fill(&mut cell.paragraphs, contents, changed);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        fill(&mut section.paragraphs, &contents, &mut changed);
        // A memo-tail metadata root is not an extra editable body paragraph.
        if let Some(first) = first_memo {
            let headers: Vec<_> = records[..first]
                .iter()
                .filter(|r| r.tag_id == PARA_HEADER && r.level == 0)
                .collect();
            if headers.len() == section.paragraphs.len()
                && section
                    .paragraphs
                    .last()
                    .is_some_and(|p| p.text.is_empty() && p.controls.is_empty() && !p.has_para_text)
            {
                section.paragraphs.pop();
                changed = true;
            }
        }
    }
    if changed {
        core.set_document(doc);
    }
    Ok(())
}
