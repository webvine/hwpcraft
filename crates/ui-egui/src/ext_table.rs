//! Owner: table/object area. 표 대화상자·표 도구.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Dialog arrangement follows Hangul 2024: 표 만들기, 표/셀 속성(기본·여백/캡션·
//! 테두리·배경·표·셀), 셀 테두리/배경(테두리·배경·대각선), 셀 나누기, 계산식.
//! Drafts are JSON in RHWP units; only changed keys are sent (DesignCraft draft pattern).
use super::*;
use hwpcraft_engine::ObjectAddress;
use serde_json::{Map, Value};

const HU_PER_MM: f64 = 7200.0 / 25.4;
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
    "얇고 굵고 얇은 삼중선",
    "물결",
    "이중 물결",
    "두꺼운 3D",
    "두꺼운 3D(역)",
    "3D 단선",
    "3D 단선(역)",
];
const LINE_WIDTHS: &[f32] = &[
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Block {
    table: ObjectAddress,
    anchor: (u16, u16),
    active: (u16, u16),
    /// F5 once: block; twice: arrows extend; three times: whole table.
    stage: u8,
}
impl Block {
    fn range(&self) -> Value {
        json!({"start_row":self.anchor.0.min(self.active.0),"start_col":self.anchor.1.min(self.active.1),
            "end_row":self.anchor.0.max(self.active.0),"end_col":self.anchor.1.max(self.active.1)})
    }
}

#[derive(Clone)]
struct Create {
    rows: u16,
    columns: u16,
    custom_width: bool,
    width_mm: f64,
    custom_height: bool,
    height_mm: f64,
    treat_as_char: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    InnerVertical,
    InnerHorizontal,
}
const EDGES: [(Edge, &str, &str); 6] = [
    (Edge::Left, "left", "왼쪽"),
    (Edge::Right, "right", "오른쪽"),
    (Edge::Top, "top", "위쪽"),
    (Edge::Bottom, "bottom", "아래쪽"),
    (Edge::InnerVertical, "inner_vertical", "안쪽 세로"),
    (Edge::InnerHorizontal, "inner_horizontal", "안쪽 가로"),
];
#[derive(Clone)]
struct Borders {
    line_type: usize,
    width: usize,
    color: [u8; 3],
    /// Edges the user applied; each keeps the line chosen when it was clicked.
    applied: Vec<(Edge, Value)>,
}
impl Default for Borders {
    fn default() -> Self {
        Self {
            line_type: 1,
            width: 1,
            color: [0, 0, 0],
            applied: Vec::new(),
        }
    }
}
#[derive(Clone)]
struct Properties {
    /// 0: 표/셀 속성, 1: 셀 테두리/배경.
    cell_only: bool,
    tab: usize,
    table: ObjectAddress,
    range: Option<Value>,
    table_original: Value,
    table_draft: Value,
    cell_original: Value,
    cell_draft: Value,
    borders: Borders,
    error: Option<String>,
}
#[derive(Clone)]
struct Split {
    by_rows: bool,
    rows: u16,
    by_columns: bool,
    columns: u16,
    equal_height: bool,
    merge_first: bool,
    multi: bool,
}

#[derive(Clone, Copy)]
struct GridLine {
    table: ObjectAddress,
    vertical: bool,
    boundary: u16,
    start: Pos2,
    /// Extent of the line across the table (page px): (from, to).
    span: (f32, f32),
}

/// Dialog and tool state owned by this module. Stored in `HwpApp::ext.table`.
#[derive(Default)]
pub(crate) struct State {
    block: Option<Block>,
    drag: Option<(ObjectAddress, (u16, u16))>,
    /// Grid line being dragged: table, vertical, boundary, start (page px), span.
    line: Option<GridLine>,
    tool: Option<TableTool>,
    /// Page control layouts per page for one document generation. Every visible
    /// page runs the canvas hooks each frame; a single-page cache made two visible
    /// pages evict each other and rebuild the layout JSON every frame.
    layout: Option<PageLayouts>,
    create: Option<Create>,
    properties: Option<Box<Properties>>,
    split: Option<Split>,
    formula: Option<(String, Option<String>)>,
}

/// (document generation, page → shared page control layout).
pub(crate) type PageLayouts = (u64, std::collections::HashMap<u32, std::sync::Arc<Value>>);
/// Cached layout of `page` in `cache`, rebuilt only after the document changes.
pub(crate) fn cached_page_layout(
    cache: &mut Option<PageLayouts>,
    session: &hwpcraft_engine::Session,
    page: u32,
) -> std::sync::Arc<Value> {
    let generation = session.generation;
    if cache.as_ref().is_none_or(|(g, _)| *g != generation) {
        *cache = Some((generation, Default::default()));
    }
    let Some((_, pages)) = cache.as_mut() else {
        return std::sync::Arc::new(Value::Null);
    };
    pages
        .entry(page)
        .or_insert_with(|| {
            std::sync::Arc::new(
                session
                    .page_control_layout(page)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Null),
            )
        })
        .clone()
}
fn color_css(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn css_color(v: &Value) -> [u8; 3] {
    let s = v.as_str().unwrap_or("#000000");
    let byte = |i: usize| {
        s.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .unwrap_or(0)
    };
    [byte(1), byte(3), byte(5)]
}
fn mm_field(ui: &mut egui::Ui, label: &str, v: &mut Value, key: &str, max_mm: f64) {
    let mut mm = v[key].as_f64().unwrap_or(0.0) / HU_PER_MM;
    ui.label(label);
    if ui
        .add(
            egui::DragValue::new(&mut mm)
                .range(0.0..=max_mm)
                .speed(0.1)
                .fixed_decimals(2)
                .suffix(" mm"),
        )
        .changed()
    {
        v[key] = json!((mm * HU_PER_MM).round() as i64);
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
        .width(110.0)
        .show_ui(ui, |ui| {
            for (i, l) in items.iter().enumerate() {
                if ui.selectable_label(current == i, *l).clicked() {
                    v[key] = json!(i);
                }
            }
        });
}
/// Keys whose values differ from the original.
fn changed(original: &Value, draft: &Value) -> Map<String, Value> {
    draft
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, v)| original.get(k.as_str()) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
fn line_picker(ui: &mut egui::Ui, id: &str, b: &mut Borders) {
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        ui.label("종류");
        egui::ComboBox::from_id_salt((id, "type"))
            .selected_text(LINE_TYPES[b.line_type])
            .width(150.0)
            .show_ui(ui, |ui| {
                for (i, l) in LINE_TYPES.iter().enumerate() {
                    ui.selectable_value(&mut b.line_type, i, *l);
                }
            });
        ui.end_row();
        ui.label("굵기");
        egui::ComboBox::from_id_salt((id, "width"))
            .selected_text(format!("{} mm", LINE_WIDTHS[b.width]))
            .width(150.0)
            .show_ui(ui, |ui| {
                for (i, w) in LINE_WIDTHS.iter().enumerate() {
                    ui.selectable_value(&mut b.width, i, format!("{w} mm"));
                }
            });
        ui.end_row();
        ui.label("색");
        egui::color_picker::color_edit_button_srgb(ui, &mut b.color);
        ui.end_row();
    });
}
fn line_value(b: &Borders) -> Value {
    json!({"type":b.line_type,"width":b.width,"color":color_css(b.color)})
}
/// Hangul's border tab: line picker, edge buttons and a preview of the block.
fn border_tab(ui: &mut egui::Ui, b: &mut Borders, multi: bool) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(290.0);
            chrome::section(ui, "선 모양");
            line_picker(ui, "table-border-line", b);
            chrome::section(ui, "적용");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(230.0);
                let apply = |b: &mut Borders, edges: &[Edge], line: Value| {
                    for e in edges {
                        b.applied.retain(|(x, _)| x != e);
                        b.applied.push((*e, line.clone()));
                    }
                };
                let line = line_value(b);
                let none = json!({"type":0,"width":0,"color":"#000000"});
                let outer = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];
                let inner = [Edge::InnerVertical, Edge::InnerHorizontal];
                if ui.button("바깥쪽").clicked() {
                    apply(b, &outer, line.clone());
                }
                if ui
                    .add_enabled(multi, egui::Button::new("안쪽"))
                    .on_disabled_hover_text(INNER_EDGE_REASON)
                    .clicked()
                {
                    apply(b, &inner, line.clone());
                }
                if ui.button("모두").clicked() {
                    apply(b, &outer, line.clone());
                    apply(b, &inner, line.clone());
                }
                if ui.button("없음").clicked() {
                    apply(b, &outer, none.clone());
                    apply(b, &inner, none);
                }
                for (edge, _, label) in EDGES {
                    let inner_edge = matches!(edge, Edge::InnerVertical | Edge::InnerHorizontal);
                    if ui
                        .add_enabled(!inner_edge || multi, egui::Button::new(label))
                        .on_disabled_hover_text(INNER_EDGE_REASON)
                        .clicked()
                    {
                        apply(b, &[edge], line.clone());
                    }
                }
            });
            if ui.small_button("적용 취소").clicked() {
                b.applied.clear();
            }
        });
        ui.add_space(12.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(170.0, 150.0), egui::Sense::hover());
        let r = rect.shrink(18.0);
        ui.painter().rect_filled(r, 0.0, Color32::WHITE);
        let guide = Stroke::new(1.0, Color32::from_gray(215));
        ui.painter()
            .rect_stroke(r, 0.0, guide, egui::StrokeKind::Middle);
        ui.painter()
            .line_segment([r.center_top(), r.center_bottom()], guide);
        ui.painter()
            .line_segment([r.left_center(), r.right_center()], guide);
        for (edge, line) in &b.applied {
            let width = line["width"].as_u64().unwrap_or(0) as usize;
            if line["type"] == 0 {
                continue;
            }
            let c = css_color(&line["color"]);
            let stroke = Stroke::new(
                (LINE_WIDTHS[width.min(15)] * 3.0).clamp(1.0, 6.0),
                Color32::from_rgb(c[0], c[1], c[2]),
            );
            let seg = match edge {
                Edge::Left => [r.left_top(), r.left_bottom()],
                Edge::Right => [r.right_top(), r.right_bottom()],
                Edge::Top => [r.left_top(), r.right_top()],
                Edge::Bottom => [r.left_bottom(), r.right_bottom()],
                Edge::InnerVertical => [r.center_top(), r.center_bottom()],
                Edge::InnerHorizontal => [r.left_center(), r.right_center()],
            };
            ui.painter().line_segment(seg, stroke);
        }
    });
}
fn fill_tab(ui: &mut egui::Ui, cell: &mut Value) {
    chrome::section(ui, "채우기");
    let mut solid = cell["fillType"] == "solid";
    ui.horizontal(|ui| {
        if ui.radio(!solid, "색 채우기 없음").clicked() {
            solid = false;
            cell["fillType"] = json!("none");
        }
        if ui.radio(solid, "색").clicked() {
            solid = true;
            cell["fillType"] = json!("solid");
        }
    });
    ui.add_enabled_ui(solid, |ui| {
        egui::Grid::new("cell-fill").num_columns(2).show(ui, |ui| {
            for (label, key) in [("면 색", "fillColor"), ("무늬 색", "patternColor")] {
                ui.label(label);
                let mut c = css_color(&cell[key]);
                if egui::color_picker::color_edit_button_srgb(ui, &mut c).changed() {
                    cell[key] = json!(color_css(c));
                }
                ui.end_row();
            }
            ui.label("무늬 모양");
            number_choice(
                ui,
                "cell-pattern",
                cell,
                "patternType",
                &["없음", "가로줄", "세로줄", "역사선", "사선", "십자", "격자"],
            );
            ui.end_row();
        });
    });
}

