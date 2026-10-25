//! Owner: shell/page area. 쪽/보기/파일 서비스: 쪽 테두리/배경·머리말/꼬리말·쪽 번호·새 번호·감추기·문서 정보.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Lengths in parameters are millimetres; RHWP stores HWPUNIT (1/7200 inch).
use super::*;
use hwpcraft_doc::hidden_text::HiddenTextOptions;
use hwpcraft_doc::model::control::PageNumberPos;
use hwpcraft_doc::model::document::Document;
use hwpcraft_doc::pii_scan::PiiKind;
use serde::{Deserialize, Serialize};

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "view.marks.get",
        "문단 부호·조판 부호·투명 선 보기 조회",
        "",
    ),
    (
        "view.marks",
        "문단 부호·조판 부호·투명 선 보기 (화면 전용, 저장 안 함)",
        "Ctrl+G,T / Ctrl+G,C",
    ),
    (
        "page.header_footer.text.get",
        "머리말/꼬리말 내용·필드 조회",
        "",
    ),
    (
        "page.header_footer.text",
        "머리말/꼬리말 내용·필드(쪽 번호·전체 쪽수·파일 이름)",
        "",
    ),
    ("page.section.get", "구역 설정 조회", ""),
    ("page.section", "구역 설정", ""),
    ("security.privacy.scan", "개인 정보·숨은 글자 찾기", ""),
    ("security.privacy.mask", "개인 정보 가리기", ""),
    ("file.save_hml", "HML로 저장 (HML 원본 문서만)", ""),
    ("file.save_pages", "쪽 범위만 저장 (현재 쪽만)", ""),
    ("page.border_fill.get", "쪽 테두리/배경 조회", ""),
    ("page.border_fill", "쪽 테두리/배경", ""),
    ("page.header", "머리말 (마당에서 만들기)", "Ctrl+N,H"),
    ("page.footer", "꼬리말 (마당에서 만들기)", "Ctrl+N,H"),
    ("page.header_footer.list", "머리말/꼬리말 목록", ""),
    ("page.header_footer.delete", "머리말/꼬리말 지우기", ""),
    ("page.number.get", "쪽 번호 매기기 조회", ""),
    ("page.number", "쪽 번호 매기기", "Ctrl+N,P"),
    ("page.new_number", "새 번호로 시작", ""),
    ("page.hide.get", "현재 쪽만 감추기 조회", ""),
    ("page.hide", "현재 쪽만 감추기", ""),
    ("page.master.list", "바탕쪽 목록", ""),
    ("file.doc_info", "문서 정보", "Ctrl+Q,I"),
    ("file.print_pdf", "인쇄용 PDF 만들기 (쪽 범위)", "Ctrl+P"),
    (
        "page.margins.presets",
        "쪽 여백 사전 값 목록 (기본·좁게1·좁게2·넓게(머리말/꼬리말 포함)·좁게·넓게)",
        "",
    ),
    (
        "page.margins.preset",
        "쪽 여백 사전 값 적용 (현재 구역)",
        "",
    ),
    ("page.section.next", "이후 구역으로", ""),
    ("page.section.prev", "이전 구역으로", ""),
    ("page.copy", "쪽 복사하기 (쪽 클립보드)", ""),
    ("page.paste", "쪽 붙이기 (현재 쪽 뒤에 새 쪽으로)", ""),
    ("page.delete", "쪽 지우기", ""),
    ("file.insert", "문서 끼워 넣기 (HWP/HWPX를 커서 위치에)", ""),
    ("file.save_block", "블록 저장 (선택 영역만 새 파일로)", ""),
];

