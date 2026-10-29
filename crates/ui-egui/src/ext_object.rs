//! Owner: table/object area. 개체 대화상자·개체 도구.
//! Other areas must not edit this file; request changes through the owner.
//!
//! 개체 선택(클릭)·이동(끌기)·지우기, 도형 그리기(끌기), 그림 넣기, 개체 속성 창.
//! The properties dialog follows Hangul 2024's tabs: 기본 · 여백/캡션 · 선 · 채우기 ·
//! 그림 · 글상자 · 그림자. Drafts hold RHWP property JSON; only changed keys are sent.
use super::*;
use hwpcraft_engine::{EquationOptions, ObjectAddress};
use serde_json::{Map, Value};
mod chart_creation;

const HU_PER_MM: f64 = 7200.0 / 25.4;
/// RHWP page coordinates are 96 dpi pixels.
const MM_PER_PX: f32 = 25.4 / 96.0;

#[derive(Clone, Copy)]
struct Selected {
    address: ObjectAddress,
    page: u32,
    /// Page pixels (96 dpi) of the object's box.
    bbox: Rect,
    inline: bool,
    /// 개체 보호: shown with a lock badge; no handles, moving or deleting.
    locked: bool,
    /// A plain straight line (not a connector): its end points in page pixels.
    line: Option<(Pos2, Pos2)>,
    /// 묶음 개체 (풀기 applies).
    group: bool,
}
#[derive(Clone)]
struct FieldInsert {
    guide: String,
    name: String,
    memo: String,
}
#[derive(Clone)]
struct FormDraft {
    at: Value,
    kind: String,
    name: String,
    text: String,
    checked: bool,
    caption: String,
    original: (String, bool, String),
}
/// Connector tool kinds: (command id suffix, label, engine `shape.insert` kind).
const CONNECTORS: &[(&str, &str, &str)] = &[
    ("straight", "직선 연결선", "connector_straight"),
    (
        "straight_arrow",
        "직선 화살표 연결선",
        "connector_straight_arrow",
    ),
    (
        "straight_both",
        "직선 양쪽 화살표 연결선",
        "connector_straight_both",
    ),
    ("stroke", "꺾인 연결선", "connector_stroke"),
    (
        "stroke_arrow",
        "꺾인 화살표 연결선",
        "connector_stroke_arrow",
    ),
    (
        "stroke_both",
        "꺾인 양쪽 화살표 연결선",
        "connector_stroke_both",
    ),
];
const Z_ORDERS: &[(&str, &str)] = &[
    ("front", "맨 앞으로"),
    ("back", "맨 뒤로"),
    ("forward", "앞으로"),
    ("backward", "뒤로"),
];
/// RHWP connection point (0 top, 1 right, 2 bottom, 3 left) nearest to `p` on `bbox`.
fn connection_point(bbox: Rect, p: Pos2) -> u32 {
    let c = bbox.center();
    let dx = (p.x - c.x) / bbox.width().max(1.0);
    let dy = (p.y - c.y) / bbox.height().max(1.0);
    if dx.abs() > dy.abs() {
        if dx > 0.0 { 1 } else { 3 }
    } else if dy > 0.0 {
        2
    } else {
        0
    }
}
#[derive(Clone)]
struct PictureInsert {
    path: std::path::PathBuf,
    treat_as_char: bool,
    custom_width: bool,
    width_mm: f64,
}
#[derive(Clone)]
struct Properties {
    address: ObjectAddress,
    kind: String,
    tab: usize,
    original: Value,
    draft: Value,
    error: Option<String>,
}

/// Dialog and tool state owned by this module. Stored in `HwpApp::ext.object`.
#[derive(Default)]
pub(crate) struct State {
    selected: Option<Selected>,
    /// Page control layouts per page for one document generation (see ext_table).
    layout: Option<super::ext_table::PageLayouts>,
    draw: Option<&'static str>,
    drag: Option<(u32, Pos2)>,
    moving: Option<Vec2>,
    /// Polygon vertices / freehand samples while drawing (page pixels) and their page.
    points: Vec<Pos2>,
    points_page: u32,
    /// Handle being dragged and the drag start (page pixels).
    resizing: Option<(usize, Pos2)>,
    /// Further selected objects (Shift+click) for 묶기; `selected` is the last one.
    multi: Vec<Selected>,
    /// Straight line end point being dragged (0 start, 1 end) and its preview position.
    endpoint: Option<(usize, Pos2)>,
    /// Connector tool: the shape under the drag start and its connection point.
    connector_start: Option<(ObjectAddress, u32)>,
    field: Option<FieldInsert>,
    forms: Option<Vec<FormDraft>>,
    picture: Option<PictureInsert>,
    properties: Option<Box<Properties>>,
    equation: Option<Box<EquationEditor>>,
    chart: Option<Box<ChartEditor>>,
    chart_authoring: Option<Box<chart_creation::Editor>>,
    memos: bool,
    /// 개체 모양 복사 (CopyShapeFormat): the look waiting to be pasted.
    format: Option<Value>,
    /// 각주/미주 모양 dialog.
    note_shape: Option<Box<NoteShapeDialog>>,
    /// Draw tool makes a 세로 글상자 (direction for the next text box).
    draw_direction: Option<&'static str>,
    /// (document generation, has drawable objects, has charts): ribbon enabled
    /// checks run every frame, so the whole-document scans are cached per edit.
    doc_facts: std::cell::Cell<Option<(u64, bool, bool)>>,
}

