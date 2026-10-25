//! Owner: table/object area. 개체: 그림·도형·글상자·수식·각주/미주·메모·차트·글맵시.
//! Other areas must not edit this file; request changes through the owner.
//!
//! 개체 속성 follows RHWP property JSON (HWPUNIT sizes, degrees, BGR integers for
//! drawing colors). Every key is validated before the document changes.
use super::*;
use hwpcraft_doc::model::{paragraph::Paragraph, shape::ShapeObject};
use serde::Deserialize;
use serde_json::Map;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("picture.insert", "그림 넣기 (PNG·JPG·BMP·GIF)", "Ctrl+N,I"),
    ("object.properties.get", "개체 속성 조회", ""),
    ("object.properties", "개체 속성", "P"),
    (
        "shape.insert",
        "도형 넣기 (직선·사각형·타원·호·다각형·곡선·글상자)",
        "",
    ),
    ("textbox.paragraphs.get", "글상자 문단 조회", ""),
    ("textbox.paragraphs", "글상자 여러 문단 고치기", ""),
    ("note.paragraphs.get", "각주·미주 문단 조회", ""),
    ("note.paragraphs", "각주·미주 여러 문단 고치기", ""),
    (
        "note.insert",
        "각주·미주 넣기 (여러 문단)",
        "Ctrl+N,N / Ctrl+N,E",
    ),
    ("equation.insert", "수식 넣기 (색·기준선·글꼴)", "Ctrl+N,M"),
    ("chart.list", "차트 목록", ""),
    ("chart.data.get", "차트 데이터 조회", ""),
    ("chart.data", "차트 데이터 고치기 (값·계열 이름)", ""),
    ("memo.list", "메모 목록 (읽기)", ""),
    ("object.group", "개체 묶기 (도형·그림 2개 이상)", ""),
    ("object.ungroup", "개체 풀기", ""),
    (
        "object.z_order",
        "개체 앞뒤 순서 (front·back·forward·backward)",
        "",
    ),
    ("object.lock", "개체 보호 (잠금 켜기/끄기)", ""),
    ("object.unlock_all", "모든 개체 보호 해제", ""),
    ("object.flip", "개체 대칭 (horizontal·vertical)", ""),
    ("object.line_endpoint", "직선 끝점 옮기기 (mm)", ""),
    ("insert.field", "누름틀 넣기 (안내문·이름)", "Ctrl+K,E"),
    ("field.list", "누름틀 목록", ""),
    ("field.set", "누름틀 값 고치기", ""),
    ("form.list", "양식 개체 목록", ""),
    ("form.get", "양식 개체 정보 조회", ""),
    ("form.set", "양식 개체 값 고치기 (value·text·caption)", ""),
];
/// Connector connection points, as RHWP numbers them.
const CONNECT_POINTS: u32 = 4; // 0 top, 1 right, 2 bottom, 3 left

const HU: i64 = 7200;
const LENGTH: Kind = Kind::Int(0, 14_173); // 0–50 mm
const OFFSET: Kind = Kind::Int(-170_079, 170_079); // ±600 mm
const SIZE: Kind = Kind::Int(57, 170_079); // 0.2–600 mm
const COLOR: Kind = Kind::Int(0, 0xff_ffff);
const WRAP: Kind = Kind::Choice(&[
    "Square",
    "Tight",
    "Through",
    "TopAndBottom",
    "BehindText",
    "InFrontOfText",
]);

enum Kind {
    Int(i64, i64),
    Bool,
    Choice(&'static [&'static str]),
    Text(usize),
}

/// Placement keys shared by every floating object (RHWP common object attributes).
const COMMON: &[(&str, Kind)] = &[
    ("width", SIZE),
    ("height", SIZE),
    ("treatAsChar", Kind::Bool),
    ("textWrap", WRAP),
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
    ("sizeProtect", Kind::Bool),
    ("description", Kind::Text(1000)),
    ("outerMarginLeft", LENGTH),
    ("outerMarginRight", LENGTH),
    ("outerMarginTop", LENGTH),
    ("outerMarginBottom", LENGTH),
    ("rotationAngle", Kind::Int(-360, 360)),
    ("horzFlip", Kind::Bool),
    ("vertFlip", Kind::Bool),
    ("hasCaption", Kind::Bool),
    (
        "captionDirection",
        Kind::Choice(&["Left", "Right", "Top", "Bottom"]),
    ),
    (
        "captionVertAlign",
        Kind::Choice(&["Top", "Center", "Bottom"]),
    ),
    ("captionWidth", Kind::Int(0, 170_079)),
    ("captionSpacing", LENGTH),
    ("captionIncludeMargin", Kind::Bool),
];
const PICTURE: &[(&str, Kind)] = &[
    ("cropLeft", Kind::Int(0, 10_000_000)),
    ("cropTop", Kind::Int(0, 10_000_000)),
    ("cropRight", Kind::Int(0, 10_000_000)),
    ("cropBottom", Kind::Int(0, 10_000_000)),
    ("paddingLeft", LENGTH),
    ("paddingRight", LENGTH),
    ("paddingTop", LENGTH),
    ("paddingBottom", LENGTH),
    ("brightness", Kind::Int(-100, 100)),
    ("contrast", Kind::Int(-100, 100)),
    (
        "effect",
        Kind::Choice(&["RealPic", "GrayScale", "BlackWhite", "Pattern8x8"]),
    ),
    ("transparency", Kind::Int(0, 100)),
    ("borderColor", COLOR),
    ("borderWidth", Kind::Int(0, 1_417)),
];
const SHAPE: &[(&str, Kind)] = &[
    ("borderColor", COLOR),
    ("borderWidth", Kind::Int(0, 1_417)),
    ("lineType", Kind::Int(0, 17)),
    ("lineEndShape", Kind::Int(0, 5)),
    ("arrowStart", Kind::Int(0, 17)),
    ("arrowEnd", Kind::Int(0, 17)),
    ("arrowStartSize", Kind::Int(0, 8)),
    ("arrowEndSize", Kind::Int(0, 8)),
    ("fillType", Kind::Choice(&["none", "solid", "gradient"])),
    ("fillBgColor", COLOR),
    ("fillPatColor", COLOR),
    ("fillPatType", Kind::Int(-1, 6)),
    ("fillAlpha", Kind::Int(0, 255)),
    ("gradientType", Kind::Int(1, 4)),
    ("gradientAngle", Kind::Int(0, 359)),
    ("gradientCenterX", Kind::Int(0, 100)),
    ("gradientCenterY", Kind::Int(0, 100)),
    ("gradientBlur", Kind::Int(0, 100)),
    ("shadowType", Kind::Int(0, 8)),
    ("shadowColor", COLOR),
    ("shadowOffsetX", Kind::Int(-100, 100)),
    ("shadowOffsetY", Kind::Int(-100, 100)),
    ("roundRate", Kind::Int(0, 50)),
    ("tbMarginLeft", LENGTH),
    ("tbMarginRight", LENGTH),
    ("tbMarginTop", LENGTH),
    ("tbMarginBottom", LENGTH),
    (
        "tbVerticalAlign",
        Kind::Choice(&["Top", "Center", "Bottom"]),
    ),
];
const EQUATION: &[(&str, Kind)] = &[
    ("script", Kind::Text(2000)),
    ("fontSize", Kind::Int(400, 10_000)),
    ("color", COLOR),
    ("baseline", Kind::Int(-100, 100)),
    ("fontName", Kind::Text(64)),
];
const CAPTION: &[&str] = &[
    "captionDirection",
    "captionVertAlign",
    "captionWidth",
    "captionSpacing",
    "captionIncludeMargin",
];

