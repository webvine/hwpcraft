//! Owner: table/object area. 표: 표/셀 속성·블록 선택·셀 테두리/배경·합치기/나누기·계산식.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Units follow RHWP: HWPUNIT (1/7200 inch) for sizes, `#rrggbb` for colors.
//! A block is an inclusive grid range and is widened to whole merged cells,
//! like a Hangul F5 cell block.
use super::*;
use hwpcraft_doc::model::{
    paragraph::Paragraph,
    shape::{HorzAlign, HorzRelTo, TextWrap, VertAlign, VertRelTo},
    table::{Table, TablePageBreak},
};
use serde::{Deserialize, Serialize};
use serde_json::Map;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("table.create", "표 만들기", "Ctrl+N,T"),
    ("table.info", "표 정보", ""),
    ("table.properties.get", "표 속성 조회", ""),
    ("table.properties", "표/셀 속성", "P"),
    ("table.cell_properties.get", "셀 속성 조회", ""),
    ("table.cell_properties", "셀 속성 / 셀 테두리·배경", "L"),
    ("table.merge", "셀 합치기", "M"),
    ("table.split", "셀 나누기", "S"),
    ("table.equal_height", "셀 높이를 같게", "H"),
    ("table.equal_width", "셀 너비를 같게", "W"),
    ("table.resize", "셀 크기 조절", "Ctrl+방향키"),
    ("table.formula", "계산식", ""),
    (
        "table.block_calc",
        "블록 합계 / 평균 / 곱",
        "Ctrl+Shift+S / A / P",
    ),
    ("table.delete", "표 지우기", ""),
    ("table.transpose", "표 뒤집기 (줄/칸 바꾸기)", "T"),
    ("table.split_table", "표 나누기", "Ctrl+N,A"),
    ("table.merge_table", "표 붙이기", "Ctrl+N,Z"),
    ("table.thousands", "1,000 단위 구분 쉼표", ""),
    ("table.move_border", "셀 경계선 옮기기 (마우스 끌기)", ""),
    ("table.from_text", "문자열을 표로", ""),
    ("table.to_text", "표를 문자열로", ""),
    ("table.sort", "표 정렬 (줄 단위)", ""),
    ("table.auto_fill", "자동 채우기", ""),
    ("table.caption_text.get", "캡션 내용 조회", ""),
    ("table.caption_text", "캡션 내용 고치기", ""),
    ("table.style", "표 마당 (기본 모양)", ""),
];

pub(crate) const MAX_ROWS: usize = 200;
pub(crate) const MAX_COLUMNS: usize = 64;
pub(crate) const MAX_CELLS: usize = 4096;
const HU_PER_MM: f64 = 7200.0 / 25.4;

/// Inclusive grid range. Merged cells crossing the edge widen the range.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CellRange {
    pub start_row: u16,
    pub start_col: u16,
    pub end_row: u16,
    pub end_col: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    #[serde(default)]
    table: Option<ObjectAddress>,
    #[serde(default)]
    range: Option<CellRange>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    rows: u16,
    columns: u16,
    #[serde(default)]
    treat_as_char: bool,
    /// None: 단(본문) 폭에 맞춤.
    #[serde(default)]
    width_mm: Option<f64>,
    /// None: 자동(기본 줄 높이).
    #[serde(default)]
    height_mm: Option<f64>,
    #[serde(default)]
    column_widths_mm: Option<Vec<f64>>,
}

fn hu(mm: f64) -> u32 {
    (mm * HU_PER_MM).round() as u32
}
fn bad(key: &str) -> anyhow::Error {
    anyhow!("잘못된 표/셀 속성 값: {key}")
}

enum Kind {
    Int(i64, i64),
    Bool,
    Choice(&'static [&'static str]),
    Text(usize),
    Color,
    Line,
}
const LENGTH: Kind = Kind::Int(0, 14_173); // 0–50 mm
const OFFSET: Kind = Kind::Int(-170_079, 170_079); // ±600 mm
const SIZE: Kind = Kind::Int(283, 170_079); // 1–600 mm

const TABLE_KEYS: &[(&str, Kind)] = &[
    ("cellSpacing", Kind::Int(0, 2_834)),
    ("paddingLeft", LENGTH),
    ("paddingRight", LENGTH),
    ("paddingTop", LENGTH),
    ("paddingBottom", LENGTH),
    ("pageBreak", Kind::Int(0, 2)),
    ("repeatHeader", Kind::Bool),
    ("treatAsChar", Kind::Bool),
    (
        "textWrap",
        Kind::Choice(&["Square", "TopAndBottom", "BehindText", "InFrontOfText"]),
    ),
    ("vertRelTo", Kind::Choice(&["Paper", "Page", "Para"])),
    (
        "vertAlign",
        Kind::Choice(&["Top", "Center", "Bottom", "Inside", "Outside"]),
    ),
    (
        "horzRelTo",
        Kind::Choice(&["Paper", "Page", "Column", "Para"]),
    ),
    (
        "horzAlign",
        Kind::Choice(&["Left", "Center", "Right", "Inside", "Outside"]),
    ),
    ("vertOffset", OFFSET),
    ("horzOffset", OFFSET),
    ("restrictInPage", Kind::Bool),
    ("allowOverlap", Kind::Bool),
    ("keepWithAnchor", Kind::Bool),
    ("outerLeft", LENGTH),
    ("outerRight", LENGTH),
    ("outerTop", LENGTH),
    ("outerBottom", LENGTH),
    ("hasCaption", Kind::Bool),
    ("captionDirection", Kind::Int(0, 3)),
    ("captionVertAlign", Kind::Int(0, 2)),
    ("captionWidth", Kind::Int(0, 170_079)),
    ("captionSpacing", LENGTH),
    ("tableWidth", SIZE),
    ("tableHeight", SIZE),
];
const CELL_KEYS: &[(&str, Kind)] = &[
    ("width", SIZE),
    ("height", SIZE),
    ("paddingLeft", LENGTH),
    ("paddingRight", LENGTH),
    ("paddingTop", LENGTH),
    ("paddingBottom", LENGTH),
    ("applyInnerMargin", Kind::Bool),
    ("verticalAlign", Kind::Int(0, 2)),
    ("textDirection", Kind::Int(0, 1)),
    ("isHeader", Kind::Bool),
    ("cellProtect", Kind::Bool),
    ("editableInForm", Kind::Bool),
    ("fieldName", Kind::Text(255)),
    ("borderLeft", Kind::Line),
    ("borderRight", Kind::Line),
    ("borderTop", Kind::Line),
    ("borderBottom", Kind::Line),
    ("fillType", Kind::Choice(&["none", "solid"])),
    ("fillColor", Kind::Color),
    ("patternColor", Kind::Color),
    ("patternType", Kind::Int(0, 6)),
    ("diagonalSlash", Kind::Int(0, 7)),
    ("diagonalBackSlash", Kind::Int(0, 7)),
    ("diagonalLine", Kind::Int(0, 17)),
    ("diagonalWidth", Kind::Int(0, 15)),
    ("diagonalColor", Kind::Color),
    (
        "centerLine",
        Kind::Choice(&["NONE", "VERTICAL", "HORIZONTAL", "CROSS"]),
    ),
];
/// Edge names for block borders: outer edges of the block and the lines between its cells.
const EDGES: &[&str] = &[
    "left",
    "right",
    "top",
    "bottom",
    "inner_vertical",
    "inner_horizontal",
];

fn color_ok(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
    })
}
fn line_ok(v: &Value) -> bool {
    let Some(o) = v.as_object() else {
        return false;
    };
    o.keys()
        .all(|k| matches!(k.as_str(), "type" | "width" | "color"))
        && o.get("type")
            .and_then(Value::as_u64)
            .is_some_and(|t| t <= 17)
        && o.get("width")
            .and_then(Value::as_u64)
            .is_some_and(|w| w <= 15)
        && o.get("color").is_some_and(color_ok)
}
/// Reject unknown keys and out-of-range values before any document change.
fn checked(props: &Value, spec: &[(&str, Kind)]) -> Result<Map<String, Value>> {
    let object = props
        .as_object()
        .ok_or_else(|| anyhow!("속성은 JSON 객체여야 합니다."))?;
    for (key, value) in object {
        let (_, kind) = spec
            .iter()
            .find(|(k, _)| k == key)
            .ok_or_else(|| anyhow!("알 수 없는 표/셀 속성: {key}"))?;
        let ok = match kind {
            Kind::Int(lo, hi) => value.as_i64().is_some_and(|v| (*lo..=*hi).contains(&v)),
            Kind::Bool => value.is_boolean(),
            Kind::Choice(list) => value.as_str().is_some_and(|v| list.contains(&v)),
            Kind::Text(max) => value
                .as_str()
                .is_some_and(|v| v.chars().count() <= *max && !v.chars().any(char::is_control)),
            Kind::Color => color_ok(value),
            Kind::Line => line_ok(value),
        };
        if !ok {
            return Err(bad(key));
        }
    }
    Ok(object.clone())
}

fn wrap_name(w: TextWrap) -> &'static str {
    match w {
        TextWrap::Square | TextWrap::Tight | TextWrap::Through => "Square",
        TextWrap::TopAndBottom => "TopAndBottom",
        TextWrap::BehindText => "BehindText",
        TextWrap::InFrontOfText => "InFrontOfText",
    }
}
fn vert_rel_name(v: VertRelTo) -> &'static str {
    match v {
        VertRelTo::Paper => "Paper",
        VertRelTo::Page => "Page",
        VertRelTo::Para => "Para",
    }
}
fn vert_align_name(v: VertAlign) -> &'static str {
    match v {
        VertAlign::Top => "Top",
        VertAlign::Center => "Center",
        VertAlign::Bottom => "Bottom",
        VertAlign::Inside => "Inside",
        VertAlign::Outside => "Outside",
    }
}
fn horz_rel_name(v: HorzRelTo) -> &'static str {
    match v {
        HorzRelTo::Paper => "Paper",
        HorzRelTo::Page => "Page",
        HorzRelTo::Column => "Column",
        HorzRelTo::Para => "Para",
    }
}
fn horz_align_name(v: HorzAlign) -> &'static str {
    match v {
        HorzAlign::Left => "Left",
        HorzAlign::Center => "Center",
        HorzAlign::Right => "Right",
        HorzAlign::Inside => "Inside",
        HorzAlign::Outside => "Outside",
    }
}