/// 각주/미주 모양 (NoteNumShape*, NoteLine*): one section's note shape being edited.
pub(crate) struct NoteShapeDialog {
    endnote: bool,
    original: Value,
    draft: Value,
}
const NOTE_NUMBER_FORMATS: &[(&str, &str)] = &[
    ("digit", "1, 2, 3"),
    ("circledDigit", "①, ②, ③"),
    ("upperRoman", "I, II, III"),
    ("lowerRoman", "i, ii, iii"),
    ("upperAlpha", "A, B, C"),
    ("lowerAlpha", "a, b, c"),
    ("circledUpperAlpha", "Ⓐ, Ⓑ, Ⓒ"),
    ("circledLowerAlpha", "ⓐ, ⓑ, ⓒ"),
    ("hangulSyllable", "가, 나, 다"),
    ("circledHangulSyllable", "㉮, ㉯, ㉰"),
    ("hangulJamo", "ㄱ, ㄴ, ㄷ"),
    ("circledHangulJamo", "㉠, ㉡, ㉢"),
    ("hangulDigit", "일, 이, 삼"),
    ("hanjaDigit", "一, 二, 三"),
    ("circledHanjaDigit", "㊀, ㊁, ㊂"),
    ("hanjaGapEul", "甲, 乙, 丙"),
    ("hanjaGapEulHanja", "갑, 을, 병"),
    ("fourSymbol", "*, †, ‡"),
    ("userChar", "사용자 기호"),
];
const SEPARATOR_WIDTHS: &[&str] = &[
    "0.1 mm", "0.12 mm", "0.15 mm", "0.2 mm", "0.25 mm", "0.3 mm", "0.4 mm", "0.5 mm", "0.6 mm",
    "0.7 mm", "1.0 mm", "1.5 mm", "2.0 mm", "3.0 mm", "4.0 mm", "5.0 mm",
];
/// Hangul 맞춤 menu (ShapeAlign*): id suffix, label, needs several objects.
const ALIGNS: &[(&str, &str, bool)] = &[
    ("left", "왼쪽 맞춤", false),
    ("center", "가운데 맞춤", false),
    ("right", "오른쪽 맞춤", false),
    ("top", "위쪽 맞춤", false),
    ("middle", "중간 맞춤", false),
    ("bottom", "아래쪽 맞춤", false),
    ("same_width", "같은 너비", true),
    ("same_height", "같은 높이", true),
    ("same_size", "같은 크기", true),
    ("space_h", "가로 간격을 동일하게", true),
    ("space_v", "세로 간격을 동일하게", true),
];
/// Quick fill / outline palette (ShapeFillColor*, ShapeOutlineColor*): id suffix, #rrggbb.
const QUICK_COLORS: &[(&str, &str)] = &[
    ("black", "#000000"),
    ("white", "#ffffff"),
    ("red", "#ff0000"),
    ("orange", "#ff7f00"),
    ("yellow", "#ffff00"),
    ("green", "#00a000"),
    ("blue", "#0000ff"),
    ("purple", "#7f00ff"),
    ("gray", "#808080"),
];
const PEN_WIDTHS: &[(&str, f64)] = &[
    ("0.1", 0.1),
    ("0.25", 0.25),
    ("0.5", 0.5),
    ("1", 1.0),
    ("2", 2.0),
    ("3", 3.0),
];
fn css_rgb(v: &Value) -> [u8; 3] {
    let h = v.as_str().unwrap_or("#000000").trim_start_matches('#');
    let n = u32::from_str_radix(h, 16).unwrap_or(0);
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}
fn css_field(ui: &mut egui::Ui, label: &str, v: &mut Value, key: &str) {
    ui.label(label);
    let mut c = css_rgb(&v[key]);
    if egui::color_picker::color_edit_button_srgb(ui, &mut c).changed() {
        v[key] = json!(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
    }
}
fn text_field(ui: &mut egui::Ui, label: &str, v: &mut Value, key: &str, max: usize) {
    let mut t = v[key].as_str().unwrap_or("").to_owned();
    ui.label(label);
    if ui
        .add(egui::TextEdit::singleline(&mut t).desired_width(60.0))
        .changed()
    {
        let t: String = t.chars().take(max).collect();
        v[key] = json!(t);
    }
}

fn bgr(v: &Value) -> [u8; 3] {
    let n = v.as_u64().unwrap_or(0);
    [
        (n & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        ((n >> 16) & 0xff) as u8,
    ]
}
fn bgr_value(c: [u8; 3]) -> Value {
    json!(u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16))
}
fn mm_field(ui: &mut egui::Ui, label: &str, v: &mut Value, key: &str, range: (f64, f64)) {
    let mut mm = v[key].as_f64().unwrap_or(0.0) / HU_PER_MM;
    ui.label(label);
    if ui
        .add(
            egui::DragValue::new(&mut mm)
                .range(range.0..=range.1)
                .speed(0.1)
                .fixed_decimals(2)
                .suffix(" mm"),
        )
        .changed()
    {
        v[key] = json!((mm * HU_PER_MM).round() as i64);
    }
}
fn int_field(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut Value,
    key: &str,
    range: (i64, i64),
    suffix: &str,
) {
    let mut n = v[key].as_i64().unwrap_or(0);
    ui.label(label);
    if ui
        .add(
            egui::DragValue::new(&mut n)
                .range(range.0..=range.1)
                .suffix(suffix),
        )
        .changed()
    {
        v[key] = json!(n);
    }
}
fn check(ui: &mut egui::Ui, label: &str, v: &mut Value, key: &str) {
    let mut b = v[key].as_bool().unwrap_or(false);
    if ui.checkbox(&mut b, label).changed() {
        v[key] = json!(b);
    }
}
fn choice(ui: &mut egui::Ui, id: &str, v: &mut Value, key: &str, items: &[(&str, &str)]) {
    let current = v[key].as_str().unwrap_or("").to_owned();
    let label = items
        .iter()
        .find(|(k, _)| *k == current)
        .map(|(_, l)| *l)
        .unwrap_or("-");
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(96.0)
        .show_ui(ui, |ui| {
            for (k, l) in items {
                if ui.selectable_label(current == *k, *l).clicked() {
                    v[key] = json!(k);
                }
            }
        });
}
fn number_choice(ui: &mut egui::Ui, id: &str, v: &mut Value, key: &str, items: &[&str]) {
    let current = v[key].as_u64().unwrap_or(0) as usize;
    egui::ComboBox::from_id_salt(id)
        .selected_text(*items.get(current).unwrap_or(&"-"))
        .width(120.0)
        .show_ui(ui, |ui| {
            for (i, l) in items.iter().enumerate() {
                if ui.selectable_label(current == i, *l).clicked() {
                    v[key] = json!(i);
                }
            }
        });
}
fn color(ui: &mut egui::Ui, label: &str, v: &mut Value, key: &str) {
    ui.label(label);
    let mut c = bgr(&v[key]);
    if egui::color_picker::color_edit_button_srgb(ui, &mut c).changed() {
        v[key] = bgr_value(c);
    }
}
fn changed(original: &Value, draft: &Value) -> Map<String, Value> {
    draft
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, v)| original.get(k.as_str()) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
const LINE_TYPES: &[&str] = &[
    "없음",
    "실선",
    "긴 점선",
    "점선",
    "일점 쇄선",
    "이점 쇄선",
    "긴 파선",
    "원형 점선",
    "이중 실선",
    "얇고 굵은 이중선",
    "굵고 얇은 이중선",
    "삼중선",
    "물결",
    "이중 물결",
    "두꺼운 3D",
    "두꺼운 3D(역)",
    "3D 단선",
    "3D 단선(역)",
];
const ARROWS: &[&str] = &[
    "모양 없음",
    "화살",
    "오목 화살",
    "속 빈 다이아몬드",
    "속 빈 원",
    "속 빈 사각",
    "다이아몬드",
    "원",
    "사각",
];
const SHAPES: &[(&str, &str)] = &[
    ("textbox", "가로 글상자"),
    ("rectangle", "직사각형"),
    ("ellipse", "타원"),
    ("line", "직선"),
    ("arc", "호"),
    ("polygon", "다각형"),
    ("curve", "곡선"),
];

impl HwpApp {
    /// `Some(enabled)` when this module owns the ribbon/menu command id.
    pub(crate) fn object_ext_enabled(&self, id: &str) -> Option<bool> {
        let body = self.session.cursor.cell.is_none();
        let selected = self.ext.object.selected.is_some();
        let unlocked = self.ext.object.selected.is_some_and(|s| !s.locked);
        if id == "insert.shape.connector" {
            return Some(body);
        }
        if let Some(kind) = id.strip_prefix("insert.shape.connector.") {
            return CONNECTORS
                .iter()
                .any(|(k, _, _)| *k == kind)
                .then_some(body);
        }
        // 세로 글상자 shares the `insert.shape.` prefix but is not in `SHAPES`.
        if matches!(id, "insert.shape.vtextbox" | "insert.shape.vtextbox_all") {
            return Some(body);
        }
        if let Some(kind) = id.strip_prefix("insert.shape.") {
            return SHAPES.iter().any(|(k, _)| *k == kind).then_some(body);
        }
        if let Some(mode) = id.strip_prefix("object.z_order.") {
            return Z_ORDERS.iter().any(|(m, _)| *m == mode).then_some(unlocked);
        }
        if let Some(mode) = id.strip_prefix("object.align.") {
            let several = !self.ext.object.multi.is_empty();
            return ALIGNS
                .iter()
                .find(|(m, _, _)| *m == mode)
                .map(|(_, _, needs_several)| {
                    unlocked && !self.selected_inline() && (!needs_several || several)
                });
        }
        if let Some(rest) = id.strip_prefix("object.style.") {
            let ok = match rest.split_once('.') {
                Some(("fill" | "outline", c)) => {
                    c == "none" || QUICK_COLORS.iter().any(|(k, _)| *k == c)
                }
                Some(("dash", n)) => n.parse::<usize>().is_ok_and(|n| n < LINE_TYPES.len()),
                Some(("pen", w)) => PEN_WIDTHS.iter().any(|(k, _)| *k == w),
                Some(("arrow_end" | "arrow_start", n)) => {
                    n.parse::<usize>().is_ok_and(|n| n < ARROWS.len())
                }
                _ => false,
            };
            return ok.then_some(unlocked);
        }
        if let Some(mode) = id.strip_prefix("object.shadow.") {
            let shape = unlocked
                && !matches!(
                    self.object_ext_selected_kind(),
                    Some("picture" | "equation")
                );
            return matches!(
                mode,
                "none" | "on" | "left" | "right" | "up" | "down" | "reset"
            )
            .then_some(shape);
        }
        if let Some(preset) = id.strip_prefix("object.outer_margin.") {
            return matches!(preset, "none" | "narrow" | "normal" | "wide").then_some(unlocked);
        }
        if let Some(preset) = id.strip_prefix("textbox.margin.") {
            return matches!(preset, "none" | "narrow" | "normal" | "wide")
                .then_some(unlocked && self.object_ext_selected_kind() == Some("textbox"));
        }
        if let Some(dir) = id.strip_prefix("textbox.direction.") {
            return matches!(dir, "horizontal" | "vertical" | "vertical_all")
                .then_some(unlocked && self.object_ext_selected_kind() == Some("textbox"));
        }
        if let Some(rest) = id.strip_prefix("textbox.align.") {
            let ok = rest.split_once('_').is_some_and(|(h, v)| {
                matches!(h, "left" | "center" | "right") && matches!(v, "top" | "center" | "bottom")
            });
            return ok.then_some(unlocked && self.object_ext_selected_kind() == Some("textbox"));
        }
        if let Some(kind) = id.strip_prefix("object.connector_type.") {
            return matches!(kind, "straight" | "stroke" | "arc")
                .then_some(unlocked && self.selected_connector());
        }
        if let Some(mode) = id.strip_prefix("picture.recolor.") {
            return matches!(mode, "original" | "grayscale" | "black_white" | "watermark")
                .then_some(unlocked && self.object_ext_selected_kind() == Some("picture"));
        }
        if let Some(kind) = id.strip_prefix("insert.auto_number.") {
            return matches!(kind, "page" | "total" | "picture" | "table" | "equation")
                .then_some(body);
        }
        match id {
            "object.protect_size" | "object.format.copy" => Some(selected),
            "object.format.paste" => Some(unlocked && self.ext.object.format.is_some()),
            "object.next" | "object.prev" | "object.select" | "picture.save_all" => {
                Some(self.doc_facts().0)
            }
            "picture.save" => Some(self.object_ext_selected_kind() == Some("picture")),
            // A new chart goes at a body caret: not in a cell, nothing selected.
            "insert.chart" => Some(body && !selected && !self.session.has_selection()),
            "note.convert" | "note.delete" => Some(self.session.note_at_caret().is_some()),
            "note.shape" | "note.shape.footnote" | "note.shape.endnote" | "field.delete" => {
                Some(true)
            }
            _ => None,
        }
        .or_else(|| self.object_ext_enabled_legacy(id))
    }
    fn object_ext_enabled_legacy(&self, id: &str) -> Option<bool> {
        let body = self.session.cursor.cell.is_none();
        let selected = self.ext.object.selected.is_some();
        let unlocked = self.ext.object.selected.is_some_and(|s| !s.locked);
        let _ = body;
        match id {
            "insert.picture_dialog"
            | "object.list"
            | "object.properties"
            | "memo.list"
            | "object.unlock_all"
            | "insert.field"
            | "insert.form_object" => Some(true),
            "object.group" => Some(selected && !self.ext.object.multi.is_empty()),
            "object.ungroup" => Some(
                self.ext
                    .object
                    .selected
                    .is_some_and(|s| s.group && !s.locked),
            ),
            "object.lock" => Some(selected),
            "chart.edit" => Some(true),
            "object.delete"
            | "object.placement.inline"
            | "object.placement.square"
            | "object.placement.top_bottom"
            | "object.placement.front"
            | "object.placement.behind"
            | "object.rotate.left"
            | "object.rotate.right"
            | "object.flip.horizontal"
            | "object.flip.vertical"
            | "object.caption" => Some(unlocked),
            _ => None,
        }
    }
    /// Return true when the command id was handled.
    pub(crate) fn object_ext_dispatch(&mut self, id: &str, _ctx: &egui::Context) -> bool {
        let connector = if id == "insert.shape.connector" {
            Some(&CONNECTORS[0])
        } else {
            id.strip_prefix("insert.shape.connector.")
                .and_then(|kind| CONNECTORS.iter().find(|(k, _, _)| *k == kind))
        };
        if let Some((_, label, engine_kind)) = connector {
            self.ext.object.draw = Some(engine_kind);
            self.clear_object_selection();
            self.message = format!(
                "{label}: 도형 위에서 시작해 다른 도형까지 끌면 두 도형에 붙습니다. 빈 곳에 끌면 자유 연결선, Esc 취소."
            );
            return true;
        }
        if matches!(id, "insert.shape.vtextbox" | "insert.shape.vtextbox_all") {
            return self.object_ext_dispatch_14(id);
        }
        if let Some(kind) = id.strip_prefix("insert.shape.") {
            if let Some((k, label)) = SHAPES.iter().find(|(k, _)| *k == kind) {
                self.ext.object.draw = Some(k);
                self.clear_object_selection();
                self.message =
                    format!("{label}: 쪽에서 끌어 그리세요. 클릭만 하면 기본 크기, Esc 취소.");
                return true;
            }
            return false;
        }
        if self.object_ext_dispatch_14(id) {
            return true;
        }
        if let Some(mode) = id.strip_prefix("object.z_order.") {
            let Some((mode, label)) = Z_ORDERS.iter().find(|(m, _)| *m == mode) else {
                return false;
            };
            if let Some(s) = self.ext.object.selected {
                let r = self
                    .session
                    .run("object.z_order", json!({"address":s.address,"mode":mode}));
                if let Some(v) = self.result(r) {
                    self.ext.object.layout = None;
                    self.message = if v["moved"] == true {
                        format!("{label}: 개체 순서를 바꿨습니다.")
                    } else {
                        format!("{label}: 이미 그 자리에 있습니다.")
                    };
                }
            }
            return true;
        }
        match id {
            "object.group" => {
                let Some(s) = self.ext.object.selected else {
                    return true;
                };
                let mut targets: Vec<ObjectAddress> =
                    self.ext.object.multi.iter().map(|m| m.address).collect();
                targets.push(s.address);
                let page = s.page;
                if self.run("object.group", json!({"targets":targets})) {
                    self.clear_object_selection();
                    self.ext.object.layout = None;
                    // Keep the new group selected so 풀기/순서 apply at once.
                    if let Some(a) = self.session.objects().as_array().and_then(|list| {
                        list.iter()
                            .filter(|o| o["kind"] == "group")
                            .filter_map(|o| {
                                serde_json::from_value::<ObjectAddress>(o["address"].clone()).ok()
                            })
                            .next_back()
                    }) {
                        self.ext.object.selected = self.selected_from_address(page, a);
                    }
                    self.message = "개체를 묶었습니다.".into();
                }
            }
            "object.ungroup" => {
                if let Some(s) = self.ext.object.selected
                    && self.run("object.ungroup", json!({"address":s.address}))
                {
                    self.clear_object_selection();
                    self.ext.object.layout = None;
                    self.message = "묶음을 풀었습니다.".into();
                }
            }
            "object.lock" => {
                let Some(s) = self.ext.object.selected else {
                    self.message = "보호할 개체를 먼저 클릭해 선택하세요.".into();
                    return true;
                };
                let r = self
                    .session
                    .run("object.lock", json!({"address":s.address}));
                if let Some(v) = self.result(r) {
                    let locked = v["locked"] == true;
                    if let Some(sel) = self.ext.object.selected.as_mut() {
                        sel.locked = locked;
                    }
                    self.message = if locked {
                        "개체를 보호했습니다. 이동·크기 조절·지우기가 막힙니다.".into()
                    } else {
                        "개체 보호를 해제했습니다.".into()
                    };
                }
            }
            "object.unlock_all" => {
                let r = self.session.run("object.unlock_all", json!({}));
                if let Some(v) = self.result(r) {
                    if let Some(sel) = self.ext.object.selected.as_mut() {
                        sel.locked = false;
                    }
                    for m in &mut self.ext.object.multi {
                        m.locked = false;
                    }
                    self.message = format!("개체 {}개의 보호를 해제했습니다.", v["unlocked"]);
                }
            }
            "insert.field" => {
                self.ext.object.field = Some(FieldInsert {
                    guide: "이곳을 마우스로 누르고 내용을 입력하세요.".into(),
                    name: String::new(),
                    memo: String::new(),
                });
            }
            "insert.form_object" => self.open_form_editor(),
            "chart.edit" => self.open_chart_editor(0),
            "insert.chart" => self.open_chart_creation(None),
            "memo.list" => self.ext.object.memos = true,
            "insert.picture_dialog" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "그림 (PNG, JPG, BMP, GIF)",
                        &["png", "jpg", "jpeg", "bmp", "gif"],
                    )
                    .pick_file()
                {
                    self.ext.object.picture = Some(PictureInsert {
                        path,
                        treat_as_char: true,
                        custom_width: false,
                        width_mm: 80.0,
                    });
                }
            }
            "object.list" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                    object_dialogs::ObjectDialog::Objects,
                ))
            }
            "object.properties" => {
                let target = self
                    .ext
                    .object
                    .selected
                    .map(|s| s.address)
                    .or_else(|| self.object_near_caret());
                match target {
                    Some(a) => self.open_object_properties(a),
                    None => {
                        self.message = "개체를 클릭해 선택하거나 개체 목록에서 고르세요.".into();
                        self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                            object_dialogs::ObjectDialog::Objects,
                        ));
                    }
                }
            }
            "object.delete" => {
                if let Some(s) = self.ext.object.selected {
                    if s.locked {
                        self.message = "보호된 개체입니다. 개체 보호를 먼저 해제하세요.".into();
                    } else {
                        self.clear_object_selection();
                        self.run("object.delete", json!(s.address));
                    }
                }
            }
            "object.caption" => {
                self.quick_props(json!({"hasCaption":true,"captionDirection":"Bottom"}))
            }
            "object.placement.inline" => self.quick_props(json!({"treatAsChar":true})),
            "object.placement.square" => {
                self.quick_props(json!({"treatAsChar":false,"textWrap":"Square"}))
            }
            "object.placement.top_bottom" => {
                self.quick_props(json!({"treatAsChar":false,"textWrap":"TopAndBottom"}))
            }
            "object.placement.front" => {
                self.quick_props(json!({"treatAsChar":false,"textWrap":"InFrontOfText"}))
            }
            "object.placement.behind" => {
                self.quick_props(json!({"treatAsChar":false,"textWrap":"BehindText"}))
            }
            "object.rotate.left"
            | "object.rotate.right"
            | "object.flip.horizontal"
            | "object.flip.vertical" => {
                let Some(s) = self.ext.object.selected else {
                    return true;
                };
                // Shapes flip through RHWP's Hangul-measured flip (bits + matrix);
                // pictures keep the picture property path.
                if id.starts_with("object.flip.")
                    && self.object_ext_selected_kind() != Some("picture")
                {
                    let axis = if id.ends_with("horizontal") {
                        "horizontal"
                    } else {
                        "vertical"
                    };
                    if self.run("object.flip", json!({"address":s.address,"axis":axis})) {
                        self.ext.object.layout = None;
                    }
                    return true;
                }
                let r = self.session.object_properties(s.address);
                if let Some(p) = self.result(r) {
                    let p = &p["props"];
                    let props = match id {
                        "object.rotate.left" | "object.rotate.right" => {
                            let step = if id.ends_with("left") { -90 } else { 90 };
                            let angle =
                                (p["rotationAngle"].as_i64().unwrap_or(0) + step).rem_euclid(360);
                            json!({"rotationAngle":angle})
                        }
                        "object.flip.horizontal" => {
                            json!({"horzFlip":p["horzFlip"] != true})
                        }
                        _ => json!({"vertFlip":p["vertFlip"] != true}),
                    };
                    self.quick_props(props);
                }
            }
            _ => return false,
        }
        true
    }
    fn quick_props(&mut self, props: Value) {
        if let Some(s) = self.ext.object.selected
            && self.run(
                "object.properties",
                json!({"address":s.address,"props":props}),
            )
        {
            self.ext.object.layout = None;
        }
    }
    fn clear_object_selection(&mut self) {
        self.ext.object.selected = None;
        self.ext.object.multi.clear();
        self.ext.object.endpoint = None;
        self.ext.object.connector_start = None;
    }
    /// Lock / group / plain-line facts from the model for a selection.
    fn object_facts(&self, a: ObjectAddress) -> (bool, bool, bool) {
        use hwpcraft_doc::model::{control::Control, shape::ShapeObject};
        let locked = self.session.object_locked(a);
        let ctrl = self
            .session
            .core
            .document()
            .sections
            .get(a.section)
            .and_then(|s| s.paragraphs.get(a.paragraph))
            .and_then(|p| p.controls.get(a.control));
        let (group, plain_line) = match ctrl {
            Some(Control::Shape(s)) => (
                matches!(s.as_ref(), ShapeObject::Group(_)),
                matches!(s.as_ref(), ShapeObject::Line(l) if l.connector.is_none()),
            ),
            _ => (false, false),
        };
        (locked, group, plain_line)
    }
    /// Selection record for an object found through the page layout.
    fn selected_from_address(&mut self, page: u32, a: ObjectAddress) -> Option<Selected> {
        let layout = self.object_page_layout(page)?;
        let n = |v: &Value, k: &str| v[k].as_u64().map(|x| x as usize);
        let c = layout["controls"].as_array()?.iter().find(|c| {
            n(c, "secIdx") == Some(a.section)
                && n(c, "paraIdx") == Some(a.paragraph)
                && n(c, "controlIdx") == Some(a.control)
                && c.get("cellIdx").is_none()
        })?;
        Some(self.selected_from_layout(page, c.clone()))
    }
    fn object_page_layout(&mut self, page: u32) -> Option<std::sync::Arc<Value>> {
        Some(super::ext_table::cached_page_layout(
            &mut self.ext.object.layout,
            &self.session,
            page,
        ))
    }
    fn selected_from_layout(&self, page: u32, c: Value) -> Selected {
        let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
        let n = |k: &str| c[k].as_u64().unwrap_or(0) as usize;
        let bbox = Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")));
        let address = ObjectAddress {
            section: n("secIdx"),
            paragraph: n("paraIdx"),
            control: n("controlIdx"),
        };
        let inline = self
            .session
            .object_properties(address)
            .ok()
            .is_some_and(|v| v["props"]["treatAsChar"] == true);
        let (locked, group, plain_line) = self.object_facts(address);
        let line = (plain_line && c["type"] == "line" && !inline)
            .then(|| (Pos2::new(f("x1"), f("y1")), Pos2::new(f("x2"), f("y2"))));
        Selected {
            address,
            page,
            bbox,
            inline,
            locked,
            line,
            group,
        }
    }
    /// The last drawable object anchored in the caret paragraph at or before the caret.
    fn object_near_caret(&self) -> Option<ObjectAddress> {
        let c = self.session.cursor;
        if c.cell.is_some() {
            return None;
        }
        let p = self
            .session
            .core
            .document()
            .sections
            .get(c.section)?
            .paragraphs
            .get(c.paragraph)?;
        let positions = p.control_text_positions();
        p.controls
            .iter()
            .enumerate()
            .zip(positions)
            .filter(|((_, ctrl), _)| {
                matches!(
                    ctrl,
                    hwpcraft_doc::model::control::Control::Picture(_)
                        | hwpcraft_doc::model::control::Control::Shape(_)
                        | hwpcraft_doc::model::control::Control::Equation(_)
                )
            })
            .filter(|(_, pos)| *pos <= c.offset)
            .map(|((i, _), _)| i)
            .next_back()
            .map(|control| ObjectAddress {
                section: c.section,
                paragraph: c.paragraph,
                control,
            })
    }
    pub(super) fn open_object_properties(&mut self, a: ObjectAddress) {
        let r = self
            .session
            .run("object.properties.get", json!({"address":a}));
        let Some(v) = self.result(r) else {
            return;
        };
        let kind = v["kind"].as_str().unwrap_or("").to_owned();
        if kind == "table" {
            self.error = Some("표는 표 안을 클릭한 뒤 표/셀 속성(P)을 쓰세요.".into());
            return;
        }
        let mut props = v["props"].clone();
        if let Some(o) = props.as_object_mut() {
            // Read-only or derived values never go back to RHWP.
            for key in [
                "originalWidth",
                "originalHeight",
                "externalPath",
                "zOrder",
                "instanceId",
                "scInstId",
                "borderAttr",
                "borderOutlineStyle",
                "shadowAlpha",
                "captionMaxWidth",
                "connectorType",
                "connectorMidX",
                "connectorMidY",
            ] {
                o.remove(key);
            }
        }
        self.ext.object.properties = Some(Box::new(Properties {
            address: a,
            kind,
            tab: 0,
            original: props.clone(),
            draft: props,
            error: None,
        }));
    }

    /// Canvas hook: shape drawing, object selection and moving. True when consumed.
    pub(crate) fn object_ext_canvas(
        &mut self,
        page: u32,
        response: &egui::Response,
        rect: Rect,
    ) -> bool {
        let to_page = |s: &Self, p: Pos2| ((p - rect.min) / s.zoom).to_pos2();
        let painter = response.ctx.layer_painter(response.layer_id);
        if let Some(kind) = self.ext.object.draw {
            if kind == "polygon" {
                return self.draw_polygon(page, response, rect);
            }
            if kind == "curve" && (response.drag_started() || !self.ext.object.points.is_empty()) {
                return self.draw_freehand(page, response, rect);
            }
            if response.drag_started()
                && let Some(p) = response.interact_pointer_pos()
            {
                let start = to_page(self, p);
                self.ext.object.drag = Some((page, start));
                if kind.starts_with("connector") {
                    self.ext.object.connector_start = self.subject_at(page, start);
                }
            }
            if let (Some((start_page, start)), Some(p)) =
                (self.ext.object.drag, response.interact_pointer_pos())
                && start_page == page
            {
                let end = to_page(self, p);
                let r = Rect::from_two_pos(start, end);
                painter.rect_stroke(
                    Rect::from_min_max(
                        rect.min + r.min.to_vec2() * self.zoom,
                        rect.min + r.max.to_vec2() * self.zoom,
                    ),
                    0.0,
                    Stroke::new(1.0, chrome::Tokens::ACCENT),
                    egui::StrokeKind::Middle,
                );
            }
            let finished = response.drag_stopped() || response.clicked();
            if finished && let Some(p) = response.interact_pointer_pos() {
                let end = to_page(self, p);
                let start = self
                    .ext
                    .object
                    .drag
                    .take()
                    .filter(|(pg, _)| *pg == page)
                    .map_or(end, |(_, s)| s);
                let mut r = Rect::from_two_pos(start, end);
                if r.width() < 4.0 && r.height() < 4.0 {
                    // A click: Hangul's default size for a click-drawn shape.
                    r = Rect::from_min_size(start, Vec2::new(30.0, 20.0) / MM_PER_PX);
                }
                // Anchor the shape in the paragraph under the start point.
                if let Ok(c) = self
                    .session
                    .hit_test(page, f64::from(start.x), f64::from(start.y))
                    && c.cell.is_none()
                {
                    let _ = self.session.select(c, false);
                }
                let connector = kind.starts_with("connector");
                let line = kind == "line" || connector;
                let mut params = json!({"kind":kind,
                    "width_mm":(r.width() * MM_PER_PX).max(if line {0.2} else {1.0}),
                    "height_mm":(r.height() * MM_PER_PX).max(if line {0.2} else {1.0}),
                    "x_mm":r.min.x * MM_PER_PX,"y_mm":r.min.y * MM_PER_PX,
                    "paragraphs": if kind == "textbox" {json!([""])} else {json!([])}});
                if kind == "textbox"
                    && let Some(d) = self.ext.object.draw_direction.take()
                {
                    params["direction"] = json!(d);
                }
                if line {
                    // Lines keep the drag direction (start corner → end corner).
                    params["flip_x"] = json!(end.x < start.x);
                    params["flip_y"] = json!(end.y < start.y);
                }
                if connector {
                    let begin = self.ext.object.connector_start.take();
                    let finish = self.subject_at(page, end);
                    if let (Some((sa, sp)), Some((ea, ep))) = (begin, finish)
                        && sa != ea
                    {
                        params["start"] = json!({"address":sa,"point":sp});
                        params["end"] = json!({"address":ea,"point":ep});
                    }
                }
                let attached = params.get("start").is_some();
                let result = self.session.run("shape.insert", params);
                if let Some(v) = self.result(result) {
                    self.ext.object.layout = None;
                    if let Ok(a) = serde_json::from_value::<ObjectAddress>(v["address"].clone()) {
                        self.ext.object.selected =
                            self.selected_from_address(page, a).or(Some(Selected {
                                address: a,
                                page,
                                bbox: r,
                                inline: false,
                                locked: false,
                                line: None,
                                group: false,
                            }));
                    }
                    self.message = if attached {
                        "연결선을 두 도형에 붙였습니다. 도형을 옮기면 연결선이 따라갑니다.".into()
                    } else if connector {
                        "자유 연결선을 넣었습니다(도형 위에서 시작·끝내면 도형에 붙습니다).".into()
                    } else {
                        "도형을 넣었습니다. 두 번 클릭하면 개체 속성을 엽니다.".into()
                    };
                }
                self.ext.object.draw = None;
            }
            return true;
        }
        let hit = response
            .interact_pointer_pos()
            .and_then(|p| self.object_at(page, to_page(self, p)));
        let pointer = response.interact_pointer_pos().map(|p| to_page(self, p));
        let hover = response.hover_pos().map(|p| to_page(self, p));
        let selected = self.ext.object.selected.filter(|s| s.page == page);
        let shift = response.ctx.input(|i| i.modifiers.shift);
        let mut consumed = false;
        let tol = 6.0 / self.zoom;
        let endpoint_at = |s: &Selected, p: Pos2| -> Option<usize> {
            let (a, b) = s.line?;
            if a.distance(p) <= tol {
                Some(0)
            } else if b.distance(p) <= tol {
                Some(1)
            } else {
                None
            }
        };
        // Hover feedback: resize/rotate cursors over the handles of the selection.
        if let (Some(s), Some(p)) = (selected, hover)
            && !s.locked
        {
            if endpoint_at(&s, p).is_some() {
                response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            } else if s.line.is_none()
                && let Some(h) = handle_at(s.bbox, p, self.zoom)
            {
                response.ctx.set_cursor_icon(handle_cursor(h));
            }
        }
        if response.drag_started() {
            self.ext.object.moving = None;
            self.ext.object.resizing = None;
            self.ext.object.endpoint = None;
            // 개체 보호: a locked object is only selectable.
            if let (Some(s), Some(p)) = (selected, pointer)
                && !s.locked
            {
                if let Some(e) = endpoint_at(&s, p) {
                    self.ext.object.endpoint = Some((e, p));
                    consumed = true;
                } else if s.line.is_none()
                    && let Some(h) = handle_at(s.bbox, p, self.zoom)
                {
                    self.ext.object.resizing = Some((h, p));
                    consumed = true;
                } else if hit.is_some_and(|h| h.address == s.address) && !s.inline {
                    self.ext.object.moving = Some(Vec2::ZERO);
                    consumed = true;
                }
            }
        }
        // Straight line end points: preview while dragging, one command on release.
        let mut endpoint_preview: Option<(Pos2, Pos2)> = None;
        if let (Some((e, _)), Some(s), Some(p)) = (self.ext.object.endpoint, selected, pointer)
            && let Some((a, b)) = s.line
        {
            consumed = true;
            let ends = if e == 0 { (p, b) } else { (a, p) };
            endpoint_preview = Some(ends);
            if response.drag_stopped() {
                self.ext.object.endpoint = None;
                let mm = |q: Pos2| [f64::from(q.x * MM_PER_PX), f64::from(q.y * MM_PER_PX)];
                if self.run(
                    "object.line_endpoint",
                    json!({"address":s.address,"start_mm":mm(ends.0),"end_mm":mm(ends.1)}),
                ) {
                    self.ext.object.layout = None;
                    self.ext.object.selected = self.selected_from_address(page, s.address);
                }
            }
        }
        if let Some(delta) = self.ext.object.moving.as_mut() {
            *delta += response.drag_delta() / self.zoom;
            consumed = true;
            if response.drag_stopped() {
                let d = *delta;
                self.ext.object.moving = None;
                if let Some(s) = self.ext.object.selected
                    && d.length() > 1.0
                {
                    let r = self.session.object_properties(s.address);
                    if let Some(v) = self.result(r) {
                        let p = &v["props"];
                        let x = p["horzOffset"].as_i64().unwrap_or(0) + (d.x * 75.0) as i64;
                        let y = p["vertOffset"].as_i64().unwrap_or(0) + (d.y * 75.0) as i64;
                        self.quick_props(json!({"horzOffset":x,"vertOffset":y}));
                        self.ext.object.selected = None;
                    }
                }
            }
        }
        // Handle drags preview locally and commit once on release (one undo step).
        let mut preview: Option<Rect> = None;
        let mut angle: Option<f32> = None;
        if let (Some((h, _)), Some(s), Some(p)) = (self.ext.object.resizing, selected, pointer) {
            consumed = true;
            let shift = response.ctx.input(|i| i.modifiers.shift);
            if h == ROTATE {
                let c = s.bbox.center();
                let v = p - c;
                angle = Some(v.x.atan2(-v.y).to_degrees().rem_euclid(360.0));
            } else {
                preview = Some(resized(s.bbox, h, p, shift));
            }
            if response.drag_stopped() {
                self.ext.object.resizing = None;
                if let Some(a) = angle {
                    let a = a.round() as i64 % 360;
                    self.quick_props(json!({"rotationAngle":a}));
                } else if let Some(r) = preview {
                    let mut props = json!({
                        "width":(r.width() * 75.0).round() as i64,
                        "height":(r.height() * 75.0).round() as i64});
                    let (dx, dy) = (r.min.x - s.bbox.min.x, r.min.y - s.bbox.min.y);
                    if !s.inline && (dx.abs() > 0.1 || dy.abs() > 0.1) {
                        let cur = self.session.object_properties(s.address);
                        if let Some(v) = self.result(cur) {
                            let p = &v["props"];
                            props["horzOffset"] =
                                json!(p["horzOffset"].as_i64().unwrap_or(0) + (dx * 75.0) as i64);
                            props["vertOffset"] =
                                json!(p["vertOffset"].as_i64().unwrap_or(0) + (dy * 75.0) as i64);
                        }
                    }
                    self.quick_props(props);
                }
                self.ext.object.selected = None;
            }
        }
        if !consumed && (response.clicked() || response.double_clicked()) {
            if shift
                && let (Some(cur), Some(h)) = (self.ext.object.selected, hit)
                && h.address != cur.address
                && h.address.section == cur.address.section
            {
                // Shift+click extends the selection (묶기 targets) or drops one again.
                if let Some(i) = self
                    .ext
                    .object
                    .multi
                    .iter()
                    .position(|m| m.address == h.address)
                {
                    self.ext.object.multi.remove(i);
                } else {
                    self.ext.object.multi.push(cur);
                    self.ext.object.selected = Some(h);
                }
            } else {
                self.ext.object.multi.clear();
                self.ext.object.selected = hit;
            }
            if hit.is_some() {
                consumed = true;
                ctx_focus(&response.ctx);
            }
        }
        if response.double_clicked()
            && let Some(h) = hit
        {
            self.open_object_editor(h.address);
            consumed = true;
        }
        let to_screen = |r: Rect| {
            Rect::from_min_max(
                rect.min + r.min.to_vec2() * self.zoom,
                rect.min + r.max.to_vec2() * self.zoom,
            )
        };
        let to_screen_pos = |p: Pos2| rect.min + p.to_vec2() * self.zoom;
        let stroke = Stroke::new(1.0, chrome::Tokens::ACCENT);
        for m in self.ext.object.multi.iter().filter(|m| m.page == page) {
            let r = to_screen(m.bbox);
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Outside);
            for p in [
                r.left_top(),
                r.right_top(),
                r.left_bottom(),
                r.right_bottom(),
            ] {
                painter.rect_filled(
                    Rect::from_center_size(p, Vec2::splat(6.0)),
                    0.0,
                    chrome::Tokens::ACCENT,
                );
            }
        }
        if let Some(s) = self.ext.object.selected.filter(|s| s.page == page) {
            let mut r = to_screen(preview.unwrap_or(s.bbox));
            if let Some(d) = self.ext.object.moving {
                r = r.translate(d * self.zoom);
            }
            if s.locked {
                let grey = Stroke::new(1.0, Color32::from_gray(140));
                painter.rect_stroke(r, 0.0, grey, egui::StrokeKind::Outside);
                // Badge below the box so it never covers the text above the object.
                painter.text(
                    r.left_bottom() + Vec2::new(0.0, 3.0),
                    egui::Align2::LEFT_TOP,
                    "개체 보호",
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(110),
                );
            } else if let Some((a, b)) = endpoint_preview.or(s.line) {
                // A straight line shows its two end-point handles instead of a box.
                let (sa, sb) = (to_screen_pos(a), to_screen_pos(b));
                if endpoint_preview.is_some() {
                    painter.line_segment([sa, sb], Stroke::new(1.0, Color32::from_gray(170)));
                }
                for p in [sa, sb] {
                    painter.circle(p, 5.0, Color32::WHITE, stroke);
                }
            } else {
                if preview.is_some() {
                    painter.rect_stroke(
                        to_screen(s.bbox),
                        0.0,
                        Stroke::new(1.0, Color32::from_gray(170)),
                        egui::StrokeKind::Outside,
                    );
                }
                painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Outside);
                for p in handle_points(r) {
                    let h = Rect::from_center_size(p, Vec2::splat(7.0));
                    painter.rect_filled(h, 0.0, Color32::WHITE);
                    painter.rect_stroke(h, 0.0, stroke, egui::StrokeKind::Inside);
                }
                if !s.inline {
                    let knob = r.center_top() - Vec2::new(0.0, ROTATE_GAP);
                    painter.line_segment([r.center_top(), knob], stroke);
                    painter.circle(knob, 4.5, Color32::from_rgb(120, 200, 120), stroke);
                }
                if let Some(a) = angle {
                    let c = r.center();
                    let dir = Vec2::angled((a - 90.0).to_radians());
                    painter.line_segment([c, c + dir * r.height().max(30.0) * 0.6], stroke);
                    painter.text(
                        c,
                        egui::Align2::CENTER_BOTTOM,
                        format!("{}°", a.round() as i64 % 360),
                        egui::FontId::proportional(12.0),
                        chrome::Tokens::ACCENT,
                    );
                }
            }
        }
        consumed
    }
    /// Polygon tool: each click adds a vertex; double click or Enter finishes.
    fn draw_polygon(&mut self, page: u32, response: &egui::Response, rect: Rect) -> bool {
        let to_page = |s: &Self, p: Pos2| ((p - rect.min) / s.zoom).to_pos2();
        if !self.ext.object.points.is_empty() && self.ext.object.points_page != page {
            return true;
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos().map(|p| to_page(self, p))
        {
            // The second click of a double click lands on the same spot; keep one vertex.
            if self
                .ext
                .object
                .points
                .last()
                .is_none_or(|q| q.distance(p) > 2.0)
            {
                self.ext.object.points.push(p);
                self.ext.object.points_page = page;
            }
        }
        let painter = response.ctx.layer_painter(response.layer_id);
        let to_screen = |q: Pos2| rect.min + q.to_vec2() * self.zoom;
        let stroke = Stroke::new(1.2, chrome::Tokens::ACCENT);
        let pts: Vec<Pos2> = self
            .ext
            .object
            .points
            .iter()
            .map(|q| to_screen(*q))
            .collect();
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], stroke);
        }
        if let (Some(last), Some(h)) = (pts.last(), response.hover_pos()) {
            painter.line_segment([*last, h], Stroke::new(1.0, Color32::from_gray(150)));
        }
        for q in &pts {
            painter.circle_filled(*q, 3.0, chrome::Tokens::ACCENT);
        }
        let finish = response.double_clicked()
            || response
                .ctx
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        if finish {
            let points = std::mem::take(&mut self.ext.object.points);
            if points.len() < 3 {
                self.message = "다각형은 꼭짓점 3개 이상이 필요합니다. 계속 클릭하세요.".into();
                self.ext.object.points = points;
            } else {
                self.insert_drawn(page, "polygon", &points);
            }
        }
        true
    }
    /// Curve tool: drag draws a freehand line, sampled every 2 px.
    fn draw_freehand(&mut self, page: u32, response: &egui::Response, rect: Rect) -> bool {
        let to_page = |s: &Self, p: Pos2| ((p - rect.min) / s.zoom).to_pos2();
        if response.drag_started() {
            self.ext.object.points.clear();
            self.ext.object.points_page = page;
        }
        if self.ext.object.points_page != page {
            return true;
        }
        if let Some(p) = response.interact_pointer_pos().map(|p| to_page(self, p))
            && (response.dragged() || response.drag_started())
            && self
                .ext
                .object
                .points
                .last()
                .is_none_or(|q| q.distance(p) >= 2.0)
            && self.ext.object.points.len() < 1024
        {
            self.ext.object.points.push(p);
        }
        let painter = response.ctx.layer_painter(response.layer_id);
        let pts: Vec<Pos2> = self
            .ext
            .object
            .points
            .iter()
            .map(|q| rect.min + q.to_vec2() * self.zoom)
            .collect();
        painter.add(egui::Shape::line(
            pts,
            Stroke::new(1.2, chrome::Tokens::ACCENT),
        ));
        if response.drag_stopped() {
            let points = std::mem::take(&mut self.ext.object.points);
            if points.len() >= 2 {
                self.insert_drawn(page, "curve", &points);
            } else {
                self.ext.object.draw = None;
            }
        }
        true
    }
    /// Insert a polygon/curve from page-pixel points, positioned on the paper.
    fn insert_drawn(&mut self, page: u32, kind: &str, points: &[Pos2]) {
        let mut bbox = Rect::from_points(points);
        // Keep a non-zero frame for straight horizontal/vertical strokes.
        bbox = bbox.union(Rect::from_min_size(bbox.min, Vec2::splat(1.0)));
        if let Ok(c) = self
            .session
            .hit_test(page, f64::from(bbox.min.x), f64::from(bbox.min.y))
            && c.cell.is_none()
        {
            let _ = self.session.select(c, false);
        }
        let rel: Vec<[f32; 2]> = points
            .iter()
            .map(|q| {
                [
                    ((q.x - bbox.min.x) * MM_PER_PX).clamp(0.0, bbox.width() * MM_PER_PX),
                    ((q.y - bbox.min.y) * MM_PER_PX).clamp(0.0, bbox.height() * MM_PER_PX),
                ]
            })
            .collect();
        let params = json!({"kind":kind,
            "width_mm":bbox.width() * MM_PER_PX,"height_mm":bbox.height() * MM_PER_PX,
            "x_mm":bbox.min.x * MM_PER_PX,"y_mm":bbox.min.y * MM_PER_PX,"points_mm":rel});
        let result = self.session.run("shape.insert", params);
        if let Some(v) = self.result(result) {
            self.ext.object.layout = None;
            if let Ok(a) = serde_json::from_value::<ObjectAddress>(v["address"].clone()) {
                self.ext.object.selected = self.selected_from_address(page, a).or(Some(Selected {
                    address: a,
                    page,
                    bbox,
                    inline: false,
                    locked: false,
                    line: None,
                    group: false,
                }));
            }
        }
        self.ext.object.draw = None;
    }
    /// Double click: equations open the equation editor, text boxes and notes their text.
    fn open_object_editor(&mut self, a: ObjectAddress) {
        use hwpcraft_doc::model::control::Control;
        let kind = self
            .session
            .core
            .document()
            .sections
            .get(a.section)
            .and_then(|s| s.paragraphs.get(a.paragraph))
            .and_then(|p| p.controls.get(a.control))
            .map(|c| match c {
                Control::Equation(_) => 0,
                Control::Shape(s) if s.drawing().and_then(|d| d.text_box.as_ref()).is_some() => 1,
                _ => 2,
            });
        match kind {
            Some(0) => self.open_equation_editor(Some(a)),
            Some(1) => {
                let r = self.session.textbox_paragraphs(a);
                if let Some(p) = self.result(r) {
                    self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                        object_dialogs::ObjectDialog::Textbox {
                            address: a,
                            text: p.join("\n"),
                        },
                    ));
                }
            }
            _ => self.open_object_properties(a),
        }
    }
    fn layout_bbox(c: &Value) -> Rect {
        let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
        Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")))
    }
    fn object_at(&mut self, page: u32, p: Pos2) -> Option<Selected> {
        let layout = self.object_page_layout(page)?;
        let c = layout["controls"]
            .as_array()?
            .iter()
            .rev()
            .filter(|c| {
                matches!(
                    c["type"].as_str(),
                    Some("image" | "shape" | "line" | "group" | "equation")
                ) && c.get("cellIdx").is_none()
                    && c.get("noteRef").is_none()
                    && c["controlIdx"].is_u64()
            })
            .find(|c| Self::layout_bbox(c).expand(3.0).contains(p))?
            .clone();
        Some(self.selected_from_layout(page, c))
    }
    /// A shape or picture that a connector can attach to, with its nearest connection point.
    fn subject_at(&mut self, page: u32, p: Pos2) -> Option<(ObjectAddress, u32)> {
        let layout = self.object_page_layout(page)?;
        let n = |v: &Value, k: &str| v[k].as_u64().map(|x| x as usize);
        layout["controls"]
            .as_array()?
            .iter()
            .rev()
            .filter(|c| {
                matches!(c["type"].as_str(), Some("image" | "shape" | "group"))
                    && c.get("cellIdx").is_none()
                    && c.get("noteRef").is_none()
            })
            .find_map(|c| {
                let bbox = Self::layout_bbox(c);
                if !bbox.expand(3.0).contains(p) {
                    return None;
                }
                let address = ObjectAddress {
                    section: n(c, "secIdx")?,
                    paragraph: n(c, "paraIdx")?,
                    control: n(c, "controlIdx")?,
                };
                Some((address, connection_point(bbox, p)))
            })
    }

    /// Keys while an object is selected or a shape tool is armed. True when consumed.
    pub(crate) fn object_ext_keys(&mut self, ctx: &egui::Context) -> bool {
        let consume = |m: egui::Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if self.ext.object.draw.is_some() && consume(egui::Modifiers::NONE, Key::Escape) {
            self.ext.object.draw = None;
            self.ext.object.points.clear();
            self.message = "도형 그리기를 취소했습니다.".into();
            return true;
        }
        let Some(s) = self.ext.object.selected else {
            return false;
        };
        if consume(egui::Modifiers::NONE, Key::Escape) {
            self.clear_object_selection();
            return true;
        }
        if consume(egui::Modifiers::NONE, Key::Delete)
            || consume(egui::Modifiers::NONE, Key::Backspace)
        {
            if s.locked {
                self.message = "보호된 개체입니다. 개체 보호를 먼저 해제하세요.".into();
                return true;
            }
            let targets: Vec<ObjectAddress> = self
                .ext
                .object
                .multi
                .iter()
                .filter(|m| !m.locked)
                .map(|m| m.address)
                .chain(std::iter::once(s.address))
                .collect();
            self.clear_object_selection();
            // Delete back to front so earlier control indices stay valid.
            let mut targets = targets;
            targets.sort_by_key(|a| (a.section, a.paragraph, std::cmp::Reverse(a.control)));
            targets.reverse();
            for a in targets {
                if !self.run("object.delete", json!(a)) {
                    break;
                }
            }
            return true;
        }
        if consume(egui::Modifiers::NONE, Key::Enter) {
            self.open_object_editor(s.address);
            return true;
        }
        // Hangul: Tab / Shift+Tab with an object selected walk to the next / previous object.
        if consume(egui::Modifiers::NONE, Key::Tab) {
            self.select_neighbor(true);
            return true;
        }
        if consume(egui::Modifiers::SHIFT, Key::Tab) {
            self.select_neighbor(false);
            return true;
        }
        if s.locked {
            for key in [
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
            ] {
                if consume(egui::Modifiers::NONE, key) {
                    self.message = "보호된 개체는 옮길 수 없습니다.".into();
                    return true;
                }
            }
        } else if !s.inline {
            for (key, dx, dy) in [
                (Key::ArrowLeft, -1, 0),
                (Key::ArrowRight, 1, 0),
                (Key::ArrowUp, 0, -1),
                (Key::ArrowDown, 0, 1),
            ] {
                if consume(egui::Modifiers::NONE, key) {
                    let r = self.session.object_properties(s.address);
                    if let Some(v) = self.result(r) {
                        // 1 mm per press, like Hangul's object nudge.
                        let step = HU_PER_MM.round() as i64;
                        let p = &v["props"];
                        self.quick_props(json!({
                            "horzOffset":p["horzOffset"].as_i64().unwrap_or(0) + dx * step,
                            "vertOffset":p["vertOffset"].as_i64().unwrap_or(0) + dy * step}));
                        if let Some(sel) = self.ext.object.selected.as_mut() {
                            sel.bbox = sel
                                .bbox
                                .translate(Vec2::new(dx as f32, dy as f32) / MM_PER_PX);
                        }
                    }
                    return true;
                }
            }
        }
        // Typing while an object is selected would edit the caret paragraph instead.
        ctx.input_mut(|i| {
            let before = i.events.len();
            i.events.retain(|e| !matches!(e, Event::Text(_)));
            before != i.events.len()
        })
    }

    /// Kind of the selected object for contextual ribbon tabs.
    pub(crate) fn object_ext_selected_kind(&self) -> Option<&'static str> {
        use hwpcraft_doc::model::control::Control;
        let a = self.ext.object.selected?.address;
        match self
            .session
            .core
            .document()
            .sections
            .get(a.section)?
            .paragraphs
            .get(a.paragraph)?
            .controls
            .get(a.control)?
        {
            Control::Picture(_) => Some("picture"),
            Control::Equation(_) => Some("equation"),
            Control::Shape(s) if s.drawing().and_then(|d| d.text_box.as_ref()).is_some() => {
                Some("textbox")
            }
            Control::Shape(_) => Some("shape"),
            _ => None,
        }
    }
    /// True while a dialog of this module is open.
    pub(crate) fn object_ext_modal_open(&self) -> bool {
        self.ext.object.picture.is_some()
            || self.ext.object.properties.is_some()
            || self.ext.object.equation.is_some()
            || self.ext.object.chart.is_some()
            || self.ext.object.chart_authoring.is_some()
            || self.ext.object.memos
            || self.ext.object.field.is_some()
            || self.ext.object.forms.is_some()
            || self.ext.object.note_shape.is_some()
    }
    /// Startup views for captures.
    pub(crate) fn object_ext_view(&mut self, name: &str, ctx: &egui::Context) -> bool {
        match name {
            "object-properties" | "picture-properties" | "shape-properties" => {
                let want = match name {
                    "picture-properties" => "picture",
                    "shape-properties" => "shape",
                    _ => "",
                };
                let target = self
                    .session
                    .objects()
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|o| {
                        want.is_empty()
                            || o["kind"] == want
                            || (want == "shape" && o["kind"] == "textbox")
                    })
                    .and_then(|o| {
                        serde_json::from_value::<ObjectAddress>(o["address"].clone()).ok()
                    });
                match target {
                    Some(a) => self.open_object_properties(a),
                    None => self.error = Some("개체가 있는 문서를 열어 주세요.".into()),
                }
            }
            "object-handles" => {
                // Select the first drawing object on page 1 to show handles and the rotation knob.
                let layout = self
                    .session
                    .page_control_layout(0)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .unwrap_or(Value::Null);
                let center = layout["controls"].as_array().and_then(|list| {
                    list.iter().find(|c| c["type"] == "shape").map(|c| {
                        let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
                        Pos2::new(f("x") + f("w") / 2.0, f("y") + f("h") / 2.0)
                    })
                });
                match center.and_then(|p| self.object_at(0, p)) {
                    Some(s) => self.ext.object.selected = Some(s),
                    None => self.error = Some("도형이 있는 문서를 열어 주세요.".into()),
                }
            }
            "chart-edit" => self.open_chart_editor(0),
            "chart-create" => self.open_chart_creation(None),
            "chart-configure" => self.open_chart_creation(Some(0)),
            "memo-list" => self.ext.object.memos = true,
            "picture-insert" => {
                self.ext.object.picture = Some(PictureInsert {
                    path: "그림.png".into(),
                    treat_as_char: true,
                    custom_width: false,
                    width_mm: 80.0,
                });
            }
            "shape-draw" => {
                self.object_ext_dispatch("insert.shape.rectangle", ctx);
            }
            "object-group" => {
                // Two drawing objects on page 1 selected together (Shift+click state).
                let mut found: Vec<Selected> = Vec::new();
                if let Some(layout) = self.object_page_layout(0) {
                    for c in layout["controls"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|c| {
                            matches!(c["type"].as_str(), Some("shape" | "image"))
                                && c.get("cellIdx").is_none()
                        })
                        .take(2)
                    {
                        found.push(self.selected_from_layout(0, c.clone()));
                    }
                }
                if found.len() == 2 {
                    self.ext.object.selected = found.pop();
                    self.ext.object.multi = found;
                } else {
                    self.error = Some("도형이 둘 이상 있는 문서를 열어 주세요.".into());
                }
            }
            "connector-draw" => {
                self.object_ext_dispatch("insert.shape.connector.stroke_arrow", ctx);
            }
            "line-handles" => {
                // The first straight line on page 1, showing its end-point handles.
                let line = self.object_page_layout(0).and_then(|layout| {
                    layout["controls"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|c| c["type"] == "line" && c.get("cellIdx").is_none())
                        .map(|c| self.selected_from_layout(0, c.clone()))
                        .find(|s| s.line.is_some())
                });
                match line {
                    Some(s) => self.ext.object.selected = Some(s),
                    None => self.error = Some("직선이 있는 문서를 열어 주세요.".into()),
                }
            }
            "field-insert" => {
                self.object_ext_dispatch("insert.field", ctx);
            }
            "form-edit" => self.open_form_editor(),
            "note-shape" => self.open_note_shape(false),
            "object-align" => {
                // Two floating drawing objects on page 1 selected, then 왼쪽 맞춤 applied.
                let mut found: Vec<Selected> = Vec::new();
                if let Some(layout) = self.object_page_layout(0) {
                    for c in layout["controls"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|c| {
                            matches!(c["type"].as_str(), Some("shape" | "image"))
                                && c.get("cellIdx").is_none()
                        })
                    {
                        let sel = self.selected_from_layout(0, c.clone());
                        if !sel.inline && !sel.locked {
                            found.push(sel);
                        }
                        if found.len() == 2 {
                            break;
                        }
                    }
                }
                if found.len() == 2 {
                    self.ext.object.selected = found.pop();
                    self.ext.object.multi = found;
                    self.object_ext_dispatch("object.align.left", ctx);
                } else {
                    self.error = Some("떠 있는 도형이 둘 이상 있는 문서를 열어 주세요.".into());
                }
            }
            "object-select" => {
                self.object_ext_dispatch("object.select", ctx);
            }
            _ => return false,
        }
        true
    }

    /// Called every frame after the built-in dialogs.
    pub(crate) fn object_ext_dialogs(&mut self, ctx: &egui::Context) {
        self.picture_insert_dialog(ctx);
        self.object_properties_dialog(ctx);
        self.equation_editor_dialog(ctx);
        self.chart_editor_dialog(ctx);
        self.chart_creation_dialog(ctx);
        self.memo_list_dialog(ctx);
        self.field_insert_dialog(ctx);
        self.note_shape_dialog(ctx);
        self.form_editor_dialog(ctx);
    }

    /// 누름틀 넣기: Hangul's dialog fields (안내문·이름·도움말).
    fn field_insert_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.field.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let r = chrome::modal("ext-field-insert").show(ctx, |ui| {
            ui.set_width(480.0);
            close = chrome::title(ui, "누름틀 넣기");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(370.0);
                    chrome::section(ui, "입력할 내용의 안내문(G)");
                    ui.add(egui::TextEdit::singleline(&mut d.guide).desired_width(f32::INFINITY));
                    chrome::section(ui, "누름틀 이름(N)");
                    ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
                    chrome::section(ui, "도움말(M)");
                    ui.add(
                        egui::TextEdit::multiline(&mut d.memo)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.label(
                        RichText::new(
                            "커서 위치(본문 또는 표 셀)에 빈 누름틀을 넣습니다. 값은 누름틀 자리에 입력하거나 field.set 명령으로 채웁니다.",
                        )
                        .small()
                        .weak(),
                    );
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "넣기(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply {
            close = self.run(
                "insert.field",
                json!({"guide":d.guide,"name":d.name,"memo":d.memo}),
            );
            if close {
                self.message = "누름틀을 넣었습니다.".into();
            }
        }
        if !(close || r.should_close()) {
            self.ext.object.field = Some(d);
        }
    }
    /// 양식 개체: value editor for the document's existing form objects.
    pub(super) fn open_form_editor(&mut self) {
        let forms = self.session.forms();
        let drafts = forms
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                let text = f["text"].as_str().unwrap_or("").to_owned();
                let checked = f["value"].as_i64().unwrap_or(0) != 0;
                let caption = f["caption"].as_str().unwrap_or("").to_owned();
                FormDraft {
                    at: f["at"].clone(),
                    kind: f["type"].as_str().unwrap_or("").to_owned(),
                    name: f["name"].as_str().unwrap_or("").to_owned(),
                    original: (text.clone(), checked, caption.clone()),
                    text,
                    checked,
                    caption,
                }
            })
            .collect();
        self.ext.object.forms = Some(drafts);
    }
    fn form_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut drafts) = self.ext.object.forms.take() else {
            return;
        };
        fn kind_label(kind: &str) -> &'static str {
            match kind {
                "PushButton" => "명령 단추",
                "CheckBox" => "선택 상자",
                "ComboBox" => "목록 상자",
                "RadioButton" => "라디오 단추",
                "Edit" => "입력 상자",
                _ => "양식 개체",
            }
        }
        use super::ext_shell::{dialog, group, notice};
        let changed = |d: &FormDraft| {
            (d.text.as_str(), d.checked, d.caption.as_str())
                != (d.original.0.as_str(), d.original.1, d.original.2.as_str())
        };
        let button = dialog(ctx, "양식 개체", 600.0, &["적용(D)", "닫기"], |ui| {
            if drafts.is_empty() {
                notice(ui, "이 문서에 양식 개체가 없습니다.");
            }
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .show(ui, |ui| {
                    for (i, d) in drafts.iter_mut().enumerate() {
                        ui.push_id(i, |ui| {
                            group(
                                ui,
                                &format!(
                                    "{} · {}{}{}",
                                    kind_label(&d.kind),
                                    if d.name.is_empty() {
                                        "(이름 없음)"
                                    } else {
                                        &d.name
                                    },
                                    if d.at["cell"].is_object() {
                                        " (표 안)"
                                    } else {
                                        ""
                                    },
                                    if changed(d) { " · 바뀜" } else { "" }
                                ),
                            );
                            match d.kind.as_str() {
                                "Edit" | "ComboBox" => {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut d.text)
                                            .desired_width(f32::INFINITY),
                                    );
                                }
                                "CheckBox" | "RadioButton" => {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut d.checked, "선택");
                                        ui.label("캡션");
                                        ui.add(
                                            egui::TextEdit::singleline(&mut d.caption)
                                                .desired_width(f32::INFINITY),
                                        );
                                    });
                                }
                                _ => {
                                    ui.horizontal(|ui| {
                                        ui.label("캡션");
                                        ui.add(
                                            egui::TextEdit::singleline(&mut d.caption)
                                                .desired_width(f32::INFINITY),
                                        );
                                    });
                                }
                            }
                        });
                    }
                });
            notice(
                ui,
                "문서에 있는 양식 개체의 값과 캡션을 고칩니다. 새 양식 개체 넣기는 조판 엔진(RHWP)에 생성 기능이 없어 지원하지 않습니다.",
            );
        });
        let close = match button {
            Some(0) => {
                let mut applied = 0;
                let mut failed = false;
                for d in drafts.iter_mut().filter(|d| changed(d)) {
                    let mut params = json!({"at":d.at});
                    match d.kind.as_str() {
                        "Edit" | "ComboBox" => {
                            if d.text != d.original.0 {
                                params["text"] = json!(d.text);
                            }
                        }
                        "CheckBox" | "RadioButton" => {
                            if d.checked != d.original.1 {
                                params["value"] = json!(i64::from(d.checked));
                            }
                            if d.caption != d.original.2 {
                                params["caption"] = json!(d.caption);
                            }
                        }
                        _ => {
                            if d.caption != d.original.2 {
                                params["caption"] = json!(d.caption);
                            }
                        }
                    }
                    if self.run("form.set", params) {
                        d.original = (d.text.clone(), d.checked, d.caption.clone());
                        applied += 1;
                    } else {
                        failed = true;
                        break;
                    }
                }
                self.message = if applied == 0 && !failed {
                    "바뀐 값이 없습니다.".into()
                } else {
                    format!("양식 개체 {applied}개를 고쳤습니다.")
                };
                // Close only when everything applied; keep the drafts after an error.
                !failed
            }
            Some(_) => true,
            None => false,
        };
        if !close {
            self.ext.object.forms = Some(drafts);
        }
    }

    fn picture_insert_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.picture.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let r = chrome::modal("ext-picture-insert").show(ctx, |ui| {
            ui.set_width(460.0);
            close = chrome::title(ui, "그림 넣기");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(350.0);
                    chrome::section(ui, "파일");
                    ui.label(
                        d.path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                    chrome::section(ui, "넣기 설정");
                    ui.checkbox(&mut d.treat_as_char, "글자처럼 취급(C)");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut d.custom_width, "너비 지정");
                        ui.add_enabled(
                            d.custom_width,
                            egui::DragValue::new(&mut d.width_mm)
                                .range(1.0..=600.0)
                                .fixed_decimals(2)
                                .suffix(" mm"),
                        );
                    });
                    ui.add_enabled(false, egui::Checkbox::new(&mut true.clone(), "문서에 포함(E)"))
                        .on_disabled_hover_text("그림은 항상 문서에 포함합니다(외부 연결 미지원).");
                    ui.label(
                        RichText::new("PNG · JPG · BMP · GIF, 20 MB / 8192픽셀 이하. 가로세로 비율을 유지합니다.")
                            .small()
                            .weak(),
                    );
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "넣기(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply {
            let mut p = json!({"path":d.path,"treat_as_char":d.treat_as_char});
            if d.custom_width {
                p["width_mm"] = json!(d.width_mm);
            }
            close = self.run("picture.insert", p);
            self.ext.object.layout = None;
        }
        if !(close || r.should_close()) {
            self.ext.object.picture = Some(d);
        }
    }

    fn object_properties_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.properties.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let kind = d.kind.clone();
        let tabs: &[&str] = match kind.as_str() {
            "picture" => &["기본", "여백/캡션", "선", "그림"],
            "textbox" => &["기본", "여백/캡션", "선", "채우기", "글상자", "그림자"],
            "equation" => &["기본", "여백", "수식"],
            _ => &["기본", "여백/캡션", "선", "채우기", "그림자"],
        };
        let title = match kind.as_str() {
            "picture" => "그림 속성",
            "equation" => "수식 속성",
            "textbox" => "글상자 속성",
            _ => "개체 속성",
        };
        let r = chrome::modal("ext-object-properties").show(ctx, |ui| {
            ui.set_width(600.0);
            close = chrome::title(ui, title);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(490.0);
                    ui.set_min_height(380.0);
                    chrome::tabs(ui, &mut d.tab, tabs);
                    let v = &mut d.draft;
                    match tabs.get(d.tab).copied().unwrap_or("기본") {
                        "기본" => basic_tab(ui, v),
                        "여백/캡션" | "여백" => {
                            chrome::section(ui, "바깥 여백");
                            egui::Grid::new("object-outer")
                                .num_columns(4)
                                .show(ui, |ui| {
                                    mm_field(ui, "왼쪽", v, "outerMarginLeft", (0.0, 50.0));
                                    mm_field(ui, "오른쪽", v, "outerMarginRight", (0.0, 50.0));
                                    ui.end_row();
                                    mm_field(ui, "위쪽", v, "outerMarginTop", (0.0, 50.0));
                                    mm_field(ui, "아래쪽", v, "outerMarginBottom", (0.0, 50.0));
                                    ui.end_row();
                                });
                            if kind != "equation" {
                                caption_section(ui, v);
                            }
                            if kind == "picture" {
                                chrome::section(ui, "그림 여백");
                                egui::Grid::new("picture-padding")
                                    .num_columns(4)
                                    .show(ui, |ui| {
                                        mm_field(ui, "왼쪽", v, "paddingLeft", (0.0, 50.0));
                                        mm_field(ui, "오른쪽", v, "paddingRight", (0.0, 50.0));
                                        ui.end_row();
                                        mm_field(ui, "위쪽", v, "paddingTop", (0.0, 50.0));
                                        mm_field(ui, "아래쪽", v, "paddingBottom", (0.0, 50.0));
                                        ui.end_row();
                                    });
                            }
                        }
                        "선" => line_tab(ui, v, kind == "picture"),
                        "채우기" => fill_tab(ui, v),
                        "그림" => picture_tab(ui, v),
                        "글상자" => {
                            chrome::section(ui, "글상자 여백");
                            egui::Grid::new("textbox-margin")
                                .num_columns(4)
                                .show(ui, |ui| {
                                    mm_field(ui, "왼쪽", v, "tbMarginLeft", (0.0, 50.0));
                                    mm_field(ui, "오른쪽", v, "tbMarginRight", (0.0, 50.0));
                                    ui.end_row();
                                    mm_field(ui, "위쪽", v, "tbMarginTop", (0.0, 50.0));
                                    mm_field(ui, "아래쪽", v, "tbMarginBottom", (0.0, 50.0));
                                    ui.end_row();
                                });
                            chrome::section(ui, "세로 정렬");
                            ui.horizontal(|ui| {
                                for (k, l) in
                                    [("Top", "위"), ("Center", "가운데"), ("Bottom", "아래")]
                                {
                                    if ui.radio(v["tbVerticalAlign"] == k, l).clicked() {
                                        v["tbVerticalAlign"] = json!(k);
                                    }
                                }
                            });
                        }
                        "그림자" => {
                            chrome::section(ui, "그림자");
                            egui::Grid::new("shadow").num_columns(2).show(ui, |ui| {
                                ui.label("종류");
                                number_choice(
                                    ui,
                                    "shadow-type",
                                    v,
                                    "shadowType",
                                    &[
                                        "없음",
                                        "왼쪽 위",
                                        "오른쪽 위",
                                        "왼쪽 아래",
                                        "오른쪽 아래",
                                        "왼쪽 뒤",
                                        "오른쪽 뒤",
                                        "왼쪽 앞",
                                        "오른쪽 앞",
                                    ],
                                );
                                ui.end_row();
                                color(ui, "색", v, "shadowColor");
                                ui.end_row();
                                int_field(ui, "가로 이동", v, "shadowOffsetX", (-100, 100), " %");
                                ui.end_row();
                                int_field(ui, "세로 이동", v, "shadowOffsetY", (-100, 100), " %");
                                ui.end_row();
                            });
                        }
                        _ => {
                            chrome::section(ui, "수식");
                            let mut script = v["script"].as_str().unwrap_or("").to_owned();
                            if ui
                                .add(
                                    egui::TextEdit::multiline(&mut script)
                                        .desired_rows(3)
                                        .desired_width(470.0),
                                )
                                .changed()
                            {
                                v["script"] = json!(script);
                            }
                            ui.horizontal(|ui| {
                                let mut pt = v["fontSize"].as_f64().unwrap_or(1000.0) / 100.0;
                                ui.label("글자 크기");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut pt)
                                            .range(4.0..=100.0)
                                            .suffix(" pt"),
                                    )
                                    .changed()
                                {
                                    v["fontSize"] = json!((pt * 100.0).round() as i64);
                                }
                                color(ui, "글자 색", v, "color");
                                int_field(ui, "기준선", v, "baseline", (-100, 100), " %");
                            });
                        }
                    }
                    if let Some(e) = &d.error {
                        ui.colored_label(Color32::DARK_RED, e);
                    }
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "설정(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply {
            let props = changed(&d.original, &d.draft);
            if props.is_empty() {
                close = true;
            } else {
                match self.session.run(
                    "object.properties",
                    json!({"address":d.address,"props":props}),
                ) {
                    Ok(_) => {
                        close = true;
                        self.ext.object.layout = None;
                        self.ext.object.selected = None;
                    }
                    Err(e) => d.error = Some(format!("{e:#}")),
                }
            }
        }
        if !(close || r.should_close()) {
            self.ext.object.properties = Some(d);
        }
    }
}