impl HwpApp {
    fn table_address(&self) -> Option<ObjectAddress> {
        let c = self.session.cursor;
        c.cell.map(|t| ObjectAddress {
            section: c.section,
            paragraph: c.paragraph,
            control: t.control,
        })
    }
    /// Block range for commands; `None` lets the engine use the caret cell.
    pub(super) fn block_range(&self) -> Option<Value> {
        self.ext
            .table
            .block
            .filter(|b| Some(b.table) == self.table_address())
            .map(|b| b.range())
    }
    fn cell_position(&self, c: Cursor) -> Option<(ObjectAddress, (u16, u16))> {
        let t = c.cell?;
        let a = ObjectAddress {
            section: c.section,
            paragraph: c.paragraph,
            control: t.control,
        };
        let p = self
            .session
            .core
            .document()
            .sections
            .get(c.section)?
            .paragraphs
            .get(c.paragraph)?;
        match p.controls.get(t.control) {
            Some(hwpcraft_doc::model::control::Control::Table(table)) => {
                let cell = table.cells.get(t.index)?;
                Some((a, (cell.row, cell.col)))
            }
            _ => None,
        }
    }
    fn run_table(&mut self, id: &str, mut params: Value) -> bool {
        if let Some(r) = self.block_range()
            && params.get("range").is_none()
            && params.is_object()
        {
            params["range"] = r;
        }
        let ok = self.run(id, params);
        if ok {
            self.ext.table.layout = None;
            if !matches!(id, "table.cell_properties" | "table.properties") {
                self.ext.table.block = None;
            }
        }
        ok
    }

    /// `Some(enabled)` when this module owns the ribbon/menu command id.
    pub(crate) fn table_ext_enabled(&self, id: &str) -> Option<bool> {
        let in_table = self.session.cursor.cell.is_some();
        match id {
            "table.create" | "table.from_text" => Some(!in_table),
            "table.to_text" | "table.sort" | "table.auto_fill" | "table.caption_text"
            | "table.style" => Some(in_table),
            "table.properties"
            | "table.cell_border_fill"
            | "table.cell_block"
            | "table.split"
            | "table.formula"
            | "table.caption"
            | "table.delete"
            | "table.block_sum"
            | "table.block_average"
            | "table.block_product"
            | "table.transpose"
            | "table.split_table"
            | "table.merge_table"
            | "table.thousands"
            | "table.thousands_remove"
            | "table.cell_margin"
            | "table.margin"
            | "table.text_direction"
            | "table.align.top"
            | "table.align.center"
            | "table.align.bottom"
            | "table.placement.inline"
            | "table.placement.square"
            | "table.placement.top_bottom"
            | "table.placement.front"
            | "table.placement.behind" => Some(in_table),
            "table.merge" | "table.equal_height" | "table.equal_width" => {
                Some(in_table && self.block_range().is_some())
            }
            "table.cell_shade.darker" | "table.cell_shade.lighter" | "table.auto_fill_user" => {
                Some(in_table)
            }
            _ => id
                .strip_prefix("table.cell_move.")
                .filter(|m| CELL_MOTIONS.iter().any(|(k, _)| k == m))
                .map(|_| in_table),
        }
    }
    /// 표 안 셀 이동 (TableRightCell · TableColBegin · …): id suffix → engine motion.
    fn cell_move(&mut self, motion: &str) {
        let r = self
            .session
            .run("table.cell_move", json!({"motion":motion}));
        if let Some(v) = self.result(r) {
            self.ext.table.block = None;
            if v["appended"] == true {
                self.ext.table.layout = None;
                self.message = "마지막 셀에서 Tab: 줄을 추가했습니다.".into();
            } else if v["moved"] == false {
                self.message = "표의 끝 셀입니다.".into();
            }
            self.follow_cursor();
        }
    }
    /// Return true when the command id was handled.
    pub(crate) fn table_ext_dispatch(&mut self, id: &str, _ctx: &egui::Context) -> bool {
        if self.table_tool_dispatch(id) {
            return true;
        }
        match id {
            "table.create" => {
                self.ext.table.create = Some(Create {
                    rows: 5,
                    columns: 5,
                    custom_width: false,
                    width_mm: 150.0,
                    custom_height: false,
                    height_mm: 40.0,
                    treat_as_char: false,
                });
            }
            "table.properties" | "table.cell_border_fill" => {
                self.open_table_properties(id == "table.cell_border_fill", 0)
            }
            "table.caption" => {
                let r = self.session.run("table.properties.get", json!({}));
                if let Some(p) = self.result(r) {
                    let on = p["hasCaption"] != true;
                    self.run_table(
                        "table.properties",
                        json!({"props":{"hasCaption":on,"captionDirection":3}}),
                    );
                    self.message = if on {
                        "표 아래에 캡션을 넣었습니다.".into()
                    } else {
                        "캡션을 지웠습니다.".into()
                    };
                }
            }
            "table.cell_block" => self.cell_block_key(),
            "table.cell_shade.darker" | "table.cell_shade.lighter" => {
                let mut p = json!({"darker":id.ends_with("darker")});
                if let Some(r) = self.block_range() {
                    p["range"] = r;
                }
                let r = self.session.run("table.cell_shade", p);
                if let Some(v) = self.result(r) {
                    self.ext.table.layout = None;
                    self.message = if v["cells"] == 0 {
                        "더 바꿀 음영이 없습니다.".into()
                    } else {
                        format!("{}개 셀의 음영을 바꿨습니다.", v["cells"])
                    };
                }
            }
            _ if id.starts_with("table.cell_move.") => {
                let motion = id["table.cell_move.".len()..].to_owned();
                self.cell_move(&motion);
            }
            "table.merge" | "table.equal_height" | "table.equal_width" | "table.split_table" => {
                self.run_table(id, json!({}));
            }
            "table.delete" | "table.transpose" | "table.merge_table" => {
                self.ext.table.block = None;
                self.run_table(id, json!({}));
            }
            "table.thousands" | "table.thousands_remove" => {
                let mut p = json!({"remove":id.ends_with("_remove")});
                if let Some(r) = self.block_range() {
                    p["range"] = r;
                }
                let r = self.session.run("table.thousands", p);
                if let Some(v) = self.result(r) {
                    self.message = format!("{}개 셀의 숫자를 바꿨습니다.", v["cells"]);
                }
            }
            "table.cell_margin" => {
                self.open_table_properties(false, 5);
                if let Some(p) = &mut self.ext.table.properties {
                    if p.cell_draft["applyInnerMargin"] != true {
                        for key in ["paddingLeft", "paddingRight", "paddingTop", "paddingBottom"] {
                            p.cell_draft[key] = p.table_draft[key].clone();
                        }
                    }
                    p.cell_draft["applyInnerMargin"] = json!(true);
                }
            }
            "table.margin" => self.open_table_properties(false, 1),
            "table.text_direction" => {
                let c = self.session.run("table.cell_properties.get", json!({}));
                if let Some(c) = self.result(c) {
                    let vertical = c["textDirection"] != 1;
                    self.run_table(
                        "table.cell_properties",
                        json!({"props":{"textDirection":u8::from(vertical)}}),
                    );
                }
            }
            "table.align.top" | "table.align.center" | "table.align.bottom" => {
                let v = match id {
                    "table.align.top" => 0,
                    "table.align.center" => 1,
                    _ => 2,
                };
                self.run_table(
                    "table.cell_properties",
                    json!({"props":{"verticalAlign":v}}),
                );
            }
            "table.placement.inline" => {
                self.run_table("table.properties", json!({"props":{"treatAsChar":true}}));
            }
            "table.placement.square"
            | "table.placement.top_bottom"
            | "table.placement.front"
            | "table.placement.behind" => {
                let wrap = match id {
                    "table.placement.square" => "Square",
                    "table.placement.top_bottom" => "TopAndBottom",
                    "table.placement.front" => "InFrontOfText",
                    _ => "BehindText",
                };
                self.run_table(
                    "table.properties",
                    json!({"props":{"treatAsChar":false,"textWrap":wrap}}),
                );
            }
            "table.split" => {
                let multi = self.block_range().is_some_and(|r| {
                    r["start_row"] != r["end_row"] || r["start_col"] != r["end_col"]
                });
                self.ext.table.split = Some(Split {
                    by_rows: false,
                    rows: 2,
                    by_columns: true,
                    columns: 2,
                    equal_height: false,
                    merge_first: false,
                    multi,
                });
            }
            "table.formula" => self.ext.table.formula = Some(("=SUM(LEFT)".into(), None)),
            "table.block_sum" | "table.block_average" | "table.block_product" => {
                let op = id.trim_start_matches("table.block_");
                let r = self.block_range();
                let mut p = json!({"op":op});
                if let Some(r) = r {
                    p["range"] = r;
                }
                let result = self.session.run("table.block_calc", p);
                if let Some(v) = self.result(result) {
                    self.message = format!("{}개 셀에 결과를 넣었습니다.", v["cells"]);
                    self.ext.table.block = None;
                }
            }
            _ => return false,
        }
        true
    }
    fn open_table_properties(&mut self, cell_only: bool, tab: usize) {
        let Some(table) = self.table_address() else {
            self.error = Some("먼저 표 셀을 선택하세요.".into());
            return;
        };
        let range = self.block_range();
        let t = self.session.run("table.properties.get", json!({}));
        let mut c = json!({});
        if let Some(r) = &range {
            c["range"] = r.clone();
        }
        let c = self.session.run("table.cell_properties.get", c);
        let (Some(t), Some(mut c)) = (self.result(t), self.result(c)) else {
            return;
        };
        if let Some(o) = c.as_object_mut() {
            // Only editable keys stay in the draft; borders go through the edge tool.
            o.retain(|k, _| {
                !matches!(
                    k.as_str(),
                    "range"
                        | "cells"
                        | "borderFillId"
                        | "borderLeft"
                        | "borderRight"
                        | "borderTop"
                        | "borderBottom"
                )
            });
        }
        self.ext.table.properties = Some(Box::new(Properties {
            cell_only,
            tab,
            table,
            range,
            table_original: t.clone(),
            table_draft: t,
            cell_original: c.clone(),
            cell_draft: c,
            borders: Borders::default(),
            error: None,
        }));
    }

