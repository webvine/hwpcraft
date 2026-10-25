//! Owner: table/object area. 개체 대화상자·개체 도구.
//! Other areas must not edit this file; request changes through the owner.
//!
//! 개체 선택(클릭)·이동(끌기)·지우기, 도형 그리기(끌기), 그림 넣기, 개체 속성 창.
//! The properties dialog follows Hangul 2024's tabs: 기본 · 여백/캡션 · 선 · 채우기 ·
//! 그림 · 글상자 · 그림자. Drafts hold RHWP property JSON; only changed keys are sent.
use super::*;
use hwpcraft_engine::{EquationOptions, ObjectAddress};
use serde_json::{Map, Value};

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
    layout: Option<(u64, u32, Value)>,
    draw: Option<&'static str>,
    drag: Option<(u32, Pos2)>,
    moving: Option<Vec2>,
    /// Polygon vertices / freehand samples while drawing (page pixels) and their page.
    points: Vec<Pos2>,
    points_page: u32,
    /// Handle being dragged and the drag start (page pixels).
    resizing: Option<(usize, Pos2)>,
    picture: Option<PictureInsert>,
    properties: Option<Box<Properties>>,
    equation: Option<Box<EquationEditor>>,
    chart: Option<Box<ChartEditor>>,
    memos: bool,
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
        if let Some(kind) = id.strip_prefix("insert.shape.") {
            return SHAPES.iter().any(|(k, _)| *k == kind).then_some(body);
        }
        match id {
            "insert.picture_dialog" | "object.list" | "object.properties" | "memo.list" => {
                Some(true)
            }
            "chart.edit" => Some(
                self.session
                    .core
                    .list_charts_native()
                    .ok()
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                    .unwrap_or(false),
            ),
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
            | "object.caption" => Some(selected),
            _ => None,
        }
    }
    /// Return true when the command id was handled.
    pub(crate) fn object_ext_dispatch(&mut self, id: &str, _ctx: &egui::Context) -> bool {
        if let Some(kind) = id.strip_prefix("insert.shape.") {
            if let Some((k, label)) = SHAPES.iter().find(|(k, _)| *k == kind) {
                self.ext.object.draw = Some(k);
                self.ext.object.selected = None;
                self.message =
                    format!("{label}: 쪽에서 끌어 그리세요. 클릭만 하면 기본 크기, Esc 취소.");
                return true;
            }
            return false;
        }
        match id {
            "chart.edit" => self.open_chart_editor(0),
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
                if let Some(s) = self.ext.object.selected.take() {
                    self.run("object.delete", json!(s.address));
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
                self.ext.object.drag = Some((page, to_page(self, p)));
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
                let line = kind == "line";
                let params = json!({"kind":kind,
                    "width_mm":(r.width() * MM_PER_PX).max(if line {0.2} else {1.0}),
                    "height_mm":(r.height() * MM_PER_PX).max(if line {0.2} else {1.0}),
                    "x_mm":r.min.x * MM_PER_PX,"y_mm":r.min.y * MM_PER_PX,
                    "paragraphs": if kind == "textbox" {json!([""])} else {json!([])}});
                let result = self.session.run("shape.insert", params);
                if let Some(v) = self.result(result) {
                    self.ext.object.layout = None;
                    if let Ok(a) = serde_json::from_value::<ObjectAddress>(v["address"].clone()) {
                        self.ext.object.selected = Some(Selected {
                            address: a,
                            page,
                            bbox: r,
                            inline: false,
                        });
                    }
                    self.message = "도형을 넣었습니다. 두 번 클릭하면 개체 속성을 엽니다.".into();
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
        let mut consumed = false;
        // Hover feedback: resize/rotate cursors over the handles of the selection.
        if let (Some(s), Some(p)) = (selected, hover)
            && let Some(h) = handle_at(s.bbox, p, self.zoom)
        {
            response.ctx.set_cursor_icon(handle_cursor(h));
        }
        if response.drag_started() {
            self.ext.object.moving = None;
            self.ext.object.resizing = None;
            if let (Some(s), Some(p)) = (selected, pointer) {
                if let Some(h) = handle_at(s.bbox, p, self.zoom) {
                    self.ext.object.resizing = Some((h, p));
                    consumed = true;
                } else if hit.is_some_and(|h| h.address == s.address) && !s.inline {
                    self.ext.object.moving = Some(Vec2::ZERO);
                    consumed = true;
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
            self.ext.object.selected = hit;
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
        if let Some(s) = self.ext.object.selected.filter(|s| s.page == page) {
            let to_screen = |r: Rect| {
                Rect::from_min_max(
                    rect.min + r.min.to_vec2() * self.zoom,
                    rect.min + r.max.to_vec2() * self.zoom,
                )
            };
            let mut r = to_screen(preview.unwrap_or(s.bbox));
            if let Some(d) = self.ext.object.moving {
                r = r.translate(d * self.zoom);
            }
            let stroke = Stroke::new(1.0, chrome::Tokens::ACCENT);
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
                self.ext.object.selected = Some(Selected {
                    address: a,
                    page,
                    bbox,
                    inline: false,
                });
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
    fn object_at(&mut self, page: u32, p: Pos2) -> Option<Selected> {
        let generation = self.session.generation;
        if self
            .ext
            .object
            .layout
            .as_ref()
            .is_none_or(|(g, pg, _)| *g != generation || *pg != page)
        {
            let layout = self
                .session
                .core
                .get_page_control_layout_native(page)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            self.ext.object.layout = Some((generation, page, layout));
        }
        let (_, _, layout) = self.ext.object.layout.as_ref()?;
        let n = |v: &Value, k: &str| v[k].as_u64().map(|x| x as usize);
        layout["controls"]
            .as_array()?
            .iter()
            .rev()
            .filter(|c| {
                matches!(
                    c["type"].as_str(),
                    Some("image" | "shape" | "line" | "group" | "equation")
                ) && c.get("cellIdx").is_none()
                    && c.get("noteRef").is_none()
            })
            .find_map(|c| {
                let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
                let bbox =
                    Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")));
                if !bbox.expand(3.0).contains(p) {
                    return None;
                }
                let address = ObjectAddress {
                    section: n(c, "secIdx")?,
                    paragraph: n(c, "paraIdx")?,
                    control: n(c, "controlIdx")?,
                };
                let inline = self
                    .session
                    .object_properties(address)
                    .ok()
                    .is_some_and(|v| v["props"]["treatAsChar"] == true);
                Some(Selected {
                    address,
                    page,
                    bbox,
                    inline,
                })
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
            self.ext.object.selected = None;
            return true;
        }
        if consume(egui::Modifiers::NONE, Key::Delete)
            || consume(egui::Modifiers::NONE, Key::Backspace)
        {
            self.ext.object.selected = None;
            self.run("object.delete", json!(s.address));
            return true;
        }
        if consume(egui::Modifiers::NONE, Key::Enter) {
            self.open_object_editor(s.address);
            return true;
        }
        if !s.inline {
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
            || self.ext.object.memos
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
                    .core
                    .get_page_control_layout_native(0)
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
        self.memo_list_dialog(ctx);
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
            self.message =
                "문서에 차트가 없습니다. 새 차트 만들기는 아직 지원하지 않습니다.".into();
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
            error: None,
        }));
    }

    fn chart_editor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.object.chart.take() else {
            return;
        };
        let (mut apply, mut close, mut switch) = (false, false, None);
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
                            "값과 계열 이름만 고칩니다. 행·계열 추가와 차트 종류 변경은 아직 지원하지 않습니다.",
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
        if let Some(i) = switch {
            self.open_chart_editor(i);
            return;
        }
        if apply {
            let series: Vec<Value> = d
                .names
                .iter()
                .zip(&d.values)
                .map(|(n, v)| json!({"name":n,"values":v}))
                .collect();
            match self
                .session
                .run("chart.data", json!({"index":d.index,"series":series}))
            {
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

#[cfg(test)]
mod tests {
    use super::*;

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