fn ctx_focus(ctx: &egui::Context) {
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
}

fn basic_tab(ui: &mut egui::Ui, v: &mut Value) {
    chrome::section(ui, "크기");
    ui.horizontal(|ui| {
        mm_field(ui, "너비", v, "width", (0.2, 600.0));
        mm_field(ui, "높이", v, "height", (0.2, 600.0));
        check(ui, "크기 고정", v, "sizeProtect");
    });
    chrome::section(ui, "위치");
    check(ui, "글자처럼 취급", v, "treatAsChar");
    ui.add_enabled_ui(v["treatAsChar"] != true, |ui| {
        ui.horizontal(|ui| {
            ui.label("본문과의 배치");
            for (k, l) in [
                ("Square", "어울림"),
                ("TopAndBottom", "자리 차지"),
                ("InFrontOfText", "글 앞으로"),
                ("BehindText", "글 뒤로"),
            ] {
                if ui.selectable_label(v["textWrap"] == k, l).clicked() {
                    v["textWrap"] = json!(k);
                }
            }
        });
        egui::Grid::new("object-position")
            .num_columns(5)
            .show(ui, |ui| {
                ui.label("가로");
                choice(
                    ui,
                    "obj-horz-rel",
                    v,
                    "horzRelTo",
                    &[
                        ("Paper", "종이"),
                        ("Page", "쪽"),
                        ("Column", "단"),
                        ("Para", "문단"),
                    ],
                );
                ui.label("의");
                choice(
                    ui,
                    "obj-horz-align",
                    v,
                    "horzAlign",
                    &[
                        ("Left", "왼쪽"),
                        ("Center", "가운데"),
                        ("Right", "오른쪽"),
                        ("Inside", "안쪽"),
                        ("Outside", "바깥쪽"),
                    ],
                );
                mm_field(ui, "기준", v, "horzOffset", (-600.0, 600.0));
                ui.end_row();
                ui.label("세로");
                choice(
                    ui,
                    "obj-vert-rel",
                    v,
                    "vertRelTo",
                    &[("Paper", "종이"), ("Page", "쪽"), ("Para", "문단")],
                );
                ui.label("의");
                choice(
                    ui,
                    "obj-vert-align",
                    v,
                    "vertAlign",
                    &[
                        ("Top", "위"),
                        ("Center", "가운데"),
                        ("Bottom", "아래"),
                        ("Inside", "안쪽"),
                        ("Outside", "바깥쪽"),
                    ],
                );
                mm_field(ui, "기준", v, "vertOffset", (-600.0, 600.0));
                ui.end_row();
            });
        check(ui, "쪽 영역 안으로 제한", v, "restrictInPage");
        check(ui, "서로 겹침 허용", v, "allowOverlap");
    });
    if v.get("rotationAngle").is_some() {
        chrome::section(ui, "개체 회전 / 대칭");
        ui.horizontal(|ui| {
            int_field(ui, "회전각", v, "rotationAngle", (-360, 360), "°");
            check(ui, "좌우 대칭", v, "horzFlip");
            check(ui, "상하 대칭", v, "vertFlip");
        });
    }
    chrome::section(ui, "개체 설명문");
    let mut text = v["description"].as_str().unwrap_or("").to_owned();
    if ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(470.0))
        .changed()
    {
        v["description"] = json!(text);
    }
}
fn caption_section(ui: &mut egui::Ui, v: &mut Value) {
    chrome::section(ui, "캡션");
    ui.horizontal(|ui| {
        let mut has = v["hasCaption"] == true;
        if ui.radio(!has, "캡션 없음").clicked() {
            has = false;
        }
        for (dir, l) in [
            ("Left", "왼쪽"),
            ("Right", "오른쪽"),
            ("Top", "위"),
            ("Bottom", "아래"),
        ] {
            if ui.radio(has && v["captionDirection"] == dir, l).clicked() {
                has = true;
                v["captionDirection"] = json!(dir);
            }
        }
        v["hasCaption"] = json!(has);
    });
    ui.add_enabled_ui(v["hasCaption"] == true, |ui| {
        ui.horizontal(|ui| {
            mm_field(ui, "크기", v, "captionWidth", (0.0, 600.0));
            mm_field(ui, "개체와의 간격", v, "captionSpacing", (0.0, 50.0));
        });
        check(ui, "여백 부분까지 너비 확대", v, "captionIncludeMargin");
    });
}
fn line_tab(ui: &mut egui::Ui, v: &mut Value, picture: bool) {
    chrome::section(ui, "선");
    egui::Grid::new("object-line")
        .num_columns(2)
        .show(ui, |ui| {
            color(ui, "색", v, "borderColor");
            ui.end_row();
            mm_field(ui, "굵기", v, "borderWidth", (0.0, 5.0));
            ui.end_row();
            if !picture {
                ui.label("종류");
                number_choice(ui, "line-type", v, "lineType", LINE_TYPES);
                ui.end_row();
                ui.label("끝 모양");
                number_choice(ui, "line-end", v, "lineEndShape", &["둥근", "평면", "사각"]);
                ui.end_row();
            }
        });
    if !picture && v.get("arrowStart").is_some() {
        chrome::section(ui, "화살표");
        egui::Grid::new("object-arrow")
            .num_columns(4)
            .show(ui, |ui| {
                ui.label("시작 모양");
                number_choice(ui, "arrow-start", v, "arrowStart", ARROWS);
                ui.label("크기");
                number_choice(
                    ui,
                    "arrow-start-size",
                    v,
                    "arrowStartSize",
                    &[
                        "작게", "작게2", "작게3", "보통", "보통2", "보통3", "크게", "크게2",
                        "크게3",
                    ],
                );
                ui.end_row();
                ui.label("끝 모양");
                number_choice(ui, "arrow-end", v, "arrowEnd", ARROWS);
                ui.label("크기");
                number_choice(
                    ui,
                    "arrow-end-size",
                    v,
                    "arrowEndSize",
                    &[
                        "작게", "작게2", "작게3", "보통", "보통2", "보통3", "크게", "크게2",
                        "크게3",
                    ],
                );
                ui.end_row();
            });
    }
    if v.get("roundRate").is_some() {
        chrome::section(ui, "사각형 모서리 곡률");
        ui.horizontal(|ui| int_field(ui, "곡률", v, "roundRate", (0, 50), " %"));
    }
}
fn fill_tab(ui: &mut egui::Ui, v: &mut Value) {
    chrome::section(ui, "채우기");
    ui.horizontal(|ui| {
        for (k, l) in [
            ("none", "색 채우기 없음"),
            ("solid", "색"),
            ("gradient", "그러데이션"),
        ] {
            if ui.radio(v["fillType"] == k, l).clicked() {
                v["fillType"] = json!(k);
                if k == "gradient" && v.get("gradientType").is_none() {
                    v["gradientType"] = json!(1);
                }
            }
        }
    });
    match v["fillType"].as_str() {
        Some("solid") => {
            egui::Grid::new("fill-solid").num_columns(2).show(ui, |ui| {
                color(ui, "면 색", v, "fillBgColor");
                ui.end_row();
                color(ui, "무늬 색", v, "fillPatColor");
                ui.end_row();
                ui.label("무늬 모양");
                let mut kind = v["fillPatType"].as_i64().unwrap_or(-1);
                egui::ComboBox::from_id_salt("fill-pattern")
                    .selected_text(match kind {
                        0 => "가로줄",
                        1 => "세로줄",
                        2 => "역사선",
                        3 => "사선",
                        4 => "십자",
                        5 => "격자",
                        _ => "없음",
                    })
                    .show_ui(ui, |ui| {
                        for (n, l) in [
                            (-1, "없음"),
                            (0, "가로줄"),
                            (1, "세로줄"),
                            (2, "역사선"),
                            (3, "사선"),
                            (4, "십자"),
                            (5, "격자"),
                        ] {
                            if ui.selectable_value(&mut kind, n, l).clicked() {
                                v["fillPatType"] = json!(n);
                            }
                        }
                    });
                ui.end_row();
            });
        }
        Some("gradient") => {
            egui::Grid::new("fill-gradient")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("유형");
                    let mut g = v["gradientType"].as_i64().unwrap_or(1);
                    egui::ComboBox::from_id_salt("gradient-type")
                        .selected_text(match g {
                            2 => "원형",
                            3 => "원뿔형",
                            4 => "사각형",
                            _ => "줄무늬",
                        })
                        .show_ui(ui, |ui| {
                            for (n, l) in [(1, "줄무늬"), (2, "원형"), (3, "원뿔형"), (4, "사각형")]
                            {
                                if ui.selectable_value(&mut g, n, l).clicked() {
                                    v["gradientType"] = json!(n);
                                }
                            }
                        });
                    ui.end_row();
                    int_field(ui, "기울임", v, "gradientAngle", (0, 359), "°");
                    ui.end_row();
                    int_field(ui, "가로 중심", v, "gradientCenterX", (0, 100), " %");
                    ui.end_row();
                    int_field(ui, "세로 중심", v, "gradientCenterY", (0, 100), " %");
                    ui.end_row();
                    int_field(ui, "번짐 정도", v, "gradientBlur", (0, 100), "");
                    ui.end_row();
                });
            ui.label(
                RichText::new("그러데이션 시작/끝 색은 기존 값을 유지합니다.")
                    .small()
                    .weak(),
            );
        }
        _ => {}
    }
    chrome::section(ui, "투명도");
    let mut alpha = v["fillAlpha"].as_i64().unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label("투명도");
        let mut percent = (alpha as f64 / 255.0 * 100.0).round() as i64;
        if ui
            .add(egui::Slider::new(&mut percent, 0..=100).suffix(" %"))
            .changed()
        {
            alpha = (percent as f64 / 100.0 * 255.0).round() as i64;
            v["fillAlpha"] = json!(alpha);
        }
    });
}
fn picture_tab(ui: &mut egui::Ui, v: &mut Value) {
    chrome::section(ui, "그림 효과");
    ui.horizontal(|ui| {
        for (k, l) in [
            ("RealPic", "실제 그림"),
            ("GrayScale", "회색조"),
            ("BlackWhite", "흑백"),
            ("Pattern8x8", "워터마크"),
        ] {
            if ui.radio(v["effect"] == k, l).clicked() {
                v["effect"] = json!(k);
            }
        }
    });
    egui::Grid::new("picture-adjust")
        .num_columns(2)
        .show(ui, |ui| {
            int_field(ui, "밝기", v, "brightness", (-100, 100), " %");
            ui.end_row();
            int_field(ui, "대비", v, "contrast", (-100, 100), " %");
            ui.end_row();
            int_field(ui, "투명도", v, "transparency", (0, 100), " %");
            ui.end_row();
        });
    chrome::section(ui, "자르기");
    egui::Grid::new("picture-crop")
        .num_columns(4)
        .show(ui, |ui| {
            mm_field(ui, "왼쪽", v, "cropLeft", (0.0, 600.0));
            mm_field(ui, "오른쪽", v, "cropRight", (0.0, 600.0));
            ui.end_row();
            mm_field(ui, "위쪽", v, "cropTop", (0.0, 600.0));
            mm_field(ui, "아래쪽", v, "cropBottom", (0.0, 600.0));
            ui.end_row();
        });
    ui.label(
        RichText::new("자르기 값은 원래 그림 기준이며 화면 크기는 개체 크기를 따릅니다.")
            .small()
            .weak(),
    );
}