fn validate(props: &Value, specs: &[&[(&str, Kind)]]) -> Result<Map<String, Value>> {
    let object = props
        .as_object()
        .ok_or_else(|| anyhow!("속성은 JSON 객체여야 합니다."))?;
    for (key, value) in object {
        let kind = specs
            .iter()
            .flat_map(|s| s.iter())
            .find(|(k, _)| k == key)
            .map(|(_, k)| k)
            .ok_or_else(|| anyhow!("이 개체에 쓸 수 없는 속성: {key}"))?;
        let ok = match kind {
            Kind::Int(lo, hi) => value.as_i64().is_some_and(|v| (*lo..=*hi).contains(&v)),
            Kind::Bool => value.is_boolean(),
            Kind::Choice(list) => value.as_str().is_some_and(|v| list.contains(&v)),
            Kind::Text(max) => value
                .as_str()
                .is_some_and(|v| v.chars().count() <= *max && !v.chars().any(char::is_control)),
        };
        if !ok {
            bail!("잘못된 개체 속성 값: {key}");
        }
    }
    Ok(object.clone())
}
fn mm_to_hu(mm: f64) -> Result<u32> {
    if !mm.is_finite() || !(0.2..=600.0).contains(&mm) {
        bail!("개체 크기는 0.2–600 mm입니다.");
    }
    Ok((mm * HU as f64 / 25.4).round() as u32)
}
fn paragraphs_ok(list: &[String]) -> Result<()> {
    if list.is_empty() || list.len() > 200 {
        bail!("문단은 1–200개입니다.");
    }
    if list.iter().map(String::len).sum::<usize>() > 100_000
        || list.iter().any(|p| p.chars().any(char::is_control))
    {
        bail!("문단마다 제어 문자 없이, 전체 100 KB 이하로 입력하세요.");
    }
    Ok(())
}
/// RHWP split APIs count inline controls as characters (see `Session::split_offset`).
fn logical_end(p: &Paragraph) -> usize {
    let len = p.text.chars().count();
    len + p
        .controls
        .iter()
        .zip(p.control_text_positions())
        .filter(|(c, pos)| {
            *pos <= len
                && matches!(
                    c,
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
        .count()
}
/// Hangul and RHWP close a polygon only when the last vertex repeats the first.
pub(crate) fn closed_polygon(
    mut points: Vec<hwpcraft_doc::model::Point>,
) -> Vec<hwpcraft_doc::model::Point> {
    if let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
        && (first.x != last.x || first.y != last.y)
    {
        points.push(first);
    }
    points
}
/// Closed triangle inscribed in the frame (HWPUNIT).
pub(crate) fn default_polygon(width: u32, height: u32) -> Vec<hwpcraft_doc::model::Point> {
    use hwpcraft_doc::model::Point;
    let (w, h) = (width as i32, height as i32);
    closed_polygon(vec![
        Point { x: w / 2, y: 0 },
        Point { x: w, y: h },
        Point { x: 0, y: h },
    ])
}
fn simple(p: &Paragraph) -> bool {
    p.controls.is_empty() && p.field_ranges.is_empty() && !p.text.chars().any(char::is_control)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Address {
    address: ObjectAddress,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Paragraphs {
    address: ObjectAddress,
    paragraphs: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShapeInsert {
    kind: ShapeKind,
    width_mm: f64,
    height_mm: f64,
    /// Hangul draws new shapes floating in front of text.
    #[serde(default)]
    treat_as_char: bool,
    #[serde(default)]
    text_wrap: Option<String>,
    /// Polygon vertices in mm, relative to the shape frame.
    #[serde(default)]
    points_mm: Option<Vec<[f64; 2]>>,
    /// Text box paragraphs.
    #[serde(default)]
    paragraphs: Vec<String>,
    /// Position from the paper's top-left (mouse drawing); default: the caret.
    #[serde(default)]
    x_mm: Option<f64>,
    #[serde(default)]
    y_mm: Option<f64>,
    /// Line direction: start at the right / bottom edge of the frame.
    #[serde(default)]
    flip_x: bool,
    #[serde(default)]
    flip_y: bool,
    /// Connector ends attached to shapes; both or none.
    #[serde(default)]
    start: Option<ConnectorEnd>,
    #[serde(default)]
    end: Option<ConnectorEnd>,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectorEnd {
    address: ObjectAddress,
    /// 0 top, 1 right, 2 bottom, 3 left.
    #[serde(default)]
    point: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FormAt {
    section: usize,
    paragraph: usize,
    /// Control index of the form object in its (body or cell) paragraph.
    control: usize,
    #[serde(default)]
    cell: Option<Cell>,
}
pub(crate) fn form_type_name(t: hwpcraft_doc::model::control::FormType) -> &'static str {
    use hwpcraft_doc::model::control::FormType;
    match t {
        FormType::PushButton => "PushButton",
        FormType::CheckBox => "CheckBox",
        FormType::ComboBox => "ComboBox",
        FormType::RadioButton => "RadioButton",
        FormType::Edit => "Edit",
    }
}
fn common_of(c: &Control) -> Option<&hwpcraft_doc::model::shape::CommonObjAttr> {
    match c {
        Control::Shape(s) => Some(s.common()),
        Control::Picture(p) => Some(&p.common),
        Control::Table(t) => Some(&t.common),
        _ => None,
    }
}
/// RHWP mutators answer `{"ok":true,...}` or `{"ok":false,"reason":...}`.
fn ok_value(raw: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(raw)?;
    if v["ok"] != true {
        let why = v["reason"]
            .as_str()
            .or_else(|| v["error"].as_str())
            .unwrap_or("실패");
        bail!("{why}");
    }
    Ok(v)
}
fn short_text(text: &str, max: usize, what: &str) -> Result<()> {
    if text.chars().count() > max || text.chars().any(char::is_control) {
        bail!("{what}은(는) 제어 문자 없이 {max}자 이하로 입력하세요.");
    }
    Ok(())
}
fn mm_point(p: [f64; 2]) -> Result<(i32, i32)> {
    if !(p[0].is_finite() && p[1].is_finite())
        || !(0.0..=1000.0).contains(&p[0])
        || !(0.0..=1000.0).contains(&p[1])
    {
        bail!("좌표는 종이 안(0–1000 mm)이어야 합니다.");
    }
    Ok((
        (p[0] * HU as f64 / 25.4).round() as i32,
        (p[1] * HU as f64 / 25.4).round() as i32,
    ))
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_object_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_object_command(id, params.clone()))
    }
    fn run_object_command(&mut self, id: &str, params: Value) -> Result<Value> {
        match id {
            "picture.insert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Picture {
                    path: std::path::PathBuf,
                    #[serde(default)]
                    width_mm: Option<f64>,
                    #[serde(default)]
                    treat_as_char: bool,
                }
                let p: Picture = serde_json::from_value(params)?;
                let a = self.insert_picture(&p.path, p.width_mm, p.treat_as_char)?;
                Ok(json!({"address":a}))
            }
            "object.properties.get" => {
                let p: Address = serde_json::from_value(params)?;
                self.object_properties(p.address)
            }
            "object.properties" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Set {
                    address: ObjectAddress,
                    props: Value,
                }
                let p: Set = serde_json::from_value(params)?;
                self.set_object_properties(p.address, &p.props)?;
                Ok(self.info())
            }
            "shape.insert" => {
                let a = self.insert_shape_ex(serde_json::from_value(params)?)?;
                Ok(json!({"address":a}))
            }
            "textbox.paragraphs.get" | "note.paragraphs.get" => {
                let p: Address = serde_json::from_value(params)?;
                Ok(json!({"paragraphs": if id == "textbox.paragraphs.get" {
                    self.textbox_paragraphs(p.address)?
                } else {
                    self.note_paragraphs(p.address)?
                }}))
            }
            "equation.insert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Equation {
                    script: String,
                    size_pt: f64,
                    #[serde(default)]
                    color: u32,
                    #[serde(default)]
                    baseline: Option<i64>,
                    #[serde(default)]
                    font_name: Option<String>,
                }
                let p: Equation = serde_json::from_value(params)?;
                if p.color > 0xff_ffff {
                    bail!("수식 색은 0x000000–0xFFFFFF(BGR)입니다.");
                }
                let mut extra = Map::new();
                if let Some(b) = p.baseline {
                    extra.insert("baseline".into(), json!(b));
                }
                if let Some(f) = &p.font_name {
                    extra.insert("fontName".into(), json!(f));
                }
                let extra = validate(&Value::Object(extra), &[EQUATION])?;
                let options = EquationOptions {
                    script: p.script,
                    size_pt: p.size_pt,
                };
                self.equation_preview(&options)?;
                self.body_insert()?;
                let mut inserted = None;
                self.transaction(|s| {
                    let c = s.reserve_object_position()?;
                    let size = (options.size_pt * 100.0).round() as u32;
                    let response = s
                        .core
                        .insert_equation_native(
                            c.section,
                            c.paragraph,
                            c.offset,
                            &options.script,
                            size,
                            p.color,
                        )
                        .map_err(err)?;
                    let v: Value = serde_json::from_str(&response)?;
                    let a = ObjectAddress {
                        section: c.section,
                        paragraph: v["paraIdx"]
                            .as_u64()
                            .ok_or_else(|| anyhow!("수식 주소 누락"))?
                            as usize,
                        control: v["controlIdx"]
                            .as_u64()
                            .ok_or_else(|| anyhow!("수식 주소 누락"))?
                            as usize,
                    };
                    if !extra.is_empty() {
                        s.core
                            .set_equation_properties_native(
                                a.section,
                                a.paragraph,
                                a.control,
                                None,
                                None,
                                &Value::Object(extra.clone()).to_string(),
                            )
                            .map_err(err)?;
                    }
                    inserted = Some(a);
                    Ok(())
                })?;
                Ok(json!({"address":inserted}))
            }
            "chart.list" | "memo.list" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                if id == "memo.list" {
                    return Ok(self.memos());
                }
                Ok(serde_json::from_str(
                    &self.core.list_charts_native().map_err(err)?,
                )?)
            }
            "chart.data.get" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Chart {
                    index: usize,
                }
                let p: Chart = serde_json::from_value(params)?;
                Ok(serde_json::from_str(
                    &self
                        .core
                        .get_chart_data_by_index_native(p.index)
                        .map_err(err)?,
                )?)
            }
            "chart.data" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Series {
                    #[serde(default)]
                    name: Option<String>,
                    values: Vec<String>,
                }
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Chart {
                    index: usize,
                    series: Vec<Series>,
                }
                let p: Chart = serde_json::from_value(params)?;
                if p.series.is_empty()
                    || p.series.len() > 64
                    || p.series.iter().any(|s| s.values.len() > 1000)
                {
                    bail!("차트 계열은 1–64개, 계열마다 값 1,000개 이하입니다.");
                }
                // Values only: RHWP rejects structural changes unless asked explicitly.
                let edits = json!({"series":p.series.iter().map(|s| {
                    let mut v = json!({"values":s.values});
                    if let Some(n) = &s.name { v["name"] = json!(n); }
                    v
                }).collect::<Vec<_>>()});
                let mut report = Value::Null;
                self.transaction(|s| {
                    let v: Value = serde_json::from_str(
                        &s.core
                            .set_chart_data_by_index_native(p.index, &edits.to_string())
                            .map_err(err)?,
                    )?;
                    if v["ok"] != true {
                        bail!("차트 데이터를 고치지 못했습니다: {v}");
                    }
                    report = v;
                    Ok(())
                })?;
                Ok(report)
            }
            "note.insert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Note {
                    endnote: bool,
                    paragraphs: Vec<String>,
                }
                let p: Note = serde_json::from_value(params)?;
                paragraphs_ok(&p.paragraphs)?;
                self.body_insert()?;
                let mut inserted = None;
                self.transaction(|s| {
                    let a = s.insert_note_raw(p.endnote, "")?;
                    s.write_note(a, &p.paragraphs)?;
                    inserted = Some(a);
                    Ok(())
                })?;
                Ok(json!({"address":inserted}))
            }
            "textbox.paragraphs" | "note.paragraphs" => {
                let p: Paragraphs = serde_json::from_value(params)?;
                if id == "textbox.paragraphs" {
                    self.set_textbox_paragraphs(p.address, &p.paragraphs)?;
                } else {
                    self.set_note_paragraphs(p.address, &p.paragraphs)?;
                }
                Ok(self.info())
            }
            "object.group" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Group {
                    targets: Vec<ObjectAddress>,
                }
                let p: Group = serde_json::from_value(params)?;
                let a = self.group_objects(&p.targets)?;
                Ok(json!({"address":a}))
            }
            "object.ungroup" => {
                let p: Address = serde_json::from_value(params)?;
                self.ungroup_object(p.address)?;
                Ok(self.info())
            }
            "object.z_order" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct ZOrder {
                    address: ObjectAddress,
                    mode: String,
                }
                let p: ZOrder = serde_json::from_value(params)?;
                let moved = self.set_z_order(p.address, &p.mode)?;
                Ok(json!({"moved":moved}))
            }
            "object.lock" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Lock {
                    address: ObjectAddress,
                    #[serde(default)]
                    locked: Option<bool>,
                }
                let p: Lock = serde_json::from_value(params)?;
                let locked = self.set_object_lock(p.address, p.locked)?;
                Ok(json!({"locked":locked}))
            }
            "object.unlock_all" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                Ok(json!({"unlocked":self.unlock_all_objects()?}))
            }
            "object.flip" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Flip {
                    address: ObjectAddress,
                    axis: String,
                }
                let p: Flip = serde_json::from_value(params)?;
                let vertical = match p.axis.as_str() {
                    "horizontal" => false,
                    "vertical" => true,
                    _ => bail!("대칭 축은 horizontal 또는 vertical입니다."),
                };
                self.flip_object(p.address, vertical)?;
                Ok(self.info())
            }
            "object.line_endpoint" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Endpoints {
                    address: ObjectAddress,
                    start_mm: [f64; 2],
                    end_mm: [f64; 2],
                }
                let p: Endpoints = serde_json::from_value(params)?;
                self.move_line_endpoints(p.address, p.start_mm, p.end_mm)?;
                Ok(self.info())
            }
            "insert.field" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Field {
                    guide: String,
                    #[serde(default)]
                    name: String,
                    #[serde(default)]
                    memo: String,
                    #[serde(default = "yes")]
                    editable: bool,
                }
                fn yes() -> bool {
                    true
                }
                let p: Field = serde_json::from_value(params)?;
                self.insert_click_here_field(&p.guide, &p.name, &p.memo, p.editable)
            }
            "field.list" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                Ok(serde_json::from_str(&self.core.get_field_list_json())?)
            }
            "field.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct FieldSet {
                    #[serde(default)]
                    field_id: Option<u32>,
                    #[serde(default)]
                    name: Option<String>,
                    #[serde(default)]
                    occurrence: usize,
                    value: String,
                }
                let p: FieldSet = serde_json::from_value(params)?;
                self.set_field_value(p.field_id, p.name.as_deref(), p.occurrence, &p.value)
            }
            "form.list" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                Ok(self.forms())
            }
            "form.get" => {
                let p: Address = serde_json::from_value(params)?;
                let a = p.address;
                if !matches!(self.object(a)?, Control::Form(_)) {
                    bail!("양식 개체가 아닙니다.");
                }
                ok_value(
                    &self
                        .core
                        .get_form_object_info_native(a.section, a.paragraph, a.control)
                        .map_err(err)?,
                )
            }
            "form.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct FormSet {
                    at: FormAt,
                    #[serde(default)]
                    value: Option<i64>,
                    #[serde(default)]
                    text: Option<String>,
                    #[serde(default)]
                    caption: Option<String>,
                }
                let p: FormSet = serde_json::from_value(params)?;
                self.set_form_value(&p.at, p.value, p.text.as_deref(), p.caption.as_deref())?;
                Ok(self.info())
            }
            _ => bail!("알 수 없는 개체 명령: {id}"),
        }
    }

    /// Memo fields in body paragraphs: anchor text and memo body (read-only;
    /// RHWP cannot write the MemoShape records a new memo needs).
    pub fn memos(&self) -> Value {
        use hwpcraft_doc::model::control::{Field, FieldType};
        fn is_memo(f: &Field) -> bool {
            f.field_type == FieldType::Memo || f.command.starts_with("MEMO/")
        }
        fn scan(p: &Paragraph, at: Value, out: &mut Vec<Value>) {
            for (ci, c) in p.controls.iter().enumerate() {
                match c {
                    Control::Field(f) if is_memo(f) => {
                        let anchor: String = p
                            .field_ranges
                            .iter()
                            .find(|r| r.control_idx == ci)
                            .map(|r| {
                                p.text
                                    .chars()
                                    .skip(r.start_char_idx)
                                    .take(r.end_char_idx.saturating_sub(r.start_char_idx))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let mut item = at.clone();
                        item["control"] = json!(ci);
                        out.push(json!({
                            "at": item,
                            "index": f.memo_index,
                            "anchor": anchor,
                            "paragraphs": f.memo_paragraphs.iter().map(|m| m.text.clone()).collect::<Vec<_>>(),
                        }));
                    }
                    Control::Table(t) => {
                        for (cell_index, cell) in t.cells.iter().enumerate() {
                            for (cp, para) in cell.paragraphs.iter().enumerate() {
                                let mut inner = at.clone();
                                inner["cell"] =
                                    json!({"control":ci,"index":cell_index,"paragraph":cp});
                                scan(para, inner, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for (si, section) in self.core.document().sections.iter().enumerate() {
            for (pi, p) in section.paragraphs.iter().enumerate() {
                scan(p, json!({"section":si,"paragraph":pi}), &mut out);
            }
        }
        json!(out)
    }

    /// `{kind, props}` where props is RHWP's property JSON for that object.
    pub fn object_properties(&self, a: ObjectAddress) -> Result<Value> {
        let (kind, raw) = match self.object(a)? {
            Control::Picture(_) => (
                "picture",
                self.core
                    .get_picture_properties_native(a.section, a.paragraph, a.control),
            ),
            Control::Shape(s) => (
                if s.drawing().and_then(|d| d.text_box.as_ref()).is_some() {
                    "textbox"
                } else {
                    "shape"
                },
                self.core
                    .get_shape_properties_native(a.section, a.paragraph, a.control),
            ),
            Control::Equation(_) => (
                "equation",
                self.core.get_equation_properties_native(
                    a.section,
                    a.paragraph,
                    a.control,
                    None,
                    None,
                ),
            ),
            Control::Table(_) => {
                return Ok(json!({"kind":"table","props":self.table_properties(a)?}));
            }
            _ => bail!("속성을 고칠 수 없는 개체입니다."),
        };
        let props: Value = serde_json::from_str(&raw.map_err(err)?)?;
        Ok(json!({"kind":kind,"props":props}))
    }

    pub fn set_object_properties(&mut self, a: ObjectAddress, props: &Value) -> Result<()> {
        let kind = match self.object(a)? {
            Control::Picture(_) => 0,
            Control::Shape(s) if matches!(s.as_ref(), ShapeObject::Picture(_)) => {
                bail!("묶음 안 그림 속성은 아직 지원하지 않습니다.")
            }
            Control::Shape(_) => 1,
            Control::Equation(_) => 2,
            Control::Table(_) => return self.set_table_properties(a, props),
            _ => bail!("속성을 고칠 수 없는 개체입니다."),
        };
        let mut map = match kind {
            0 => validate(props, &[COMMON, PICTURE])?,
            1 => validate(props, &[COMMON, SHAPE])?,
            _ => validate(props, &[COMMON, EQUATION])?,
        };
        if map.is_empty() {
            return Ok(());
        }
        // Hangul's 개체 보호 keeps a locked object out of every edit except unlocking.
        self.guard_object_lock(a)?;
        if kind == 2 && map.contains_key("hasCaption") {
            bail!("수식 캡션은 아직 지원하지 않습니다.");
        }
        // RHWP applies caption details only alongside hasCaption.
        if map.keys().any(|k| CAPTION.contains(&k.as_str())) && !map.contains_key("hasCaption") {
            let current = self.object_properties(a)?;
            map.insert("hasCaption".into(), current["props"]["hasCaption"].clone());
        }
        if let Some(script) = map.get("script").and_then(Value::as_str) {
            let size = map
                .get("fontSize")
                .and_then(Value::as_f64)
                .map(|v| v / 100.0)
                .unwrap_or(12.0);
            self.equation_preview(&EquationOptions {
                script: script.to_owned(),
                size_pt: size,
            })?;
        }
        let json = Value::Object(map).to_string();
        self.transaction(|s| {
            match kind {
                0 => s
                    .core
                    .set_picture_properties_native(a.section, a.paragraph, a.control, &json),
                1 => s
                    .core
                    .set_shape_properties_native(a.section, a.paragraph, a.control, &json),
                _ => s.core.set_equation_properties_native(
                    a.section,
                    a.paragraph,
                    a.control,
                    None,
                    None,
                    &json,
                ),
            }
            .map_err(err)?;
            if kind != 2 {
                s.reroute_connectors(a)?;
            }
            Ok(())
        })
    }

    /// Floating shapes start at the caret position on the page, like a click-to-draw in Hangul.
    fn insert_shape_ex(&mut self, p: ShapeInsert) -> Result<ObjectAddress> {
        self.body_insert()?;
        let (width, height) = (mm_to_hu(p.width_mm)?, mm_to_hu(p.height_mm)?);
        if p.kind != ShapeKind::Textbox && !p.paragraphs.is_empty() {
            bail!("글은 글상자에만 넣을 수 있습니다.");
        }
        if !p.paragraphs.is_empty() {
            paragraphs_ok(&p.paragraphs)?;
        }
        if (p.flip_x || p.flip_y) && !p.kind.is_line() {
            bail!("방향(flip_x·flip_y)은 직선과 연결선에만 지정합니다.");
        }
        if (p.start.is_some() || p.end.is_some()) && !p.kind.is_connector() {
            bail!("시작·끝 도형은 연결선에만 지정합니다.");
        }
        if p.kind.is_connector() && p.treat_as_char {
            bail!("연결선은 글자처럼 취급할 수 없습니다.");
        }
        // Attached connector: both ends name a shape in the caret's section; its
        // geometry comes from the shapes' connection points, not the request.
        let ends = match (p.start, p.end) {
            (Some(s), Some(e)) => Some(self.connector_ends(s, e)?),
            (None, None) => None,
            _ => bail!("연결선의 시작과 끝 도형은 함께 지정하세요."),
        };
        let wrap = p.text_wrap.as_deref().unwrap_or(if p.treat_as_char {
            "TopAndBottom"
        } else {
            "InFrontOfText"
        });
        if !matches!(
            wrap,
            "Square" | "TopAndBottom" | "BehindText" | "InFrontOfText"
        ) {
            bail!("본문과의 배치는 Square, TopAndBottom, BehindText, InFrontOfText 중 하나입니다.");
        }
        let points: Vec<hwpcraft_doc::model::Point> = match (&p.points_mm, p.kind) {
            (Some(list), ShapeKind::Polygon | ShapeKind::Curve) => {
                let curve = p.kind == ShapeKind::Curve;
                if curve && !(2..=1024).contains(&list.len()) {
                    bail!("곡선 점은 2–1,024개입니다.");
                }
                if !curve && !(3..=256).contains(&list.len()) {
                    bail!("다각형 꼭짓점은 3–256개입니다.");
                }
                let points = list
                    .iter()
                    .map(|[x, y]| {
                        if !(x.is_finite() && y.is_finite())
                            || !(0.0..=p.width_mm).contains(x)
                            || !(0.0..=p.height_mm).contains(y)
                        {
                            bail!("꼭짓점은 도형 크기 안에 있어야 합니다.");
                        }
                        Ok(hwpcraft_doc::model::Point {
                            x: (x * HU as f64 / 25.4).round() as i32,
                            y: (y * HU as f64 / 25.4).round() as i32,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                // A freehand curve stays open; a polygon closes on its first vertex.
                if curve {
                    points
                } else {
                    closed_polygon(points)
                }
            }
            (Some(_), _) => bail!("점은 다각형과 곡선에만 지정합니다."),
            (None, ShapeKind::Polygon) => default_polygon(width, height),
            (None, _) => Vec::new(),
        };
        // Caret position on its page (px at 96 dpi → HWPUNIT ×75).
        let (mut width, mut height) = (width, height);
        let (mut flip_x, mut flip_y) = (p.flip_x, p.flip_y);
        let (x, y) = if let Some(e) = &ends {
            let (sx, sy, ex, ey) = e.points;
            width = (sx - ex).unsigned_abs().max(1);
            height = (sy - ey).unsigned_abs().max(1);
            flip_x = sx > ex;
            flip_y = sy > ey;
            (sx.min(ex).max(0) as u32, sy.min(ey).max(0) as u32)
        } else if p.treat_as_char {
            (0, 0)
        } else if let (Some(x), Some(y)) = (p.x_mm, p.y_mm) {
            if !(x.is_finite() && y.is_finite())
                || !(0.0..=1000.0).contains(&x)
                || !(0.0..=1000.0).contains(&y)
            {
                bail!("도형 위치는 종이 안(0–1000 mm)이어야 합니다.");
            }
            (
                (x * HU as f64 / 25.4).round() as u32,
                (y * HU as f64 / 25.4).round() as u32,
            )
        } else if p.x_mm.is_some() || p.y_mm.is_some() {
            bail!("도형 위치는 x_mm와 y_mm를 함께 지정하세요.");
        } else {
            let caret = self.caret()?;
            (
                (caret["x"].as_f64().unwrap_or(0.0) * 75.0).max(0.0) as u32,
                (caret["y"].as_f64().unwrap_or(0.0) * 75.0).max(0.0) as u32,
            )
        };
        let mut address = None;
        self.transaction(|s| {
            let c = s.reserve_object_position()?;
            let response = s
                .core
                .create_shape_control_native(
                    c.section,
                    c.paragraph,
                    c.offset,
                    width,
                    height,
                    x,
                    y,
                    p.treat_as_char,
                    wrap,
                    p.kind.name(),
                    flip_x,
                    flip_y,
                    &points,
                )
                .map_err(err)?;
            let v: Value = serde_json::from_str(&response)?;
            let index = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("도형 삽입 결과에 {k}가 없습니다: {v}"))
            };
            let a = ObjectAddress {
                section: c.section,
                paragraph: index("paraIdx")?,
                control: index("controlIdx")?,
            };
            if p.kind == ShapeKind::Curve {
                s.polygon_to_curve(a, p.points_mm.is_some())?;
            }
            if p.kind.is_connector() {
                s.finish_connector(a, p.kind, ends.as_ref())?;
            }
            if !p.paragraphs.is_empty() {
                s.write_textbox(a, &p.paragraphs)?;
            }
            address = Some(a);
            Ok(())
        })?;
        address.ok_or_else(|| anyhow!("도형을 넣지 못했습니다."))
    }

    fn textbox_list(&self, a: ObjectAddress) -> Result<&[Paragraph]> {
        let Control::Shape(shape) = self.object(a)? else {
            bail!("글상자가 아닙니다.");
        };
        Ok(&shape
            .drawing()
            .and_then(|d| d.text_box.as_ref())
            .ok_or_else(|| anyhow!("글상자가 아닙니다."))?
            .paragraphs)
    }
    pub fn textbox_paragraphs(&self, a: ObjectAddress) -> Result<Vec<String>> {
        let list = self.textbox_list(a)?;
        if !list.iter().all(simple) {
            bail!("개체·필드가 있는 글상자는 문단 텍스트 편집을 지원하지 않습니다.");
        }
        Ok(list.iter().map(|p| p.text.clone()).collect())
    }
    pub fn set_textbox_paragraphs(
        &mut self,
        a: ObjectAddress,
        paragraphs: &[String],
    ) -> Result<()> {
        paragraphs_ok(paragraphs)?;
        if self.textbox_paragraphs(a)? == paragraphs {
            return Ok(());
        }
        self.transaction(|s| s.write_textbox(a, paragraphs))
    }
    /// Collapse the text box to one paragraph, then rebuild with splits.
    fn write_textbox(&mut self, a: ObjectAddress, paragraphs: &[String]) -> Result<()> {
        let count = self.textbox_list(a)?.len();
        for i in (1..count).rev() {
            self.core
                .merge_paragraph_in_cell_native(a.section, a.paragraph, a.control, 0, i)
                .map_err(err)?;
        }
        let old = self
            .textbox_list(a)?
            .first()
            .map_or(0, |p| p.text.chars().count());
        if old > 0 {
            self.core
                .delete_text_in_cell_native(a.section, a.paragraph, a.control, 0, 0, 0, old)
                .map_err(err)?;
        }
        for (i, text) in paragraphs.iter().enumerate() {
            if i > 0 {
                let end = self
                    .textbox_list(a)?
                    .get(i - 1)
                    .map(logical_end)
                    .ok_or_else(|| anyhow!("글상자 문단을 찾지 못했습니다."))?;
                self.core
                    .split_paragraph_in_cell_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        0,
                        i - 1,
                        end,
                        None,
                    )
                    .map_err(err)?;
            }
            if !text.is_empty() {
                self.core
                    .insert_text_in_cell_native(a.section, a.paragraph, a.control, 0, i, 0, text)
                    .map_err(err)?;
            }
        }
        if self.textbox_list(a)?.len() != paragraphs.len() {
            bail!("글상자 문단 수가 맞지 않습니다.");
        }
        Ok(())
    }

    fn note_list(&self, a: ObjectAddress) -> Result<&[Paragraph]> {
        match self.object(a)? {
            Control::Footnote(n) => Ok(&n.paragraphs),
            Control::Endnote(n) => Ok(&n.paragraphs),
            _ => bail!("각주/미주가 아닙니다."),
        }
    }
    /// First paragraph keeps its automatic number and the two-space separator.
    pub fn note_paragraphs(&self, a: ObjectAddress) -> Result<Vec<String>> {
        let list = self.note_list(a)?;
        let first = list
            .first()
            .ok_or_else(|| anyhow!("주석 문단이 없습니다."))?;
        if first.controls.len() != 1
            || !matches!(first.controls[0], Control::AutoNumber(_))
            || !first.field_ranges.is_empty()
            || !first.text.starts_with("  ")
            || first.text.chars().any(char::is_control)
            || !list[1..].iter().all(simple)
        {
            bail!("번호 외 개체·필드가 있는 주석은 문단 텍스트 편집을 지원하지 않습니다.");
        }
        Ok(std::iter::once(first.text[2..].to_owned())
            .chain(list[1..].iter().map(|p| p.text.clone()))
            .collect())
    }
    pub fn set_note_paragraphs(&mut self, a: ObjectAddress, paragraphs: &[String]) -> Result<()> {
        paragraphs_ok(paragraphs)?;
        if self.note_paragraphs(a)? == paragraphs {
            return Ok(());
        }
        self.transaction(|s| s.write_note(a, paragraphs))
    }
    fn write_note(&mut self, a: ObjectAddress, paragraphs: &[String]) -> Result<()> {
        {
            let s = self;
            let count = s.note_list(a)?.len();
            for i in (1..count).rev() {
                s.core
                    .merge_paragraph_in_footnote_native(a.section, a.paragraph, a.control, i)
                    .map_err(err)?;
            }
            let old = s
                .note_list(a)?
                .first()
                .map_or(0, |p| p.text.chars().count());
            if old > 2 {
                s.core
                    .delete_text_in_footnote_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        0,
                        2,
                        old - 2,
                    )
                    .map_err(err)?;
            }
            for (i, text) in paragraphs.iter().enumerate() {
                if i > 0 {
                    let end = s
                        .note_list(a)?
                        .get(i - 1)
                        .map(logical_end)
                        .ok_or_else(|| anyhow!("주석 문단을 찾지 못했습니다."))?;
                    s.core
                        .split_paragraph_in_footnote_native(
                            a.section,
                            a.paragraph,
                            a.control,
                            i - 1,
                            end,
                            None,
                        )
                        .map_err(err)?;
                }
                if !text.is_empty() {
                    s.core
                        .insert_text_in_footnote_native(
                            a.section,
                            a.paragraph,
                            a.control,
                            i,
                            if i == 0 { 2 } else { 0 },
                            text,
                        )
                        .map_err(err)?;
                }
            }
            if s.note_paragraphs(a)? != paragraphs {
                bail!("주석 문단을 고치지 못했습니다.");
            }
            Ok(())
        }
    }
}

/// Resolved connector ends: subject ids, connection point indices and the
/// global HWPUNIT coordinates of the two connection points.
pub(crate) struct ConnectorEnds {
    start_id: u32,
    start_point: u32,
    end_id: u32,
    end_point: u32,
    /// (start x, start y, end x, end y)
    points: (i32, i32, i32, i32),
}

impl Session {
    /// Hangul numbers body paragraphs across sections; RHWP `*_at` mutators take that index.
    fn para_in_list(&self, a: ObjectAddress) -> usize {
        self.core
            .document()
            .sections
            .iter()
            .take(a.section)
            .map(|s| s.paragraphs.len())
            .sum::<usize>()
            + a.paragraph
    }
    /// 개체 보호 (HWP5 attr bit 30) on a table, shape or picture.
    pub fn object_locked(&self, a: ObjectAddress) -> bool {
        self.object(a)
            .ok()
            .and_then(common_of)
            .is_some_and(|c| c.locked)
    }
    pub(crate) fn guard_object_lock(&self, a: ObjectAddress) -> Result<()> {
        if self.object_locked(a) {
            bail!("보호된 개체입니다. 개체 보호를 먼저 해제하세요.");
        }
        Ok(())
    }
    /// Lay out again after a model-only RHWP mutator (z-order, flip, connector routing).
    fn relayout_object(&mut self, a: ObjectAddress) -> Result<()> {
        match self.object(a)? {
            Control::Shape(_) => {
                self.core
                    .set_shape_properties_native(a.section, a.paragraph, a.control, "{}")
            }
            Control::Picture(_) => {
                self.core
                    .set_picture_properties_native(a.section, a.paragraph, a.control, "{}")
            }
            Control::Table(_) => {
                self.core
                    .set_table_properties_native(a.section, a.paragraph, a.control, "{}")
            }
            _ => return Ok(()),
        }
        .map_err(err)?;
        Ok(())
    }
    fn section_has_connectors(&self, section: usize) -> bool {
        self.core.document().sections.get(section).is_some_and(|s| {
            s.paragraphs
                .iter()
                .flat_map(|p| p.controls.iter())
                .any(|c| {
                    matches!(c, Control::Shape(s)
                            if matches!(s.as_ref(), ShapeObject::Line(l) if l.connector.is_some()))
                })
        })
    }
    /// Connectors follow the shapes they are attached to.
    pub(crate) fn reroute_connectors(&mut self, a: ObjectAddress) -> Result<()> {
        if !self.section_has_connectors(a.section) {
            return Ok(());
        }
        self.core.update_connectors_in_section(a.section);
        self.relayout_object(a)
    }

    fn connector_ends(&self, start: ConnectorEnd, end: ConnectorEnd) -> Result<ConnectorEnds> {
        let section = self.cursor.section;
        let resolve = |e: ConnectorEnd| -> Result<(u32, u32, [(i32, i32); 4])> {
            if e.address.section != section {
                bail!("연결선과 연결할 도형은 같은 구역에 있어야 합니다.");
            }
            if e.point >= CONNECT_POINTS {
                bail!("연결점은 0(위)·1(오른쪽)·2(아래)·3(왼쪽)입니다.");
            }
            let (common, id) = match self.object(e.address)? {
                Control::Shape(s) => {
                    if matches!(s.as_ref(), ShapeObject::Line(_)) {
                        bail!("직선·연결선에는 연결선을 붙일 수 없습니다.");
                    }
                    let id = s
                        .drawing()
                        .map(|d| d.inst_id)
                        .filter(|i| *i != 0)
                        .unwrap_or(s.common().instance_id);
                    (s.common(), id)
                }
                Control::Picture(p) => (&p.common, p.common.instance_id),
                _ => bail!("연결선은 도형·그림에만 붙일 수 있습니다."),
            };
            if common.treat_as_char {
                bail!("글자처럼 취급한 개체에는 연결선을 붙일 수 없습니다.");
            }
            if id == 0 {
                bail!("개체 식별자가 없어 연결선을 붙일 수 없습니다.");
            }
            let (x, y) = (
                common.horizontal_offset as i32,
                common.vertical_offset as i32,
            );
            let (w, h) = (common.width as i32, common.height as i32);
            let (cx, cy) = (x + w / 2, y + h / 2);
            Ok((id, e.point, [(cx, y), (x + w, cy), (cx, y + h), (x, cy)]))
        };
        let (start_id, start_point, s_pts) = resolve(start)?;
        let (end_id, end_point, e_pts) = resolve(end)?;
        if start_id == end_id {
            bail!("연결선의 시작과 끝은 다른 개체여야 합니다.");
        }
        let (sx, sy) = s_pts[start_point as usize % 4];
        let (ex, ey) = e_pts[end_point as usize % 4];
        Ok(ConnectorEnds {
            start_id,
            start_point,
            end_id,
            end_point,
            points: (sx, sy, ex, ey),
        })
    }
    /// After RHWP created a one-way connector: attach ends, widen to both-way, add arrow heads.
    fn finish_connector(
        &mut self,
        a: ObjectAddress,
        kind: ShapeKind,
        ends: Option<&ConnectorEnds>,
    ) -> Result<()> {
        if let Some(e) = ends {
            self.core.update_connector_subject_ids(
                a.section,
                a.paragraph,
                a.control,
                e.start_id,
                e.start_point,
                e.end_id,
                e.end_point,
            );
            self.core.recalculate_connector_routing(
                a.section,
                a.paragraph,
                a.control,
                e.start_point,
                e.end_point,
            );
        }
        let (start, end) = kind.arrows();
        if start && end {
            use hwpcraft_doc::model::shape::LinkLineType;
            let mut changed = false;
            if let Some(section) = self.core.document_mut().sections.get_mut(a.section) {
                if let Some(Control::Shape(sh)) = section
                    .paragraphs
                    .get_mut(a.paragraph)
                    .and_then(|p| p.controls.get_mut(a.control))
                    && let ShapeObject::Line(l) = sh.as_mut()
                    && let Some(c) = l.connector.as_mut()
                {
                    c.link_type = match c.link_type {
                        LinkLineType::StraightOneWay => LinkLineType::StraightBoth,
                        LinkLineType::StrokeOneWay => LinkLineType::StrokeBoth,
                        LinkLineType::ArcOneWay => LinkLineType::ArcBoth,
                        other => other,
                    };
                    changed = true;
                }
                if changed {
                    section.raw_stream = None;
                }
            }
            if !changed {
                bail!("연결선을 만들지 못했습니다.");
            }
        }
        let mut props = Map::new();
        if start {
            props.insert("arrowStart".into(), json!(1));
            props.insert("arrowStartSize".into(), json!(3));
        }
        if end {
            props.insert("arrowEnd".into(), json!(1));
            props.insert("arrowEndSize".into(), json!(3));
        }
        // The property call also refreshes layout after the model-only edits above.
        self.core
            .set_shape_properties_native(
                a.section,
                a.paragraph,
                a.control,
                &Value::Object(props).to_string(),
            )
            .map_err(err)?;
        Ok(())
    }

    /// 묶기: two or more floating shapes/pictures of one section become a group object.
    pub fn group_objects(&mut self, targets: &[ObjectAddress]) -> Result<ObjectAddress> {
        let Some(first) = targets.first() else {
            bail!("묶기는 같은 구역의 도형·그림 2–256개를 고르세요.");
        };
        if !(2..=256).contains(&targets.len()) {
            bail!("묶기는 같은 구역의 도형·그림 2–256개를 고르세요.");
        }
        let section = first.section;
        let mut seen = std::collections::HashSet::new();
        for a in targets {
            if a.section != section {
                bail!("다른 구역의 개체는 함께 묶을 수 없습니다.");
            }
            if !seen.insert((a.paragraph, a.control)) {
                bail!("같은 개체를 두 번 골랐습니다.");
            }
            let common = match self.object(*a)? {
                Control::Shape(s) => s.common(),
                Control::Picture(p) => &p.common,
                _ => bail!("도형·그림만 묶을 수 있습니다."),
            };
            if common.treat_as_char {
                bail!("글자처럼 취급한 개체는 묶을 수 없습니다.");
            }
            self.guard_object_lock(*a)?;
        }
        let pairs: Vec<(usize, usize)> = targets.iter().map(|a| (a.paragraph, a.control)).collect();
        let mut out = None;
        self.transaction(|s| {
            let v = ok_value(&s.core.group_shapes_native(section, &pairs).map_err(err)?)?;
            let index = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("묶기 결과에 {k}가 없습니다: {v}"))
            };
            let a = ObjectAddress {
                section,
                paragraph: index("paraIdx")?,
                control: index("controlIdx")?,
            };
            if !matches!(s.object(a)?, Control::Shape(sh) if matches!(sh.as_ref(), ShapeObject::Group(_)))
            {
                bail!("묶음 개체를 만들지 못했습니다.");
            }
            s.cursor.offset = s.cursor.offset.min(s.len());
            s.clear_selection();
            s.core.update_connectors_in_section(section);
            s.relayout_object(a)?;
            out = Some(a);
            Ok(())
        })?;
        out.ok_or_else(|| anyhow!("개체를 묶지 못했습니다."))
    }
    /// 풀기: one level; nested groups stay grouped (RHWP rule).
    pub fn ungroup_object(&mut self, a: ObjectAddress) -> Result<()> {
        if !matches!(self.object(a)?, Control::Shape(s) if matches!(s.as_ref(), ShapeObject::Group(_)))
        {
            bail!("묶음 개체가 아닙니다.");
        }
        self.guard_object_lock(a)?;
        self.transaction(|s| {
            ok_value(
                &s.core
                    .ungroup_shape_native(a.section, a.paragraph, a.control)
                    .map_err(err)?,
            )?;
            s.cursor.offset = s.cursor.offset.min(s.len());
            s.clear_selection();
            s.core.update_connectors_in_section(a.section);
            if matches!(s.object(a), Ok(Control::Shape(_) | Control::Picture(_))) {
                s.relayout_object(a)?;
            }
            Ok(())
        })
    }
    /// 앞뒤 순서. Returns false (no undo step) when the object is already there.
    pub fn set_z_order(&mut self, a: ObjectAddress, mode: &str) -> Result<bool> {
        if !matches!(mode, "front" | "back" | "forward" | "backward") {
            bail!("앞뒤 순서는 front, back, forward, backward 중 하나입니다.");
        }
        let z = match self.object(a)? {
            Control::Shape(s) => s.common().z_order,
            Control::Picture(p) => p.common.z_order,
            _ => bail!("앞뒤 순서는 도형·그림에만 적용합니다."),
        };
        self.guard_object_lock(a)?;
        let zs: Vec<i32> = self
            .core
            .document()
            .sections
            .get(a.section)
            .map(|s| {
                s.paragraphs
                    .iter()
                    .flat_map(|p| p.controls.iter())
                    .filter_map(common_of)
                    .map(|c| c.z_order)
                    .collect()
            })
            .unwrap_or_default();
        let top = zs.iter().copied().max().unwrap_or(z);
        let bottom = zs.iter().copied().min().unwrap_or(z);
        let moves = match mode {
            "front" | "forward" => z < top,
            _ => z > bottom,
        };
        if !moves {
            return Ok(false);
        }
        let pil = self.para_in_list(a);
        self.transaction(|s| {
            let v = ok_value(
                &s.core
                    .set_control_z_order_at(pil, a.control, mode)
                    .map_err(err)?,
            )?;
            if v["moved"] != true {
                bail!("앞뒤 순서를 바꾸지 못했습니다.");
            }
            s.relayout_object(a)
        })?;
        Ok(true)
    }
    /// 개체 보호 toggle (`None`) or set; returns the new state.
    pub fn set_object_lock(&mut self, a: ObjectAddress, locked: Option<bool>) -> Result<bool> {
        let current = common_of(self.object(a)?)
            .ok_or_else(|| anyhow!("개체 보호는 표·도형·그림에만 적용합니다."))?
            .locked;
        let want = locked.unwrap_or(!current);
        if want == current {
            return Ok(want);
        }
        let pil = self.para_in_list(a);
        self.transaction(|s| {
            let v = ok_value(
                &s.core
                    .set_control_lock(Some(pil), Some(a.control), want)
                    .map_err(err)?,
            )?;
            if v["touched"] != 1 {
                bail!("개체 보호를 바꾸지 못했습니다: {v}");
            }
            Ok(())
        })?;
        Ok(want)
    }
    /// 모두 풀기 (ShapeObjUnlockAll); returns how many objects were locked.
    pub fn unlock_all_objects(&mut self) -> Result<usize> {
        let locked = self
            .core
            .document()
            .sections
            .iter()
            .flat_map(|s| s.paragraphs.iter())
            .flat_map(|p| p.controls.iter())
            .filter_map(common_of)
            .filter(|c| c.locked)
            .count();
        if locked == 0 {
            return Ok(0);
        }
        self.transaction(|s| {
            ok_value(&s.core.set_control_lock(None, None, false).map_err(err)?)?;
            Ok(())
        })?;
        Ok(locked)
    }
    /// 대칭 toggle through RHWP's Hangul-measured flip (bits + matrix). Shapes only;
    /// pictures keep the `horzFlip`/`vertFlip` picture properties.
    pub fn flip_object(&mut self, a: ObjectAddress, vertical: bool) -> Result<()> {
        match self.object(a)? {
            Control::Shape(_) => {}
            Control::Picture(_) => {
                bail!("그림 대칭은 그림 속성(horzFlip·vertFlip)으로 지정하세요.")
            }
            _ => bail!("대칭은 도형에만 적용합니다."),
        }
        self.guard_object_lock(a)?;
        let pil = self.para_in_list(a);
        self.transaction(|s| {
            ok_value(
                &s.core
                    .set_control_flip_at(pil, a.control, vertical, false)
                    .map_err(err)?,
            )?;
            s.relayout_object(a)
        })
    }
    /// Straight line end points in paper mm (global coordinates).
    pub fn move_line_endpoints(
        &mut self,
        a: ObjectAddress,
        start_mm: [f64; 2],
        end_mm: [f64; 2],
    ) -> Result<()> {
        let Control::Shape(s) = self.object(a)? else {
            bail!("직선이 아닙니다.");
        };
        let ShapeObject::Line(l) = s.as_ref() else {
            bail!("직선이 아닙니다.");
        };
        if l.connector.is_some() {
            bail!("연결선 끝점은 연결된 도형을 옮기면 따라갑니다.");
        }
        if l.common.treat_as_char {
            bail!("글자처럼 취급한 직선의 끝점은 옮길 수 없습니다.");
        }
        self.guard_object_lock(a)?;
        let (sx, sy) = mm_point(start_mm)?;
        let (ex, ey) = mm_point(end_mm)?;
        if (sx, sy) == (ex, ey) {
            bail!("직선의 두 끝점이 같습니다.");
        }
        self.transaction(|s| {
            ok_value(
                &s.core
                    .move_line_endpoint_native(a.section, a.paragraph, a.control, sx, sy, ex, ey)
                    .map_err(err)?,
            )?;
            Ok(())
        })
    }

    /// 누름틀 at the caret (body or table cell), empty with a guide text.
    fn insert_click_here_field(
        &mut self,
        guide: &str,
        name: &str,
        memo: &str,
        editable: bool,
    ) -> Result<Value> {
        short_text(guide, 200, "안내문")?;
        short_text(name, 64, "누름틀 이름")?;
        short_text(memo, 1000, "도움말")?;
        if guide.trim().is_empty() {
            bail!("누름틀 안내문을 입력하세요.");
        }
        self.guard_protected()?;
        if self.has_selection() {
            bail!("선택을 해제하고 누름틀을 넣을 위치에 커서를 놓으세요.");
        }
        let c = self.cursor;
        self.paragraph(c)?;
        if c.offset > self.len() {
            bail!("커서가 문단 범위를 벗어났습니다.");
        }
        let mut out = Value::Null;
        self.transaction(|s| {
            let raw = if let Some(t) = c.cell {
                s.core.insert_click_here_field_at_in_cell(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    c.offset,
                    false,
                    guide,
                    memo,
                    name,
                    editable,
                )
            } else {
                s.core.insert_click_here_field_at(
                    c.section,
                    c.paragraph,
                    c.offset,
                    guide,
                    memo,
                    name,
                    editable,
                )
            }
            .map_err(err)?;
            let v = ok_value(&raw)?;
            s.cursor.offset = s.cursor.offset.min(s.len());
            out = json!({"field_id":v["fieldId"],"char_offset":v["charOffset"]});
            Ok(())
        })?;
        Ok(out)
    }
    fn set_field_value(
        &mut self,
        id: Option<u32>,
        name: Option<&str>,
        occurrence: usize,
        value: &str,
    ) -> Result<Value> {
        short_text(value, 10_000, "누름틀 값")?;
        if id.is_some() == name.is_some() {
            bail!("field_id 또는 name 중 하나만 지정하세요.");
        }
        if let Some(n) = name {
            short_text(n, 64, "누름틀 이름")?;
        }
        let mut out = Value::Null;
        self.transaction(|s| {
            let raw = match (id, name) {
                (Some(i), _) => s.core.set_field_value_by_id(i, value),
                (None, Some(n)) => s.core.set_field_value_by_name_at(n, occurrence, value),
                (None, None) => bail!("누름틀을 지정하세요."),
            }
            .map_err(err)?;
            out = ok_value(&raw)?;
            s.cursor.offset = s.cursor.offset.min(s.len());
            Ok(())
        })?;
        Ok(out)
    }

    /// 양식 개체 in body paragraphs and first-level table cells.
    pub fn forms(&self) -> Value {
        use hwpcraft_doc::model::control::FormObject;
        fn item(f: &FormObject, at: Value) -> Value {
            json!({"at":at,"type":form_type_name(f.form_type),"name":f.name,"caption":f.caption,
                "text":f.text,"value":f.value,"enabled":f.enabled})
        }
        let mut out = Vec::new();
        for (si, section) in self.core.document().sections.iter().enumerate() {
            for (pi, p) in section.paragraphs.iter().enumerate() {
                for (ci, c) in p.controls.iter().enumerate() {
                    match c {
                        Control::Form(f) => {
                            out.push(item(f, json!({"section":si,"paragraph":pi,"control":ci})));
                        }
                        Control::Table(t) => {
                            for (cell_index, cell) in t.cells.iter().enumerate() {
                                for (cp, para) in cell.paragraphs.iter().enumerate() {
                                    for (fi, fc) in para.controls.iter().enumerate() {
                                        if let Control::Form(f) = fc {
                                            out.push(item(f, json!({"section":si,"paragraph":pi,
                                                "control":fi,
                                                "cell":{"control":ci,"index":cell_index,"paragraph":cp}})));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        json!(out)
    }
    fn form_at(&self, at: &FormAt) -> Result<&hwpcraft_doc::model::control::FormObject> {
        let p = self.paragraph(Cursor {
            section: at.section,
            paragraph: at.paragraph,
            offset: 0,
            cell: at.cell,
        })?;
        match p.controls.get(at.control) {
            Some(Control::Form(f)) => Ok(f),
            _ => bail!("양식 개체를 찾지 못했습니다."),
        }
    }
    fn set_form_value(
        &mut self,
        at: &FormAt,
        value: Option<i64>,
        text: Option<&str>,
        caption: Option<&str>,
    ) -> Result<()> {
        use hwpcraft_doc::model::control::FormType;
        let kind = self.form_at(at)?.form_type;
        if value.is_none() && text.is_none() && caption.is_none() {
            bail!("value, text, caption 중 하나 이상을 지정하세요.");
        }
        let mut props = Map::new();
        if let Some(v) = value {
            if !matches!(kind, FormType::CheckBox | FormType::RadioButton) || !(0..=1).contains(&v)
            {
                bail!("value는 선택 상자·라디오 단추에 0 또는 1로 지정합니다.");
            }
            props.insert("value".into(), json!(v));
        }
        if let Some(t) = text {
            if !matches!(kind, FormType::Edit | FormType::ComboBox) {
                bail!("text는 입력 상자·목록 상자에만 지정합니다.");
            }
            short_text(t, 1000, "양식 개체 값")?;
            props.insert("text".into(), json!(t));
        }
        if let Some(c) = caption {
            if !matches!(
                kind,
                FormType::PushButton | FormType::CheckBox | FormType::RadioButton
            ) {
                bail!("caption은 명령 단추·선택 상자·라디오 단추에만 지정합니다.");
            }
            short_text(c, 1000, "캡션")?;
            props.insert("caption".into(), json!(c));
        }
        let json = Value::Object(props).to_string();
        let (section, paragraph, control, cell) = (at.section, at.paragraph, at.control, at.cell);
        self.transaction(|s| {
            let raw = if let Some(t) = cell {
                s.core.set_form_value_in_cell_native(
                    section,
                    paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    control,
                    &json,
                )
            } else {
                s.core
                    .set_form_value_native(section, paragraph, control, &json)
            }
            .map_err(err)?;
            ok_value(&raw)?;
            Ok(())
        })
    }
}