    /// F5: start a cell block on the caret cell; again: extend mode; again: whole table.
    fn cell_block_key(&mut self) {
        let Some((table, pos)) = self.cell_position(self.session.cursor) else {
            return;
        };
        let rows_cols = self
            .session
            .core
            .document()
            .sections
            .get(table.section)
            .and_then(|s| s.paragraphs.get(table.paragraph))
            .and_then(|p| p.controls.get(table.control))
            .and_then(|c| match c {
                hwpcraft_doc::model::control::Control::Table(t) => Some((t.row_count, t.col_count)),
                _ => None,
            });
        let block = match self.ext.table.block.filter(|b| b.table == table) {
            None => Block {
                table,
                anchor: pos,
                active: pos,
                stage: 1,
            },
            Some(b) if b.stage == 1 => Block { stage: 2, ..b },
            Some(b) => match rows_cols {
                Some((rows, cols)) => Block {
                    anchor: (0, 0),
                    active: (rows.saturating_sub(1), cols.saturating_sub(1)),
                    stage: 3,
                    ..b
                },
                None => b,
            },
        };
        self.ext.table.block = Some(block);
        self.session.clear_selection();
        self.message = match block.stage {
            1 => "셀 블록: 방향키로 셀 이동, Shift+방향키로 확장, F5 한 번 더 누르면 확장 모드",
            2 => "셀 블록 확장: 방향키로 블록을 넓힙니다. F5 한 번 더 누르면 표 전체",
            _ => "표 전체 셀 블록",
        }
        .into();
    }
    fn move_block(&mut self, d: (i32, i32), extend: bool) {
        let Some(mut b) = self.ext.table.block else {
            return;
        };
        let Ok(info) = self.session.run("table.info", json!({"table":b.table})) else {
            return;
        };
        let rows = info["rows"].as_i64().unwrap_or(1);
        let cols = info["columns"].as_i64().unwrap_or(1);
        let next = (
            (i32::from(b.active.0) + d.0).clamp(0, rows as i32 - 1) as u16,
            (i32::from(b.active.1) + d.1).clamp(0, cols as i32 - 1) as u16,
        );
        b.active = next;
        if !extend {
            b.anchor = next;
        }
        self.ext.table.block = Some(b);
        // Keep the caret on the active cell so Enter/typing still edits a visible cell.
        if let Some(index) = info["cells"].as_array().and_then(|cells| {
            cells.iter().find(|c| {
                let (r, c0) = (
                    c["row"].as_u64().unwrap_or(0),
                    c["col"].as_u64().unwrap_or(0),
                );
                let (rs, cs) = (
                    c["rowSpan"].as_u64().unwrap_or(1),
                    c["colSpan"].as_u64().unwrap_or(1),
                );
                (r..r + rs).contains(&u64::from(next.0))
                    && (c0..c0 + cs).contains(&u64::from(next.1))
            })
        }) && let Some(i) = index["index"].as_u64()
        {
            let mut c = self.session.cursor;
            if let Some(cell) = c.cell.as_mut() {
                cell.index = i as usize;
                cell.paragraph = 0;
                c.offset = 0;
                let r = self.session.select(c, false);
                self.result(r);
            }
        }
    }