/// 수식 편집기 draft. Arrangement follows Hangul 2024's equation editor: two
/// command rows (구조 / 기호), a preview pane, then the script pane.
#[derive(Clone)]
pub(crate) struct EquationEditor {
    address: Option<ObjectAddress>,
    script: String,
    size_pt: f64,
    color: [u8; 3],
    baseline: i64,
    font: String,
    original: Option<Value>,
    error: Option<String>,
}

/// (button label, tooltip, items: (menu label, script inserted at the caret)).
type Group = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);
const STRUCTURE: &[Group] = &[
    (
        "A¹",
        "첨자",
        &[
            ("위첨자  x^{}", "^{} "),
            ("아래첨자  x_{}", "_{} "),
            ("위·아래첨자", "_{} ^{} "),
            ("왼쪽 위첨자 LSUP", "LSUP{} "),
            ("왼쪽 아래첨자 LSUB", "LSUB{} "),
        ],
    ),
    (
        "ā",
        "장식 기호",
        &[
            ("벡터 vec", "vec{} "),
            ("모자 hat", "hat{} "),
            ("윗줄 bar", "bar{} "),
            ("점 dot", "dot{} "),
            ("두 점 ddot", "ddot{} "),
            ("물결 tilde", "tilde{} "),
            ("밑줄 under", "under{} "),
            ("호 arch", "arch{} "),
            ("양쪽 화살 dyad", "dyad{} "),
            ("acute", "acute{} "),
            ("grave", "grave{} "),
            ("check", "check{} "),
        ],
    ),
    (
        "▭/▭",
        "분수",
        &[("분수 over", "{} over {} "), ("atop", "{} atop {} ")],
    ),
    (
        "√",
        "근호",
        &[
            ("제곱근 sqrt", "sqrt{} "),
            ("거듭제곱근 root of", "root{} of {} "),
        ],
    ),
    (
        "Σ",
        "합",
        &[
            ("합 sum", "sum_{} ^{} "),
            ("곱 prod", "prod_{} ^{} "),
            ("쌍대곱 coprod", "coprod_{} ^{} "),
            ("합집합 bigcup", "bigcup_{} ^{} "),
            ("교집합 bigcap", "bigcap_{} ^{} "),
            ("bigoplus", "bigoplus_{} ^{} "),
            ("bigotimes", "bigotimes_{} ^{} "),
        ],
    ),
    (
        "∫",
        "적분",
        &[
            ("적분 int", "int_{} ^{} "),
            ("이중 적분 dint", "dint_{} ^{} "),
            ("삼중 적분 tint", "tint_{} ^{} "),
            ("선적분 oint", "oint_{} ^{} "),
            ("odint", "odint_{} ^{} "),
            ("otint", "otint_{} ^{} "),
        ],
    ),
    (
        "lim",
        "극한",
        &[("극한 lim", "lim_{x -> } "), ("Lim", "Lim_{} ")],
    ),
    (
        "rel",
        "상호 관계",
        &[("rel", "rel{->}{}{} "), ("buildrel", "buildrel{}{=} ")],
    ),
    (
        "( )",
        "괄호",
        &[
            ("( )", "left( {} right) "),
            ("[ ]", "left[ {} right] "),
            ("{ }", "left{ {} right} "),
            ("| |", "left| {} right| "),
            ("‖ ‖", "left‖ {} right‖ "),
            ("⟨ ⟩", "left langle {} right rangle "),
        ],
    ),
    ("{:", "경우", &[("경우 cases", "cases{ {} # {} } ")]),
    (
        "≡",
        "세로 쌓기",
        &[
            ("가운데 pile", "pile{ {} # {} } "),
            ("왼쪽 lpile", "lpile{ {} # {} } "),
            ("오른쪽 rpile", "rpile{ {} # {} } "),
        ],
    ),
    (
        "▦",
        "행렬",
        &[
            ("행렬 matrix", "matrix{ {} & {} # {} & {} } "),
            ("( ) pmatrix", "pmatrix{ {} & {} # {} & {} } "),
            ("[ ] bmatrix", "bmatrix{ {} & {} # {} & {} } "),
            ("| | dmatrix", "dmatrix{ {} & {} # {} & {} } "),
        ],
    ),
    (
        "&=",
        "줄 맞춤",
        &[
            ("줄 맞춤 eqalign", "eqalign{ {} &= {} # {} &= {} } "),
            ("줄 바꿈 #", " # "),
        ],
    ),
];
const SYMBOLS: &[Group] = &[
    (
        "Λ",
        "그리스 대문자",
        &[
            ("Α", "ALPHA "),
            ("Β", "BETA "),
            ("Γ", "GAMMA "),
            ("Δ", "DELTA "),
            ("Ε", "EPSILON "),
            ("Ζ", "ZETA "),
            ("Η", "ETA "),
            ("Θ", "THETA "),
            ("Ι", "IOTA "),
            ("Κ", "KAPPA "),
            ("Λ", "LAMBDA "),
            ("Μ", "MU "),
            ("Ν", "NU "),
            ("Ξ", "XI "),
            ("Ο", "OMICRON "),
            ("Π", "PI "),
            ("Ρ", "RHO "),
            ("Σ", "SIGMA "),
            ("Τ", "TAU "),
            ("Υ", "UPSILON "),
            ("Φ", "PHI "),
            ("Χ", "CHI "),
            ("Ψ", "PSI "),
            ("Ω", "OMEGA "),
        ],
    ),
    (
        "λ",
        "그리스 소문자",
        &[
            ("α", "alpha "),
            ("β", "beta "),
            ("γ", "gamma "),
            ("δ", "delta "),
            ("ε", "epsilon "),
            ("ζ", "zeta "),
            ("η", "eta "),
            ("θ", "theta "),
            ("ι", "iota "),
            ("κ", "kappa "),
            ("λ", "lambda "),
            ("μ", "mu "),
            ("ν", "nu "),
            ("ξ", "xi "),
            ("ο", "omicron "),
            ("π", "pi "),
            ("ρ", "rho "),
            ("σ", "sigma "),
            ("τ", "tau "),
            ("υ", "upsilon "),
            ("φ", "phi "),
            ("χ", "chi "),
            ("ψ", "psi "),
            ("ω", "omega "),
        ],
    ),
    (
        "ℵ",
        "그리스 기호",
        &[
            ("ℵ", "aleph "),
            ("ℏ", "hbar "),
            ("ı", "imath "),
            ("ȷ", "jmath "),
            ("ℓ", "ell "),
            ("℘", "wp "),
            ("ℑ", "IMAG "),
            ("ϑ", "vartheta "),
            ("ϖ", "varpi "),
            ("ς", "varsigma "),
            ("φ", "varphi "),
        ],
    ),
    (
        "∪",
        "합집합 기호",
        &[
            ("∪", "cup "),
            ("∩", "cap "),
            ("⊂", "subset "),
            ("⊃", "supset "),
            ("⊆", "subseteq "),
            ("⊇", "supseteq "),
            ("∈", "in "),
            ("∋", "owns "),
            ("∉", "notin "),
            ("∅", "emptyset "),
            ("⊕", "oplus "),
            ("⊗", "otimes "),
        ],
    ),
    (
        "±",
        "연산, 논리 기호",
        &[
            ("±", "+- "),
            ("∓", "-+ "),
            ("×", "times "),
            ("÷", "div "),
            ("·", "cdot "),
            ("∘", "circ "),
            ("≤", "<= "),
            ("≥", ">= "),
            ("≠", "!= "),
            ("≈", "approx "),
            ("≡", "equiv "),
            ("∼", "sim "),
            ("∝", "propto "),
            ("∀", "forall "),
            ("∃", "exist "),
            ("¬", "lnot "),
            ("∧", "wedge "),
            ("∨", "vee "),
            ("∴", "therefore "),
            ("∵", "because "),
        ],
    ),
    (
        "→",
        "화살표",
        &[
            ("→", "-> "),
            ("←", "larrow "),
            ("↔", "<-> "),
            ("⇒", "=> "),
            ("⇐", "LARROW "),
            ("⇔", "<=> "),
            ("↑", "uparrow "),
            ("↓", "downarrow "),
            ("↦", "mapsto "),
            ("↗", "nearrow "),
            ("↘", "searrow "),
        ],
    ),
    (
        "△",
        "기타 기호",
        &[
            ("∞", "inf "),
            ("∂", "partial "),
            ("∇", "nabla "),
            ("′", "prime "),
            ("°", "deg "),
            ("∠", "angle "),
            ("△", "triangle "),
            ("⊥", "bot "),
            ("…", "ldots "),
            ("⋯", "cdots "),
            ("⋮", "vdots "),
            ("⋱", "ddots "),
            ("℃", "CENTIGRADE "),
            ("Å", "ANGSTROM "),
        ],
    ),
];
const FONTS: &[&str] = &["HancomEQN", "HYhwpEQ", "함초롬바탕"];