/// Column letters used by Hangul table formulas: A..Z, AA..
fn column_name(mut col: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    name.reverse();
    String::from_utf8_lossy(&name).into_owned()
}

fn separator(delimiter: &str) -> Result<String> {
    Ok(match delimiter {
        "tab" => "\t".into(),
        "comma" => ",".into(),
        "space" => " ".into(),
        "semicolon" => ";".into(),
        other
            if !other.is_empty() && other.chars().count() <= 8 && !other.contains(['\n', '\r']) =>
        {
            other.into()
        }
        _ => bail!("구분 기호는 tab, comma, space, semicolon 또는 1–8자 문자열입니다."),
    })
}

/// 자동 채우기 series: a number with a step, or a cycle through a known list.
struct Series {
    list: Option<&'static [&'static str]>,
    start: f64,
    step: f64,
    decimals: usize,
    /// Whether the second cell defined the step (and is kept as is).
    step_from_second: bool,
}
const SERIES_LISTS: &[&[&str]] = &[
    &[
        "월요일",
        "화요일",
        "수요일",
        "목요일",
        "금요일",
        "토요일",
        "일요일",
    ],
    &[
        "1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월",
    ],
    &["1분기", "2분기", "3분기", "4분기"],
    &["갑", "을", "병", "정", "무", "기", "경", "신", "임", "계"],
    &[
        "자", "축", "인", "묘", "진", "사", "오", "미", "신", "유", "술", "해",
    ],
    &[
        "가", "나", "다", "라", "마", "바", "사", "아", "자", "차", "카", "타", "파", "하",
    ],
    &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
];
impl Series {
    fn detect(first: &str, second: Option<&str>) -> Option<Self> {
        let number = |t: &str| {
            t.replace(',', "")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
        };
        let decimals = |t: &str| t.split_once('.').map_or(0, |(_, f)| f.len());
        if let Some(a) = number(first) {
            let b = second.and_then(number);
            return Some(Self {
                list: None,
                start: a,
                step: b.map_or(1.0, |b| b - a),
                decimals: decimals(first).max(second.map_or(0, decimals)),
                step_from_second: b.is_some(),
            });
        }
        let list = *SERIES_LISTS.iter().find(|l| l.contains(&first))?;
        let i = list.iter().position(|x| *x == first)?;
        let j = second.and_then(|t| list.iter().position(|x| *x == t));
        let step = j.map_or(1, |j| (j + list.len() - i) % list.len()).max(1);
        Some(Self {
            list: Some(list),
            start: i as f64,
            step: step as f64,
            decimals: 0,
            step_from_second: j.is_some(),
        })
    }
    fn nth(&self, k: usize) -> String {
        let v = self.start + self.step * k as f64;
        match self.list {
            Some(list) => list[(v as usize) % list.len()].to_owned(),
            None => format!("{:.*}", self.decimals, v),
        }
    }
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_table_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_table_command(id, params.clone()))
    }
    fn run_table_command(&mut self, id: &str, params: Value) -> Result<Value> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Props {
            #[serde(default)]
            table: Option<ObjectAddress>,
            #[serde(default)]
            range: Option<CellRange>,
            #[serde(default)]
            props: Value,
            #[serde(default)]
            borders: Option<Map<String, Value>>,
        }
        match id {
            "table.create" => {
                let a = self.create_table(serde_json::from_value(params)?)?;
                Ok(json!({"table":a}))
            }
            "table.info" | "table.properties.get" | "table.cell_properties.get" => {
                let t: Target = serde_json::from_value(params)?;
                let a = self.table_target(t.table)?;
                match id {
                    "table.info" => self.table_layout(a, t.range),
                    "table.properties.get" => {
                        if t.range.is_some() {
                            bail!("표 속성 조회에는 range를 쓰지 않습니다.");
                        }
                        self.table_properties(a)
                    }
                    _ => self.cell_properties(a, t.range),
                }
            }
            "table.properties" => {
                let p: Props = serde_json::from_value(params)?;
                if p.range.is_some() || p.borders.is_some() {
                    bail!("표 속성에는 props만 사용합니다. 셀은 table.cell_properties를 쓰세요.");
                }
                let a = self.table_target(p.table)?;
                self.set_table_properties(a, &p.props)?;
                Ok(self.info())
            }
            "table.cell_properties" => {
                let p: Props = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.set_cell_properties(a, p.range, &p.props, p.borders.as_ref())?;
                Ok(self.info())
            }
            "table.transpose" | "table.split_table" | "table.merge_table" => {
                let t: Target = serde_json::from_value(params)?;
                let a = self.table_target(t.table)?;
                if t.range.is_some() && id != "table.split_table" {
                    bail!("{id}에는 range를 쓰지 않습니다.");
                }
                self.restructure_table(a, id, t.range)?;
                Ok(self.info())
            }
            "table.thousands" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Thousands {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    #[serde(default)]
                    range: Option<CellRange>,
                    #[serde(default)]
                    remove: bool,
                }
                let p: Thousands = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                Ok(json!({"cells":self.thousands(a, p.range, p.remove)?}))
            }
            "table.merge" | "table.equal_height" | "table.equal_width" | "table.delete" => {
                let t: Target = serde_json::from_value(params)?;
                let a = self.table_target(t.table)?;
                match id {
                    "table.merge" => self.merge_cells(a, t.range)?,
                    "table.equal_height" => self.equalize(a, t.range, true)?,
                    "table.equal_width" => self.equalize(a, t.range, false)?,
                    _ => {
                        if t.range.is_some() {
                            bail!("표 지우기에는 range를 쓰지 않습니다.");
                        }
                        self.delete_table(a)?
                    }
                }
                Ok(self.info())
            }
            "table.split" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Split {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    #[serde(default)]
                    range: Option<CellRange>,
                    rows: u16,
                    columns: u16,
                    #[serde(default)]
                    equal_height: bool,
                    #[serde(default)]
                    merge_first: bool,
                }
                let p: Split = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.split_cells(a, p.range, p.rows, p.columns, p.equal_height, p.merge_first)?;
                Ok(self.info())
            }
            "table.from_text" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct FromText {
                    delimiter: String,
                }
                let p: FromText = serde_json::from_value(params)?;
                Ok(json!({"table":self.text_to_table(&p.delimiter)?}))
            }
            "table.to_text" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct ToText {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    delimiter: String,
                }
                let p: ToText = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.table_to_text(a, &p.delimiter)?;
                Ok(self.info())
            }
            "table.sort" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Sort {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    #[serde(default)]
                    range: Option<CellRange>,
                    column: u16,
                    #[serde(default)]
                    descending: bool,
                }
                let p: Sort = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.sort_rows(a, p.range, p.column, p.descending)?;
                Ok(self.info())
            }
            "table.auto_fill" => {
                let t: Target = serde_json::from_value(params)?;
                let a = self.table_target(t.table)?;
                Ok(json!({"cells":self.auto_fill(a, t.range)?}))
            }
            "table.caption_text.get" => {
                let t: Target = serde_json::from_value(params)?;
                let a = self.table_target(t.table)?;
                Ok(json!({"text":self.table_caption_text(a)?}))
            }
            "table.caption_text" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Caption {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    text: String,
                }
                let p: Caption = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.set_table_caption_text(a, &p.text)?;
                Ok(self.info())
            }
            "table.style" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Style {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    preset: String,
                }
                let p: Style = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.apply_table_style(a, &p.preset)?;
                Ok(self.info())
            }
            "table.move_border" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Border {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    vertical: bool,
                    boundary: u16,
                    delta_mm: f64,
                }
                let p: Border = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.move_border(a, p.vertical, p.boundary, p.delta_mm)?;
                Ok(self.info())
            }
            "table.resize" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Resize {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    #[serde(default)]
                    range: Option<CellRange>,
                    vertical: bool,
                    delta_mm: f64,
                }
                let p: Resize = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                self.resize_cells(a, p.range, p.vertical, p.delta_mm)?;
                Ok(self.info())
            }
            "table.formula" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Formula {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    formula: String,
                    #[serde(default)]
                    row: Option<u16>,
                    #[serde(default)]
                    col: Option<u16>,
                }
                let p: Formula = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                let result = self.table_formula(a, &p.formula, p.row, p.col)?;
                Ok(json!({"result":result}))
            }
            "table.block_calc" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Calc {
                    #[serde(default)]
                    table: Option<ObjectAddress>,
                    #[serde(default)]
                    range: Option<CellRange>,
                    op: String,
                }
                let p: Calc = serde_json::from_value(params)?;
                let a = self.table_target(p.table)?;
                let written = self.block_calc(a, p.range, &p.op)?;
                Ok(json!({"cells":written}))
            }
            _ => bail!("알 수 없는 표 명령: {id}"),
        }
    }

    pub(super) fn table_at(&self, a: ObjectAddress) -> Result<&Table> {
        match self.object(a)? {
            Control::Table(t) => Ok(t),
            _ => bail!("표가 아닙니다."),
        }
    }
    /// Explicit address, or the table that holds the cursor.
    pub(crate) fn table_target(&self, explicit: Option<ObjectAddress>) -> Result<ObjectAddress> {
        let a = match explicit {
            Some(a) => a,
            None => {
                let cell = self
                    .cursor
                    .cell
                    .ok_or_else(|| anyhow!("먼저 표 셀을 선택하세요."))?;
                ObjectAddress {
                    section: self.cursor.section,
                    paragraph: self.cursor.paragraph,
                    control: cell.control,
                }
            }
        };
        self.table_at(a)?;
        Ok(a)
    }
    fn cursor_in(&self, a: ObjectAddress) -> Option<usize> {
        let c = self.cursor;
        c.cell
            .filter(|t| {
                c.section == a.section && c.paragraph == a.paragraph && t.control == a.control
            })
            .map(|t| t.index)
    }
    /// Default block: the cursor cell. The range grows until no merged cell crosses its edge.
    pub(crate) fn block(&self, a: ObjectAddress, range: Option<CellRange>) -> Result<CellRange> {
        let t = self.table_at(a)?;
        let mut r = match range {
            Some(r) => r,
            None => {
                let index = self
                    .cursor_in(a)
                    .ok_or_else(|| anyhow!("셀 블록(range)을 지정하거나 표 셀을 선택하세요."))?;
                let cell = t
                    .cells
                    .get(index)
                    .ok_or_else(|| anyhow!("셀 범위를 벗어났습니다."))?;
                CellRange {
                    start_row: cell.row,
                    start_col: cell.col,
                    end_row: cell.row,
                    end_col: cell.col,
                }
            }
        };
        if r.start_row > r.end_row
            || r.start_col > r.end_col
            || r.end_row >= t.row_count
            || r.end_col >= t.col_count
        {
            bail!("셀 블록이 표 범위를 벗어났습니다.");
        }
        loop {
            let before = r;
            for c in &t.cells {
                let (last_row, last_col) =
                    (c.row + c.row_span.max(1) - 1, c.col + c.col_span.max(1) - 1);
                let overlaps = c.row <= r.end_row
                    && last_row >= r.start_row
                    && c.col <= r.end_col
                    && last_col >= r.start_col;
                if overlaps {
                    r.start_row = r.start_row.min(c.row);
                    r.start_col = r.start_col.min(c.col);
                    r.end_row = r.end_row.max(last_row);
                    r.end_col = r.end_col.max(last_col);
                }
            }
            if r == before {
                return Ok(r);
            }
        }
    }
    fn block_cells(t: &Table, r: CellRange) -> Vec<usize> {
        t.cells
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                (r.start_row..=r.end_row).contains(&c.row)
                    && (r.start_col..=r.end_col).contains(&c.col)
            })
            .map(|(i, _)| i)
            .collect()
    }
    /// Keep the caret inside the edited table, at the cell now covering (row, col).
    pub(super) fn reseat(&mut self, a: ObjectAddress, row: u16, col: u16) -> Result<()> {
        let t = self.table_at(a)?;
        let row = row.min(t.row_count.saturating_sub(1));
        let col = col.min(t.col_count.saturating_sub(1));
        let index = t
            .cells
            .iter()
            .position(|v| {
                v.row <= row
                    && row < v.row + v.row_span.max(1)
                    && v.col <= col
                    && col < v.col + v.col_span.max(1)
            })
            .ok_or_else(|| anyhow!("편집 후 셀을 찾지 못했습니다."))?;
        self.cursor = Cursor {
            section: a.section,
            paragraph: a.paragraph,
            offset: 0,
            cell: Some(Cell {
                control: a.control,
                index,
                paragraph: 0,
            }),
        };
        self.clear_selection();
        Ok(())
    }
    fn reseat_if_inside(&mut self, a: ObjectAddress, r: CellRange) -> Result<()> {
        if self.cursor_in(a).is_some() {
            self.reseat(a, r.start_row, r.start_col)?;
        }
        Ok(())
    }

    fn create_table(&mut self, p: Create) -> Result<ObjectAddress> {
        let (rows, cols) = (usize::from(p.rows), usize::from(p.columns));
        if rows == 0
            || cols == 0
            || rows > MAX_ROWS
            || cols > MAX_COLUMNS
            || rows * cols > MAX_CELLS
        {
            bail!("표는 1–{MAX_ROWS}줄, 1–{MAX_COLUMNS}칸, {MAX_CELLS}셀 이하로 만들 수 있습니다.");
        }
        if self.cursor.cell.is_some() {
            bail!("표 안의 표 만들기는 아직 지원하지 않습니다. 본문에 커서를 놓으세요.");
        }
        let setup = self.page_setup()?;
        let body = (if setup.landscape {
            setup.height_mm
        } else {
            setup.width_mm
        }) - setup.left_mm
            - setup.right_mm
            - setup.gutter_mm;
        let widths: Option<Vec<u32>> = match (&p.column_widths_mm, p.width_mm) {
            (Some(list), _) => {
                if list.len() != cols {
                    bail!("칸 너비 개수는 칸 수와 같아야 합니다.");
                }
                Some(list.iter().map(|w| hu(*w)).collect())
            }
            (None, Some(w)) => Some(vec![hu(w / cols as f64); cols]),
            (None, None) => None,
        };
        let total_mm: f64 = match (&p.column_widths_mm, p.width_mm) {
            (Some(list), _) => list.iter().sum(),
            (None, Some(w)) => w,
            (None, None) => body,
        };
        if p.column_widths_mm
            .iter()
            .flatten()
            .chain(p.width_mm.iter())
            .chain(p.height_mm.iter())
            .any(|v| !v.is_finite() || *v <= 0.0)
            || total_mm / (cols as f64) < 1.0
            || total_mm > 600.0
        {
            bail!("표 너비는 칸마다 1 mm 이상, 전체 600 mm 이하로 입력하세요.");
        }
        if let Some(h) = p.height_mm
            && (h / (rows as f64) < 1.0 || h > 2000.0)
        {
            bail!("표 높이는 줄마다 1 mm 이상, 전체 2,000 mm 이하로 입력하세요.");
        }
        self.transaction(|s| {
            s.delete_selection()?;
            let c = s.cursor;
            let response = if p.treat_as_char {
                // RHWP appends the inline table control last; a later control
                // in the same paragraph would then be misordered.
                let positions = s.paragraph(c)?.control_text_positions();
                if positions.iter().any(|pos| *pos >= c.offset)
                    && s.paragraph(c)?.controls.iter().any(|x| {
                        !matches!(x, Control::SectionDef(_) | Control::ColumnDef(_))
                    })
                {
                    bail!(
                        "글자처럼 취급하는 표는 뒤에 다른 개체가 없는 위치에 넣으세요."
                    );
                }
                let r = s.reserve_object_position()?;
                s.core
                    .create_table_ex_native(
                        r.section,
                        r.paragraph,
                        r.offset,
                        p.rows,
                        p.columns,
                        true,
                        widths.as_deref(),
                        None,
                    )
                    .map_err(err)?
            } else {
                s.core
                    .create_table_native(c.section, c.paragraph, c.offset, p.rows, p.columns)
                    .map_err(err)?
            };
            let v: Value = serde_json::from_str(&response)?;
            let index = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("표 삽입 결과에 {k}가 없습니다: {v}"))
            };
            let a = ObjectAddress {
                section: c.section,
                paragraph: index("paraIdx")?,
                control: index("controlIdx")?,
            };
            if !p.treat_as_char
                && let Some(w) = &widths
            {
                s.core
                    .set_table_column_widths_native(a.section, a.paragraph, a.control, w.clone())
                    .map_err(err)?;
            }
            if let Some(h) = p.height_mm {
                let row = hu(h / rows as f64);
                let t = s.table_at(a)?;
                let updates: Vec<Value> = t
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        json!({"cellIdx":i,"heightDelta":
                            i64::from(row) * i64::from(cell.row_span.max(1)) - i64::from(cell.height)})
                    })
                    .collect();
                s.core
                    .resize_table_cells_by_cell_path_native(
                        a.section,
                        a.paragraph,
                        &[(a.control, 0, 0)],
                        &Value::Array(updates).to_string(),
                    )
                    .map_err(err)?;
            }
            s.reseat(a, 0, 0)
        })?;
        self.table_target(None)
    }

    /// Grid, spans, sizes and short cell text for dialogs and block painting.
    pub fn table_layout(&self, a: ObjectAddress, range: Option<CellRange>) -> Result<Value> {
        let t = self.table_at(a)?;
        let block = match range {
            Some(_) => Some(self.block(a, range)?),
            None if self.cursor_in(a).is_some() => Some(self.block(a, None)?),
            None => None,
        };
        let cells: Vec<Value> = t
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                json!({"index":i,"row":c.row,"col":c.col,"rowSpan":c.row_span,"colSpan":c.col_span,
                "width":c.width,"height":c.height,"protected":c.cell_protect(),
                "text":c.paragraphs.iter().map(|p|p.text.as_str()).collect::<Vec<_>>().join("\n").chars().take(40).collect::<String>()})
            })
            .collect();
        Ok(json!({"table":a,"rows":t.row_count,"columns":t.col_count,
            "columnWidths":t.get_column_widths(),"rowHeights":t.get_row_heights(),
            "current":self.cursor_in(a),"block":block,"cells":cells}))
    }

    pub fn table_properties(&self, a: ObjectAddress) -> Result<Value> {
        let t = self.table_at(a)?;
        let c = &t.common;
        let mut v = json!({
            "rows":t.row_count,"columns":t.col_count,
            "cellSpacing":t.cell_spacing,
            "paddingLeft":t.padding.left,"paddingRight":t.padding.right,
            "paddingTop":t.padding.top,"paddingBottom":t.padding.bottom,
            "pageBreak":match t.page_break { TablePageBreak::None=>0, TablePageBreak::CellBreak=>1, TablePageBreak::RowBreak=>2 },
            "repeatHeader":t.repeat_header,"treatAsChar":c.treat_as_char,
            "textWrap":wrap_name(c.text_wrap),"vertRelTo":vert_rel_name(c.vert_rel_to),
            "vertAlign":vert_align_name(c.vert_align),"horzRelTo":horz_rel_name(c.horz_rel_to),
            "horzAlign":horz_align_name(c.horz_align),
            "vertOffset":c.vertical_offset as i32,"horzOffset":c.horizontal_offset as i32,
            "restrictInPage":(t.attr >> 13) & 1 != 0,"allowOverlap":(t.attr >> 14) & 1 != 0,
            "keepWithAnchor":c.prevent_page_break != 0,
            "outerLeft":c.margin.left,"outerRight":c.margin.right,
            "outerTop":c.margin.top,"outerBottom":c.margin.bottom,
            "tableWidth":t.get_column_widths().iter().sum::<u32>(),
            // Stored row heights are minimums; the object height follows layout.
            "tableHeight":t.get_row_heights().iter().sum::<u32>().max(c.height),
            "hasCaption":t.caption.is_some(),
        });
        if let Some(cap) = &t.caption {
            use hwpcraft_doc::model::shape::{CaptionDirection, CaptionVertAlign};
            v["captionDirection"] = json!(match cap.direction {
                CaptionDirection::Left => 0,
                CaptionDirection::Right => 1,
                CaptionDirection::Top => 2,
                CaptionDirection::Bottom => 3,
            });
            v["captionVertAlign"] = json!(match cap.vert_align {
                CaptionVertAlign::Top => 0,
                CaptionVertAlign::Center => 1,
                CaptionVertAlign::Bottom => 2,
            });
            v["captionWidth"] = json!(cap.width);
            v["captionSpacing"] = json!(cap.spacing);
        }
        Ok(v)
    }

    /// Properties of the first cell of the block, as RHWP reports them (effective border/fill).
    pub fn cell_properties(&self, a: ObjectAddress, range: Option<CellRange>) -> Result<Value> {
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let index = t
            .cell_index_at(r.start_row, r.start_col)
            .or_else(|| Self::block_cells(t, r).first().copied())
            .ok_or_else(|| anyhow!("셀을 찾지 못했습니다."))?;
        let mut v: Value = serde_json::from_str(
            &self
                .core
                .get_cell_properties_by_cell_path_native(
                    a.section,
                    a.paragraph,
                    &[(a.control, index, 0)],
                    index,
                )
                .map_err(err)?,
        )?;
        v["range"] = json!(r);
        v["cells"] = json!(Self::block_cells(t, r).len());
        Ok(v)
    }

    pub fn set_table_properties(&mut self, a: ObjectAddress, props: &Value) -> Result<()> {
        let mut map = checked(props, TABLE_KEYS)?;
        if map.is_empty() {
            return Ok(());
        }
        let width = map.remove("tableWidth").and_then(|v| v.as_u64());
        let height = map.remove("tableHeight").and_then(|v| v.as_u64());
        let margins: Vec<(String, i16)> = ["outerLeft", "outerRight", "outerTop", "outerBottom"]
            .iter()
            .filter_map(|k| {
                map.get(*k)
                    .and_then(Value::as_i64)
                    .map(|v| ((*k).to_owned(), v as i16))
            })
            .collect();
        let t = self.table_at(a)?;
        let old_widths = t.get_column_widths();
        let old_heights: Vec<(usize, u32, u16)> = t
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.height, c.row_span))
            .collect();
        let old_total_h: u32 = t.get_row_heights().iter().sum();
        self.transaction(|s| {
            if !map.is_empty() {
                s.core
                    .set_table_properties_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        &Value::Object(map.clone()).to_string(),
                    )
                    .map_err(err)?;
            }
            // RHWP writes outer margins through the raw HWP header; HWPX-created tables
            // have none, so apply the common attribute directly when it did not stick.
            let mut relayout = false;
            if let Some(Control::Table(t)) = s
                .core
                .document_mut()
                .sections
                .get_mut(a.section)
                .and_then(|x| x.paragraphs.get_mut(a.paragraph))
                .and_then(|p| p.controls.get_mut(a.control))
            {
                for (key, v) in &margins {
                    let m = &mut t.common.margin;
                    let slot = match key.as_str() {
                        "outerLeft" => &mut m.left,
                        "outerRight" => &mut m.right,
                        "outerTop" => &mut m.top,
                        _ => &mut m.bottom,
                    };
                    if *slot != *v {
                        *slot = *v;
                        relayout = true;
                    }
                }
            }
            if relayout {
                s.core
                    .set_table_properties_native(a.section, a.paragraph, a.control, "{}")
                    .map_err(err)?;
            }
            if let Some(w) = width {
                let total: u64 = old_widths.iter().map(|v| u64::from(*v)).sum();
                if total == 0 {
                    bail!("표 너비를 계산하지 못했습니다.");
                }
                let mut acc = 0u64;
                let mut prev = 0u64;
                let widths: Vec<u32> = old_widths
                    .iter()
                    .map(|x| {
                        acc += u64::from(*x);
                        let edge = (acc * w + total / 2) / total;
                        let width = (edge - prev).max(283);
                        prev = edge;
                        width as u32
                    })
                    .collect();
                s.core
                    .set_table_column_widths_native(a.section, a.paragraph, a.control, widths)
                    .map_err(err)?;
            }
            if let Some(h) = height {
                if old_total_h == 0 {
                    bail!("표 높이를 계산하지 못했습니다.");
                }
                let scale = h as f64 / f64::from(old_total_h);
                let updates: Vec<Value> = old_heights
                    .iter()
                    .map(|(i, old, _)| {
                        let new = ((f64::from(*old) * scale).round() as i64).max(283);
                        json!({"cellIdx":i,"heightDelta":new - i64::from(*old)})
                    })
                    .collect();
                s.core
                    .resize_table_cells_by_cell_path_native(
                        a.section,
                        a.paragraph,
                        &[(a.control, 0, 0)],
                        &Value::Array(updates).to_string(),
                    )
                    .map_err(err)?;
            }
            Ok(())
        })
    }

    /// Apply cell properties to every cell of the block. `borders` sets block edges:
    /// `left/right/top/bottom` for the outline and `inner_vertical/inner_horizontal` between cells.
    pub fn set_cell_properties(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        props: &Value,
        borders: Option<&Map<String, Value>>,
    ) -> Result<()> {
        let r = self.block(a, range)?;
        // Validate before opening a transaction; an empty request leaves no undo step.
        let map = if props.is_null() {
            Map::new()
        } else {
            checked(props, CELL_KEYS)?
        };
        if let Some(b) = borders {
            for (k, v) in b {
                if !EDGES.contains(&k.as_str()) || !line_ok(v) {
                    return Err(bad(k));
                }
            }
        }
        if map.is_empty() && borders.is_none_or(Map::is_empty) {
            return Ok(());
        }
        self.transaction(|s| s.cell_properties_raw(a, r, props, borders))
    }
    /// Apply cell properties inside the caller's transaction.
    pub(super) fn cell_properties_raw(
        &mut self,
        a: ObjectAddress,
        r: CellRange,
        props: &Value,
        borders: Option<&Map<String, Value>>,
    ) -> Result<()> {
        let mut map = if props.is_null() {
            Map::new()
        } else {
            checked(props, CELL_KEYS)?
        };
        if let Some(b) = borders {
            for (k, v) in b {
                if !EDGES.contains(&k.as_str()) || !line_ok(v) {
                    return Err(bad(k));
                }
            }
        }
        let width = map.remove("width").and_then(|v| v.as_u64());
        let height = map.remove("height").and_then(|v| v.as_u64());
        if map.is_empty()
            && borders.is_none_or(Map::is_empty)
            && width.is_none()
            && height.is_none()
        {
            return Ok(());
        }
        let t = self.table_at(a)?;
        let cells: Vec<(usize, u16, u16, u16, u16)> = Self::block_cells(t, r)
            .into_iter()
            .filter_map(|i| {
                t.cells
                    .get(i)
                    .map(|c| (i, c.row, c.col, c.row_span.max(1), c.col_span.max(1)))
            })
            .collect();
        if cells.len() > MAX_CELLS {
            bail!("한 번에 {MAX_CELLS}셀 이하에 적용할 수 있습니다.");
        }
        let col_widths = t.get_column_widths();
        let row_heights = t.get_row_heights();
        {
            let s = &mut *self;
            for &(index, row, col, rs, cs) in &cells {
                let mut each = map.clone();
                // Enabling a cell override for one side must preserve the other
                // effective table defaults, including on a mixed cell block.
                if each.get("applyInnerMargin") == Some(&Value::Bool(true)) {
                    let table = s.table_at(a)?;
                    let cell = &table.cells[index];
                    for (key, value, fallback) in [
                        ("paddingLeft", cell.padding.left, table.padding.left),
                        ("paddingRight", cell.padding.right, table.padding.right),
                        ("paddingTop", cell.padding.top, table.padding.top),
                        ("paddingBottom", cell.padding.bottom, table.padding.bottom),
                    ] {
                        let effective = if cell.use_cell_padding_axis(value, fallback) {
                            value
                        } else {
                            fallback
                        };
                        each.entry(key.to_owned())
                            .or_insert(json!(effective.max(0)));
                    }
                }
                if each.contains_key("paddingLeft") || each.contains_key("paddingRight") {
                    let table = s.table_at(a)?;
                    let cell = &table.cells[index];
                    let use_cell = each
                        .get("applyInnerMargin")
                        .and_then(Value::as_bool)
                        .unwrap_or(cell.apply_inner_margin);
                    let value = |key: &str, old: i16, fallback: i16| -> i64 {
                        if use_cell {
                            each.get(key)
                                .and_then(Value::as_i64)
                                .unwrap_or(i64::from(old.max(0)))
                        } else {
                            i64::from(fallback.max(0))
                        }
                    };
                    if value("paddingLeft", cell.padding.left, table.padding.left)
                        + value("paddingRight", cell.padding.right, table.padding.right)
                        >= i64::from(cell.width)
                    {
                        bail!("셀의 왼쪽·오른쪽 안 여백 합계는 셀 너비보다 작아야 합니다.");
                    }
                }
                if let Some(b) = borders {
                    let side = |outer: bool, edge: &str, inner: &str| {
                        b.get(if outer { edge } else { inner }).cloned()
                    };
                    for (key, outer, edge, inner) in [
                        ("borderLeft", col == r.start_col, "left", "inner_vertical"),
                        (
                            "borderRight",
                            col + cs - 1 == r.end_col,
                            "right",
                            "inner_vertical",
                        ),
                        ("borderTop", row == r.start_row, "top", "inner_horizontal"),
                        (
                            "borderBottom",
                            row + rs - 1 == r.end_row,
                            "bottom",
                            "inner_horizontal",
                        ),
                    ] {
                        if let Some(line) = side(outer, edge, inner) {
                            each.insert(key.into(), line);
                        }
                    }
                }
                if each.is_empty() {
                    continue;
                }
                // RHWP finds the cell index after its own reflow; recompute from the grid.
                let current = s.table_at(a)?.cell_index_at(row, col).unwrap_or(index);
                // Without a base id RHWP rebuilds the border fill from defaults and the
                // sides/fill not named here would be lost (Studio sends the current id too).
                if each.keys().any(|k| {
                    k.starts_with("border")
                        || k.starts_with("fill")
                        || k.starts_with("pattern")
                        || k.starts_with("diagonal")
                        || k == "centerLine"
                }) {
                    let now: Value = serde_json::from_str(
                        &s.core
                            .get_cell_properties_by_cell_path_native(
                                a.section,
                                a.paragraph,
                                &[(a.control, current, 0)],
                                current,
                            )
                            .map_err(err)?,
                    )?;
                    if let Some(id) = now.get("borderFillId").filter(|v| v.as_u64() != Some(0)) {
                        each.insert("borderFillId".into(), id.clone());
                    }
                    // RHWP detects a border change only through "borderLeft"/"fillType"
                    // keys, so a single edge is sent with the other three current edges.
                    let sides = ["borderLeft", "borderRight", "borderTop", "borderBottom"];
                    if sides.iter().any(|k| each.contains_key(*k)) {
                        for k in sides {
                            if !each.contains_key(k)
                                && let Some(v) = now.get(k)
                            {
                                each.insert(k.into(), v.clone());
                            }
                        }
                    }
                    // RHWP rebuilds the fill only when fillType is present; a color implies solid.
                    if !each.contains_key("fillType")
                        && ["fillColor", "patternColor", "patternType"]
                            .iter()
                            .any(|k| each.contains_key(*k))
                    {
                        each.insert("fillType".into(), json!("solid"));
                        for k in ["fillColor", "patternColor", "patternType"] {
                            if !each.contains_key(k)
                                && let Some(v) = now.get(k)
                            {
                                each.insert(k.into(), v.clone());
                            }
                        }
                    }
                }
                s.core
                    .set_cell_properties_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        current,
                        &Value::Object(each).to_string(),
                    )
                    .map_err(err)?;
            }
            if let Some(w) = width {
                let mut widths = col_widths.clone();
                let span = usize::from(r.end_col - r.start_col) + 1;
                for slot in &mut widths[usize::from(r.start_col)..=usize::from(r.end_col)] {
                    *slot = (w / span as u64).max(283) as u32;
                }
                s.core
                    .set_table_column_widths_native(a.section, a.paragraph, a.control, widths)
                    .map_err(err)?;
            }
            if let Some(h) = height {
                let span = u64::from(r.end_row - r.start_row) + 1;
                let mut heights = row_heights.clone();
                for slot in &mut heights[usize::from(r.start_row)..=usize::from(r.end_row)] {
                    *slot = (h / span).max(283) as u32;
                }
                s.set_row_heights(a, &heights)?;
            }
            s.reseat_if_inside(a, r)
        }
    }
    /// Cell heights follow the target row heights, summed over each cell's span.
    fn set_row_heights(&mut self, a: ObjectAddress, heights: &[u32]) -> Result<()> {
        let t = self.table_at(a)?;
        let updates: Vec<Value> = t
            .cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let start = usize::from(c.row);
                let end = (start + usize::from(c.row_span.max(1))).min(heights.len());
                let target: i64 = heights.get(start..end)?.iter().map(|h| i64::from(*h)).sum();
                let delta = target - i64::from(c.height);
                (delta != 0).then(|| json!({"cellIdx":i,"heightDelta":delta}))
            })
            .collect();
        if updates.is_empty() {
            return Ok(());
        }
        self.core
            .resize_table_cells_by_cell_path_native(
                a.section,
                a.paragraph,
                &[(a.control, 0, 0)],
                &Value::Array(updates).to_string(),
            )
            .map_err(err)?;
        Ok(())
    }

    pub fn merge_cells(&mut self, a: ObjectAddress, range: Option<CellRange>) -> Result<()> {
        let r = self.block(a, range)?;
        if r.start_row == r.end_row && r.start_col == r.end_col {
            bail!("합칠 셀 블록을 두 칸 이상 선택하세요.");
        }
        if Self::block_cells(self.table_at(a)?, r).len() < 2 {
            bail!("이미 하나로 합쳐진 셀입니다.");
        }
        self.transaction(|s| {
            s.core
                .merge_table_cells_native(
                    a.section,
                    a.paragraph,
                    a.control,
                    r.start_row,
                    r.start_col,
                    r.end_row,
                    r.end_col,
                )
                .map_err(err)?;
            s.reseat(a, r.start_row, r.start_col)
        })
    }

    pub fn split_cells(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        rows: u16,
        columns: u16,
        equal_height: bool,
        merge_first: bool,
    ) -> Result<()> {
        if rows == 0 || columns == 0 || (rows == 1 && columns == 1) {
            bail!("줄 수와 칸 수는 1 이상이며 둘 다 1일 수는 없습니다.");
        }
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let cells = Self::block_cells(t, r);
        let (block_rows, block_cols) = (
            usize::from(r.end_row - r.start_row) + 1,
            usize::from(r.end_col - r.start_col) + 1,
        );
        let units = if merge_first { 1 } else { cells.len() };
        let new_rows = usize::from(t.row_count) + block_rows.max(1) * usize::from(rows);
        let new_cols = usize::from(t.col_count) + block_cols.max(1) * usize::from(columns);
        if units * usize::from(rows) * usize::from(columns) + t.cells.len() > MAX_CELLS
            || new_rows > MAX_ROWS + block_rows
            || new_cols > MAX_COLUMNS + block_cols
            || usize::from(rows) > MAX_ROWS
            || usize::from(columns) > MAX_COLUMNS
        {
            bail!("나눈 뒤 표는 {MAX_ROWS}줄, {MAX_COLUMNS}칸, {MAX_CELLS}셀 이하여야 합니다.");
        }
        let single = cells.len() == 1;
        self.transaction(|s| {
            if single || merge_first {
                s.core
                    .split_table_cell_into_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        r.start_row,
                        r.start_col,
                        rows,
                        columns,
                        equal_height,
                        merge_first,
                    )
                    .map_err(err)?;
            } else {
                s.core
                    .split_table_cells_in_range_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        r.start_row,
                        r.start_col,
                        r.end_row,
                        r.end_col,
                        rows,
                        columns,
                        equal_height,
                    )
                    .map_err(err)?;
            }
            let t = s.table_at(a)?;
            if usize::from(t.row_count) > MAX_ROWS || usize::from(t.col_count) > MAX_COLUMNS {
                bail!("나눈 뒤 표가 {MAX_ROWS}줄 또는 {MAX_COLUMNS}칸을 넘습니다.");
            }
            s.reseat(a, r.start_row, r.start_col)
        })
    }

    /// 셀 높이/너비를 같게: share the block's total among its rows or columns.
    pub fn equalize(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        rows: bool,
    ) -> Result<()> {
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let (start, end) = if rows {
            (usize::from(r.start_row), usize::from(r.end_row))
        } else {
            (usize::from(r.start_col), usize::from(r.end_col))
        };
        if start == end {
            bail!(
                "같게 맞출 {}을 두 개 이상 포함하는 셀 블록을 선택하세요.",
                if rows { "줄" } else { "칸" }
            );
        }
        let mut sizes = if rows {
            t.get_row_heights()
        } else {
            t.get_column_widths()
        };
        let slice = &mut sizes[start..=end];
        let total: u64 = slice.iter().map(|v| u64::from(*v)).sum();
        let n = slice.len() as u64;
        let mut acc = 0u64;
        for (i, slot) in slice.iter_mut().enumerate() {
            let edge = total * (i as u64 + 1) / n;
            *slot = (edge - acc) as u32;
            acc = edge;
        }
        self.transaction(|s| {
            if rows {
                s.set_row_heights(a, &sizes)?;
            } else {
                s.core
                    .set_table_column_widths_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        sizes.clone(),
                    )
                    .map_err(err)?;
            }
            s.reseat_if_inside(a, r)
        })
    }

    /// Drag a grid line. `vertical` lines sit between columns `boundary-1` and `boundary`
    /// (they trade width, the table keeps its width); horizontal lines resize the row above.
    /// The last boundary is the outer edge and resizes the table. Moving one cell's
    /// edge alone is not offered: RHWP's TableResizeCell re-derives the grid after
    /// the first step and then refuses the now-spanned table.
    pub fn move_border(
        &mut self,
        a: ObjectAddress,
        vertical: bool,
        boundary: u16,
        delta_mm: f64,
    ) -> Result<()> {
        if !delta_mm.is_finite() || delta_mm.abs() > 600.0 {
            bail!("경계선 이동은 ±600 mm 이내입니다.");
        }
        let delta = (delta_mm * HU_PER_MM).round() as i64;
        if delta == 0 {
            return Ok(());
        }
        let t = self.table_at(a)?;
        let count = if vertical { t.col_count } else { t.row_count };
        if boundary == 0 || boundary > count {
            bail!("경계선 번호는 1–{count}입니다.");
        }
        let b = usize::from(boundary);
        let mut sizes = if vertical {
            t.get_column_widths()
        } else {
            t.get_row_heights()
        };
        let grow = |v: u32, d: i64| -> Result<u32> {
            let n = i64::from(v) + d;
            if !(283..=170_079).contains(&n) {
                bail!("셀 크기는 1–600 mm 범위여야 합니다.");
            }
            Ok(n as u32)
        };
        sizes[b - 1] = grow(sizes[b - 1], delta)?;
        if vertical && b < sizes.len() {
            sizes[b] = grow(sizes[b], -delta)?;
        }
        if vertical && sizes.iter().map(|v| u64::from(*v)).sum::<u64>() > 170_079 {
            bail!("표 너비는 600 mm 이하입니다.");
        }
        self.transaction(|s| {
            if vertical {
                s.core
                    .set_table_column_widths_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        sizes.clone(),
                    )
                    .map_err(err)?;
            } else {
                s.set_row_heights(a, &sizes)?;
            }
            Ok(())
        })
    }

    /// Grow or shrink the block's columns (widths) or rows (heights) by `delta_mm` each.
    pub fn resize_cells(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        vertical: bool,
        delta_mm: f64,
    ) -> Result<()> {
        if !delta_mm.is_finite() || delta_mm == 0.0 || delta_mm.abs() > 300.0 {
            bail!("크기 변화는 0이 아닌 ±300 mm 이내입니다.");
        }
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let delta = (delta_mm * HU_PER_MM).round() as i64;
        let (mut sizes, start, end) = if vertical {
            (t.get_row_heights(), r.start_row, r.end_row)
        } else {
            (t.get_column_widths(), r.start_col, r.end_col)
        };
        for slot in &mut sizes[usize::from(start)..=usize::from(end)] {
            let next = i64::from(*slot) + delta;
            if !(283..=170_079).contains(&next) {
                bail!("셀 크기는 1–600 mm 범위여야 합니다.");
            }
            *slot = next as u32;
        }
        if !vertical && sizes.iter().map(|v| u64::from(*v)).sum::<u64>() > 170_079 {
            bail!("표 너비는 600 mm 이하입니다.");
        }
        self.transaction(|s| {
            if vertical {
                s.set_row_heights(a, &sizes)?;
            } else {
                s.core
                    .set_table_column_widths_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        sizes.clone(),
                    )
                    .map_err(err)?;
            }
            s.reseat_if_inside(a, r)
        })
    }

    /// Evaluate a Hangul-style formula (`=SUM(A1:A3)`, `=AVG(LEFT)`) into the target cell.
    pub fn table_formula(
        &mut self,
        a: ObjectAddress,
        formula: &str,
        row: Option<u16>,
        col: Option<u16>,
    ) -> Result<f64> {
        let formula = formula.trim();
        if formula.is_empty()
            || formula.chars().count() > 1000
            || formula.chars().any(char::is_control)
        {
            bail!("계산식은 제어 문자 없이 1–1,000자로 입력하세요.");
        }
        let mut depth = 0i32;
        for ch in formula.chars() {
            depth += match ch {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            if depth < 0 {
                break;
            }
        }
        if depth != 0 {
            bail!("계산식의 괄호 짝이 맞지 않습니다.");
        }
        let (row, col) = match (row, col) {
            (Some(r), Some(c)) => (r, c),
            (None, None) => {
                let r = self.block(a, None)?;
                (r.start_row, r.start_col)
            }
            _ => bail!("결과 셀은 row와 col을 함께 지정하세요."),
        };
        let t = self.table_at(a)?;
        let index = t
            .cell_index_at(row, col)
            .filter(|i| {
                t.cells
                    .get(*i)
                    .is_some_and(|c| c.row == row && c.col == col)
            })
            .ok_or_else(|| anyhow!("결과 셀이 없습니다."))?;
        if t.cells.get(index).is_some_and(|c| c.cell_protect()) {
            bail!("보호된 셀에는 계산 결과를 쓸 수 없습니다.");
        }
        let formula = if formula.starts_with('=') {
            formula.to_owned()
        } else {
            format!("={formula}")
        };
        let mut result = 0.0;
        self.transaction(|s| {
            let v: Value = serde_json::from_str(
                &s.core
                    .evaluate_table_formula(
                        a.section,
                        a.paragraph,
                        a.control,
                        usize::from(row),
                        usize::from(col),
                        &formula,
                        true,
                    )
                    .map_err(err)?,
            )?;
            result = v["result"]
                .as_f64()
                .ok_or_else(|| anyhow!("계산 결과가 숫자가 아닙니다."))?;
            if s.cursor_in(a).is_some() {
                s.reseat(a, row, col)?;
            }
            Ok(())
        })?;
        Ok(result)
    }

    /// 블록 합계/평균/곱: results go into an empty last row (column totals) or,
    /// failing that, an empty last column (row totals) of the block.
    pub fn block_calc(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        op: &str,
    ) -> Result<usize> {
        let function = match op {
            "sum" => "SUM",
            "average" => "AVG",
            "product" => "PRODUCT",
            _ => bail!("op는 sum, average, product 중 하나입니다."),
        };
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let empty = |row: u16, col: u16| {
            t.cell_index_at(row, col)
                .and_then(|i| t.cells.get(i))
                .filter(|c| c.row == row && c.col == col)
                .map(|c| c.paragraphs.iter().all(|p| p.text.trim().is_empty()))
        };
        let row_empty = r.end_row > r.start_row
            && (r.start_col..=r.end_col).all(|c| empty(r.end_row, c) != Some(false));
        let col_empty = r.end_col > r.start_col
            && (r.start_row..=r.end_row).all(|row| empty(row, r.end_col) != Some(false));
        let mut jobs = Vec::new();
        if row_empty {
            for c in r.start_col..=r.end_col {
                if empty(r.end_row, c) == Some(true) {
                    let name = column_name(usize::from(c));
                    jobs.push((
                        r.end_row,
                        c,
                        format!("={function}({name}{}:{name}{})", r.start_row + 1, r.end_row),
                    ));
                }
            }
        } else if col_empty {
            for row in r.start_row..=r.end_row {
                if empty(row, r.end_col) == Some(true) {
                    jobs.push((
                        row,
                        r.end_col,
                        format!(
                            "={function}({}{n}:{}{n})",
                            column_name(usize::from(r.start_col)),
                            column_name(usize::from(r.end_col) - 1),
                            n = row + 1
                        ),
                    ));
                }
            }
        } else {
            bail!("결과를 넣을 빈 줄(블록의 마지막 줄) 또는 빈 칸(마지막 칸)을 블록에 포함하세요.");
        }
        if jobs.is_empty() {
            bail!("결과를 쓸 수 있는 빈 셀이 없습니다.");
        }
        let count = jobs.len();
        self.transaction(|s| {
            for (row, col, formula) in &jobs {
                s.core
                    .evaluate_table_formula(
                        a.section,
                        a.paragraph,
                        a.control,
                        usize::from(*row),
                        usize::from(*col),
                        formula,
                        true,
                    )
                    .map_err(err)?;
            }
            s.reseat_if_inside(a, r)
        })?;
        Ok(count)
    }

    /// 셀 보호: true when `c` is in a table cell marked protected.
    pub(crate) fn cell_protected(&self, c: Cursor) -> bool {
        let Some(t) = c.cell else {
            return false;
        };
        self.table_at(ObjectAddress {
            section: c.section,
            paragraph: c.paragraph,
            control: t.control,
        })
        .ok()
        .and_then(|table| table.cells.get(t.index))
        .is_some_and(|cell| cell.cell_protect())
    }

    /// 표 뒤집기(줄↔칸), 표 나누기(블록 첫 줄에서), 표 붙이기(바로 아래 표와).
    fn restructure_table(
        &mut self,
        a: ObjectAddress,
        id: &str,
        range: Option<CellRange>,
    ) -> Result<()> {
        let t = self.table_at(a)?;
        let (rows, cols) = (t.row_count, t.col_count);
        let row = match id {
            "table.split_table" => {
                let r = self.block(a, range)?;
                if r.start_row == 0 {
                    bail!("표 나누기는 둘째 줄 이후의 셀에서 사용하세요.");
                }
                r.start_row
            }
            "table.transpose" => {
                if t.cells.iter().any(|c| c.row_span > 1 || c.col_span > 1) {
                    bail!("합쳐진 셀이 있는 표는 뒤집을 수 없습니다.");
                }
                if usize::from(cols) > MAX_ROWS || usize::from(rows) > MAX_COLUMNS {
                    bail!("뒤집은 표가 {MAX_ROWS}줄, {MAX_COLUMNS}칸을 넘습니다.");
                }
                0
            }
            _ => 0,
        };
        self.transaction(|s| {
            match id {
                "table.transpose" => {
                    s.core
                        .transpose_table_cells_in_place_native(a.section, a.paragraph, a.control)
                }
                "table.split_table" => {
                    s.core
                        .split_table_native(a.section, a.paragraph, a.control, row)
                }
                _ => s
                    .core
                    .merge_table_with_next_native(a.section, a.paragraph, a.control),
            }
            .map_err(err)?;
            if s.table_at(a).is_ok() {
                s.reseat(a, 0, 0)
            } else {
                s.cursor = Cursor {
                    section: a.section,
                    paragraph: a.paragraph,
                    offset: 0,
                    cell: None,
                };
                s.clear_selection();
                Ok(())
            }
        })
    }

    /// Insert or remove thousands separators in numeric cell paragraphs of the block.
    pub fn thousands(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        remove: bool,
    ) -> Result<usize> {
        fn grouped(text: &str) -> Option<String> {
            let t = text.trim();
            let (sign, rest) = t.strip_prefix('-').map_or(("", t), |r| ("-", r));
            let (int, frac) = rest
                .split_once('.')
                .map_or((rest, None), |(i, f)| (i, Some(f)));
            if int.is_empty()
                || !int.chars().all(|c| c.is_ascii_digit())
                || frac.is_some_and(|f| !f.chars().all(|c| c.is_ascii_digit()))
            {
                return None;
            }
            let digits: Vec<char> = int.chars().collect();
            let mut out = String::from(sign);
            for (i, d) in digits.iter().enumerate() {
                if i > 0 && (digits.len() - i).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(*d);
            }
            if let Some(f) = frac {
                out.push('.');
                out.push_str(f);
            }
            Some(out)
        }
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let mut edits = Vec::new();
        for index in Self::block_cells(t, r) {
            let Some(cell) = t.cells.get(index) else {
                continue;
            };
            for (pi, p) in cell.paragraphs.iter().enumerate() {
                if !p.controls.is_empty() || p.text.chars().any(char::is_control) {
                    continue;
                }
                let plain = p.text.replace(',', "");
                let next = if remove {
                    (p.text.contains(',') && grouped(&plain).is_some())
                        .then(|| plain.trim().to_owned())
                } else {
                    grouped(&plain)
                };
                if let Some(next) = next.filter(|n| *n != p.text) {
                    edits.push((index, pi, p.text.chars().count(), next));
                }
            }
        }
        let count = edits.len();
        if count == 0 {
            return Ok(0);
        }
        self.transaction(|s| {
            for (cell, para, old, text) in &edits {
                s.core
                    .delete_text_in_cell_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        *cell,
                        *para,
                        0,
                        *old,
                    )
                    .map_err(err)?;
                s.core
                    .insert_text_in_cell_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        *cell,
                        *para,
                        0,
                        text,
                    )
                    .map_err(err)?;
            }
            s.reseat_if_inside(a, r)
        })?;
        Ok(count)
    }

    /// 문자열을 표로: each selected body paragraph becomes a row, split by `delimiter`.
    pub fn text_to_table(&mut self, delimiter: &str) -> Result<ObjectAddress> {
        let sep = separator(delimiter)?;
        if self.cursor.cell.is_some() {
            bail!("문자열을 표로 바꾸기는 본문 문단에서 사용하세요.");
        }
        let paragraphs = self.selected_paragraphs()?;
        let mut rows: Vec<Vec<String>> = Vec::new();
        for (c, _) in &paragraphs {
            let p = self.paragraph(*c)?;
            if !p
                .controls
                .iter()
                .all(|x| matches!(x, Control::SectionDef(_) | Control::ColumnDef(_)))
                || !p.field_ranges.is_empty()
            {
                bail!("개체·필드가 있는 문단은 표로 바꿀 수 없습니다.");
            }
            rows.push(
                p.text
                    .split(sep.as_str())
                    .map(|t| t.trim().to_owned())
                    .collect(),
            );
        }
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1);
        if rows.is_empty()
            || rows.len() > MAX_ROWS
            || cols > MAX_COLUMNS
            || rows.len() * cols > MAX_CELLS
        {
            bail!("표는 {MAX_ROWS}줄, {MAX_COLUMNS}칸, {MAX_CELLS}셀 이하여야 합니다.");
        }
        let (first, _) = paragraphs[0];
        let (last, _) = paragraphs[paragraphs.len() - 1];
        let last_len = self.paragraph(last)?.text.chars().count();
        let mut address = None;
        self.transaction(|s| {
            // Remove the paragraphs' text (merging them into one empty paragraph).
            s.select(Cursor { offset: 0, ..first }, false)?;
            s.select(
                Cursor {
                    offset: last_len,
                    ..last
                },
                true,
            )?;
            s.delete_selection()?;
            let c = s.cursor;
            let v: Value = serde_json::from_str(
                &s.core
                    .create_table_native(c.section, c.paragraph, 0, rows.len() as u16, cols as u16)
                    .map_err(err)?,
            )?;
            let a = ObjectAddress {
                section: c.section,
                paragraph: v["paraIdx"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("표 주소 누락"))? as usize,
                control: v["controlIdx"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("표 주소 누락"))? as usize,
            };
            for (r, row) in rows.iter().enumerate() {
                for (col, text) in row.iter().enumerate().filter(|(_, t)| !t.is_empty()) {
                    let index = s
                        .table_at(a)?
                        .cell_index_at(r as u16, col as u16)
                        .ok_or_else(|| anyhow!("셀을 찾지 못했습니다."))?;
                    s.core
                        .insert_text_in_cell_native(
                            a.section,
                            a.paragraph,
                            a.control,
                            index,
                            0,
                            0,
                            text,
                        )
                        .map_err(err)?;
                }
            }
            s.reseat(a, 0, 0)?;
            address = Some(a);
            Ok(())
        })?;
        address.ok_or_else(|| anyhow!("표를 만들지 못했습니다."))
    }

    /// 표를 문자열로: one paragraph per row, cells joined by `delimiter`.
    pub fn table_to_text(&mut self, a: ObjectAddress, delimiter: &str) -> Result<()> {
        let sep = separator(delimiter)?;
        let t = self.table_at(a)?;
        if t.cells.iter().any(|c| {
            c.paragraphs
                .iter()
                .any(|p| !p.controls.is_empty() || !p.field_ranges.is_empty())
        }) {
            bail!("개체·필드가 있는 셀이 있어 문자열로 바꿀 수 없습니다.");
        }
        let mut lines = Vec::new();
        for r in 0..t.row_count {
            let row: Vec<String> = (0..t.col_count)
                .map(|c| {
                    t.cells
                        .iter()
                        .find(|x| x.row == r && x.col == c)
                        .map(|x| {
                            x.paragraphs
                                .iter()
                                .map(|p| p.text.trim())
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_default()
                })
                .collect();
            lines.push(row.join(&sep));
        }
        let text = lines.join("\n");
        self.transaction(|s| {
            s.core
                .delete_table_control_native(a.section, a.paragraph, a.control)
                .map_err(err)?;
            s.cursor = Cursor {
                section: a.section,
                paragraph: a.paragraph,
                offset: 0,
                cell: None,
            };
            s.clear_selection();
            s.cursor.offset = s.len();
            s.insert(&text)
        })
    }

    /// 정렬: reorder the block's rows by the text of `column` (numbers compare as numbers).
    pub fn sort_rows(
        &mut self,
        a: ObjectAddress,
        range: Option<CellRange>,
        column: u16,
        descending: bool,
    ) -> Result<()> {
        let r = self.block(a, range)?;
        if r.start_row == r.end_row {
            bail!("정렬할 줄을 두 줄 이상 선택하세요.");
        }
        if !(r.start_col..=r.end_col).contains(&column) {
            bail!("기준 칸은 셀 블록 안에 있어야 합니다.");
        }
        let t = self.table_at(a)?;
        let mut rows: Vec<Vec<(usize, String)>> = Vec::new();
        for row in r.start_row..=r.end_row {
            let mut cells = Vec::new();
            for col in r.start_col..=r.end_col {
                let cell = t
                    .cells
                    .iter()
                    .position(|c| c.row == row && c.col == col)
                    .and_then(|i| t.cells.get(i).map(|c| (i, c)))
                    .ok_or_else(|| anyhow!("합쳐진 셀이 있는 줄은 정렬할 수 없습니다."))?;
                if cell.1.row_span != 1
                    || cell.1.col_span != 1
                    || cell.1.paragraphs.len() != 1
                    || cell.1.paragraphs.iter().any(|p| !p.controls.is_empty())
                {
                    bail!("합쳐진 셀이나 여러 문단·개체가 있는 셀은 정렬할 수 없습니다.");
                }
                cells.push((cell.0, cell.1.paragraphs[0].text.clone()));
            }
            rows.push(cells);
        }
        let key = usize::from(column - r.start_col);
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&x, &y| {
            let (a, b) = (rows[x][key].1.trim(), rows[y][key].1.trim());
            let ord = match (
                a.replace(',', "").parse::<f64>(),
                b.replace(',', "").parse::<f64>(),
            ) {
                (Ok(m), Ok(n)) => m.total_cmp(&n),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            };
            if descending { ord.reverse() } else { ord }
        });
        if order.iter().enumerate().all(|(i, o)| i == *o) {
            return Ok(());
        }
        let edits: Vec<(usize, usize, String)> = order
            .iter()
            .enumerate()
            .flat_map(|(target, source)| {
                rows[target]
                    .iter()
                    .zip(&rows[*source])
                    .map(|((cell, old), (_, new))| (*cell, old.chars().count(), new.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.transaction(|s| {
            for (cell, old, text) in &edits {
                s.replace_cell_text(a, *cell, *old, text)?;
            }
            s.reseat_if_inside(a, r)
        })
    }
    fn replace_cell_text(
        &mut self,
        a: ObjectAddress,
        cell: usize,
        old: usize,
        text: &str,
    ) -> Result<()> {
        if old > 0 {
            self.core
                .delete_text_in_cell_native(a.section, a.paragraph, a.control, cell, 0, 0, old)
                .map_err(err)?;
        }
        if !text.is_empty() {
            self.core
                .insert_text_in_cell_native(a.section, a.paragraph, a.control, cell, 0, 0, text)
                .map_err(err)?;
        }
        Ok(())
    }

    /// 자동 채우기: continue the series started in the first cell(s) of each column
    /// (or row, for a one-row block): numbers, 월, 요일, 분기, 갑을병정, 가나다.
    pub fn auto_fill(&mut self, a: ObjectAddress, range: Option<CellRange>) -> Result<usize> {
        let r = self.block(a, range)?;
        let t = self.table_at(a)?;
        let down = r.end_row > r.start_row;
        if !down && r.end_col == r.start_col {
            bail!("채울 셀 블록을 두 칸 이상 선택하세요.");
        }
        let text_at = |row: u16, col: u16| -> Option<(usize, String)> {
            let i = t.cells.iter().position(|c| c.row == row && c.col == col)?;
            let c = t.cells.get(i)?;
            (c.paragraphs.len() == 1 && c.paragraphs[0].controls.is_empty())
                .then(|| (i, c.paragraphs[0].text.clone()))
        };
        let lines: Vec<Vec<(u16, u16)>> = if down {
            (r.start_col..=r.end_col)
                .map(|c| (r.start_row..=r.end_row).map(|row| (row, c)).collect())
                .collect()
        } else {
            vec![
                (r.start_col..=r.end_col)
                    .map(|c| (r.start_row, c))
                    .collect(),
            ]
        };
        let mut edits = Vec::new();
        for line in lines {
            let Some((_, first)) = text_at(line[0].0, line[0].1) else {
                continue;
            };
            let second = line
                .get(1)
                .and_then(|(row, col)| text_at(*row, *col))
                .map(|(_, t)| t)
                .filter(|t| !t.trim().is_empty());
            let Some(series) = Series::detect(first.trim(), second.as_deref().map(str::trim))
            else {
                continue;
            };
            let skip = if second.is_some() && series.step_from_second {
                2
            } else {
                1
            };
            for (k, (row, col)) in line.iter().enumerate().skip(skip) {
                if let Some((cell, old)) = text_at(*row, *col) {
                    let next = series.nth(k);
                    if next != old {
                        edits.push((cell, old.chars().count(), next));
                    }
                }
            }
        }
        if edits.is_empty() {
            bail!("블록의 첫 셀에서 이어 갈 값(숫자·월·요일 등)을 찾지 못했습니다.");
        }
        let count = edits.len();
        self.transaction(|s| {
            for (cell, old, text) in &edits {
                s.replace_cell_text(a, *cell, *old, text)?;
            }
            s.reseat_if_inside(a, r)
        })?;
        Ok(count)
    }

    /// Text after the automatic "표 N" number of the table caption.
    pub fn table_caption_text(&self, a: ObjectAddress) -> Result<String> {
        let (p, number) = self.caption_paragraph(a)?;
        let after: String = p.text.chars().skip(number).collect();
        // Readers place the number before or after the label's space; skip spaces.
        Ok(after.trim_start_matches(' ').to_owned())
    }
    /// The single caption paragraph and the character index of its number control.
    fn caption_paragraph(&self, a: ObjectAddress) -> Result<(&Paragraph, usize)> {
        let cap = self
            .table_at(a)?
            .caption
            .as_ref()
            .ok_or_else(|| anyhow!("캡션이 없는 표입니다. 먼저 캡션을 넣으세요."))?;
        let [p] = cap.paragraphs.as_slice() else {
            bail!("여러 문단 캡션은 아직 고칠 수 없습니다.");
        };
        if p.controls.len() != 1
            || !matches!(p.controls[0], Control::AutoNumber(_))
            || !p.field_ranges.is_empty()
            || p.text.chars().any(char::is_control)
        {
            bail!("번호 외 개체가 있는 캡션은 고칠 수 없습니다.");
        }
        let number = p
            .control_text_positions()
            .first()
            .copied()
            .unwrap_or(0)
            .min(p.text.chars().count());
        Ok((p, number))
    }
    /// Rebuild the caption paragraph as `prefix` + number control (8 code units) +
    /// " " + text, with consistent offsets. RHWP has no caption text API.
    pub fn set_table_caption_text(&mut self, a: ObjectAddress, text: &str) -> Result<()> {
        if text.chars().count() > 1000 || text.chars().any(char::is_control) {
            bail!("캡션은 제어 문자 없이 1,000자 이하로 입력하세요.");
        }
        let (p, number) = self.caption_paragraph(a)?;
        // The label before the number ("표"), whatever spacing the source used.
        let before: String = p.text.chars().take(number).collect();
        let label = before.split(' ').next().unwrap_or("").to_owned();
        // Hangul's own caption (read by RHWP): "표 " + a placeholder space that stands
        // for the number control (8 code units) + " " + text; char_count = units + 1.
        let prefix = format!("{label} ");
        let suffix = format!(" {text}");
        let mut offsets = Vec::new();
        let mut at = 0u32;
        for ch in prefix.chars() {
            offsets.push(at);
            at += ch.len_utf16() as u32;
        }
        offsets.push(at);
        at += 8;
        for ch in suffix.chars() {
            offsets.push(at);
            at += ch.len_utf16() as u32;
        }
        let full = format!("{prefix} {suffix}");
        self.transaction(|s| {
            {
                let Some(Control::Table(t)) = s
                    .core
                    .document_mut()
                    .sections
                    .get_mut(a.section)
                    .and_then(|x| x.paragraphs.get_mut(a.paragraph))
                    .and_then(|p| p.controls.get_mut(a.control))
                else {
                    bail!("표를 찾지 못했습니다.");
                };
                let p = t
                    .caption
                    .as_mut()
                    .and_then(|c| c.paragraphs.first_mut())
                    .ok_or_else(|| anyhow!("캡션 문단이 없습니다."))?;
                p.text = full.clone();
                p.char_offsets = offsets.clone();
                // Characters, the number control and the paragraph end.
                p.char_count = at + 1;
            }
            // Re-run table layout and pagination through the property path.
            s.core
                .set_table_properties_native(a.section, a.paragraph, a.control, "{}")
                .map_err(err)?;
            Ok(())
        })
    }

    /// 표 마당: a few presets expressed only as cell borders and fills (portable to HWP/HWPX).
    pub fn apply_table_style(&mut self, a: ObjectAddress, preset: &str) -> Result<()> {
        let t = self.table_at(a)?;
        let all = CellRange {
            start_row: 0,
            start_col: 0,
            end_row: t.row_count - 1,
            end_col: t.col_count - 1,
        };
        let header = CellRange { end_row: 0, ..all };
        let line = |ty: u8, w: u8, color: &str| json!({"type":ty,"width":w,"color":color});
        let thin = line(1, 1, "#000000");
        let (all_props, all_borders, header_props, header_bottom, stripe) = match preset {
            "basic" => (
                json!({"fillType":"none"}),
                json!({"left":thin,"right":thin,"top":thin,"bottom":thin,"inner_vertical":thin,"inner_horizontal":thin}),
                json!({}),
                None,
                None,
            ),
            "blue_header" => (
                json!({"fillType":"none"}),
                json!({"left":line(1, 1, "#4f81bd"),"right":line(1, 1, "#4f81bd"),"top":line(1, 4, "#4f81bd"),
                    "bottom":line(1, 4, "#4f81bd"),"inner_vertical":line(1, 1, "#95b3d7"),"inner_horizontal":line(1, 1, "#95b3d7")}),
                json!({"fillType":"solid","fillColor":"#dbe5f1"}),
                Some(line(1, 4, "#4f81bd")),
                None,
            ),
            "gray_grid" => (
                json!({"fillType":"none"}),
                json!({"left":line(1, 1, "#808080"),"right":line(1, 1, "#808080"),"top":line(1, 1, "#808080"),
                    "bottom":line(1, 1, "#808080"),"inner_vertical":line(1, 1, "#bfbfbf"),"inner_horizontal":line(1, 1, "#bfbfbf")}),
                json!({"fillType":"solid","fillColor":"#d9d9d9"}),
                Some(line(8, 3, "#595959")),
                None,
            ),
            "striped" => (
                json!({"fillType":"none"}),
                json!({"left":line(0, 0, "#000000"),"right":line(0, 0, "#000000"),"top":line(1, 4, "#000000"),
                    "bottom":line(1, 4, "#000000"),"inner_vertical":line(0, 0, "#000000"),"inner_horizontal":line(1, 1, "#bfbfbf")}),
                json!({}),
                Some(line(1, 3, "#000000")),
                Some(json!({"fillType":"solid","fillColor":"#f2f2f2"})),
            ),
            _ => bail!("표 마당 preset은 basic, blue_header, gray_grid, striped 중 하나입니다."),
        };
        let all_borders = all_borders
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow!("표 마당 정의 오류"))?;
        self.transaction(|s| {
            s.cell_properties_raw(a, all, &all_props, Some(&all_borders))?;
            if header_props.as_object().is_some_and(|o| !o.is_empty()) || header_bottom.is_some() {
                let mut b = Map::new();
                if let Some(l) = &header_bottom {
                    b.insert("bottom".into(), l.clone());
                }
                s.cell_properties_raw(a, header, &header_props, Some(&b))?;
            }
            if let Some(fill) = &stripe {
                let rows = s.table_at(a)?.row_count;
                for row in (2..rows).step_by(2) {
                    let r = CellRange {
                        start_row: row,
                        end_row: row,
                        ..all
                    };
                    s.cell_properties_raw(a, r, fill, None)?;
                }
            }
            Ok(())
        })
    }

    pub fn delete_table(&mut self, a: ObjectAddress) -> Result<()> {
        self.transaction(|s| {
            s.core
                .delete_table_control_native(a.section, a.paragraph, a.control)
                .map_err(err)?;
            let paragraphs = s
                .core
                .document()
                .sections
                .get(a.section)
                .map(|x| x.paragraphs.len())
                .unwrap_or(0);
            s.cursor = Cursor {
                section: a.section,
                paragraph: a.paragraph.min(paragraphs.saturating_sub(1)),
                offset: 0,
                cell: None,
            };
            s.clear_selection();
            Ok(())
        })
    }
}