    /// Canvas hook: mouse drags and Shift+clicks across cells make a cell block.
    /// Returns true when the default caret handling must be skipped.
    pub(crate) fn table_ext_canvas(
        &mut self,
        page: u32,
        response: &egui::Response,
        rect: Rect,
    ) -> bool {
        // Objects (selection, drawing) come first; both modules are owned by 03.
        if self.object_ext_canvas(page, response, rect) {
            self.ext.table.block = None;
            return true;
        }
        if self.grid_line_canvas(page, response, rect) {
            self.ext.table.block = None;
            return true;
        }
        let hit = |s: &Self, p: Pos2| {
            let pos = (p - rect.min) / s.zoom;
            s.session
                .hit_test(page, pos.x as f64, pos.y as f64)
                .ok()
                .and_then(|c| s.cell_position(c))
        };
        let mut consumed = false;
        if response.drag_started() {
            self.ext.table.drag = response
                .interact_pointer_pos()
                .and_then(|p| hit(self, p))
                .or_else(|| self.cell_position(self.session.cursor));
            self.ext.table.block = None;
        }
        if response.dragged()
            && let Some((table, start)) = self.ext.table.drag
            && let Some((t, pos)) = response.interact_pointer_pos().and_then(|p| hit(self, p))
            && t == table
            && (pos != start || self.ext.table.block.is_some())
        {
            self.ext.table.block = Some(Block {
                table,
                anchor: start,
                active: pos,
                stage: 1,
            });
            self.session.clear_selection();
            consumed = true;
        }
        if response.drag_stopped() {
            self.ext.table.drag = None;
            consumed |= self.ext.table.block.is_some();
        }
        if response.clicked() {
            let shift = response.ctx.input(|i| i.modifiers.shift);
            let target = response.interact_pointer_pos().and_then(|p| hit(self, p));
            match (shift, self.cell_position(self.session.cursor), target) {
                (true, Some((a, from)), Some((b, to))) if a == b && from != to => {
                    let anchor = self
                        .ext
                        .table
                        .block
                        .filter(|x| x.table == a)
                        .map(|x| x.anchor)
                        .unwrap_or(from);
                    self.ext.table.block = Some(Block {
                        table: a,
                        anchor,
                        active: to,
                        stage: 1,
                    });
                    consumed = true;
                }
                _ => self.ext.table.block = None,
            }
        }
        self.paint_block(page, response, rect);
        consumed
    }
    pub(super) fn page_layout(&mut self, page: u32) -> std::sync::Arc<Value> {
        cached_page_layout(&mut self.ext.table.layout, &self.session, page)
    }
    /// Inner and right/bottom outer grid lines of top-level tables under `p` (page px).
    fn grid_line_at(&mut self, page: u32, p: Pos2, tol: f32) -> Option<GridLine> {
        let layout = self.page_layout(page);
        let n = |v: &Value, k: &str| v[k].as_u64();
        let f = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
        for t in layout["controls"].as_array()?.iter().rev() {
            if t["type"] != "table" {
                continue;
            }
            let (Some(si), Some(pi), Some(ci)) =
                (n(t, "secIdx"), n(t, "paraIdx"), n(t, "controlIdx"))
            else {
                continue;
            };
            let table = ObjectAddress {
                section: si as usize,
                paragraph: pi as usize,
                control: ci as usize,
            };
            let (tx, ty, tw, th) = (f(t, "x"), f(t, "y"), f(t, "w"), f(t, "h"));
            if !Rect::from_min_size(Pos2::new(tx, ty), Vec2::new(tw, th))
                .expand(tol)
                .contains(p)
            {
                continue;
            }
            for c in t["cells"].as_array()? {
                let (x, y, w, h) = (f(c, "x"), f(c, "y"), f(c, "w"), f(c, "h"));
                let col = n(c, "col").unwrap_or(0) + n(c, "colSpan").unwrap_or(1);
                let row = n(c, "row").unwrap_or(0) + n(c, "rowSpan").unwrap_or(1);
                if (p.x - (x + w)).abs() <= tol && (y..=y + h).contains(&p.y) {
                    return Some(GridLine {
                        table,
                        vertical: true,
                        boundary: col as u16,
                        start: p,
                        span: (ty, ty + th),
                    });
                }
                if (p.y - (y + h)).abs() <= tol && (x..=x + w).contains(&p.x) {
                    return Some(GridLine {
                        table,
                        vertical: false,
                        boundary: row as u16,
                        start: p,
                        span: (tx, tx + tw),
                    });
                }
            }
        }
        None
    }
    /// Drag a table grid line; the move is applied once on release (one undo step).
    fn grid_line_canvas(&mut self, page: u32, response: &egui::Response, rect: Rect) -> bool {
        let to_page = |s: &Self, p: Pos2| ((p - rect.min) / s.zoom).to_pos2();
        let tol = 3.0 / self.zoom;
        if self.ext.table.line.is_none()
            && let Some(p) = response.hover_pos().map(|p| to_page(self, p))
            && let Some(l) = self.grid_line_at(page, p, tol)
        {
            response.ctx.set_cursor_icon(if l.vertical {
                egui::CursorIcon::ResizeHorizontal
            } else {
                egui::CursorIcon::ResizeVertical
            });
        }
        if let Some(p) = crate::table_drawing::page_press(response).map(|p| to_page(self, p)) {
            self.ext.table.line = self.grid_line_at(page, p, tol);
        }
        let Some(l) = self.ext.table.line else {
            return false;
        };
        let Some(p) = response
            .ctx
            .input(|i| i.pointer.interact_pos())
            .map(|p| to_page(self, p))
        else {
            return true;
        };
        let painter = response.ctx.layer_painter(response.layer_id);
        let to_screen = |q: Pos2| rect.min + q.to_vec2() * self.zoom;
        let guide = Stroke::new(1.0, chrome::Tokens::ACCENT);
        let (a, b) = if l.vertical {
            (Pos2::new(p.x, l.span.0), Pos2::new(p.x, l.span.1))
        } else {
            (Pos2::new(l.span.0, p.y), Pos2::new(l.span.1, p.y))
        };
        painter.line_segment([to_screen(a), to_screen(b)], guide);
        response.ctx.set_cursor_icon(if l.vertical {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        });
        if crate::table_drawing::page_release(response) {
            self.ext.table.line = None;
            let px = if l.vertical {
                p.x - l.start.x
            } else {
                p.y - l.start.y
            };
            let mm = f64::from(px) * 25.4 / 96.0;
            if mm.abs() >= 0.2 {
                let r = self.session.run(
                    "table.move_border",
                    json!({"table":l.table,"vertical":l.vertical,"boundary":l.boundary,"delta_mm":mm}),
                );
                self.result(r);
                self.ext.table.layout = None;
            }
        }
        true
    }
    fn paint_block(&mut self, page: u32, response: &egui::Response, rect: Rect) {
        let Some(b) = self.ext.table.block else {
            return;
        };
        let layout = self.page_layout(page);
        let range = b.range();
        let n = |v: &Value, k: &str| v[k].as_u64().unwrap_or(0);
        let painter = response.ctx.layer_painter(response.layer_id);
        let fill = Color32::from_rgba_unmultiplied(46, 105, 220, 70);
        for t in layout["controls"].as_array().into_iter().flatten() {
            if t["type"] != "table"
                || n(t, "secIdx") as usize != b.table.section
                || n(t, "paraIdx") as usize != b.table.paragraph
                || n(t, "controlIdx") as usize != b.table.control
            {
                continue;
            }
            for c in t["cells"].as_array().into_iter().flatten() {
                let (row, col) = (n(c, "row"), n(c, "col"));
                if (n(&range, "start_row")..=n(&range, "end_row")).contains(&row)
                    && (n(&range, "start_col")..=n(&range, "end_col")).contains(&col)
                {
                    let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
                    let r = Rect::from_min_size(
                        rect.min + Vec2::new(f("x"), f("y")) * self.zoom,
                        Vec2::new(f("w"), f("h")) * self.zoom,
                    );
                    painter.rect_filled(r.intersect(rect), 0.0, fill);
                }
            }
        }
    }