impl HwpApp {
    /// Open the editor for a new equation or an existing one.
    pub(crate) fn open_equation_editor(&mut self, address: Option<ObjectAddress>) {
        let mut editor = EquationEditor {
            address,
            script: "{a+b} over {c+d}".into(),
            size_pt: 10.0,
            color: [0, 0, 0],
            baseline: 0,
            font: "HancomEQN".into(),
            original: None,
            error: None,
        };
        if let Some(a) = address {
            let r = self.session.object_properties(a);
            let Some(v) = self.result(r) else {
                return;
            };
            let p = v["props"].clone();
            editor.script = p["script"].as_str().unwrap_or("").to_owned();
            editor.size_pt = p["fontSize"].as_f64().unwrap_or(1000.0) / 100.0;
            editor.color = bgr(&p["color"]);
            editor.baseline = p["baseline"].as_i64().unwrap_or(0);
            if let Some(f) = p["fontName"].as_str().filter(|f| !f.is_empty()) {
                editor.font = f.to_owned();
            }
            editor.original = Some(p);
        }
        self.ext.object.equation = Some(Box::new(editor));
    }

    fn equation_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.equation.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let script_id = egui::Id::new("equation-script");
        let mut insert: Option<&'static str> = None;
        let r = chrome::modal("ext-equation-editor").show(ctx, |ui| {
            ui.set_width(760.0);
            close = chrome::title(
                ui,
                if d.address.is_some() {
                    "수식 편집기 — 고치기"
                } else {
                    "수식 편집기"
                },
            );
            for (row, groups) in [STRUCTURE, SYMBOLS].iter().enumerate() {
                ui.horizontal(|ui| {
                    for (label, tip, items) in groups.iter() {
                        ui.menu_button(RichText::new(*label).size(15.0), |ui| {
                            ui.set_min_width(170.0);
                            ui.label(RichText::new(*tip).strong());
                            ui.separator();
                            let grid = items.len() > 12;
                            if grid {
                                egui::Grid::new(("eq-grid", *tip)).show(ui, |ui| {
                                    for (i, (name, script)) in items.iter().enumerate() {
                                        if ui
                                            .add_sized([30.0, 26.0], egui::Button::new(*name))
                                            .on_hover_text(script.trim())
                                            .clicked()
                                        {
                                            insert = Some(script);
                                            ui.close();
                                        }
                                        if i % 6 == 5 {
                                            ui.end_row();
                                        }
                                    }
                                });
                            } else {
                                for (name, script) in items.iter() {
                                    if ui.button(*name).on_hover_text(script.trim()).clicked() {
                                        insert = Some(script);
                                        ui.close();
                                    }
                                }
                            }
                        })
                        .response
                        .on_hover_text(*tip);
                    }
                    if row == 1 {
                        ui.separator();
                        egui::ComboBox::from_id_salt("equation-font")
                            .selected_text(d.font.clone())
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for f in FONTS {
                                    ui.selectable_value(&mut d.font, (*f).to_owned(), *f);
                                }
                            })
                            .response
                            .on_hover_text("글꼴");
                        ui.add(
                            egui::DragValue::new(&mut d.size_pt)
                                .range(4.0..=100.0)
                                .speed(0.5)
                                .suffix(" pt"),
                        )
                        .on_hover_text("글자 크기");
                        egui::color_picker::color_edit_button_srgb(ui, &mut d.color)
                            .on_hover_text("글자 색");
                    }
                });
            }
            ui.separator();
            // Preview pane: RHWP equation renderer, re-rendered when the script changes.
            let key = format!("{}:{}", d.size_pt, d.script);
            // Shares HwpApp::equation_preview, the cache of the previous popup.
            if self.equation_preview.as_ref().is_none_or(|p| p.0 != key) {
                let options = EquationOptions {
                    script: d.script.clone(),
                    size_pt: d.size_pt,
                };
                match self
                    .session
                    .equation_preview(&options)
                    .and_then(|svg| self.renderer.svg(&svg, 2.0))
                {
                    Ok(p) => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [p.width as usize, p.height as usize],
                            &p.rgba,
                        );
                        let texture = ctx.load_texture(
                            "ext-equation-preview",
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.equation_preview =
                            Some((key, texture, Vec2::new(p.width as f32, p.height as f32)));
                        d.error = None;
                    }
                    Err(e) => {
                        self.equation_preview = None;
                        d.error = Some(format!("{e:#}"));
                    }
                }
            }
            egui::Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0, chrome::Tokens::BORDER))
                .show(ui, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(740.0, 190.0), egui::Sense::hover());
                    if let Some((_, texture, size)) = &self.equation_preview {
                        let scale = (rect.width() / size.x).min(rect.height() / size.y).min(1.0);
                        let dest = Rect::from_center_size(rect.center(), *size * scale);
                        ui.painter().image(
                            texture.id(),
                            dest,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("수식 스크립트");
                ui.label(
                    RichText::new("명령을 직접 입력하거나 위 도구를 누르면 커서 위치에 넣습니다.")
                        .small()
                        .weak(),
                );
            });
            let output = egui::TextEdit::multiline(&mut d.script)
                .id(script_id)
                .font(egui::TextStyle::Monospace)
                .desired_rows(4)
                .desired_width(740.0)
                .show(ui);
            if let Some(snippet) = insert {
                let at = output
                    .cursor_range
                    .map(|r| r.primary.index.0)
                    .or_else(|| {
                        egui::TextEdit::load_state(ctx, script_id)
                            .and_then(|s| s.cursor.char_range())
                            .map(|r| r.primary.index.0)
                    })
                    .unwrap_or_else(|| d.script.chars().count());
                let byte = d
                    .script
                    .char_indices()
                    .nth(at)
                    .map_or(d.script.len(), |(i, _)| i);
                d.script.insert_str(byte, snippet);
                if let Some(mut state) = egui::TextEdit::load_state(ctx, script_id) {
                    let caret = egui::text::CCursor::new(at + snippet.chars().count());
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(caret)));
                    state.store(ctx, script_id);
                }
                ctx.memory_mut(|m| m.request_focus(script_id));
            }
            ui.horizontal(|ui| {
                ui.label("기준선");
                ui.add(
                    egui::DragValue::new(&mut d.baseline)
                        .range(-100..=100)
                        .suffix(" %"),
                );
                if let Some(e) = &d.error {
                    ui.colored_label(Color32::DARK_RED, e);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= ui
                        .add(egui::Button::new("닫기").min_size(Vec2::new(80.0, 28.0)))
                        .clicked();
                    apply = ui
                        .add_enabled(
                            d.error.is_none() && !d.script.trim().is_empty(),
                            egui::Button::new(if d.address.is_some() {
                                "고치기(Shift+Esc)"
                            } else {
                                "넣기(Shift+Esc)"
                            })
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.5, chrome::Tokens::ACCENT))
                            .min_size(Vec2::new(120.0, 28.0)),
                        )
                        .on_disabled_hover_text(if d.script.trim().is_empty() {
                            "수식 스크립트를 입력하세요."
                        } else {
                            "수식 스크립트의 오류를 먼저 고치세요."
                        })
                        .clicked();
                });
            });
        });
        // Hangul: Shift+Esc inserts the equation and closes the editor.
        apply |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::Escape));
        if apply {
            let color = bgr_value(d.color);
            let result = match d.address {
                Some(a) => {
                    let draft = json!({"script":d.script,"fontSize":(d.size_pt * 100.0).round() as i64,
                        "color":color,"baseline":d.baseline,"fontName":d.font});
                    let props = d
                        .original
                        .as_ref()
                        .map(|o| changed(o, &draft))
                        .unwrap_or_default();
                    if props.is_empty() {
                        Ok(Value::Null)
                    } else {
                        self.session
                            .run("object.properties", json!({"address":a,"props":props}))
                    }
                }
                None => self.session.run(
                    "equation.insert",
                    json!({"script":d.script,"size_pt":d.size_pt,"color":color,
                        "baseline":d.baseline,"font_name":d.font}),
                ),
            };
            match result {
                Ok(_) => {
                    close = true;
                    self.ext.object.layout = None;
                }
                Err(e) => d.error = Some(format!("{e:#}")),
            }
        }
        if !(close || r.should_close()) {
            self.ext.object.equation = Some(d);
        }
    }
}