/// (command id, parameter help, JSON example object) for list_commands and
/// docs/mcp-commands.md; owner fills this for every command above.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "view.marks.get",
        "No parameters; paragraphMarks, controlCodes, transparentBorders (screen only).",
        "{}",
    ),
    (
        "view.marks",
        "paragraphMarks, controlCodes, transparentBorders: bool (default false). Screen only: not saved and not part of undo.",
        r#"{"paragraphMarks":true,"controlCodes":true,"transparentBorders":false}"#,
    ),
    (
        "page.header_footer.text.get",
        "header: bool; apply_to: both|odd|even (default both). Returns exists, text and fields [{offset, kind}].",
        r#"{"header":true,"apply_to":"both"}"#,
    ),
    (
        "page.header_footer.text",
        "header: bool; apply_to: both|odd|even; text: one line; fields: [{offset, kind: page_number|total_pages|file_name}] inserted at text offsets.",
        r#"{"header":true,"apply_to":"both","text":"보고서 ","fields":[{"offset":4,"kind":"page_number"}]}"#,
    ),
    (
        "page.section.get",
        "No parameters; current section: pageNum, pageNumType, pictureNum, tableNum, equationNum (0 = continue), defaultTabSpacingMm, hideHeader, hideFooter, hideMasterPage, hideBorder, hideFill, hideEmptyLine.",
        "{}",
    ),
    (
        "page.section",
        "Optional pageNum, pageNumType, pictureNum, tableNum, equationNum (0 = continue from the previous section), defaultTabSpacingMm, hideHeader, hideFooter, hideMasterPage, hideBorder, hideFill, hideEmptyLine; wholeDocument applies to every section.",
        r#"{"pageNum":1,"defaultTabSpacingMm":40,"hideHeader":false}"#,
    ),
    (
        "security.privacy.scan",
        "kinds: subset of ssn|card|phone|email (default all); mask: mask character(s); hiddenText: also list hidden text (default true); offPage: include text off the page.",
        r#"{"kinds":["ssn","phone"],"mask":"*"}"#,
    ),
    (
        "security.privacy.mask",
        "items: [{raw, masked}] with equal character counts, as returned by the scan; every occurrence is replaced in one undo step.",
        r#"{"items":[{"raw":"010-1234-5678","masked":"010-****-****"}]}"#,
    ),
    (
        "file.save_hml",
        "path ending in .hml; only documents opened from HML can be written back.",
        r#"{"path":"out.hml"}"#,
    ),
    (
        "file.save_pages",
        "path (.hwp/.hwpx/.pdf/.txt); from, to: 1-based inclusive page range. The open document is unchanged.",
        r#"{"path":"2-3쪽.hwpx","from":2,"to":3}"#,
    ),
    (
        "page.border_fill.get",
        "No parameters; current section border/fill with spacingMm [left, right, top, bottom].",
        "{}",
    ),
    (
        "page.border_fill",
        "basis: paper|page; spacingMm: [left, right, top, bottom]; headerInside, footerInside: bool; applyPage: all|exceptFirst; borders: four {type, width, color #rrggbb} for left, right, top, bottom; fill: #rrggbb or null; fillArea: paper|page|border; wholeDocument: bool.",
        r##"{"basis":"paper","spacingMm":[5,5,5,5],"headerInside":false,"footerInside":false,"applyPage":"all","borders":[{"type":1,"width":2,"color":"#000000"},{"type":1,"width":2,"color":"#000000"},{"type":1,"width":2,"color":"#000000"},{"type":1,"width":2,"color":"#000000"}],"fill":null}"##,
    ),
    (
        "page.header",
        "apply_to: both|odd|even (default both); template: 0-10 (0 none, 1-3 page number left/centre/right, 4-5 with file name, 6-10 the same bold and underlined).",
        r#"{"apply_to":"both","template":2}"#,
    ),
    (
        "page.footer",
        "Same parameters as page.header.",
        r#"{"apply_to":"both","template":2}"#,
    ),
    (
        "page.header_footer.list",
        "No parameters; headers and footers of the current section.",
        "{}",
    ),
    (
        "page.header_footer.delete",
        "header: bool; apply_to: both|odd|even (default both).",
        r#"{"header":true,"apply_to":"both"}"#,
    ),
    (
        "page.number.get",
        "No parameters; exists, position, format, dash, start.",
        "{}",
    ),
    (
        "page.number",
        "position: none|top_left|top_center|top_right|bottom_left|bottom_center|bottom_right|outside_top|outside_bottom|inside_top|inside_bottom; format: 0 1,2,3 / 1 circled / 2 I,II,III / 3 i,ii,iii / 4 A,B,C / 5 a,b,c / 8 가,나,다 / 10 ㄱ,ㄴ,ㄷ / 12 일,이,삼 / 13 一,二,三; dash: bool; start: optional first number.",
        r#"{"position":"bottom_center","format":0,"dash":true,"start":1}"#,
    ),
    (
        "page.new_number",
        "start: page number (1 or more) that begins at the caret page.",
        r#"{"start":5}"#,
    ),
    (
        "page.hide.get",
        "No parameters; hide flags of the caret page.",
        "{}",
    ),
    (
        "page.hide",
        "header, footer, master_page, border, fill, page_number: bool, for the caret page only.",
        r#"{"header":true,"footer":false,"master_page":false,"border":false,"fill":false,"page_number":true}"#,
    ),
    (
        "page.master.list",
        "No parameters; master pages of the current section (read only).",
        "{}",
    ),
    (
        "file.doc_info",
        "No parameters; file {path, format, bytes, modified, created}, pages, sections, paragraphs, lines, characters, words, tables, pictures, shapes, equations, notes, fonts, fontSubstitutions, binData, pictureData.",
        "{}",
    ),
    (
        "file.print_pdf",
        "path (.pdf); pages: all|current|to_current|from_current or a list like 1,3,5-7; current: 0-based caret page used by the current* ranges.",
        r#"{"path":"print.pdf","pages":"1,3,5-7","current":0}"#,
    ),
    (
        "page.margins.presets",
        "No parameters; the six Hangul 쪽 여백 presets with their millimetre values.",
        "{}",
    ),
    (
        "page.margins.preset",
        "preset: default|narrow1_hf|narrow2_hf|wide_hf|narrow|wide, the Hangul gallery in order (기본, 좁게1/좁게2/넓게 with header and footer margins, 좁게, 넓게). Applies to the caret section; paper size and gutter stay.",
        r#"{"preset":"narrow"}"#,
    ),
    (
        "page.section.next",
        "No parameters; caret to the first paragraph of the next section.",
        "{}",
    ),
    (
        "page.section.prev",
        "No parameters; caret to the first paragraph of the previous section.",
        "{}",
    ),
    (
        "page.copy",
        "page: optional 1-based page (default: caret page). Copies the paragraphs that start on that page to the process-wide page clipboard.",
        r#"{"page":2}"#,
    ),
    (
        "page.paste",
        "page: optional 1-based page (default: caret page). Inserts the copied page as a new page after it; the caret ends after the pasted text.",
        "{}",
    ),
    (
        "page.delete",
        "page: optional 1-based page (default: caret page). Removes the paragraphs of that page so later pages move up; refused for single-page documents.",
        "{}",
    ),
    (
        "file.insert",
        "path of an HWP/HWPX/HWP3 document; password when it is protected. Its body is merged at the caret with fonts, styles and pictures remapped; one undo step.",
        r#"{"path":"부록.hwpx"}"#,
    ),
    (
        "file.save_block",
        "path (.hwp/.hwpx/.pdf/.txt); writes only the selected body text, including whole middle paragraphs of a multi-paragraph selection.",
        r#"{"path":"블록.hwpx"}"#,
    ),
];
const HU_PER_MM: f64 = 7200.0 / 25.4;
/// Hangul's 쪽 번호 위치 codes (HWP spec table 149) by dialog name.
pub const NUMBER_POSITIONS: &[(&str, u8)] = &[
    ("none", 0),
    ("top_left", 1),
    ("top_center", 2),
    ("top_right", 3),
    ("bottom_left", 4),
    ("bottom_center", 5),
    ("bottom_right", 6),
    ("outside_top", 7),
    ("outside_bottom", 8),
    ("inside_top", 9),
    ("inside_bottom", 10),
];
/// Number formats offered by Hangul's 쪽 번호 모양 list (HWP table 41 codes).
pub const NUMBER_FORMATS: &[(u8, &str)] = &[
    (0, "1,2,3"),
    (1, "①,②,③"),
    (2, "I,II,III"),
    (3, "i,ii,iii"),
    (4, "A,B,C"),
    (5, "a,b,c"),
    (8, "가,나,다"),
    (10, "ㄱ,ㄴ,ㄷ"),
    (12, "일,이,삼"),
    (13, "一,二,三"),
];
/// Hangul 머리말/꼬리말 마당 entries supported by RHWP's template builder.
pub const HEADER_TEMPLATES: &[&str] = &[
    "(모양 없음)",
    "왼쪽 쪽 번호",
    "가운데 쪽 번호",
    "오른쪽 쪽 번호",
    "쪽 번호 · 파일 이름",
    "파일 이름 · 쪽 번호",
    "왼쪽 쪽 번호 (굵게·밑줄)",
    "가운데 쪽 번호 (굵게·밑줄)",
    "오른쪽 쪽 번호 (굵게·밑줄)",
    "쪽 번호 · 파일 이름 (굵게·밑줄)",
    "파일 이름 · 쪽 번호 (굵게·밑줄)",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BorderLine {
    /// RHWP border line type code: 0 none, 1 solid, 2 dash, 3 dot, …
    #[serde(rename = "type")]
    kind: u8,
    /// Width index 0–15 (0.1 mm … 5 mm).
    width: u8,
    color: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BorderFill {
    /// "paper" (종이 기준) or "page" (쪽 기준).
    basis: String,
    /// left, right, top, bottom spacing in mm.
    spacing_mm: [f64; 4],
    header_inside: bool,
    footer_inside: bool,
    /// "all" (모두), "exceptFirst" (첫 쪽 제외).
    apply_page: String,
    /// left, right, top, bottom.
    borders: [BorderLine; 4],
    /// `None` for no fill, `#rrggbb` for a solid fill.
    #[serde(default)]
    fill: Option<String>,
    /// "paper", "page" or "border" — area the fill covers.
    #[serde(default)]
    fill_area: Option<String>,
    /// Apply to every section instead of the cursor's.
    #[serde(default)]
    whole_document: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderFooter {
    /// "both", "odd" or "even".
    #[serde(default = "both")]
    apply_to: String,
    #[serde(default)]
    template: u8,
}
fn both() -> String {
    "both".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageNumber {
    position: String,
    #[serde(default)]
    format: u8,
    #[serde(default)]
    dash: bool,
    /// Section start number; `None` keeps the current value.
    #[serde(default)]
    start: Option<u16>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageHide {
    header: bool,
    footer: bool,
    master_page: bool,
    border: bool,
    fill: bool,
    page_number: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HfTarget {
    header: bool,
    #[serde(default = "both")]
    apply_to: String,
}

/// Header/footer field kinds RHWP can insert (`insert_field_in_hf_native` codes 1–3).
/// Hangul also offers a 날짜 field; RHWP has no header/footer date field.
pub const HF_FIELDS: &[(&str, u8, char)] = &[
    ("page_number", 1, '\u{15}'),
    ("total_pages", 2, '\u{16}'),
    ("file_name", 3, '\u{17}'),
];

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
pub struct HfField {
    /// Character offset in `text` (before fields are inserted) where the field goes.
    pub offset: usize,
    /// One of `HF_FIELDS` names.
    pub kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HfText {
    header: bool,
    #[serde(default = "both")]
    apply_to: String,
    /// One line without control characters; fields are added separately.
    text: String,
    #[serde(default)]
    fields: Vec<HfField>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SectionDef {
    /// Section start page number; `None` keeps the current value.
    #[serde(default)]
    page_num: Option<u16>,
    /// HWP page number kind 0–3 (이어서/새 번호/홀수/짝수 as stored by Hangul).
    #[serde(default)]
    page_num_type: Option<u8>,
    #[serde(default)]
    picture_num: Option<u16>,
    #[serde(default)]
    table_num: Option<u16>,
    #[serde(default)]
    equation_num: Option<u16>,
    #[serde(default)]
    default_tab_spacing_mm: Option<f64>,
    #[serde(default)]
    hide_header: Option<bool>,
    #[serde(default)]
    hide_footer: Option<bool>,
    #[serde(default)]
    hide_master_page: Option<bool>,
    #[serde(default)]
    hide_border: Option<bool>,
    #[serde(default)]
    hide_fill: Option<bool>,
    #[serde(default)]
    hide_empty_line: Option<bool>,
    #[serde(default)]
    whole_document: bool,
}

/// Personal-data kinds RHWP's scanner validates (checksum/Luhn/format), by command name.
pub const PII_KINDS: &[(&str, &str)] = &[
    ("ssn", "주민등록번호"),
    ("card", "신용카드 번호"),
    ("phone", "전화번호"),
    ("email", "전자우편 주소"),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PrivacyScan {
    /// Subset of `PII_KINDS` names; empty means all four.
    #[serde(default)]
    kinds: Vec<String>,
    /// Single character used for the masked preview.
    #[serde(default = "star")]
    mask: String,
    /// Also report hidden text (same colour as background, zero/near-zero size).
    #[serde(default = "yes")]
    hidden_text: bool,
    /// Include paragraphs laid out completely off the page (slower, layout based).
    #[serde(default)]
    off_page: bool,
}
fn star() -> String {
    "*".into()
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskItem {
    raw: String,
    masked: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivacyMask {
    /// Values to replace everywhere they occur (body, cells, text boxes), as returned by the scan.
    items: Vec<MaskItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavePages {
    path: std::path::PathBuf,
    /// 1-based inclusive page range.
    from: u32,
    to: u32,
}

/// 0-based page numbers for a Hangul 인쇄 범위. `current` is 0-based; lists are 1-based.
pub fn print_pages(range: &str, current: u32, count: u32) -> Result<Vec<u32>> {
    if count == 0 {
        bail!("인쇄할 쪽이 없습니다.");
    }
    let current = current.min(count - 1);
    let pages: Vec<u32> = match range {
        "all" => (0..count).collect(),
        "current" => vec![current],
        "to_current" => (0..=current).collect(),
        "from_current" => (current..count).collect(),
        list => {
            let mut out = Vec::new();
            for part in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let (a, b) = part.split_once('-').unwrap_or((part, part));
                let parse = |v: &str| -> Result<u32> {
                    let n: u32 = v
                        .trim()
                        .parse()
                        .map_err(|_| anyhow!("쪽 범위를 읽지 못했습니다: {part}"))?;
                    if n == 0 || n > count {
                        bail!("{n}쪽은 범위(1–{count})를 벗어났습니다.");
                    }
                    Ok(n - 1)
                };
                let (a, b) = (parse(a)?, parse(b)?);
                if a > b {
                    bail!("쪽 범위는 작은 번호부터 적으세요: {part}");
                }
                out.extend(a..=b);
            }
            out
        }
    };
    if pages.is_empty() {
        bail!("인쇄할 쪽 범위를 입력하세요. 예) 1,3,5-7");
    }
    Ok(pages)
}

fn apply_code(apply_to: &str) -> Result<u8> {
    Ok(match apply_to {
        "both" => 0,
        "even" => 1,
        "odd" => 2,
        _ => bail!("적용 쪽은 both, odd, even 중 하나여야 합니다."),
    })
}
fn color(value: &str) -> Result<&str> {
    let hex = value.strip_prefix('#').unwrap_or("");
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("색은 #rrggbb 형식이어야 합니다: {value}");
    }
    Ok(value)
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_page_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_page_command(id, params.clone()))
    }

    fn run_page_command(&mut self, id: &str, params: Value) -> Result<Value> {
        let section = self.cursor.section;
        match id {
            "page.border_fill.get" => self.page_border_fill(),
            "page.border_fill" => {
                let p: BorderFill = serde_json::from_value(params)?;
                let json = Self::border_fill_json(&p)?;
                let sections = if p.whole_document {
                    0..self.core.document().sections.len()
                } else {
                    section..section + 1
                };
                self.transaction(|s| {
                    for i in sections {
                        s.core
                            .set_page_border_fill_native(i, &json.to_string())
                            .map_err(err)?;
                    }
                    Ok(())
                })?;
                self.page_border_fill()
            }
            "page.header" | "page.footer" => {
                let p: HeaderFooter = serde_json::from_value(params)?;
                let apply = apply_code(&p.apply_to)?;
                if usize::from(p.template) >= HEADER_TEMPLATES.len() {
                    bail!(
                        "머리말/꼬리말 마당 번호는 0–{}입니다.",
                        HEADER_TEMPLATES.len() - 1
                    );
                }
                let is_header = id == "page.header";
                self.transaction(|s| {
                    s.core
                        .apply_hf_template_native(section, is_header, apply, p.template)
                        .map_err(err)?;
                    Ok(())
                })?;
                self.header_footer_list()
            }
            "page.header_footer.list" => self.header_footer_list(),
            "page.header_footer.delete" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Delete {
                    header: bool,
                    #[serde(default = "both")]
                    apply_to: String,
                }
                let p: Delete = serde_json::from_value(params)?;
                let apply = apply_code(&p.apply_to)?;
                self.transaction(|s| {
                    s.core
                        .delete_header_footer_native(section, p.header, apply)
                        .map_err(err)?;
                    Ok(())
                })?;
                self.header_footer_list()
            }
            "page.number.get" => self.page_number(),
            "page.number" => {
                let p: PageNumber = serde_json::from_value(params)?;
                self.set_page_number(&p)?;
                self.page_number()
            }
            "page.new_number" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Start {
                    start: u16,
                }
                let p: Start = serde_json::from_value(params)?;
                if p.start == 0 {
                    bail!("새 번호는 1 이상이어야 합니다.");
                }
                self.require_body()?;
                let c = self.cursor;
                self.transaction(|s| {
                    s.core
                        .insert_new_number_native(c.section, c.paragraph, c.offset, p.start)
                        .map_err(err)?;
                    Ok(())
                })?;
                Ok(json!({"start": p.start}))
            }
            "page.hide.get" => {
                self.require_body()?;
                let c = self.cursor;
                Ok(serde_json::from_str(
                    &self
                        .core
                        .get_page_hide_native(c.section, c.paragraph)
                        .map_err(err)?,
                )?)
            }
            "page.hide" => {
                let p: PageHide = serde_json::from_value(params)?;
                self.require_body()?;
                let c = self.cursor;
                self.transaction(|s| {
                    s.core
                        .set_page_hide_native(
                            c.section,
                            c.paragraph,
                            p.header,
                            p.footer,
                            p.master_page,
                            p.border,
                            p.fill,
                            p.page_number,
                        )
                        .map_err(err)?;
                    Ok(())
                })?;
                Ok(serde_json::from_str(
                    &self
                        .core
                        .get_page_hide_native(c.section, c.paragraph)
                        .map_err(err)?,
                )?)
            }
            "page.master.list" => {
                let doc = self.core.document();
                let list: Vec<Value> = doc
                    .sections
                    .get(section)
                    .map(|s| {
                        s.section_def
                            .master_pages
                            .iter()
                            .map(|m| {
                                json!({
                                    "applyTo": format!("{:?}", m.apply_to),
                                    "extension": m.is_extension,
                                    "overlap": m.overlap,
                                    "text": m.paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(json!({"section": section, "masterPages": list}))
            }
            "view.marks.get" => Ok(serde_json::to_value(self.view_marks())?),
            "view.marks" => {
                let m: ViewMarks = serde_json::from_value(params)?;
                Ok(serde_json::to_value(self.set_view_marks(m))?)
            }
            "page.header_footer.text.get" => {
                let p: HfTarget = serde_json::from_value(params)?;
                self.header_footer_fields(p.header, apply_code(&p.apply_to)?)
            }
            "page.header_footer.text" => {
                let p: HfText = serde_json::from_value(params)?;
                self.set_header_footer_fields(&p)?;
                self.header_footer_fields(p.header, apply_code(&p.apply_to)?)
            }
            "page.section.get" => self.section_def(),
            "page.section" => {
                let p: SectionDef = serde_json::from_value(params)?;
                let json = Self::section_def_json(&p)?;
                let whole = p.whole_document;
                self.transaction(|s| {
                    if whole {
                        s.core
                            .set_section_def_all_native(&json.to_string())
                            .map_err(err)?;
                    } else {
                        s.core
                            .set_section_def_native(section, &json.to_string())
                            .map_err(err)?;
                    }
                    Ok(())
                })?;
                self.section_def()
            }
            "security.privacy.scan" => {
                let p: PrivacyScan = serde_json::from_value(params)?;
                self.privacy_scan(&p)
            }
            "security.privacy.mask" => {
                let p: PrivacyMask = serde_json::from_value(params)?;
                self.privacy_mask(&p)
            }
            "file.save_hml" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Target {
                    path: std::path::PathBuf,
                }
                let p: Target = serde_json::from_value(params)?;
                if p.path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_none_or(|e| !e.eq_ignore_ascii_case("hml"))
                {
                    bail!("HML 저장 파일 이름은 .hml로 끝나야 합니다.");
                }
                let bytes = self.core.export_hml_native().map_err(|e| {
                    anyhow!("HML로 저장하지 못했습니다: {e}. RHWP는 HML로 연 문서만 HML로 다시 저장합니다.")
                })?;
                hwpcraft_formats::atomic_write(&p.path, &bytes)?;
                Ok(json!({"path": p.path, "bytes": bytes.len()}))
            }
            "file.save_pages" => {
                let p: SavePages = serde_json::from_value(params)?;
                self.save_pages(&p)
            }
            "file.doc_info" => Ok(self.document_statistics()),
            "file.print_pdf" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Print {
                    path: std::path::PathBuf,
                    /// "all", "current", "to_current", "from_current" or a list like "1,3,5-7".
                    pages: String,
                    #[serde(default)]
                    current: u32,
                }
                let p: Print = serde_json::from_value(params)?;
                let count = self.core.page_count();
                let pages = print_pages(&p.pages, p.current, count)?;
                let options = hwpcraft_formats::pdf_options();
                let bytes = self
                    .core
                    .render_pages_pdf_native_with_options(&pages, &options)
                    .map_err(err)?;
                hwpcraft_formats::atomic_write(&p.path, &bytes)?;
                Ok(
                    json!({"path": p.path, "pages": pages.iter().map(|n| n + 1).collect::<Vec<_>>(), "bytes": bytes.len()}),
                )
            }
            "page.margins.presets" => Ok(margin_presets_json()),
            "page.margins.preset" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Preset {
                    preset: String,
                }
                let p: Preset = serde_json::from_value(params)?;
                self.apply_margin_preset(&p.preset)
            }
            "page.section.next" | "page.section.prev" => {
                self.move_section(id == "page.section.next")
            }
            "page.copy" => {
                let p: PageTarget = serde_json::from_value(params)?;
                self.copy_page(p.page)
            }
            "page.paste" => {
                let p: PageTarget = serde_json::from_value(params)?;
                self.paste_page(p.page)
            }
            "page.delete" => {
                let p: PageTarget = serde_json::from_value(params)?;
                self.delete_page(p.page)
            }
            "file.insert" => {
                let p: InsertFile = serde_json::from_value(params)?;
                self.insert_file(&p)
            }
            "file.save_block" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Target {
                    path: std::path::PathBuf,
                }
                let p: Target = serde_json::from_value(params)?;
                self.save_block(&p.path)
            }
            _ => bail!("알 수 없는 편집 명령: {id}"),
        }
    }

    fn require_body(&self) -> Result<()> {
        if self.cursor.cell.is_some() {
            bail!("표 밖 본문에 커서를 두고 실행하세요.");
        }
        Ok(())
    }

    fn page_border_fill(&self) -> Result<Value> {
        let mut v: Value = serde_json::from_str(
            &self
                .core
                .get_page_border_fill_native(self.cursor.section)
                .map_err(err)?,
        )?;
        let mm = |k: &str, v: &Value| v[k].as_f64().unwrap_or(0.0) / HU_PER_MM;
        let spacing = [
            mm("spacingLeft", &v),
            mm("spacingRight", &v),
            mm("spacingTop", &v),
            mm("spacingBottom", &v),
        ];
        v["spacingMm"] = json!(spacing.map(|x| (x * 100.0).round() / 100.0));
        Ok(v)
    }

    fn border_fill_json(p: &BorderFill) -> Result<Value> {
        if !matches!(p.basis.as_str(), "paper" | "page") {
            bail!("위치 기준은 paper 또는 page입니다.");
        }
        if !matches!(p.apply_page.as_str(), "all" | "exceptFirst") {
            bail!("적용 쪽은 all 또는 exceptFirst입니다. 첫 쪽만은 지원하지 않습니다.");
        }
        let mut spacing = [0i64; 4];
        for (out, mm) in spacing.iter_mut().zip(p.spacing_mm) {
            if !mm.is_finite() || !(0.0..=100.0).contains(&mm) {
                bail!("테두리 간격은 0–100 mm여야 합니다.");
            }
            *out = (mm * HU_PER_MM).round() as i64;
        }
        let mut v = json!({
            "basis": p.basis,
            "spacingLeft": spacing[0],
            "spacingRight": spacing[1],
            "spacingTop": spacing[2],
            "spacingBottom": spacing[3],
            "headerInside": p.header_inside,
            "footerInside": p.footer_inside,
            "applyPage": p.apply_page,
            "fillArea": p.fill_area.as_deref().unwrap_or("paper"),
        });
        for (key, line) in ["borderLeft", "borderRight", "borderTop", "borderBottom"]
            .iter()
            .zip(&p.borders)
        {
            if line.kind > 16 || line.width > 15 {
                bail!("테두리 종류 0–16, 굵기 0–15 범위를 벗어났습니다.");
            }
            v[*key] = json!({"type": line.kind, "width": line.width, "color": color(&line.color)?});
        }
        match &p.fill {
            Some(c) => {
                v["fillType"] = json!("solid");
                v["fillColor"] = json!(color(c)?);
            }
            None => v["fillType"] = json!("none"),
        }
        Ok(v)
    }

    fn header_footer_list(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self
                .core
                .get_header_footer_list_native(self.cursor.section, true, 0)
                .map_err(err)?,
        )?)
    }

    /// First 쪽 번호 위치 control of the cursor's section.
    fn page_number_control(&self) -> Option<(usize, usize, PageNumberPos)> {
        let section = self.core.document().sections.get(self.cursor.section)?;
        section.paragraphs.iter().enumerate().find_map(|(pi, p)| {
            p.controls.iter().enumerate().find_map(|(ci, c)| match c {
                Control::PageNumberPos(n) => Some((pi, ci, n.clone())),
                _ => None,
            })
        })
    }

    fn page_number(&self) -> Result<Value> {
        let start = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .map(|s| s.section_def.page_num)
            .unwrap_or(0);
        Ok(match self.page_number_control() {
            Some((_, _, n)) => json!({
                "exists": true,
                "position": NUMBER_POSITIONS.iter().find(|(_, c)| *c == n.position).map(|(k, _)| *k).unwrap_or("none"),
                "format": n.format,
                "dash": n.dash_char != '\0',
                "start": start,
            }),
            None => {
                json!({"exists": false, "position": "bottom_center", "format": 0, "dash": true, "start": start})
            }
        })
    }

    fn set_page_number(&mut self, p: &PageNumber) -> Result<()> {
        let position = NUMBER_POSITIONS
            .iter()
            .find(|(k, _)| *k == p.position)
            .map(|(_, c)| *c)
            .ok_or_else(|| anyhow!("알 수 없는 쪽 번호 위치: {}", p.position))?;
        if !NUMBER_FORMATS.iter().any(|(c, _)| *c == p.format) {
            bail!("지원하지 않는 번호 모양입니다: {}", p.format);
        }
        if p.start == Some(0) {
            bail!("시작 번호는 1 이상이어야 합니다.");
        }
        self.require_body()?;
        let control = PageNumberPos {
            format: p.format,
            position,
            dash_char: if p.dash { '-' } else { '\0' },
            ..Default::default()
        };
        let existing = self.page_number_control();
        let cursor = self.cursor;
        self.transaction(|s| {
            let (pi, ci) = match existing {
                Some((pi, ci, _)) => (pi, ci),
                None => {
                    // RHWP has no public pgnp insert; reuse its inline-control insertion
                    // (placeholder, char count, line starts) and swap the control kind.
                    let r: Value = serde_json::from_str(
                        &s.core
                            .insert_new_number_native(cursor.section, cursor.paragraph, 0, 1)
                            .map_err(err)?,
                    )?;
                    let ci = r["controlIdx"]
                        .as_u64()
                        .and_then(|v| usize::try_from(v).ok())
                        .ok_or_else(|| anyhow!("쪽 번호 컨트롤 위치를 찾지 못했습니다."))?;
                    (cursor.paragraph, ci)
                }
            };
            {
                let para = s
                    .core
                    .document_mut()
                    .sections
                    .get_mut(cursor.section)
                    .and_then(|sec| sec.paragraphs.get_mut(pi))
                    .ok_or_else(|| anyhow!("쪽 번호 문단을 찾지 못했습니다."))?;
                let slot = para
                    .controls
                    .get_mut(ci)
                    .ok_or_else(|| anyhow!("쪽 번호 컨트롤을 찾지 못했습니다."))?;
                *slot = Control::PageNumberPos(control);
                if !para
                    .controls
                    .iter()
                    .any(|c| matches!(c, Control::NewNumber(_)))
                {
                    para.control_mask &= !(1 << 0x12);
                }
                para.control_mask |= 1 << 0x15;
            }
            if let Some(sec) = s.core.document_mut().sections.get_mut(cursor.section) {
                sec.raw_stream = None;
            }
            // Recompose and paginate the section; also sets the start number.
            let def = match p.start {
                Some(n) => json!({"pageNum": n}),
                None => json!({}),
            };
            s.core
                .set_section_def_native(cursor.section, &def.to_string())
                .map_err(err)?;
            Ok(())
        })
    }

    /// 문서 정보: statistics, fonts and pictures (RHWP keeps no summary properties).
    fn document_statistics(&self) -> Value {
        let doc = self.core.document();
        let mut paragraphs = 0usize;
        let mut chars = 0usize;
        let mut spaces = 0usize;
        let mut words = 0usize;
        let mut lines = 0usize;
        let (mut tables, mut pictures, mut shapes, mut equations, mut notes) = (0, 0, 0, 0, 0);
        for section in &doc.sections {
            for p in &section.paragraphs {
                paragraphs += 1;
                chars += p.text.chars().filter(|c| !c.is_control()).count();
                spaces += p.text.chars().filter(|c| c.is_whitespace()).count();
                words += p.text.split_whitespace().count();
                lines += p.line_segs.len().max(1);
                for c in &p.controls {
                    match c {
                        Control::Table(_) => tables += 1,
                        Control::Picture(_) => pictures += 1,
                        Control::Shape(_) => shapes += 1,
                        Control::Equation(_) => equations += 1,
                        Control::Footnote(_) | Control::Endnote(_) => notes += 1,
                        _ => {}
                    }
                }
            }
        }
        let fonts: Vec<String> = {
            let mut names: Vec<String> = doc
                .doc_info
                .font_faces
                .iter()
                .flatten()
                .map(|f| f.name.clone())
                .filter(|n| !n.is_empty())
                .collect();
            names.sort();
            names.dedup();
            names
        };
        let secs = |t: std::io::Result<std::time::SystemTime>| {
            t.ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
        };
        let file = self.path.as_ref().and_then(|p| {
            let meta = std::fs::metadata(p).ok()?;
            Some(json!({
                "path": p.display().to_string(),
                "format": p.extension().and_then(|e| e.to_str()).map(str::to_ascii_uppercase),
                "bytes": meta.len(),
                "created": secs(meta.created()),
                "modified": secs(meta.modified()),
            }))
        });
        // RHWP's own summary adds the fonts it had to substitute on this machine.
        let rhwp: Value =
            serde_json::from_str(&self.core.get_document_info()).unwrap_or(Value::Null);
        let mut used_by = std::collections::HashMap::<u16, usize>::new();
        let mut count_pictures = |paragraphs: &[Paragraph]| {
            for p in paragraphs {
                for c in &p.controls {
                    match c {
                        Control::Picture(pic) => {
                            *used_by.entry(pic.image_attr.bin_data_id).or_default() += 1
                        }
                        Control::Table(t) => {
                            for cell in &t.cells {
                                for p in &cell.paragraphs {
                                    for c in &p.controls {
                                        if let Control::Picture(pic) = c {
                                            *used_by
                                                .entry(pic.image_attr.bin_data_id)
                                                .or_default() += 1;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        };
        for section in &doc.sections {
            count_pictures(&section.paragraphs);
        }
        let picture_data: Vec<Value> = doc
            .doc_info
            .bin_data_list
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let id = u16::try_from(i + 1).unwrap_or(u16::MAX);
                let bytes = doc
                    .bin_data_content
                    .iter()
                    .find(|c| c.id == b.storage_id)
                    .map(|c| c.data.len());
                json!({
                    "id": id,
                    "type": format!("{:?}", b.data_type),
                    "extension": b.extension.clone().or_else(|| b.abs_path.clone()).or_else(|| b.rel_path.clone()),
                    "bytes": bytes,
                    "usedBy": used_by.get(&id).copied().unwrap_or(0),
                })
            })
            .collect();
        json!({
            "file": file,
            "pages": self.core.page_count(),
            "sections": doc.sections.len(),
            "paragraphs": paragraphs,
            "lines": lines,
            "characters": chars,
            "charactersNoSpaces": chars - spaces,
            "words": words,
            "tables": tables,
            "pictures": pictures,
            "shapes": shapes,
            "equations": equations,
            "notes": notes,
            "fonts": fonts,
            "binData": doc.doc_info.bin_data_list.len(),
            "pictureData": picture_data,
            "fontSubstitutions": rhwp["fontSubstitutions"].clone(),
            "fallbackFont": rhwp["fallbackFont"].clone(),
            "formatVersion": rhwp["version"].clone(),
            "summarySupported": false,
        })
    }
}

// ----- 0.13: 보기 표식 · 머리말/꼬리말 필드 · 구역 설정 · 개인 정보 · 쪽 범위 저장 -----

/// Screen-only view flags (문단 부호·조판 부호·투명 선). Never written to the document
/// and not part of the undo history; changing them bumps `generation` so cached page
/// images are redrawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ViewMarks {
    #[serde(default)]
    pub paragraph_marks: bool,
    #[serde(default)]
    pub control_codes: bool,
    #[serde(default)]
    pub transparent_borders: bool,
}

fn hf_field(kind: &str) -> Result<(u8, char)> {
    HF_FIELDS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, code, c)| (*code, *c))
        .ok_or_else(|| {
            anyhow!(
                "머리말/꼬리말 필드는 page_number, total_pages, file_name 중 하나입니다: {kind}"
            )
        })
}

impl Session {
    /// RHWP keeps the view flags private on `DocumentCore`; only its wasm wrapper
    /// `HwpDocument` has setters. Swap the live core into a throw-away wrapper.
    fn with_view_wrapper<T>(&mut self, f: impl FnOnce(&mut hwpcraft_doc::HwpDocument) -> T) -> T {
        let mut wrapper = hwpcraft_doc::HwpDocument::create_empty();
        std::mem::swap(&mut *wrapper, &mut self.core);
        let out = f(&mut wrapper);
        std::mem::swap(&mut *wrapper, &mut self.core);
        out
    }
    pub fn view_marks(&mut self) -> ViewMarks {
        self.with_view_wrapper(|d| ViewMarks {
            paragraph_marks: d.get_show_paragraph_marks(),
            control_codes: d.get_show_control_codes(),
            transparent_borders: d.get_show_transparent_borders(),
        })
    }
    /// Returns the flags now in effect; a new document (`open`/`reset`) starts with none.
    pub fn set_view_marks(&mut self, marks: ViewMarks) -> ViewMarks {
        let changed = self.with_view_wrapper(|d| {
            let before = ViewMarks {
                paragraph_marks: d.get_show_paragraph_marks(),
                control_codes: d.get_show_control_codes(),
                transparent_borders: d.get_show_transparent_borders(),
            };
            if before != marks {
                d.set_show_paragraph_marks(marks.paragraph_marks);
                d.set_show_control_codes(marks.control_codes);
                d.set_show_transparent_borders(marks.transparent_borders);
            }
            before != marks
        });
        if changed {
            self.generation += 1;
        }
        marks
    }

    /// First paragraph of the header/footer `(section, header, apply)`; `None` when absent.
    fn hf_paragraph(&self, header: bool, apply: u8) -> Result<Option<&Paragraph>> {
        let info: Value = serde_json::from_str(
            &self
                .core
                .get_header_footer_native(self.cursor.section, header, apply)
                .map_err(err)?,
        )?;
        if info["exists"] != true {
            return Ok(None);
        }
        let index = |k: &str| {
            info[k]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| anyhow!("머리말/꼬리말 주소를 읽지 못했습니다."))
        };
        let (pi, ci) = (index("paraIndex")?, index("controlIndex")?);
        let control = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .and_then(|s| s.paragraphs.get(pi))
            .and_then(|p| p.controls.get(ci));
        let paragraphs = match control {
            Some(Control::Header(h)) => &h.paragraphs,
            Some(Control::Footer(f)) => &f.paragraphs,
            _ => bail!("머리말/꼬리말을 찾지 못했습니다."),
        };
        if paragraphs.len() != 1 {
            bail!("여러 문단으로 된 머리말/꼬리말은 이 창에서 고칠 수 없습니다.");
        }
        Ok(paragraphs.first())
    }

    /// Text and field markers of a header/footer: `{exists, text, fields:[{offset, kind}]}`.
    /// `text` has the markers removed; each field offset refers to that text.
    fn header_footer_fields(&self, header: bool, apply: u8) -> Result<Value> {
        let Some(p) = self.hf_paragraph(header, apply)? else {
            return Ok(json!({"exists": false, "text": "", "fields": [], "applyTo": apply}));
        };
        use hwpcraft_doc::model::control::{AutoNumberType, FieldType};
        // Freshly inserted fields are marker characters; after a save RHWP lowers them to
        // AutoNumber controls (placeholder space) and a path Field wrapping the file name.
        let positions = p.control_text_positions();
        let chars: Vec<char> = p.text.chars().collect();
        let mut at_control: Vec<(usize, usize, &str)> = Vec::new(); // (start, end, kind)
        for (ci, control) in p.controls.iter().enumerate() {
            let Some(&start) = positions.get(ci) else {
                continue;
            };
            // An AutoNumber owns one placeholder space; RHWP reports a trailing one at `len`.
            let placeholder = |start: usize| {
                let at = start.min(chars.len().saturating_sub(1));
                if chars.get(at) == Some(&' ') {
                    (at, at + 1)
                } else {
                    (at, at)
                }
            };
            match control {
                Control::AutoNumber(a) if a.number_type == AutoNumberType::Page => {
                    let (s, e) = placeholder(start);
                    at_control.push((s, e, "page_number"));
                }
                Control::AutoNumber(a) if a.number_type == AutoNumberType::TotalPage => {
                    let (s, e) = placeholder(start);
                    at_control.push((s, e, "total_pages"));
                }
                Control::Field(f) if f.field_type == FieldType::Path => {
                    // The field wraps the literal file name (empty for an unsaved document).
                    let (s, e) = p
                        .field_ranges
                        .iter()
                        .find(|r| r.control_idx == ci)
                        .map_or((start, start), |r| {
                            (r.start_char_idx, r.end_char_idx.max(r.start_char_idx))
                        });
                    at_control.push((s, e, "file_name"));
                }
                _ => {}
            }
        }
        at_control.sort_by_key(|(s, e, _)| (*s, *e));
        let mut text = String::new();
        let mut fields = Vec::new();
        let mut offset = 0usize;
        let mut skip_until = 0usize;
        let mut next_control = 0usize;
        for (i, c) in chars.iter().copied().enumerate() {
            // Zero-width fields (empty file name) sit before the character at `i`.
            let mut skipped = false;
            while let Some((s, e, kind)) = at_control.get(next_control).copied()
                && s <= i
            {
                fields.push(HfField {
                    offset,
                    kind: kind.to_owned(),
                });
                skip_until = skip_until.max(e);
                next_control += 1;
                skipped |= e > i;
            }
            if i < skip_until || skipped {
                continue;
            }
            match HF_FIELDS.iter().find(|(_, _, m)| *m == c) {
                Some((kind, _, _)) => fields.push(HfField {
                    offset,
                    kind: (*kind).to_owned(),
                }),
                None if c.is_control() => {}
                None => {
                    text.push(c);
                    offset += 1;
                }
            }
        }
        Ok(json!({"exists": true, "text": text, "fields": fields, "applyTo": apply}))
    }

    /// Replace a header/footer with one line of text plus page-number/total/file-name fields.
    fn set_header_footer_fields(&mut self, p: &HfText) -> Result<()> {
        if p.text.len() > 10_000 || p.text.chars().any(char::is_control) {
            bail!("머리말/꼬리말은 제어 문자 없는 한 줄, 10 KB 이하로 입력하세요.");
        }
        let length = p.text.chars().count();
        let mut fields: Vec<(usize, usize, u8)> = Vec::with_capacity(p.fields.len());
        for (i, f) in p.fields.iter().enumerate() {
            if f.offset > length {
                bail!("필드 위치 {}가 글자 수 {length}를 넘습니다.", f.offset);
            }
            fields.push((f.offset, i, hf_field(&f.kind)?.0));
        }
        // Insert from the end so earlier offsets stay valid. Fields sharing an offset are
        // inserted last-listed first, so the listed order is the final order.
        fields.sort_by_key(|f| std::cmp::Reverse((f.0, f.1)));
        let apply = apply_code(&p.apply_to)?;
        self.require_body()?;
        let exists = self.hf_paragraph(p.header, apply)?.is_some();
        let header = p.header;
        let text = p.text.clone();
        self.transaction(|s| {
            let section = s.cursor.section;
            // A reopened header may hold lowered AutoNumber/Field controls; rebuilding it
            // from scratch keeps text, fields and controls consistent.
            if exists {
                s.core
                    .delete_header_footer_native(section, header, apply)
                    .map_err(err)?;
            }
            s.core
                .create_header_footer_native(section, header, apply)
                .map_err(err)?;
            if !text.is_empty() {
                s.core
                    .insert_text_in_header_footer_native(section, header, apply, 0, 0, &text)
                    .map_err(err)?;
            }
            for (offset, _, code) in &fields {
                s.core
                    .insert_field_in_hf_native(section, header, apply, 0, *offset, *code)
                    .map_err(err)?;
            }
            Ok(())
        })
    }

    /// 구역 설정 of the cursor's section, numbers as stored plus mm conversions.
    fn section_def(&self) -> Result<Value> {
        let mut v: Value = serde_json::from_str(
            &self
                .core
                .get_section_def_native(self.cursor.section)
                .map_err(err)?,
        )?;
        let tab = v["defaultTabSpacing"].as_f64().unwrap_or(0.0) / HU_PER_MM;
        v["defaultTabSpacingMm"] = json!((tab * 100.0).round() / 100.0);
        v["section"] = json!(self.cursor.section);
        v["sections"] = json!(self.core.document().sections.len());
        v["lineNumbersSupported"] = json!(false);
        Ok(v)
    }

    fn section_def_json(p: &SectionDef) -> Result<Value> {
        let mut v = json!({});
        if let Some(n) = p.page_num {
            v["pageNum"] = json!(n);
        }
        if let Some(t) = p.page_num_type {
            if t > 3 {
                bail!("쪽 번호 종류는 0–3입니다.");
            }
            v["pageNumType"] = json!(t);
        }
        for (key, value) in [
            ("pictureNum", p.picture_num),
            ("tableNum", p.table_num),
            ("equationNum", p.equation_num),
        ] {
            if let Some(n) = value {
                v[key] = json!(n);
            }
        }
        if let Some(mm) = p.default_tab_spacing_mm {
            if !mm.is_finite() || !(0.0..=100.0).contains(&mm) {
                bail!("기본 탭 간격은 0–100 mm입니다.");
            }
            v["defaultTabSpacing"] = json!((mm * HU_PER_MM).round() as u32);
        }
        for (key, value) in [
            ("hideHeader", p.hide_header),
            ("hideFooter", p.hide_footer),
            ("hideMasterPage", p.hide_master_page),
            ("hideBorder", p.hide_border),
            ("hideFill", p.hide_fill),
            ("hideEmptyLine", p.hide_empty_line),
        ] {
            if let Some(b) = value {
                v[key] = json!(b);
            }
        }
        if v.as_object().is_some_and(|o| o.is_empty()) {
            bail!("바꿀 구역 설정 항목을 하나 이상 지정하세요.");
        }
        Ok(v)
    }

    /// Personal data (validated by RHWP) and hidden text. Read only; `raw` values are the data itself.
    fn privacy_scan(&self, p: &PrivacyScan) -> Result<Value> {
        let mask = {
            let mut chars = p.mask.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => c,
                _ => bail!("가릴 문자는 한 글자여야 합니다."),
            }
        };
        let kinds: Vec<PiiKind> = if p.kinds.is_empty() {
            PiiKind::all().to_vec()
        } else {
            p.kinds
                .iter()
                .map(|k| PiiKind::parse(k).ok_or_else(|| anyhow!("알 수 없는 개인 정보 종류: {k}")))
                .collect::<Result<_>>()?
        };
        let doc = self.core.document();
        let findings: Vec<Value> = self
            .core
            .scan_pii(&kinds, mask)
            .into_iter()
            .map(|f| {
                // Body matches can be selected; cell/text-box matches only name the body paragraph.
                let in_body = doc
                    .sections
                    .get(f.section)
                    .and_then(|s| s.paragraphs.get(f.paragraph))
                    .is_some_and(|q| {
                        q.text
                            .chars()
                            .skip(f.char_offset)
                            .take(f.raw.chars().count())
                            .eq(f.raw.chars())
                    });
                let label = PII_KINDS
                    .iter()
                    .find(|(k, _)| *k == f.kind)
                    .map_or(f.kind, |(_, l)| *l);
                json!({
                    "kind": f.kind, "label": label, "raw": f.raw, "masked": f.masked,
                    "section": f.section, "paragraph": f.paragraph, "page": f.page,
                    "charOffset": f.char_offset, "inBody": in_body,
                })
            })
            .collect();
        let hidden = if p.hidden_text {
            let options = HiddenTextOptions {
                include_off_page: p.off_page,
                ..Default::default()
            };
            serde_json::to_value(self.core.detect_hidden_text(&options))?
        } else {
            Value::Null
        };
        Ok(json!({"findings": findings, "hiddenText": hidden}))
    }

    /// Replace every occurrence of each value with its mask in one undo step.
    fn privacy_mask(&mut self, p: &PrivacyMask) -> Result<Value> {
        if p.items.is_empty() {
            bail!("가릴 항목을 선택하세요.");
        }
        for item in &p.items {
            if item.raw.is_empty()
                || item.raw.chars().count() != item.masked.chars().count()
                || item.raw.chars().any(char::is_control)
                || item.masked.chars().any(char::is_control)
            {
                bail!("가릴 값과 가린 결과는 글자 수가 같은 한 줄 텍스트여야 합니다.");
            }
            if item.raw.len() > 1_000 {
                bail!("가릴 값은 1,000바이트 이하입니다.");
            }
        }
        let items: Vec<(String, String)> = p
            .items
            .iter()
            .filter(|i| i.raw != i.masked)
            .map(|i| (i.raw.clone(), i.masked.clone()))
            .collect();
        let mut replaced = 0u64;
        self.transaction(|s| {
            for (raw, masked) in &items {
                let r: Value = serde_json::from_str(
                    &s.core.replace_all_native(raw, masked, true).map_err(err)?,
                )?;
                replaced += r["count"].as_u64().unwrap_or(0);
            }
            Ok(())
        })?;
        Ok(json!({"replaced": replaced}))
    }

    /// Save only `from..=to` pages (1-based). The open document, cursor, path and undo
    /// history stay unchanged: the extraction runs on a snapshot that is restored afterwards.
    fn save_pages(&mut self, p: &SavePages) -> Result<Value> {
        let count = self.core.page_count();
        if p.from == 0 || p.to < p.from || p.to > count {
            bail!(
                "쪽 범위는 1–{count} 안에서 작은 번호부터 적으세요: {}–{}",
                p.from,
                p.to
            );
        }
        if !matches!(
            p.path
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("hwp" | "hwpx" | "pdf" | "txt")
        ) {
            bail!("쪽 범위 저장 형식: HWP, HWPX, PDF, TXT");
        }
        let id = self.core.save_snapshot_native();
        let extracted = self
            .core
            .extract_page_range(p.from, p.to)
            .map_err(err)
            .and_then(|report| self.save_with_tab_widths(&p.path).map(|()| report));
        let restored = self.core.restore_snapshot_native(id).map_err(err);
        self.core.discard_snapshot_native(id);
        restored?;
        let report = extracted?;
        Ok(json!({
            "path": p.path,
            "pagesBefore": report.pages_before,
            "pagesAfter": report.pages_after,
            "paragraphsKept": report.kept,
            "paragraphsRemoved": report.removed,
        }))
    }
}

// ----- 0.14: 쪽 여백 사전 값 · 구역 이동 · 쪽 복사/붙이기/지우기 · 문서 끼워 넣기 · 블록 저장 -----

/// Hangul's 쪽 여백 gallery, in its order: id, label, [top, bottom, left, right, header,
/// footer] in millimetres. The gutter and paper keep their current values.
pub const MARGIN_PRESETS: &[(&str, &str, [f64; 6])] = &[
    ("default", "기본", [20.0, 15.0, 30.0, 30.0, 15.0, 15.0]),
    (
        "narrow1_hf",
        "좁게1(머리말/꼬리말 여백포함)",
        [15.0, 15.0, 20.0, 20.0, 10.0, 10.0],
    ),
    (
        "narrow2_hf",
        "좁게2(머리말/꼬리말 여백포함)",
        [5.0, 5.0, 10.0, 10.0, 10.0, 10.0],
    ),
    (
        "wide_hf",
        "넓게(머리말/꼬리말 여백포함)",
        [35.0, 35.0, 40.0, 40.0, 10.0, 10.0],
    ),
    ("narrow", "좁게", [10.0, 10.0, 10.0, 10.0, 0.0, 0.0]),
    ("wide", "넓게", [35.0, 35.0, 40.0, 40.0, 0.0, 0.0]),
];

fn margin_presets_json() -> Value {
    Value::Array(
        MARGIN_PRESETS
            .iter()
            .map(|(id, label, m)| {
                json!({"preset": id, "label": label, "topMm": m[0], "bottomMm": m[1],
                    "leftMm": m[2], "rightMm": m[3], "headerMm": m[4], "footerMm": m[5]})
            })
            .collect(),
    )
}

/// 1-based page for CLI/MCP callers; `None` means the caret's page.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PageTarget {
    #[serde(default)]
    page: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InsertFile {
    path: std::path::PathBuf,
    /// Password of the inserted document, if it is protected.
    #[serde(default)]
    password: Option<String>,
}

/// The copied page (Hangul puts it on the system clipboard; HwpCraft keeps it per process so
/// the CLI, MCP and desktop sessions share one 쪽 클립보드). Never written to disk.
static PAGE_CLIPBOARD: std::sync::Mutex<Option<Document>> = std::sync::Mutex::new(None);

fn page_clipboard() -> std::sync::MutexGuard<'static, Option<Document>> {
    match PAGE_CLIPBOARD.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// `Document` holding only `range` of section `sec` of `doc`, with the format tables kept so
/// `paste_foreign_document_native` can remap them.
fn excerpt(doc: &Document, sec: usize, range: std::ops::Range<usize>) -> Result<Document> {
    let section = doc
        .sections
        .get(sec)
        .ok_or_else(|| anyhow!("구역 {}이 없습니다.", sec + 1))?;
    let paragraphs = section
        .paragraphs
        .get(range)
        .ok_or_else(|| anyhow!("문단 범위를 읽지 못했습니다."))?
        .to_vec();
    let mut copy = section.clone();
    copy.paragraphs = paragraphs;
    copy.raw_stream = None;
    copy.raw_provenance = None;
    let mut foreign = doc.clone();
    foreign.sections = vec![copy];
    Ok(foreign)
}

fn usize_field(v: &Value, key: &str) -> Result<usize> {
    v[key]
        .as_u64()
        .and_then(|x| usize::try_from(x).ok())
        .ok_or_else(|| anyhow!("RHWP 응답에 {key}가 없습니다: {v}"))
}

impl Session {
    fn apply_margin_preset(&mut self, preset: &str) -> Result<Value> {
        let Some((_, label, m)) = MARGIN_PRESETS.iter().find(|(id, _, _)| *id == preset) else {
            let ids: Vec<&str> = MARGIN_PRESETS.iter().map(|(id, _, _)| *id).collect();
            bail!(
                "쪽 여백 사전 값은 {} 중 하나입니다: {preset}",
                ids.join(", ")
            );
        };
        let mut setup = self.page_setup()?;
        setup.top_mm = m[0];
        setup.bottom_mm = m[1];
        setup.left_mm = m[2];
        setup.right_mm = m[3];
        setup.header_mm = m[4];
        setup.footer_mm = m[5];
        self.execute(Command::PageSetup(setup.clone()))?;
        Ok(json!({"preset": preset, "label": label, "setup": setup}))
    }

    /// Hangul SectionToNext/SectionToPrev: caret to the first paragraph of the neighbouring
    /// section.
    fn move_section(&mut self, next: bool) -> Result<Value> {
        let count = self.core.document().sections.len();
        let current = self.cursor.section;
        let target = if next {
            current + 1
        } else {
            current.wrapping_sub(1)
        };
        if target >= count {
            bail!(if next {
                "마지막 구역입니다."
            } else {
                "첫 구역입니다."
            });
        }
        self.select(
            Cursor {
                section: target,
                paragraph: 0,
                offset: 0,
                cell: None,
            },
            false,
        )?;
        Ok(json!({"section": target + 1, "sections": count}))
    }

    /// 0-based page: the caller's 1-based `page`, else the caret's page.
    fn page_target(&self, page: Option<u32>) -> Result<u32> {
        let count = self.core.page_count();
        match page {
            Some(0) => bail!("쪽 번호는 1부터입니다."),
            Some(p) if p > count => bail!("{p}쪽은 범위(1–{count})를 벗어났습니다."),
            Some(p) => Ok(p - 1),
            None => {
                let caret = self.caret()?;
                let page = caret["pageIndex"].as_u64().unwrap_or(0);
                Ok(u32::try_from(page)
                    .unwrap_or(0)
                    .min(count.saturating_sub(1)))
            }
        }
    }

    /// (section, first paragraph) of a 0-based page from RHWP's pagination.
    fn page_start(&self, page: u32) -> Result<(usize, usize)> {
        let v: Value = serde_json::from_str(
            &self
                .core
                .get_position_of_page_native(page as usize)
                .map_err(err)?,
        )?;
        Ok((usize_field(&v, "sec")?, usize_field(&v, "para")?))
    }

    /// Paragraphs that start on `page` and end before the next page starts, as
    /// `(section, range)`. The range is empty when one paragraph covers the whole page.
    fn page_paragraphs(&self, page: u32) -> Result<(usize, std::ops::Range<usize>)> {
        let (sec, first) = self.page_start(page)?;
        let total = self
            .core
            .document()
            .sections
            .get(sec)
            .map_or(0, |s| s.paragraphs.len());
        let end = if page + 1 < self.core.page_count() {
            match self.page_start(page + 1)? {
                (s, p) if s == sec => p.min(total),
                _ => total,
            }
        } else {
            total
        };
        Ok((sec, first..end.max(first)))
    }

    fn text_len(&self, section: usize, paragraph: usize) -> Result<usize> {
        Ok(self
            .paragraph(Cursor {
                section,
                paragraph,
                offset: 0,
                cell: None,
            })?
            .text
            .chars()
            .count())
    }

    /// Hangul CopyPage: the page's paragraphs go to the 쪽 클립보드 (paragraph granularity:
    /// a paragraph that starts on an earlier page stays out).
    fn copy_page(&mut self, page: Option<u32>) -> Result<Value> {
        let page = self.page_target(page)?;
        let (sec, range) = self.page_paragraphs(page)?;
        let range = if range.is_empty() {
            range.start..range.start + 1
        } else {
            range
        };
        let foreign = excerpt(self.core.document(), sec, range.clone())?;
        *page_clipboard() = Some(foreign);
        Ok(json!({"page": page + 1, "paragraphs": range.len()}))
    }

    /// Hangul PastePage: the copied page becomes a new page right after the current one and
    /// the caret lands at the end of the pasted content (observed behaviour).
    fn paste_page(&mut self, page: Option<u32>) -> Result<Value> {
        let foreign = page_clipboard()
            .clone()
            .ok_or_else(|| anyhow!("복사한 쪽이 없습니다. 먼저 쪽 복사하기를 실행하세요."))?;
        let count: usize = foreign.sections.iter().map(|s| s.paragraphs.len()).sum();
        let page = self.page_target(page)?;
        let (sec, range) = self.page_paragraphs(page)?;
        let last = range.end.max(range.start + 1) - 1;
        let len = self.text_len(sec, last)?;
        let mut end = Cursor::default();
        self.transaction(|s| {
            let split = s.split_offset(Cursor {
                section: sec,
                paragraph: last,
                offset: len,
                cell: None,
            })?;
            s.core
                .insert_page_break_native(sec, last, split)
                .map_err(err)?;
            let r: Value = serde_json::from_str(
                &s.core
                    .paste_foreign_document_native(sec, last + 1, 0, foreign)
                    .map_err(err)?,
            )?;
            if r["ok"] != true {
                bail!("쪽을 붙이지 못했습니다: {}", r["error"]);
            }
            let paragraph = usize_field(&r, "paraIdx")?;
            let offset = usize_field(&r, "charOffset")?.min(s.text_len(sec, paragraph)?);
            end = Cursor {
                section: sec,
                paragraph,
                offset,
                cell: None,
            };
            s.cursor = end;
            s.anchor = None;
            s.span_anchor = None;
            Ok(())
        })?;
        Ok(json!({
            "afterPage": page + 1,
            "paragraphs": count,
            "pages": self.core.page_count(),
            "cursor": end,
        }))
    }

    /// Hangul DeleteCurrentPage: remove the paragraphs of the current page so the following
    /// pages move up. The section's first paragraph is emptied instead of removed because it
    /// carries the section definition and RHWP keeps at least one paragraph per section.
    fn delete_page(&mut self, page: Option<u32>) -> Result<Value> {
        let page = self.page_target(page)?;
        if self.core.page_count() <= 1 {
            bail!("쪽이 하나뿐인 문서에서는 쪽을 지울 수 없습니다.");
        }
        let (sec, range) = self.page_paragraphs(page)?;
        if range.is_empty() {
            bail!(
                "한 문단이 여러 쪽에 걸쳐 있어 이 쪽만 지울 수 없습니다. 문단 안에서 범위를 선택해 지우세요."
            );
        }
        let removed = range.len();
        self.transaction(|s| {
            let keep_first = range.start == 0;
            let from = if keep_first { 1 } else { range.start };
            for i in (from..range.end).rev() {
                if s.core.document().sections[sec].paragraphs.len() > 1 {
                    s.core.delete_paragraph_native(sec, i).map_err(err)?;
                }
            }
            if keep_first {
                let remaining = s.core.document().sections[sec].paragraphs.len();
                let len = s.text_len(sec, 0)?;
                if remaining > 1 {
                    // Merge the next page's first paragraph into the emptied first one.
                    s.core
                        .delete_range_native(sec, 0, 0, 1, 0, None)
                        .map_err(err)?;
                } else if len > 0 {
                    s.core
                        .delete_range_native(sec, 0, 0, 0, len, None)
                        .map_err(err)?;
                }
            }
            let remaining = s.core.document().sections[sec].paragraphs.len();
            s.cursor = Cursor {
                section: sec,
                paragraph: range.start.min(remaining.saturating_sub(1)),
                offset: 0,
                cell: None,
            };
            s.anchor = None;
            s.span_anchor = None;
            Ok(())
        })?;
        Ok(json!({"page": page + 1, "paragraphsRemoved": removed, "pages": self.core.page_count()}))
    }

    /// Hangul 문서 끼워 넣기 (FileInsertionDlg): the other document's body goes in at the caret
    /// with its fonts, styles and pictures merged into this document's tables.
    fn insert_file(&mut self, p: &InsertFile) -> Result<Value> {
        self.require_body()?;
        let core = match p.password.as_deref() {
            Some(password) => hwpcraft_formats::open_with_password(&p.path, password),
            None => hwpcraft_formats::open(&p.path),
        }?;
        let foreign = core.document().clone();
        let count: usize = foreign.sections.iter().map(|s| s.paragraphs.len()).sum();
        let c = self.cursor;
        let mut end = c;
        self.transaction(|s| {
            let split = s.split_offset(c)?;
            let r: Value = serde_json::from_str(
                &s.core
                    .paste_foreign_document_native(c.section, c.paragraph, split, foreign)
                    .map_err(err)?,
            )?;
            if r["ok"] != true {
                bail!("문서를 끼워 넣지 못했습니다: {}", r["error"]);
            }
            let paragraph = usize_field(&r, "paraIdx")?;
            let offset = usize_field(&r, "charOffset")?.min(s.text_len(c.section, paragraph)?);
            end = Cursor {
                section: c.section,
                paragraph,
                offset,
                cell: None,
            };
            s.cursor = end;
            s.anchor = None;
            s.span_anchor = None;
            Ok(())
        })?;
        Ok(json!({
            "path": p.path,
            "paragraphs": count,
            "pages": self.core.page_count(),
            "cursor": end,
        }))
    }

    /// Hangul SaveBlock: only the selected body text (one or more paragraphs) goes to a new
    /// file. The open document, cursor and history are untouched.
    fn save_block(&mut self, path: &Path) -> Result<Value> {
        if !self.has_selection() {
            bail!("저장할 블록을 먼저 선택하세요.");
        }
        if self.cursor.cell.is_some() {
            bail!("표 밖 본문의 블록만 저장할 수 있습니다.");
        }
        if !matches!(
            path.extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("hwp" | "hwpx" | "pdf" | "txt")
        ) {
            bail!("블록 저장 형식: HWP, HWPX, PDF, TXT");
        }
        let (start, end) = match self.span() {
            Some(span) => span,
            None => {
                let r = self.selection();
                (
                    Cursor {
                        offset: r.start,
                        ..self.cursor
                    },
                    Cursor {
                        offset: r.end,
                        ..self.cursor
                    },
                )
            }
        };
        let foreign = excerpt(
            self.core.document(),
            start.section,
            start.paragraph..end.paragraph + 1,
        )?;
        let span = end.paragraph - start.paragraph;
        let mut out = blank()?;
        let r: Value = serde_json::from_str(
            &out.paste_foreign_document_native(0, 0, 0, foreign)
                .map_err(err)?,
        )?;
        if r["ok"] != true {
            bail!("블록을 옮기지 못했습니다: {}", r["error"]);
        }
        let last = usize_field(&r, "paraIdx")?;
        let first = last.saturating_sub(span);
        let chars = |doc: &DocumentCore, i: usize| {
            doc.document().sections[0]
                .paragraphs
                .get(i)
                .map_or(0, |p| p.text.chars().count())
        };
        // Trim the tail, then the head, then drop whatever the blank document left behind.
        let tail = chars(&out, last);
        if end.offset < tail {
            out.delete_range_native(0, last, end.offset, last, tail, None)
                .map_err(err)?;
        }
        if start.offset > 0 {
            out.delete_range_native(0, first, 0, first, start.offset, None)
                .map_err(err)?;
        }
        while out.document().sections[0].paragraphs.len() > last + 1 {
            out.delete_paragraph_native(0, last + 1).map_err(err)?;
        }
        for _ in 0..first {
            // Leading blank paragraphs: fold them into the first kept one.
            out.delete_range_native(0, 0, 0, 1, 0, None).map_err(err)?;
        }
        hwpcraft_formats::save(&out, path)?;
        let text: Vec<String> = out.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect();
        Ok(json!({"path": path, "paragraphs": text.len(), "text": text.join("\n")}))
    }
}