    /// Keyboard hook, called before the editor consumes keys.
    pub(crate) fn table_ext_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.object_ext_keys(ctx) {
            return true;
        }
        if self.session.cursor.cell.is_none() {
            self.ext.table.block = None;
            return false;
        }
        let consume = |m: egui::Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if consume(egui::Modifiers::NONE, Key::F5) {
            self.cell_block_key();
            return true;
        }
        // COMMAND is Ctrl on Windows/Linux and Cmd on macOS.
        let ctrl_shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        for (key, id) in [
            (Key::S, "table.block_sum"),
            (Key::A, "table.block_average"),
            (Key::P, "table.block_product"),
        ] {
            if self.ext.table.block.is_some() && consume(ctrl_shift, key) {
                self.table_ext_dispatch(id, ctx);
                return true;
            }
        }
        // Ctrl+arrows resize the block's columns/rows by 1 mm (Hangul: 셀 크기 조절).
        for (key, vertical, delta) in [
            (Key::ArrowLeft, false, -1.0),
            (Key::ArrowRight, false, 1.0),
            (Key::ArrowUp, true, -1.0),
            (Key::ArrowDown, true, 1.0),
        ] {
            if consume(egui::Modifiers::COMMAND, key) {
                let mut p = json!({"vertical":vertical,"delta_mm":delta});
                if let Some(r) = self.block_range() {
                    p["range"] = r;
                }
                let r = self.session.run("table.resize", p);
                self.result(r);
                self.ext.table.layout = None;
                return true;
            }
        }
        let Some(b) = self.ext.table.block else {
            return false;
        };
        if consume(egui::Modifiers::NONE, Key::Escape) {
            self.ext.table.block = None;
            return true;
        }
        for (key, d) in [
            (Key::ArrowLeft, (0, -1)),
            (Key::ArrowRight, (0, 1)),
            (Key::ArrowUp, (-1, 0)),
            (Key::ArrowDown, (1, 0)),
        ] {
            if consume(egui::Modifiers::SHIFT, key) {
                self.move_block(d, true);
                return true;
            }
            if consume(egui::Modifiers::NONE, key) {
                self.move_block(d, b.stage >= 2);
                return true;
            }
        }
        // Hangul block shortcuts: single letters act on the block instead of typing.
        for (key, id) in [
            (Key::M, "table.merge"),
            (Key::S, "table.split"),
            (Key::H, "table.equal_height"),
            (Key::W, "table.equal_width"),
            (Key::L, "table.cell_border_fill"),
            (Key::P, "table.properties"),
        ] {
            if consume(egui::Modifiers::NONE, key) {
                self.table_ext_dispatch(id, ctx);
                return true;
            }
        }
        // Swallow typed letters for the block so they do not reach the cell.
        ctx.input_mut(|i| {
            let before = i.events.len();
            i.events.retain(|e| !matches!(e, Event::Text(_)));
            before != i.events.len()
        })
    }

    /// Startup views for reproducible captures (`--view <name>`). False for unknown names.
    pub(crate) fn table_ext_view(&mut self, name: &str, ctx: &egui::Context) -> bool {
        if self.object_ext_view(name, ctx) {
            return true;
        }
        const VIEWS: &[&str] = &[
            "table-create",
            "table-properties",
            "table-cell-margin",
            "table-cell-border",
            "table-split",
            "table-formula",
            "table-block",
            "table-style",
            "table-sort",
            "table-user-fill",
        ];
        // Leave other modules' view names alone.
        if !VIEWS.contains(&name) {
            return false;
        }
        let needs_table = name != "table-create";
        if needs_table && self.session.cursor.cell.is_none() {
            let Some((cursor, _)) = self
                .session
                .entries()
                .into_iter()
                .find(|(c, _)| c.cell.is_some())
            else {
                self.error = Some("표가 있는 문서를 열어 주세요.".into());
                return true;
            };
            let r = self.session.select(cursor, false);
            self.result(r);
        }
        match name {
            "table-create" => {
                self.table_ext_dispatch("table.create", ctx);
            }
            "table-properties" => self.open_table_properties(false, 0),
            "table-cell-margin" => {
                self.table_ext_dispatch("table.cell_margin", ctx);
            }
            "table-cell-border" => {
                self.ext_block_demo();
                self.open_table_properties(true, 0);
            }
            "table-split" => {
                self.table_ext_dispatch("table.split", ctx);
            }
            "table-formula" => {
                self.table_ext_dispatch("table.formula", ctx);
            }
            "table-block" => self.ext_block_demo(),
            "table-user-fill" => {
                self.enter_first_cell();
                self.ext_block_demo();
                self.table_ext_dispatch("table.auto_fill_user", ctx);
            }
            "table-style" => {
                self.enter_first_cell();
                self.table_ext_dispatch("table.style", ctx);
            }
            "table-sort" => {
                self.ext_block_demo();
                self.table_ext_dispatch("table.sort", ctx);
            }
            _ => return false,
        }
        true
    }
    /// A 2×2 (or smaller) block from the caret cell, for screenshots.
    /// Captures: put the caret in the document's first table cell when it is in the body.
    fn enter_first_cell(&mut self) {
        if self.session.cursor.cell.is_some() {
            return;
        }
        if let Some((c, _)) = self
            .session
            .entries()
            .into_iter()
            .find(|(c, _)| c.cell.is_some())
        {
            let r = self.session.select(c, false);
            self.result(r);
        }
    }
    fn ext_block_demo(&mut self) {
        let Some((table, pos)) = self.cell_position(self.session.cursor) else {
            return;
        };
        let info = self.session.run("table.info", json!({"table":table}));
        let Some(info) = self.result(info) else {
            return;
        };
        let last =
            |k: &str, v: u16| (v + 1).min(info[k].as_u64().unwrap_or(1).saturating_sub(1) as u16);
        self.ext.table.block = Some(Block {
            table,
            anchor: pos,
            active: (last("rows", pos.0), last("columns", pos.1)),
            stage: 1,
        });
    }
    /// True while a dialog of this module (or the object module) is open.
    pub(crate) fn table_ext_modal_open(&self) -> bool {
        let t = &self.ext.table;
        t.create.is_some()
            || t.properties.is_some()
            || t.split.is_some()
            || t.formula.is_some()
            || t.tool.is_some()
            || self.object_ext_modal_open()
    }
    /// Called every frame after the built-in dialogs.
    pub(crate) fn table_ext_dialogs(&mut self, ctx: &egui::Context) {
        if self.session.cursor.cell.is_none() {
            self.ext.table.block = None;
        }
        self.table_create_dialog(ctx);
        self.table_properties_dialog(ctx);
        self.table_split_dialog(ctx);
        self.table_formula_dialog(ctx);
        self.table_tool_dialog(ctx);
    }

    fn table_create_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.table.create.take() else {
            return;
        };
        let (mut create, mut close) = (false, false);
        let r = chrome::modal("ext-table-create").show(ctx, |ui| {
            ui.set_width(520.0);
            close = chrome::title(ui, "표 만들기");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(400.0);
                    chrome::section(ui, "줄/칸");
                    ui.horizontal(|ui| {
                        ui.label("줄 개수(R)");
                        ui.add(egui::DragValue::new(&mut d.rows).range(1..=200));
                        ui.add_space(18.0);
                        ui.label("칸 개수(C)");
                        ui.add(egui::DragValue::new(&mut d.columns).range(1..=64));
                    });
                    chrome::section(ui, "크기 지정");
                    egui::Grid::new("table-create-size")
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.label("너비(W)");
                            egui::ComboBox::from_id_salt("table-create-width")
                                .selected_text(if d.custom_width {
                                    "임의 값"
                                } else {
                                    "단에 맞춤"
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut d.custom_width, false, "단에 맞춤");
                                    ui.selectable_value(&mut d.custom_width, true, "임의 값");
                                });
                            ui.add_enabled(
                                d.custom_width,
                                egui::DragValue::new(&mut d.width_mm)
                                    .range(1.0..=600.0)
                                    .fixed_decimals(2)
                                    .suffix(" mm"),
                            );
                            ui.end_row();
                            ui.label("높이(H)");
                            egui::ComboBox::from_id_salt("table-create-height")
                                .selected_text(if d.custom_height {
                                    "임의 값"
                                } else {
                                    "자동"
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut d.custom_height, false, "자동");
                                    ui.selectable_value(&mut d.custom_height, true, "임의 값");
                                });
                            ui.add_enabled(
                                d.custom_height,
                                egui::DragValue::new(&mut d.height_mm)
                                    .range(1.0..=2000.0)
                                    .fixed_decimals(2)
                                    .suffix(" mm"),
                            );
                            ui.end_row();
                        });
                    chrome::section(ui, "기타");
                    ui.checkbox(&mut d.treat_as_char, "글자처럼 취급(O)");
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(390.0, 110.0), egui::Sense::hover());
                    let rect = rect.shrink(8.0);
                    let rows = d.rows.min(12) as f32;
                    let cols = d.columns.min(16) as f32;
                    let stroke = Stroke::new(1.0, Color32::GRAY);
                    for i in 0..=rows as u16 {
                        let y = rect.top() + f32::from(i) * rect.height() / rows;
                        ui.painter().line_segment(
                            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                            stroke,
                        );
                    }
                    for i in 0..=cols as u16 {
                        let x = rect.left() + f32::from(i) * rect.width() / cols;
                        ui.painter().line_segment(
                            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                            stroke,
                        );
                    }
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "만들기(D)");
                    create = a;
                    close |= c;
                    ui.add_enabled(
                        false,
                        egui::Button::new("표 마당(Y)").min_size(Vec2::new(80.0, 28.0)),
                    )
                    .on_disabled_hover_text(
                        "표를 만든 뒤 표 디자인 탭의 표 마당에서 모양을 고르세요.",
                    );
                });
            });
        });
        if create {
            let mut p = json!({"rows":d.rows,"columns":d.columns,"treat_as_char":d.treat_as_char});
            if d.custom_width {
                p["width_mm"] = json!(d.width_mm);
            }
            if d.custom_height {
                p["height_mm"] = json!(d.height_mm);
            }
            close = self.run("table.create", p);
        }
        if !(close || r.should_close()) {
            self.ext.table.create = Some(d);
        }
    }

    fn table_properties_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.table.properties.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let multi = d
            .range
            .as_ref()
            .is_some_and(|r| r["start_row"] != r["end_row"] || r["start_col"] != r["end_col"]);
        let r = chrome::modal("ext-table-properties").show(ctx, |ui| {
            ui.set_width(600.0);
            close = chrome::title(
                ui,
                if d.cell_only {
                    "셀 테두리/배경"
                } else {
                    "표/셀 속성"
                },
            );
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(490.0);
                    ui.set_min_height(360.0);
                    let labels: &[&str] = if d.cell_only {
                        &["테두리", "배경", "대각선"]
                    } else {
                        &["기본", "여백/캡션", "테두리", "배경", "표", "셀"]
                    };
                    chrome::tabs(ui, &mut d.tab, labels);
                    let tab = labels.get(d.tab).copied().unwrap_or("기본");
                    let t = &mut d.table_draft;
                    let c = &mut d.cell_draft;
                    match tab {
                        "기본" => {
                            chrome::section(ui, "크기");
                            ui.horizontal(|ui| {
                                mm_field(ui, "너비", t, "tableWidth", 600.0);
                                mm_field(ui, "높이", t, "tableHeight", 2000.0);
                            });
                            chrome::section(ui, "위치");
                            check(ui, "글자처럼 취급", t, "treatAsChar");
                            let inline = t["treatAsChar"] == true;
                            ui.add_enabled_ui(!inline, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("본문과의 배치");
                                    for (k, l) in [
                                        ("Square", "어울림"),
                                        ("TopAndBottom", "자리 차지"),
                                        ("InFrontOfText", "글 앞으로"),
                                        ("BehindText", "글 뒤로"),
                                    ] {
                                        if ui.selectable_label(t["textWrap"] == k, l).clicked() {
                                            t["textWrap"] = json!(k);
                                        }
                                    }
                                });
                                egui::Grid::new("table-position").num_columns(5).show(ui, |ui| {
                                    ui.label("가로");
                                    choice(ui, "horz-rel", t, "horzRelTo", &[("Paper", "종이"), ("Page", "쪽"), ("Column", "단"), ("Para", "문단")]);
                                    ui.label("의");
                                    choice(ui, "horz-align", t, "horzAlign", &[("Left", "왼쪽"), ("Center", "가운데"), ("Right", "오른쪽"), ("Inside", "안쪽"), ("Outside", "바깥쪽")]);
                                    mm_field(ui, "기준", t, "horzOffset", 600.0);
                                    ui.end_row();
                                    ui.label("세로");
                                    choice(ui, "vert-rel", t, "vertRelTo", &[("Paper", "종이"), ("Page", "쪽"), ("Para", "문단")]);
                                    ui.label("의");
                                    choice(ui, "vert-align", t, "vertAlign", &[("Top", "위"), ("Center", "가운데"), ("Bottom", "아래"), ("Inside", "안쪽"), ("Outside", "바깥쪽")]);
                                    mm_field(ui, "기준", t, "vertOffset", 600.0);
                                    ui.end_row();
                                });
                            });
                            chrome::section(ui, "기타");
                            check(ui, "쪽 영역 안으로 제한", t, "restrictInPage");
                            check(ui, "서로 겹침 허용", t, "allowOverlap");
                            check(ui, "개체와 조판 부호를 항상 같은 쪽에 놓기", t, "keepWithAnchor");
                        }
                        "여백/캡션" => {
                            chrome::section(ui, "바깥 여백");
                            egui::Grid::new("table-outer").num_columns(4).show(ui, |ui| {
                                mm_field(ui, "왼쪽", t, "outerLeft", 50.0);
                                mm_field(ui, "오른쪽", t, "outerRight", 50.0);
                                ui.end_row();
                                mm_field(ui, "위쪽", t, "outerTop", 50.0);
                                mm_field(ui, "아래쪽", t, "outerBottom", 50.0);
                                ui.end_row();
                            });
                            chrome::section(ui, "캡션");
                            ui.horizontal(|ui| {
                                let mut has = t["hasCaption"] == true;
                                if ui.radio(!has, "캡션 없음").clicked() {
                                    has = false;
                                }
                                for (dir, l) in [(0, "왼쪽"), (1, "오른쪽"), (2, "위"), (3, "아래")] {
                                    if ui.radio(has && t["captionDirection"] == dir, l).clicked() {
                                        has = true;
                                        t["captionDirection"] = json!(dir);
                                    }
                                }
                                t["hasCaption"] = json!(has);
                            });
                            ui.add_enabled_ui(t["hasCaption"] == true, |ui| {
                                ui.horizontal(|ui| {
                                    mm_field(ui, "크기", t, "captionWidth", 600.0);
                                    mm_field(ui, "개체와의 간격", t, "captionSpacing", 50.0);
                                });
                            });
                            ui.label(RichText::new("캡션 내용은 아직 화면에서 직접 고칠 수 없습니다(번호 자동 유지).").small().weak());
                        }
                        "테두리" => border_tab(ui, &mut d.borders, multi),
                        "배경" => fill_tab(ui, c),
                        "대각선" => {
                            chrome::section(ui, "대각선");
                            egui::Grid::new("cell-diagonal").num_columns(2).show(ui, |ui| {
                                ui.label("╱ 방향");
                                number_choice(ui, "diag-slash", c, "diagonalSlash", &["없음", "중앙선", "", "", "", "", "", "모두"]);
                                ui.end_row();
                                ui.label("╲ 방향");
                                number_choice(ui, "diag-back", c, "diagonalBackSlash", &["없음", "중앙선", "", "", "", "", "", "모두"]);
                                ui.end_row();
                                ui.label("선 종류");
                                number_choice(ui, "diag-line", c, "diagonalLine", LINE_TYPES);
                                ui.end_row();
                                ui.label("굵기");
                                let widths: Vec<String> = LINE_WIDTHS.iter().map(|w| format!("{w} mm")).collect();
                                let refs: Vec<&str> = widths.iter().map(String::as_str).collect();
                                number_choice(ui, "diag-width", c, "diagonalWidth", &refs);
                                ui.end_row();
                                ui.label("색");
                                let mut col = css_color(&c["diagonalColor"]);
                                if egui::color_picker::color_edit_button_srgb(ui, &mut col).changed() {
                                    c["diagonalColor"] = json!(color_css(col));
                                }
                                ui.end_row();
                                ui.label("중심선");
                                choice(ui, "center-line", c, "centerLine", &[("NONE", "없음"), ("VERTICAL", "세로"), ("HORIZONTAL", "가로"), ("CROSS", "십자")]);
                                ui.end_row();
                            });
                        }
                        "표" => {
                            chrome::section(ui, "여러 쪽 지원");
                            ui.horizontal(|ui| {
                                ui.label("쪽 경계에서");
                                for (v, l) in [(2, "나눔"), (1, "셀 단위로 나눔"), (0, "나누지 않음")] {
                                    if ui.radio(t["pageBreak"] == v, l).clicked() {
                                        t["pageBreak"] = json!(v);
                                    }
                                }
                            });
                            check(ui, "제목 줄 자동 반복", t, "repeatHeader");
                            chrome::section(ui, "모든 셀의 안 여백");
                            egui::Grid::new("table-padding").num_columns(4).show(ui, |ui| {
                                mm_field(ui, "왼쪽", t, "paddingLeft", 50.0);
                                mm_field(ui, "오른쪽", t, "paddingRight", 50.0);
                                ui.end_row();
                                mm_field(ui, "위쪽", t, "paddingTop", 50.0);
                                mm_field(ui, "아래쪽", t, "paddingBottom", 50.0);
                                ui.end_row();
                            });
                            chrome::section(ui, "셀 간격");
                            ui.horizontal(|ui| mm_field(ui, "간격", t, "cellSpacing", 10.0));
                        }
                        _ => {
                            chrome::section(ui, "셀 크기 적용");
                            ui.horizontal(|ui| {
                                mm_field(ui, "너비", c, "width", 600.0);
                                mm_field(ui, "높이", c, "height", 600.0);
                            });
                            chrome::section(ui, "안 여백 지정");
                            check(ui, "안 여백 지정", c, "applyInnerMargin");
                            ui.add_enabled_ui(c["applyInnerMargin"] == true, |ui| {
                                egui::Grid::new("cell-padding").num_columns(4).show(ui, |ui| {
                                    mm_field(ui, "왼쪽", c, "paddingLeft", 50.0);
                                    mm_field(ui, "오른쪽", c, "paddingRight", 50.0);
                                    ui.end_row();
                                    mm_field(ui, "위쪽", c, "paddingTop", 50.0);
                                    mm_field(ui, "아래쪽", c, "paddingBottom", 50.0);
                                    ui.end_row();
                                });
                            });
                            chrome::section(ui, "속성");
                            ui.horizontal(|ui| {
                                ui.label("세로 정렬");
                                for (v, l) in [(0, "위"), (1, "가운데"), (2, "아래")] {
                                    if ui.radio(c["verticalAlign"] == v, l).clicked() {
                                        c["verticalAlign"] = json!(v);
                                    }
                                }
                            });
                            let mut vertical = c["textDirection"] == 1;
                            if ui.checkbox(&mut vertical, "세로쓰기").changed() {
                                c["textDirection"] = json!(u8::from(vertical));
                            }
                            check(ui, "셀 보호", c, "cellProtect");
                            check(ui, "제목 셀", c, "isHeader");
                            check(ui, "양식 모드에서 편집 가능", c, "editableInForm");
                            ui.horizontal(|ui| {
                                ui.label("필드 이름");
                                let mut name = c["fieldName"].as_str().unwrap_or("").to_owned();
                                if ui.text_edit_singleline(&mut name).changed() {
                                    c["fieldName"] = json!(name);
                                }
                            });
                        }
                    }
                    if let Some(e) = &d.error {
                        ui.colored_label(Color32::DARK_RED, e);
                    }
                    ui.label(
                        RichText::new(if multi {
                            "셀 항목은 선택한 셀 블록 전체에 적용합니다."
                        } else {
                            "셀 항목은 현재 셀에 적용합니다. F5로 셀 블록을 지정할 수 있습니다."
                        })
                        .small()
                        .weak(),
                    );
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
            let table = changed(&d.table_original, &d.table_draft);
            let cell = changed(&d.cell_original, &d.cell_draft);
            let borders: Map<String, Value> = d
                .borders
                .applied
                .iter()
                .filter_map(|(e, line)| {
                    EDGES
                        .iter()
                        .find(|(x, _, _)| x == e)
                        .map(|(_, k, _)| ((*k).to_owned(), line.clone()))
                })
                .collect();
            let mut ok = true;
            if !table.is_empty() {
                let r = self
                    .session
                    .run("table.properties", json!({"table":d.table,"props":table}));
                if let Err(e) = r {
                    d.error = Some(format!("{e:#}"));
                    ok = false;
                }
            }
            if ok && (!cell.is_empty() || !borders.is_empty()) {
                let mut p = json!({"table":d.table,"props":cell});
                if !borders.is_empty() {
                    p["borders"] = Value::Object(borders);
                }
                if let Some(r) = &d.range {
                    p["range"] = r.clone();
                }
                if let Err(e) = self.session.run("table.cell_properties", p) {
                    d.error = Some(format!("{e:#}"));
                    ok = false;
                }
            }
            self.ext.table.layout = None;
            self.sync_formatting();
            close = ok;
        }
        if !(close || r.should_close()) {
            self.ext.table.properties = Some(d);
        }
    }

    fn table_split_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.table.split.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let r = chrome::modal("ext-table-split").show(ctx, |ui| {
            ui.set_width(380.0);
            close = chrome::title(ui, "셀 나누기");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(270.0);
                    chrome::section(ui, "줄/칸 나누기");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut d.by_rows, "줄 개수(R)");
                        ui.add_enabled(d.by_rows, egui::DragValue::new(&mut d.rows).range(2..=50));
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut d.by_columns, "칸 개수(C)");
                        ui.add_enabled(
                            d.by_columns,
                            egui::DragValue::new(&mut d.columns).range(2..=50),
                        );
                    });
                    chrome::section(ui, "선택 사항");
                    ui.add_enabled(
                        d.by_rows,
                        egui::Checkbox::new(&mut d.equal_height, "줄 높이를 같게 나누기(H)"),
                    );
                    ui.add_enabled(
                        d.multi,
                        egui::Checkbox::new(&mut d.merge_first, "셀을 합친 후 나누기(M)"),
                    );
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, d.by_rows || d.by_columns, "나누기(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply {
            close = self.run_table(
                "table.split",
                json!({"rows":if d.by_rows {d.rows} else {1},"columns":if d.by_columns {d.columns} else {1},
                    "equal_height":d.equal_height && d.by_rows,"merge_first":d.merge_first && d.multi}),
            );
        }
        if !(close || r.should_close()) {
            self.ext.table.split = Some(d);
        }
    }

    fn table_formula_dialog(&mut self, ctx: &egui::Context) {
        let Some((mut formula, mut error)) = self.ext.table.formula.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let r = chrome::modal("ext-table-formula").show(ctx, |ui| {
            ui.set_width(460.0);
            close = chrome::title(ui, "계산식");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(350.0);
                    ui.label("계산식(F)");
                    ui.add(egui::TextEdit::singleline(&mut formula).desired_width(340.0));
                    ui.horizontal(|ui| {
                        ui.label("함수(U)");
                        egui::ComboBox::from_id_salt("table-formula-fn")
                            .selected_text("함수 넣기")
                            .show_ui(ui, |ui| {
                                for f in [
                                    "SUM", "AVG", "PRODUCT", "MIN", "MAX", "COUNT", "ABS", "SQRT",
                                    "INT", "ROUND", "MOD", "IF",
                                ] {
                                    if ui.selectable_label(false, f).clicked() {
                                        formula = format!("={f}(LEFT)");
                                    }
                                }
                            });
                    });
                    ui.label(
                        RichText::new(
                            "셀 이름: A1(첫 칸 첫 줄), 범위 A1:B3, LEFT/ABOVE. 결과는 현재 셀에 씁니다.",
                        )
                        .small()
                        .weak(),
                    );
                    if let Some(e) = &error {
                        ui.colored_label(Color32::DARK_RED, e);
                    }
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, !formula.trim().is_empty(), "넣기(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply {
            match self
                .session
                .run("table.formula", json!({"formula":formula}))
            {
                Ok(v) => {
                    self.message = format!("계산 결과: {}", v["result"]);
                    self.ext.table.layout = None;
                    close = true;
                }
                Err(e) => error = Some(format!("{e:#}")),
            }
        }
        if !(close || r.should_close()) {
            self.ext.table.formula = Some((formula, error));
        }
    }
}