/// 차트 데이터 편집: values grid (rows = categories, columns = series).
#[derive(Clone)]
pub(crate) struct ChartEditor {
    index: usize,
    count: usize,
    labels: Vec<String>,
    names: Vec<String>,
    values: Vec<Vec<String>>,
    plot: String,
    configurable: bool,
    error: Option<String>,
}

impl HwpApp {
    pub(crate) fn open_chart_editor(&mut self, index: usize) {
        let list = self.session.run("chart.list", json!({}));
        let Some(list) = self.result(list) else {
            return;
        };
        let count = list.as_array().map_or(0, Vec::len);
        if count == 0 {
            self.open_chart_creation(None);
            return;
        }
        let index = index.min(count - 1);
        let data = self.session.run("chart.data.get", json!({"index":index}));
        let Some(data) = self.result(data) else {
            return;
        };
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .into_iter()
                .flatten()
                .map(|x| x.as_str().unwrap_or("").to_owned())
                .collect()
        };
        let series = data["series"].as_array().cloned().unwrap_or_default();
        let configurable = chart_creation::can_configure(&mut self.session, index);
        self.ext.object.chart = Some(Box::new(ChartEditor {
            index,
            count,
            labels: strings(&data["labels"]),
            names: series
                .iter()
                .map(|s| s["name"].as_str().unwrap_or("").to_owned())
                .collect(),
            values: series.iter().map(|s| strings(&s["values"])).collect(),
            plot: data["plot"].as_str().unwrap_or("").to_owned(),
            configurable,
            error: None,
        }));
    }

    fn chart_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.chart.take() else {
            return;
        };
        let (mut apply, mut close, mut switch) = (false, false, None);
        let (mut create, mut configure) = (false, false);
        let r = chrome::modal("ext-chart-data").show(ctx, |ui| {
            ui.set_width(640.0);
            close = chrome::title(ui, "차트 데이터 편집");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(530.0);
                    ui.horizontal(|ui| {
                        ui.label("차트");
                        egui::ComboBox::from_id_salt("chart-index")
                            .selected_text(format!("{} / {}", d.index + 1, d.count))
                            .show_ui(ui, |ui| {
                                for i in 0..d.count {
                                    if ui
                                        .selectable_label(i == d.index, format!("차트 {}", i + 1))
                                        .clicked()
                                    {
                                        switch = Some(i);
                                    }
                                }
                            });
                        ui.label(RichText::new(format!("종류: {}", d.plot)).weak());
                        create = ui.button("새 차트").clicked();
                        configure = ui.add_enabled(d.configurable, egui::Button::new("전체 구성"))
                            .on_hover_text("HwpCraft에서 작성한 11종의 지원 본문 차트만 전체 구성을 바꿉니다.")
                            .clicked();
                    });
                    ui.separator();
                    egui::ScrollArea::both().max_height(320.0).show(ui, |ui| {
                        egui::Grid::new("chart-grid").striped(true).show(ui, |ui| {
                            ui.label("");
                            for name in &mut d.names {
                                ui.add(egui::TextEdit::singleline(name).desired_width(90.0));
                            }
                            ui.end_row();
                            let rows = d.values.iter().map(Vec::len).max().unwrap_or(0);
                            for row in 0..rows {
                                ui.label(d.labels.get(row).cloned().unwrap_or_default());
                                for series in &mut d.values {
                                    if let Some(v) = series.get_mut(row) {
                                        ui.add(egui::TextEdit::singleline(v).desired_width(90.0));
                                    } else {
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    });
                    ui.label(
                        RichText::new(
                            "여기서는 값과 계열 이름을 고칩니다. 지원되는 자체 차트의 행·계열·종류는 전체 구성에서 바꿉니다.",
                        )
                        .small()
                        .weak(),
                    );
                    if let Some(e) = &d.error {
                        ui.colored_label(Color32::DARK_RED, e);
                    }
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "설정(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if create || configure {
            self.ext.object.chart = Some(d.clone());
            self.open_chart_creation(if configure { Some(d.index) } else { None });
            return;
        }
        if let Some(i) = switch {
            self.open_chart_editor(i);
            return;
        }
        if apply {
            match chart_creation::apply_legacy_data(&mut self.session, &d) {
                Ok(_) => {
                    close = true;
                    self.message = "차트 데이터를 고쳤습니다.".into();
                }
                Err(e) => d.error = Some(format!("{e:#}")),
            }
        }
        if !(close || r.should_close()) {
            self.ext.object.chart = Some(d);
        }
    }

    fn memo_list_dialog(&mut self, ctx: &egui::Context) {
        if !self.ext.object.memos {
            return;
        }
        let memos = self.session.memos();
        let mut close = false;
        let mut goto = None;
        let r = chrome::modal("ext-memo-list").show(ctx, |ui| {
            ui.set_width(520.0);
            close = chrome::title(ui, "메모 목록");
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    let list = memos.as_array().cloned().unwrap_or_default();
                    if list.is_empty() {
                        ui.label("이 문서에 메모가 없습니다.");
                    }
                    for (i, m) in list.iter().enumerate() {
                        ui.push_id(i, |ui| {
                            ui.group(|ui| {
                                ui.set_width(480.0);
                                ui.label(format!(
                                    "메모 {} · {}구역 {}문단{}",
                                    m["index"],
                                    m["at"]["section"].as_u64().unwrap_or(0) + 1,
                                    m["at"]["paragraph"].as_u64().unwrap_or(0) + 1,
                                    if m["at"]["cell"].is_object() {
                                        " (표 안)"
                                    } else {
                                        ""
                                    }
                                ));
                                let anchor = m["anchor"].as_str().unwrap_or("");
                                if !anchor.is_empty() {
                                    ui.label(RichText::new(format!("「{anchor}」")).weak());
                                }
                                let body: Vec<&str> = m["paragraphs"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .collect();
                                if body.iter().all(|t| t.trim().is_empty()) {
                                    ui.label(
                                        RichText::new(
                                            "(메모 내용은 원본에 보존되지만 표시할 수 없습니다)",
                                        )
                                        .small()
                                        .weak(),
                                    );
                                } else {
                                    ui.label(body.join("\n"));
                                }
                                if ui.button("위치로 이동").clicked() {
                                    goto = Some(m["at"].clone());
                                }
                            });
                        });
                    }
                });
            ui.label(
                RichText::new(
                    "새 메모 넣기와 메모 고치기는 아직 지원하지 않습니다(메모 모양 저장 미지원).",
                )
                .small()
                .weak(),
            );
            ui.add_space(6.0);
            if ui.button("닫기").clicked() {
                close = true;
            }
        });
        if let Some(at) = goto {
            let mut c = Cursor {
                section: at["section"].as_u64().unwrap_or(0) as usize,
                paragraph: at["paragraph"].as_u64().unwrap_or(0) as usize,
                offset: 0,
                cell: None,
            };
            if at["cell"].is_object() {
                c.cell = Some(hwpcraft_engine::Cell {
                    control: at["cell"]["control"].as_u64().unwrap_or(0) as usize,
                    index: at["cell"]["index"].as_u64().unwrap_or(0) as usize,
                    paragraph: at["cell"]["paragraph"].as_u64().unwrap_or(0) as usize,
                });
            }
            let r = self.session.select(c, false);
            if self.result(r).is_some() {
                self.follow_cursor();
                close = true;
            }
        }
        if close || r.should_close() {
            self.ext.object.memos = false;
        }
    }
}

/// Handle index 8 is the rotation knob above the top edge.
const ROTATE: usize = 8;
const ROTATE_GAP: f32 = 22.0;
/// Handle order: left-top, top, right-top, left, right, left-bottom, bottom, right-bottom.
fn handle_points(r: Rect) -> [Pos2; 8] {
    [
        r.left_top(),
        r.center_top(),
        r.right_top(),
        r.left_center(),
        r.right_center(),
        r.left_bottom(),
        r.center_bottom(),
        r.right_bottom(),
    ]
}
/// Handle under page point `p` (page pixels); tolerances follow the screen size.
fn handle_at(bbox: Rect, p: Pos2, zoom: f32) -> Option<usize> {
    let tol = 6.0 / zoom;
    let knob = bbox.center_top() - Vec2::new(0.0, ROTATE_GAP / zoom);
    if knob.distance(p) <= tol {
        return Some(ROTATE);
    }
    handle_points(bbox)
        .iter()
        .position(|h| (h.x - p.x).abs() <= tol && (h.y - p.y).abs() <= tol)
}
fn handle_cursor(h: usize) -> egui::CursorIcon {
    use egui::CursorIcon::*;
    match h {
        0 | 7 => ResizeNwSe,
        2 | 5 => ResizeNeSw,
        1 | 6 => ResizeVertical,
        3 | 4 => ResizeHorizontal,
        _ => Grab,
    }
}
/// New box for dragging handle `h` to `p`; Shift on a corner keeps the aspect ratio.
fn resized(b: Rect, h: usize, p: Pos2, keep_ratio: bool) -> Rect {
    let min = 1.0 / MM_PER_PX; // 1 mm
    let (mut l, mut t, mut r, mut bt) = (b.left(), b.top(), b.right(), b.bottom());
    match h {
        0 | 3 | 5 => l = p.x.min(r - min),
        2 | 4 | 7 => r = p.x.max(l + min),
        _ => {}
    }
    match h {
        0..=2 => t = p.y.min(bt - min),
        5..=7 => bt = p.y.max(t + min),
        _ => {}
    }
    if keep_ratio && matches!(h, 0 | 2 | 5 | 7) && b.height() > 0.0 && b.width() > 0.0 {
        let ratio = b.width() / b.height();
        let w = (r - l).max((bt - t) * ratio);
        let hgt = w / ratio;
        if matches!(h, 0 | 5) {
            l = r - w;
        } else {
            r = l + w;
        }
        if matches!(h, 0 | 2) {
            t = bt - hgt;
        } else {
            bt = t + hgt;
        }
    }
    Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, bt))
}

impl HwpApp {
    /// (has drawable objects, has charts) for the current document generation.
    fn doc_facts(&self) -> (bool, bool) {
        let generation = self.session.generation;
        if let Some((g, objects, charts)) = self.ext.object.doc_facts.get()
            && g == generation
        {
            return (objects, charts);
        }
        let objects = self.session.objects().as_array().is_some_and(|l| {
            l.iter().any(|o| {
                matches!(
                    o["kind"].as_str(),
                    Some("shape" | "picture" | "textbox" | "group" | "equation")
                )
            })
        });
        let charts = self
            .session
            .core
            .list_charts_native()
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.as_array().map(|a| !a.is_empty()))
            .unwrap_or(false);
        self.ext
            .object
            .doc_facts
            .set(Some((generation, objects, charts)));
        (objects, charts)
    }
    fn selected_inline(&self) -> bool {
        self.ext.object.selected.is_some_and(|s| s.inline)
    }
    fn selected_connector(&self) -> bool {
        use hwpcraft_doc::model::{control::Control, shape::ShapeObject};
        let Some(a) = self.ext.object.selected.map(|s| s.address) else {
            return false;
        };
        matches!(
            self.session
                .core
                .document()
                .sections
                .get(a.section)
                .and_then(|s| s.paragraphs.get(a.paragraph))
                .and_then(|p| p.controls.get(a.control)),
            Some(Control::Shape(sh)) if matches!(sh.as_ref(), ShapeObject::Line(l) if l.connector.is_some())
        )
    }
    /// Select an object by address (any page), like a click on it.
    fn select_address(&mut self, a: ObjectAddress) -> bool {
        let page = self.session.page_of_object(a).unwrap_or(0);
        match self.selected_from_address(page, a) {
            Some(sel) => {
                self.ext.object.multi.clear();
                self.ext.object.endpoint = None;
                self.ext.object.selected = Some(sel);
                self.session.clear_selection();
                true
            }
            None => {
                self.message =
                    "이 개체는 쪽에서 선택할 수 없습니다 (글자처럼 취급·표 안 개체).".into();
                false
            }
        }
    }
    /// ShapeObjectNextObject / PrevObject (Tab · Shift+Tab with an object selected).
    fn select_neighbor(&mut self, next: bool) {
        let from = self.ext.object.selected.map(|s| s.address);
        let mut p = json!({"direction": if next { "next" } else { "prev" }, "wrap": true});
        if let Some(a) = from {
            p["address"] = json!(a);
        }
        let r = self.session.run("object.neighbor", p);
        let Some(v) = self.result(r) else {
            return;
        };
        match serde_json::from_value::<ObjectAddress>(v["address"].clone()) {
            Ok(a) => {
                if self.select_address(a) {
                    self.message = format!(
                        "{} 개체를 선택했습니다 (Tab: 다음, Shift+Tab: 이전).",
                        match v["kind"].as_str() {
                            Some("picture") => "그림",
                            Some("textbox") => "글상자",
                            Some("group") => "묶음",
                            Some("equation") => "수식",
                            _ => "도형",
                        }
                    );
                }
            }
            Err(_) => self.message = "문서에 개체가 없습니다.".into(),
        }
    }
    fn selected_targets(&self) -> Vec<ObjectAddress> {
        let mut v: Vec<ObjectAddress> = self.ext.object.multi.iter().map(|m| m.address).collect();
        if let Some(s) = self.ext.object.selected {
            v.push(s.address);
        }
        v
    }
    fn refresh_selection(&mut self) {
        self.ext.object.layout = None;
        if let Some(s) = self.ext.object.selected {
            self.ext.object.selected = self.selected_from_address(s.page, s.address).or(Some(s));
        }
        let multi = std::mem::take(&mut self.ext.object.multi);
        self.ext.object.multi = multi
            .into_iter()
            .map(|m| self.selected_from_address(m.page, m.address).unwrap_or(m))
            .collect();
    }
    /// 0.14 table-object commands: alignment, quick styles, navigation, text boxes,
    /// pictures, numbers and notes. True when handled.
    fn object_ext_dispatch_14(&mut self, id: &str) -> bool {
        let selected = self.ext.object.selected.map(|s| s.address);
        if let Some(mode) = id.strip_prefix("object.align.") {
            let Some((_, label, _)) = ALIGNS.iter().find(|(m, _, _)| *m == mode) else {
                return false;
            };
            let targets = self.selected_targets();
            if targets.is_empty() {
                self.message = "맞출 개체를 먼저 클릭해 선택하세요 (여러 개는 Shift+클릭).".into();
                return true;
            }
            let r = self
                .session
                .run("object.align", json!({"targets":targets,"mode":mode}));
            if let Some(v) = self.result(r) {
                self.refresh_selection();
                self.message = format!("{label}: 개체 {}개를 옮겼습니다.", v["moved"]);
            }
            return true;
        }
        if let Some(rest) = id.strip_prefix("object.style.") {
            let Some(a) = selected else {
                return true;
            };
            let mut p = json!({"address":a});
            match rest.split_once('.') {
                Some((key @ ("fill" | "outline"), c)) => {
                    let value = if c == "none" {
                        "none".to_owned()
                    } else {
                        match QUICK_COLORS.iter().find(|(k, _)| *k == c) {
                            Some((_, hex)) => (*hex).to_owned(),
                            None => return false,
                        }
                    };
                    p[key] = json!(value);
                }
                Some(("dash", n)) => p["line_type"] = json!(n.parse::<i64>().unwrap_or(1)),
                Some(("pen", w)) => {
                    let Some((_, mm)) = PEN_WIDTHS.iter().find(|(k, _)| *k == w) else {
                        return false;
                    };
                    p["line_width_mm"] = json!(mm);
                }
                Some(("arrow_end", n)) => p["arrow_end"] = json!(n.parse::<i64>().unwrap_or(0)),
                Some(("arrow_start", n)) => p["arrow_start"] = json!(n.parse::<i64>().unwrap_or(0)),
                _ => return false,
            }
            if self.run("object.style", p) {
                self.ext.object.layout = None;
                self.message = "개체 스타일을 바꿨습니다.".into();
            }
            return true;
        }
        if let Some(mode) = id.strip_prefix("object.shadow.") {
            let Some(a) = selected else {
                return true;
            };
            let r = self
                .session
                .run("object.shadow", json!({"address":a,"mode":mode}));
            if let Some(v) = self.result(r) {
                self.ext.object.layout = None;
                self.message = if v["shadowType"] == 0 {
                    "그림자를 없앴습니다.".into()
                } else {
                    format!(
                        "그림자 이동: 가로 {} %, 세로 {} %",
                        v["shadowOffsetX"], v["shadowOffsetY"]
                    )
                };
            }
            return true;
        }
        if let Some(preset) = id.strip_prefix("object.outer_margin.") {
            if let Some(a) = selected
                && self.run("object.outer_margin", json!({"address":a,"preset":preset}))
            {
                self.ext.object.layout = None;
                self.message = "바깥 여백을 바꿨습니다.".into();
            }
            return true;
        }
        if let Some(preset) = id.strip_prefix("textbox.margin.") {
            if let Some(a) = selected
                && self.run("textbox.margin", json!({"address":a,"preset":preset}))
            {
                self.ext.object.layout = None;
                self.message = "글상자 안 여백을 바꿨습니다.".into();
            }
            return true;
        }
        if let Some(dir) = id.strip_prefix("textbox.direction.") {
            if let Some(a) = selected
                && self.run("textbox.direction", json!({"address":a,"direction":dir}))
            {
                self.ext.object.layout = None;
                self.message = "글상자 글자 방향을 바꿨습니다.".into();
            }
            return true;
        }
        if let Some(rest) = id.strip_prefix("textbox.align.") {
            let Some((h, v)) = rest.split_once('_') else {
                return false;
            };
            if let Some(a) = selected
                && self.run(
                    "textbox.align",
                    json!({"address":a,"horizontal":h,"vertical":v}),
                )
            {
                self.ext.object.layout = None;
                self.message = "글상자 정렬을 바꿨습니다.".into();
            }
            return true;
        }
        if let Some(kind) = id.strip_prefix("object.connector_type.") {
            if let Some(a) = selected
                && self.run("object.connector_type", json!({"address":a,"kind":kind}))
            {
                self.refresh_selection();
                self.message = "연결선 종류를 바꿨습니다.".into();
            }
            return true;
        }
        if let Some(mode) = id.strip_prefix("picture.recolor.") {
            if let Some(a) = selected
                && self.run("picture.recolor", json!({"address":a,"mode":mode}))
            {
                self.ext.object.layout = None;
                self.message = "그림 색을 조정했습니다.".into();
            }
            return true;
        }
        if let Some(kind) = id.strip_prefix("insert.auto_number.") {
            if self.run("insert.auto_number", json!({"kind":kind})) {
                self.message = match kind {
                    "page" => "쪽 번호를 넣었습니다.",
                    "total" => "전체 쪽수를 넣었습니다.",
                    "picture" => "그림 번호를 넣었습니다.",
                    "table" => "표 번호를 넣었습니다.",
                    _ => "수식 번호를 넣었습니다.",
                }
                .into();
            }
            return true;
        }
        match id {
            "object.protect_size" => {
                if let Some(a) = selected {
                    let r = self
                        .session
                        .run("object.protect_size", json!({"address":a}));
                    if let Some(v) = self.result(r) {
                        self.message = if v["protected"] == true {
                            "크기를 고정했습니다 (손잡이로 크기를 바꿀 수 없습니다).".into()
                        } else {
                            "크기 고정을 풀었습니다.".into()
                        };
                    }
                }
            }
            "object.format.copy" => {
                if let Some(a) = selected {
                    let r = self.session.run("object.format.get", json!({"address":a}));
                    if let Some(v) = self.result(r) {
                        self.ext.object.format = Some(v["format"].clone());
                        self.message = "개체 모양을 복사했습니다. 다른 개체를 선택하고 모양 붙이기를 누르세요.".into();
                    }
                }
            }
            "object.format.paste" => {
                let Some(f) = self.ext.object.format.clone() else {
                    self.message = "먼저 개체 모양 복사를 하세요.".into();
                    return true;
                };
                let mut n = 0;
                for a in self.selected_targets() {
                    if self.run("object.format", json!({"address":a,"format":f})) {
                        n += 1;
                    } else {
                        break;
                    }
                }
                if n > 0 {
                    self.ext.object.layout = None;
                    self.message = format!("개체 {n}개에 모양을 붙였습니다.");
                }
            }
            "object.next" => self.select_neighbor(true),
            "object.prev" => self.select_neighbor(false),
            "object.select" => {
                // 개체 선택 단추: the object at the caret, else the first one after it.
                if self.ext.object.selected.is_some() {
                    self.select_neighbor(true);
                } else if let Some(a) = self.object_near_caret() {
                    if self.select_address(a) {
                        self.message = "개체를 선택했습니다 (Tab: 다음 개체, Esc: 해제).".into();
                    }
                } else {
                    self.select_neighbor(true);
                }
            }
            "insert.shape.vtextbox" | "insert.shape.vtextbox_all" => {
                self.ext.object.draw = Some("textbox");
                self.ext.object.draw_direction = Some(if id.ends_with("_all") {
                    "vertical_all"
                } else {
                    "vertical"
                });
                self.clear_object_selection();
                self.message =
                    "세로 글상자: 쪽에서 끌어 그리세요. 클릭만 하면 기본 크기, Esc 취소.".into();
            }
            "picture.save" => {
                let Some(a) = selected else {
                    return true;
                };
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("그림.png")
                    .add_filter("그림 파일", &["png", "jpg", "jpeg", "bmp", "gif"])
                    .save_file()
                {
                    let r = self.session.run(
                        "picture.save",
                        json!({"address":a,"path":path,"overwrite":true}),
                    );
                    if let Some(v) = self.result(r) {
                        self.message =
                            format!("그림을 저장했습니다: {}", v["path"].as_str().unwrap_or(""));
                    }
                }
            }
            "picture.save_all" => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    let r = self.session.run("picture.save_all", json!({"dir":dir}));
                    if let Some(v) = self.result(r) {
                        self.message = format!(
                            "그림 {}개를 저장했습니다.",
                            v["files"].as_array().map_or(0, Vec::len)
                        );
                    }
                }
            }
            "note.convert" => {
                if let Some(a) = self.session.note_at_caret() {
                    let endnote =
                        self.session
                            .objects()
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|o| {
                                o["kind"] == "endnote"
                                    && serde_json::from_value::<ObjectAddress>(o["address"].clone())
                                        .ok()
                                        == Some(a)
                            });
                    if self.run("note.convert", json!({"address":a})) {
                        self.message = if endnote {
                            "미주를 각주로 바꿨습니다.".into()
                        } else {
                            "각주를 미주로 바꿨습니다.".into()
                        };
                    }
                } else {
                    self.message = "커서 문단에 각주·미주가 없습니다.".into();
                }
            }
            "note.delete" => {
                if self.run("note.delete", json!({})) {
                    self.message = "각주·미주를 지웠습니다.".into();
                }
            }
            "note.shape" | "note.shape.footnote" => self.open_note_shape(false),
            "note.shape.endnote" => self.open_note_shape(true),
            "field.delete" => {
                let r = self.session.run("field.delete", json!({}));
                if let Some(v) = self.result(r) {
                    self.message = if v["removed"] == true {
                        "누름틀을 지웠습니다.".into()
                    } else {
                        "커서 위치에 누름틀이 없습니다.".into()
                    };
                }
            }
            _ => return false,
        }
        true
    }

    fn open_note_shape(&mut self, endnote: bool) {
        let kind = if endnote { "endnote" } else { "footnote" };
        let r = self.session.run("note.shape.get", json!({"kind":kind}));
        if let Some(v) = self.result(r) {
            self.ext.object.note_shape = Some(Box::new(NoteShapeDialog {
                endnote,
                original: v.clone(),
                draft: v,
            }));
        }
    }
    /// 각주/미주 모양: Hangul's 주석 모양 dialog (번호 모양·구분선).
    fn note_shape_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.note_shape.take() else {
            return;
        };
        let (mut apply, mut close, mut switch) = (false, false, None);
        let r = chrome::modal("ext-note-shape").show(ctx, |ui| {
            ui.set_width(520.0);
            close = chrome::title(ui, if d.endnote { "미주 모양" } else { "각주 모양" });
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(400.0);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!d.endnote, "각주").clicked() && d.endnote {
                            switch = Some(false);
                        }
                        if ui.selectable_label(d.endnote, "미주").clicked() && !d.endnote {
                            switch = Some(true);
                        }
                    });
                    chrome::section(ui, "번호 모양");
                    egui::Grid::new("note-number").num_columns(4).show(ui, |ui| {
                        ui.label("번호 모양");
                        choice(ui, "note-format", &mut d.draft, "numberFormat", NOTE_NUMBER_FORMATS);
                        int_field(ui, "시작 번호", &mut d.draft, "startNumber", (1, 9999), "");
                        ui.end_row();
                        text_field(ui, "앞 장식 문자", &mut d.draft, "prefixChar", 1);
                        text_field(ui, "뒤 장식 문자", &mut d.draft, "suffixChar", 1);
                        ui.end_row();
                        if d.draft["numberFormat"] == "userChar" {
                            text_field(ui, "사용자 기호", &mut d.draft, "userChar", 1);
                            ui.end_row();
                        }
                    });
                    check(ui, "번호 위 첨자", &mut d.draft, "numberCodeSuperscript");
                    ui.horizontal(|ui| {
                        ui.label("번호 매기기");
                        for (k, l) in [("continue", "앞 구역에 이어서"), ("section", "구역마다 새로"), ("page", "쪽마다 새로")] {
                            if ui.radio(d.draft["numbering"] == k, l).clicked() {
                                d.draft["numbering"] = json!(k);
                            }
                        }
                    });
                    chrome::section(ui, "구분선");
                    check(ui, "구분선 넣기", &mut d.draft, "separatorEnabled");
                    if d.draft["separatorEnabled"] == true {
                        egui::Grid::new("note-separator").num_columns(4).show(ui, |ui| {
                            ui.label("길이");
                            let mut len = d.draft["separatorLength"].as_i64().unwrap_or(0);
                            let mut column_width = len < 0;
                            if ui.checkbox(&mut column_width, "단 너비").changed() {
                                len = if column_width { -1 } else { (50.0 * HU_PER_MM) as i64 };
                                d.draft["separatorLength"] = json!(len);
                            }
                            if !column_width {
                                let mut mm = len as f64 / HU_PER_MM;
                                if ui
                                    .add(egui::DragValue::new(&mut mm).range(0.0..=600.0).speed(0.5).fixed_decimals(1).suffix(" mm"))
                                    .changed()
                                {
                                    d.draft["separatorLength"] = json!((mm * HU_PER_MM).round() as i64);
                                }
                            } else {
                                ui.label("");
                            }
                            ui.end_row();
                            ui.label("종류");
                            number_choice(ui, "note-line-type", &mut d.draft, "separatorLineType", LINE_TYPES);
                            ui.label("굵기");
                            number_choice(ui, "note-line-width", &mut d.draft, "separatorLineWidth", SEPARATOR_WIDTHS);
                            ui.end_row();
                            css_field(ui, "색", &mut d.draft, "separatorColor");
                            ui.end_row();
                        });
                    }
                    ui.label(
                        RichText::new(if d.endnote {
                            "현재 구역의 미주 번호와 구분선에 적용됩니다."
                        } else {
                            "현재 구역의 각주 번호와 구분선에 적용됩니다. 구분선 길이 -1(단 너비)은 화면에 짧게 표시될 수 있습니다."
                        })
                        .small()
                        .weak(),
                    );
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "설정(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if let Some(endnote) = switch {
            self.open_note_shape(endnote);
            return;
        }
        if apply {
            let props = changed(&d.original, &d.draft);
            close = if props.is_empty() {
                true
            } else {
                let kind = if d.endnote { "endnote" } else { "footnote" };
                let ok = self.run(
                    "note.shape",
                    json!({"kind":kind,"props":Value::Object(props)}),
                );
                if ok {
                    self.message = if d.endnote {
                        "미주 모양을 바꿨습니다."
                    } else {
                        "각주 모양을 바꿨습니다."
                    }
                    .into();
                }
                ok
            };
        }
        if !(close || r.should_close()) {
            self.ext.object.note_shape = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_text_box_tools_are_enabled_in_the_body_and_start_drawing() {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let ctx = egui::Context::default();
        for (id, direction) in [
            ("insert.shape.vtextbox", "vertical"),
            ("insert.shape.vtextbox_all", "vertical_all"),
        ] {
            assert_eq!(a.object_ext_enabled(id), Some(true), "{id}");
            assert!(a.object_ext_dispatch(id, &ctx), "{id}");
            assert_eq!(a.ext.object.draw, Some("textbox"), "{id}");
            assert_eq!(a.ext.object.draw_direction, Some(direction), "{id}");
        }
        // Inside a table cell the body-only tools are disabled.
        a.session
            .run("table.create", json!({"rows":1,"columns":1}))
            .unwrap();
        assert_eq!(a.object_ext_enabled("insert.shape.vtextbox"), Some(false));
        // Unknown shape kinds still belong to no module.
        assert_eq!(a.object_ext_enabled("insert.shape.zigzag"), None);
    }

    #[test]
    fn insert_chart_opens_the_new_chart_dialog_at_a_body_caret() {
        let ctx = egui::Context::default();
        let mut a = HwpApp::with_context(ctx.clone(), None, None, false).unwrap();
        a.session
            .execute(hwpcraft_engine::Command::Insert("본문".into()))
            .unwrap();
        assert_eq!(a.object_ext_enabled("insert.chart"), Some(true));
        assert!(a.object_ext_dispatch("insert.chart", &ctx));
        assert!(a.ext.object.chart_authoring.is_some(), "{}", a.message);
        a.ext.object.chart_authoring = None;
        // A text selection or a table cell disables it.
        let at = |offset| Cursor {
            offset,
            ..Default::default()
        };
        a.session.select(at(0), false).unwrap();
        a.session.select(at(2), true).unwrap();
        assert_eq!(a.object_ext_enabled("insert.chart"), Some(false));
        a.session.select(at(2), false).unwrap();
        a.session
            .run("table.create", json!({"rows":1,"columns":1}))
            .unwrap();
        assert_eq!(a.object_ext_enabled("insert.chart"), Some(false));
    }

    #[test]
    fn object_ribbon_checks_scan_the_document_once_per_generation() {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        assert_eq!(a.object_ext_enabled("object.next"), Some(false));
        assert_eq!(a.object_ext_enabled("chart.edit"), Some(true));
        let cached = a.ext.object.doc_facts.get();
        assert_eq!(cached, Some((a.session.generation, false, false)));
        // Repeated frame checks reuse the cached scan.
        a.object_ext_enabled("object.select");
        assert_eq!(a.ext.object.doc_facts.get(), cached);
        // An edit bumps the generation and the next check rescans.
        a.session
            .run(
                "shape.insert",
                json!({"kind":"rectangle","width_mm":20.0,"height_mm":10.0}),
            )
            .unwrap();
        assert_eq!(a.object_ext_enabled("object.next"), Some(true));
        assert_eq!(
            a.ext.object.doc_facts.get(),
            Some((a.session.generation, true, false))
        );
        // Undo restores an object-free document; the cache follows.
        a.session.execute(hwpcraft_engine::Command::Undo).unwrap();
        assert_eq!(a.object_ext_enabled("picture.save_all"), Some(false));
    }

    #[test]
    fn handles_resize_from_the_dragged_side_and_keep_ratio_with_shift() {
        let b = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 150.0));
        // Right edge only changes the width.
        let r = resized(b, 4, Pos2::new(260.0, 999.0), false);
        assert_eq!(
            (r.left(), r.top(), r.right(), r.bottom()),
            (100.0, 100.0, 260.0, 150.0)
        );
        // Top-left corner moves the origin (offset) and keeps the opposite corner.
        let r = resized(b, 0, Pos2::new(80.0, 90.0), false);
        assert_eq!(
            (r.left(), r.top(), r.right(), r.bottom()),
            (80.0, 90.0, 200.0, 150.0)
        );
        // Shift on a corner keeps 2:1.
        let r = resized(b, 7, Pos2::new(300.0, 160.0), true);
        assert!((r.width() / r.height() - 2.0).abs() < 1e-3, "{r:?}");
        assert_eq!(r.min, b.min);
        // Crossing the opposite side stops at the 1 mm minimum.
        let r = resized(b, 4, Pos2::new(0.0, 0.0), false);
        assert!((r.width() - 1.0 / MM_PER_PX).abs() < 1e-3);
    }

    #[test]
    fn handle_hit_testing_includes_the_rotation_knob() {
        let b = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 150.0));
        assert_eq!(handle_at(b, Pos2::new(101.0, 99.0), 1.0), Some(0));
        assert_eq!(handle_at(b, Pos2::new(200.0, 125.0), 1.0), Some(4));
        assert_eq!(
            handle_at(b, Pos2::new(150.0, 100.0 - ROTATE_GAP), 1.0),
            Some(ROTATE)
        );
        assert_eq!(handle_at(b, Pos2::new(150.0, 125.0), 1.0), None);
        // At 200 % zoom the screen tolerance halves in page pixels.
        assert_eq!(handle_at(b, Pos2::new(104.5, 104.5), 2.0), None);
    }
}