/// Small table dialogs added in stage 5.
#[derive(Clone)]
pub(crate) enum TableTool {
    /// 문자열을 표로 (true) / 표를 문자열로 (false) with the delimiter kind and custom text.
    Convert {
        to_table: bool,
        kind: usize,
        custom: String,
    },
    Sort {
        column: u16,
        first: u16,
        last: u16,
        descending: bool,
    },
    Caption(String),
    /// 표 마당: preset index and the seven option check boxes (`STYLE_OPTIONS`).
    Style(usize, [bool; 7]),
    /// 사용자 정의 자동 채우기: list items, one per line or comma-separated.
    UserFill(String),
}
fn default_style_options() -> [bool; 7] {
    let mut o = [false; 7];
    for (i, (_, _, d)) in STYLE_OPTIONS.iter().enumerate() {
        o[i] = *d;
    }
    o
}
const DELIMITERS: &[(&str, &str)] = &[
    ("tab", "탭"),
    ("comma", "쉼표"),
    ("space", "빈칸"),
    ("semicolon", "세미콜론"),
];
/// Shown on the inner-line buttons of the border tab while only one cell is chosen.
const INNER_EDGE_REASON: &str = "안쪽 선은 셀 블록(F5)을 두 칸 이상 지정했을 때 고를 수 있습니다.";
/// Engine motions for 표 안 셀 이동 and their Hangul menu labels.
const CELL_MOTIONS: &[(&str, &str)] = &[
    ("left", "왼쪽 셀"),
    ("right", "오른쪽 셀"),
    ("up", "위 셀"),
    ("down", "아래 셀"),
    ("row_begin", "줄의 첫 셀"),
    ("row_end", "줄의 끝 셀"),
    ("col_top", "칸의 첫 셀"),
    ("col_bottom", "칸의 끝 셀"),
    ("next", "다음 셀 (Tab)"),
    ("prev", "이전 셀 (Shift+Tab)"),
];
/// 표 마당 옵션 check boxes (ApplyTable*): engine key, label, Hangul default.
const STYLE_OPTIONS: &[(&str, &str, bool)] = &[
    ("header_row", "제목 줄", true),
    ("first_column", "첫째 칸", false),
    ("last_column", "마지막 칸", false),
    ("last_row", "마지막 줄", false),
    ("grayscale", "회색조", false),
    ("borders", "테두리 적용", true),
    ("background", "셀 배경 적용", true),
];
const STYLES: &[(&str, &str)] = &[
    ("basic", "기본"),
    ("blue_header", "파란 제목 줄"),
    ("gray_grid", "회색 격자"),
    ("striped", "줄무늬"),
];

impl HwpApp {
    fn table_tool_dispatch(&mut self, id: &str) -> bool {
        let tool = match id {
            "table.from_text" => TableTool::Convert {
                to_table: true,
                kind: 0,
                custom: String::new(),
            },
            "table.to_text" => TableTool::Convert {
                to_table: false,
                kind: 0,
                custom: String::new(),
            },
            "table.sort" => {
                let r = self.block_range();
                let (first, last) = match &r {
                    Some(r) => (
                        r["start_col"].as_u64().unwrap_or(0) as u16,
                        r["end_col"].as_u64().unwrap_or(0) as u16,
                    ),
                    None => (0, 0),
                };
                TableTool::Sort {
                    column: first,
                    first,
                    last,
                    descending: false,
                }
            }
            "table.caption_text" => {
                let r = self.session.run("table.caption_text.get", json!({}));
                match r {
                    Ok(v) => TableTool::Caption(v["text"].as_str().unwrap_or("").to_owned()),
                    Err(_) => {
                        // No caption yet: add one under the table first, like Hangul.
                        if !self.run_table(
                            "table.properties",
                            json!({"props":{"hasCaption":true,"captionDirection":3}}),
                        ) {
                            return true;
                        }
                        TableTool::Caption(String::new())
                    }
                }
            }
            "table.style" => TableTool::Style(1, default_style_options()),
            "table.auto_fill_user" => TableTool::UserFill(String::new()),
            "table.auto_fill" => {
                let mut p = json!({});
                if let Some(r) = self.block_range() {
                    p["range"] = r;
                }
                let r = self.session.run("table.auto_fill", p);
                if let Some(v) = self.result(r) {
                    self.message = format!("{}개 셀을 채웠습니다.", v["cells"]);
                    self.ext.table.layout = None;
                }
                return true;
            }
            _ => return false,
        };
        self.ext.table.tool = Some(tool);
        true
    }

    fn table_tool_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut tool) = self.ext.table.tool.take() else {
            return;
        };
        let (mut apply, mut close) = (false, false);
        let title = match &tool {
            TableTool::Convert { to_table: true, .. } => "문자열을 표로",
            TableTool::Convert { .. } => "표를 문자열로",
            TableTool::Sort { .. } => "정렬",
            TableTool::Caption(_) => "캡션 고치기",
            TableTool::Style(..) => "표 마당",
            TableTool::UserFill(_) => "사용자 정의 자동 채우기",
        };
        let r = chrome::modal("ext-table-tool").show(ctx, |ui| {
            ui.set_width(440.0);
            close = chrome::title(ui, title);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(330.0);
                    match &mut tool {
                        TableTool::Convert {
                            to_table,
                            kind,
                            custom,
                        } => {
                            chrome::section(ui, "구분 기호");
                            ui.horizontal_wrapped(|ui| {
                                for (i, (_, label)) in DELIMITERS.iter().enumerate() {
                                    ui.radio_value(kind, i, *label);
                                }
                                ui.radio_value(kind, DELIMITERS.len(), "기타 문자");
                                ui.add_enabled(
                                    *kind == DELIMITERS.len(),
                                    egui::TextEdit::singleline(custom).desired_width(60.0),
                                );
                            });
                            ui.label(
                                RichText::new(if *to_table {
                                    "선택한 문단마다 한 줄, 구분 기호마다 한 칸이 됩니다."
                                } else {
                                    "줄마다 한 문단으로, 칸 사이에는 구분 기호를 넣습니다."
                                })
                                .small()
                                .weak(),
                            );
                        }
                        TableTool::Sort {
                            column,
                            first,
                            last,
                            descending,
                        } => {
                            chrome::section(ui, "기준");
                            ui.horizontal(|ui| {
                                ui.label("기준 칸");
                                egui::ComboBox::from_id_salt("sort-column")
                                    .selected_text(format!("{}번째 칸", *column + 1))
                                    .show_ui(ui, |ui| {
                                        for c in *first..=*last {
                                            ui.selectable_value(column, c, format!("{}번째 칸", c + 1));
                                        }
                                    });
                            });
                            ui.horizontal(|ui| {
                                ui.radio_value(descending, false, "오름차순");
                                ui.radio_value(descending, true, "내림차순");
                            });
                            ui.label(
                                RichText::new(
                                    "F5로 정렬할 줄(셀 블록)을 먼저 지정하세요. 숫자는 값 크기로 비교합니다.",
                                )
                                .small()
                                .weak(),
                            );
                        }
                        TableTool::Caption(text) => {
                            chrome::section(ui, "캡션 내용");
                            ui.horizontal(|ui| {
                                ui.label("표 (번호)");
                                ui.add(egui::TextEdit::singleline(text).desired_width(230.0));
                            });
                            ui.label(
                                RichText::new("표 번호는 자동으로 매겨지며 그 뒤의 글을 고칩니다.")
                                    .small()
                                    .weak(),
                            );
                        }
                        TableTool::UserFill(text) => {
                            chrome::section(ui, "채울 목록 (줄마다 하나, 또는 쉼표로 구분)");
                            ui.add(
                                egui::TextEdit::multiline(text)
                                    .desired_rows(6)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.label(
                                RichText::new(
                                    "블록의 첫 셀 값이 목록에 있으면 그다음 항목부터, 없으면 첫 항목부터 차례로 채우고 끝에서 다시 돕니다.",
                                )
                                .small()
                                .weak(),
                            );
                        }
                        TableTool::Style(selected, options) => {
                            chrome::section(ui, "표 마당 옵션");
                            ui.horizontal_wrapped(|ui| {
                                for (i, (_, label, _)) in STYLE_OPTIONS.iter().enumerate() {
                                    ui.checkbox(&mut options[i], *label);
                                }
                                if ui.small_button("옵션 초기화").clicked() {
                                    *options = default_style_options();
                                }
                            });
                            chrome::section(ui, "표 마당 목록");
                            ui.horizontal_wrapped(|ui| {
                                for (i, (_, label)) in STYLES.iter().enumerate() {
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(74.0, 64.0),
                                        egui::Sense::click(),
                                    );
                                    paint_style_preview(ui, rect.shrink(4.0), i);
                                    if *selected == i {
                                        ui.painter().rect_stroke(
                                            rect,
                                            2.0,
                                            Stroke::new(2.0, chrome::Tokens::ACCENT),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    if response.on_hover_text(*label).clicked() {
                                        *selected = i;
                                    }
                                }
                            });
                            ui.label(STYLES[*selected].1);
                            ui.label(
                                RichText::new("테두리와 배경만 바꿉니다(글자 모양은 그대로).")
                                    .small()
                                    .weak(),
                            );
                        }
                    }
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "설정(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply {
            let delimiter = |kind: usize, custom: &str| {
                DELIMITERS
                    .get(kind)
                    .map(|(k, _)| (*k).to_owned())
                    .unwrap_or_else(|| custom.to_owned())
            };
            close = match &tool {
                TableTool::Convert {
                    to_table: true,
                    kind,
                    custom,
                } => self.run(
                    "table.from_text",
                    json!({"delimiter":delimiter(*kind, custom)}),
                ),
                TableTool::Convert { kind, custom, .. } => {
                    self.ext.table.block = None;
                    self.run(
                        "table.to_text",
                        json!({"delimiter":delimiter(*kind, custom)}),
                    )
                }
                TableTool::Sort {
                    column, descending, ..
                } => self.run_table(
                    "table.sort",
                    json!({"column":column,"descending":descending}),
                ),
                TableTool::Caption(text) => {
                    self.run_table("table.caption_text", json!({"text":text}))
                }
                TableTool::Style(i, options) => {
                    let mut p = json!({"preset":STYLES[*i].0});
                    for (k, (key, _, _)) in STYLE_OPTIONS.iter().enumerate() {
                        p[*key] = json!(options[k]);
                    }
                    self.run_table("table.style", p)
                }
                TableTool::UserFill(text) => {
                    let values: Vec<String> = text
                        .split(['\n', ','])
                        .map(|v| v.trim().to_owned())
                        .filter(|v| !v.is_empty())
                        .collect();
                    if values.is_empty() {
                        self.error = Some("채울 목록을 입력하세요.".into());
                        false
                    } else {
                        let mut p = json!({"values":values});
                        if let Some(r) = self.block_range() {
                            p["range"] = r;
                        }
                        let r = self.session.run("table.auto_fill", p);
                        match self.result(r) {
                            Some(v) => {
                                self.message = format!("{}개 셀을 채웠습니다.", v["cells"]);
                                true
                            }
                            None => false,
                        }
                    }
                }
            };
            self.ext.table.layout = None;
        }
        if !(close || r.should_close()) {
            self.ext.table.tool = Some(tool);
        }
    }
}

fn paint_style_preview(ui: &egui::Ui, r: Rect, style: usize) {
    let p = ui.painter();
    let (rows, cols) = (4, 3);
    let (cw, rh) = (r.width() / cols as f32, r.height() / rows as f32);
    let rgb = |c: (u8, u8, u8)| Color32::from_rgb(c.0, c.1, c.2);
    for row in 0..rows {
        let band = Rect::from_min_size(
            r.min + Vec2::new(0.0, rh * row as f32),
            Vec2::new(r.width(), rh),
        );
        let fill = match (style, row) {
            (1, 0) => Some(rgb((219, 229, 241))),
            (2, 0) => Some(rgb((217, 217, 217))),
            (3, 2) => Some(rgb((242, 242, 242))),
            _ => None,
        };
        if let Some(f) = fill {
            p.rect_filled(band, 0.0, f);
        }
    }
    let line = match style {
        1 => Stroke::new(1.0, rgb((149, 179, 215))),
        2 => Stroke::new(1.0, rgb((191, 191, 191))),
        3 => Stroke::new(1.0, rgb((191, 191, 191))),
        _ => Stroke::new(1.0, Color32::BLACK),
    };
    for row in 1..rows {
        let y = r.top() + rh * row as f32;
        p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], line);
    }
    if style != 3 {
        for col in 1..cols {
            let x = r.left() + cw * col as f32;
            p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], line);
        }
    }
    let outer = match style {
        1 => Stroke::new(2.0, rgb((79, 129, 189))),
        2 => Stroke::new(1.0, rgb((128, 128, 128))),
        3 => Stroke::new(2.0, Color32::BLACK),
        _ => Stroke::new(1.0, Color32::BLACK),
    };
    if style == 3 {
        p.line_segment([r.left_top(), r.right_top()], outer);
        p.line_segment([r.left_bottom(), r.right_bottom()], outer);
    } else {
        p.rect_stroke(r, 0.0, outer, egui::StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_layouts_stay_cached_per_page_until_the_document_changes() {
        use hwpcraft_engine::Command;
        let mut s = hwpcraft_engine::Session::new().unwrap();
        s.execute(Command::Insert("첫 쪽".into())).unwrap();
        s.execute(Command::PageBreak).unwrap();
        s.execute(Command::Insert("둘째 쪽".into())).unwrap();
        s.run("table.create", json!({"rows":1,"columns":3}))
            .unwrap();
        s.select(Cursor::default(), false).unwrap();
        s.run("table.create", json!({"rows":2,"columns":2}))
            .unwrap();
        assert!(s.core.page_count() >= 2);
        let mut cache = None;
        let first = cached_page_layout(&mut cache, &s, 0);
        let second = cached_page_layout(&mut cache, &s, 1);
        // Two visible pages alternate every frame; neither evicts the other.
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &cached_page_layout(&mut cache, &s, 0)
        ));
        assert!(std::sync::Arc::ptr_eq(
            &second,
            &cached_page_layout(&mut cache, &s, 1)
        ));
        let tables = |v: &Value| {
            v["controls"]
                .as_array()
                .map_or(0, |c| c.iter().filter(|t| t["type"] == "table").count())
        };
        assert_eq!((tables(&first), tables(&second)), (1, 1));
        // An edit changes the generation; the next lookup rebuilds the layout.
        s.run("table.cell_move", json!({"motion":"next"})).unwrap();
        s.execute(Command::Insert("가".into())).unwrap();
        let rebuilt = cached_page_layout(&mut cache, &s, 1);
        assert!(!std::sync::Arc::ptr_eq(&second, &rebuilt));
        assert_eq!(
            cache.as_ref().map(|(g, pages)| (*g, pages.len())),
            Some((s.generation, 1))
        );
    }
}
