//! Canvas pen/eraser, separate from normal cell selection and border resizing.
use super::*;
use hwpcraft_engine::ObjectAddress;
use serde_json::Value;
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Draw,
    Erase,
}
#[derive(Clone)]
struct Target {
    address: ObjectAddress,
    x_map: Vec<(f32, f64)>,
    y_map: Vec<(f32, f64)>,
    fragmented: bool,
}
#[derive(Default)]
pub(crate) struct State {
    mode: Option<Mode>,
    drag: Option<(u32, Pos2, Option<Target>, u64)>,
    line_type: usize,
    width: usize,
    color: [u8; 3],
    selected: Option<ObjectAddress>,
    size_draft: Option<(u64, f64, f64)>,
    resize: Option<(u32, usize, Pos2, Rect, f64, f64, u64)>,
    moving: Option<(u32, Pos2, Rect, u64)>,
    /// Editable top-level tables per page (address, page-space box, cell count on
    /// that page) for the current document generation. The selection canvas runs
    /// every frame for every visible page; rebuilding the page layout JSON there
    /// cost ~10 ms per page in debug builds on a 381-page sample.
    selection_tables: Option<(u64, std::collections::HashMap<u32, Vec<SelectionTable>>)>,
}
type SelectionTable = (ObjectAddress, Rect, usize);
impl HwpApp {
    pub(crate) fn pen_checked(&self, id: &str) -> bool {
        matches!(
            (id, self.ext.pen.mode),
            ("table.draw", Some(Mode::Draw)) | ("table.eraser", Some(Mode::Erase))
        )
    }
    pub(crate) fn pen_enabled(&self, id: &str) -> Option<bool> {
        if id == "table.select" {
            return Some(self.session.cursor.cell.is_some());
        }
        matches!(id, "table.draw" | "table.eraser").then_some(true)
    }
    pub(crate) fn pen_dispatch(&mut self, id: &str) -> bool {
        if id == "table.select" {
            let c = self.session.cursor;
            if let Some(t) = c.cell {
                self.select_whole_table(ObjectAddress {
                    section: c.section,
                    paragraph: c.paragraph,
                    control: t.control,
                });
            }
            self.ext.pen.mode = None;
            self.ext.pen.resize = None;
            self.ext.pen.moving = None;
            self.ext.pen.size_draft = None;
            self.session.clear_selection();
            return true;
        }
        let mode = match id {
            "table.draw" => Mode::Draw,
            "table.eraser" => Mode::Erase,
            _ => return false,
        };
        self.ext.pen.mode = if self.ext.pen.mode == Some(mode) {
            None
        } else {
            Some(mode)
        };
        self.ext.pen.drag = None;
        self.ext.pen.selected = None;
        self.ext.pen.resize = None;
        self.ext.pen.moving = None;
        self.ext.table = Default::default();
        self.ext.object = Default::default();
        self.session.clear_selection();
        true
    }
    pub(crate) fn pen_keys(&mut self, ctx: &egui::Context) -> bool {
        if (self.ext.pen.mode.is_some() || self.ext.pen.selected.is_some())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            self.ext.pen.mode = None;
            self.ext.pen.drag = None;
            self.ext.pen.selected = None;
            self.ext.pen.resize = None;
            self.ext.pen.moving = None;
            return true;
        }
        false
    }
    pub(crate) fn pen_dialog(&mut self, ctx: &egui::Context) {
        self.table_selection_tools(ctx);
        let Some(mode) = self.ext.pen.mode else {
            return;
        };
        let mut close = false;
        egui::Window::new(if mode == Mode::Draw {
            "표 그리기"
        } else {
            "표 지우개"
        })
        .id(egui::Id::new("table-pen-tools"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -70.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(mode == Mode::Draw, "그리기").clicked() {
                    self.ext.pen.mode = Some(Mode::Draw);
                }
                if ui.selectable_label(mode == Mode::Erase, "지우개").clicked() {
                    self.ext.pen.mode = Some(Mode::Erase);
                }
                ui.separator();
                let labels = ["실선", "긴 점선", "점선", "일점 쇄선", "이중 실선"];
                egui::ComboBox::from_id_salt("pen-line")
                    .selected_text(labels[self.ext.pen.line_type])
                    .width(85.0)
                    .show_ui(ui, |ui| {
                        for (i, l) in labels.iter().enumerate() {
                            ui.selectable_value(&mut self.ext.pen.line_type, i, *l);
                        }
                    });
                let widths = [
                    "0.1 mm", "0.12 mm", "0.15 mm", "0.2 mm", "0.25 mm", "0.3 mm", "0.4 mm",
                    "0.5 mm", "0.6 mm", "0.7 mm", "1 mm",
                ];
                egui::ComboBox::from_id_salt("pen-width")
                    .selected_text(widths[self.ext.pen.width])
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for (i, l) in widths.iter().enumerate() {
                            ui.selectable_value(&mut self.ext.pen.width, i, *l);
                        }
                    });
                ui.color_edit_button_srgb(&mut self.ext.pen.color);
                close = ui.button("끝내기 (Esc)").clicked();
            });
            ui.label(if mode == Mode::Draw {
                "빈 곳에서 외곽을 그리고, 셀 테두리 사이를 이어 줄·칸·대각선을 만드세요."
            } else {
                "지울 선을 따라 끄세요. 공유 경계는 합치고 외곽선은 숨깁니다."
            });
        });
        if close {
            self.ext.pen.mode = None;
            self.ext.pen.drag = None;
        }
    }
    fn pen_target(&self, page: u32, p: Pos2) -> Option<Target> {
        let raw = self.session.page_control_layout(page).ok()?;
        let layout: Value = serde_json::from_str(&raw).ok()?;
        for t in layout["controls"].as_array()?.iter().rev() {
            if t["type"] != "table" {
                continue;
            }
            let f = |k: &str| t[k].as_f64().unwrap_or(0.0) as f32;
            let rect = Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")));
            if !rect.expand(6.0 / self.zoom).contains(p) {
                continue;
            }
            let address = ObjectAddress {
                section: t["secIdx"].as_u64()? as usize,
                paragraph: t["paraIdx"].as_u64()? as usize,
                control: t["controlIdx"].as_u64()? as usize,
            };
            let info = self.session.table_layout(address, None).ok()?;
            let axis_map =
                |sizes: &str, coordinate: &str, dimension: &str, index: &str, span: &str| {
                    let mut edges = vec![0.0];
                    for v in info[sizes].as_array().into_iter().flatten() {
                        edges.push(
                            edges.last().copied().unwrap_or(0.0)
                                + v.as_f64().unwrap_or(0.0) * 25.4 / 7200.0,
                        );
                    }
                    let mut points = Vec::new();
                    for c in t["cells"].as_array().into_iter().flatten() {
                        let start = c[index].as_u64().unwrap_or(0) as usize;
                        let end = start + c[span].as_u64().unwrap_or(1) as usize;
                        if let (Some(a), Some(b)) = (edges.get(start), edges.get(end)) {
                            let x = c[coordinate].as_f64().unwrap_or(0.0) as f32;
                            points.push((x, *a));
                            points.push((x + c[dimension].as_f64().unwrap_or(0.0) as f32, *b));
                        }
                    }
                    points.sort_by(|a, b| a.0.total_cmp(&b.0));
                    points.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01);
                    points
                };
            let fragmented =
                t["cells"].as_array().map(Vec::len) != info["cells"].as_array().map(Vec::len);
            return Some(Target {
                address,
                x_map: axis_map("columnWidths", "x", "w", "col", "colSpan"),
                y_map: axis_map("rowHeights", "y", "h", "row", "rowSpan"),
                fragmented,
            });
        }
        None
    }
    pub(crate) fn pen_canvas(&mut self, page: u32, response: &egui::Response, rect: Rect) -> bool {
        let Some(mode) = self.ext.pen.mode else {
            return self.table_selection_canvas(page, response, rect);
        };
        if self.ext_modal_open() {
            return true;
        }
        let to_page = |p: Pos2| (p - rect.min) / self.zoom;
        if response.hovered() {
            response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if let Some(p) = page_press(response) {
            let p = to_page(p);
            let start = Pos2::new(p.x, p.y);
            let target = self.pen_target(page, start);
            self.ext.pen.drag = Some((page, start, target, self.session.generation));
        }
        if let Some((owner, start, target, generation)) = self.ext.pen.drag.clone() {
            if owner != page {
                return true;
            }
            if generation != self.session.generation {
                self.ext.pen.drag = None;
                return true;
            }
            let Some(p) = response.ctx.input(|i| i.pointer.interact_pos()) else {
                return true;
            };
            let local = to_page(p);
            let end = Pos2::new(local.x, local.y);
            let painter = response.ctx.layer_painter(response.layer_id);
            let from = rect.min + start.to_vec2() * self.zoom;
            let stroke = Stroke::new(
                if mode == Mode::Erase { 4.0 } else { 1.5 },
                if mode == Mode::Erase {
                    Color32::RED
                } else {
                    chrome::Tokens::ACCENT
                },
            );
            if target.is_some() {
                painter.line_segment([from, p], stroke);
            } else {
                painter.rect_stroke(
                    Rect::from_two_pos(from, p),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            if page_release(response) {
                self.ext.pen.drag = None;
                if start.distance(end) < 4.0 {
                    return true;
                }
                if let Some(t) = target {
                    if t.fragmented {
                        self.error = Some("쪽에 나뉜 표는 셀 블록·셀 나누기를 이용하세요.".into());
                        return true;
                    }
                    let coords = |p: Pos2| {
                        [
                            axis_coordinate(&t.x_map, p.x),
                            axis_coordinate(&t.y_map, p.y),
                        ]
                    };
                    let c = self.ext.pen.color;
                    let kinds = [1, 2, 3, 4, 8];
                    self.run("table.pen",json!({"table":t.address,"start":coords(start),"end":coords(end),"erase":mode==Mode::Erase,"line":{"type":kinds[self.ext.pen.line_type],"width":self.ext.pen.width,"color":format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2])}}));
                } else if mode == Mode::Draw {
                    let bounds = Rect::from_two_pos(start, end);
                    if bounds.left() < 0.0
                        || bounds.top() < 0.0
                        || bounds.right() > rect.width() / self.zoom
                        || bounds.bottom() > rect.height() / self.zoom
                    {
                        self.error = Some("쪽 안에서 표를 그리세요.".into());
                        return true;
                    }
                    let hit = self.session.hit_test(page, start.x as f64, start.y as f64);
                    if let Some(c) = self.result(hit) {
                        let selected = self.session.select(c, false);
                        if self.result(selected).is_some() {
                            self.run("table.draw_create",json!({"x_mm":bounds.left()*25.4/96.0,"y_mm":bounds.top()*25.4/96.0,"width_mm":bounds.width()*25.4/96.0,"height_mm":bounds.height()*25.4/96.0,"line":{"type":([1,2,3,4,8][self.ext.pen.line_type]),"width":self.ext.pen.width,"color":format!("#{:02x}{:02x}{:02x}",self.ext.pen.color[0],self.ext.pen.color[1],self.ext.pen.color[2])}}));
                        }
                    }
                }
                self.ext.table = Default::default();
            }
        } else if response.clicked() {
            let target = response.interact_pointer_pos().and_then(|p| {
                let p = to_page(p);
                self.pen_target(page, Pos2::new(p.x, p.y))
            });
            if target.is_none() {
                self.ext.pen.mode = None;
            }
            return false; // a click still places a text caret, as in HWP.
        }
        true
    }
}

/// Size keys the selection panel must send: only dimensions the user changed.
/// Re-sending an unchanged size redistributed row heights (282 → 283 HWPUNIT)
/// and added an undo step for a no-op.
fn changed_table_size(
    props: &Value,
    width_mm: f64,
    height_mm: f64,
) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for (key, mm) in [("tableWidth", width_mm), ("tableHeight", height_mm)] {
        let hu = (mm * 7200.0 / 25.4).round();
        if (hu - props[key].as_f64().unwrap_or(0.0)).abs() >= 1.0 {
            out.insert(key.into(), json!(hu as u32));
        }
    }
    out
}

impl HwpApp {
    fn table_selection_tools(&mut self, ctx: &egui::Context) {
        let Some(a) = self.ext.pen.selected else {
            return;
        };
        if self.ext_modal_open() {
            return;
        }
        let Ok(props) = self.session.table_properties(a) else {
            self.ext.pen.selected = None;
            return;
        };
        let mut w = props["tableWidth"].as_f64().unwrap_or(0.0) * 25.4 / 7200.0;
        let mut h = props["tableHeight"].as_f64().unwrap_or(0.0) * 25.4 / 7200.0;
        if let Some((g, dw, dh)) = self.ext.pen.size_draft
            && g == self.session.generation
        {
            w = dw;
            h = dh;
        }
        let mut apply = false;
        let mut action = None;
        let mut close = false;
        egui::Window::new("표 선택 · 크기와 스타일")
            .id(egui::Id::new("table-selection-tools"))
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -70.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("너비");
                    ui.add(
                        egui::DragValue::new(&mut w)
                            .range(1.0..=600.0)
                            .speed(0.1)
                            .fixed_decimals(2)
                            .suffix(" mm"),
                    );
                    ui.label("높이");
                    ui.add(
                        egui::DragValue::new(&mut h)
                            .range(1.0..=600.0)
                            .speed(0.1)
                            .fixed_decimals(2)
                            .suffix(" mm"),
                    );
                    apply = ui.button("적용").clicked();
                    for (id, label) in [
                        ("table.properties", "표/셀 속성"),
                        ("table.cell_border_fill", "테두리/배경"),
                        ("table.style", "표 마당"),
                    ] {
                        if ui.button(label).clicked() {
                            action = Some(id);
                        }
                    }
                    close = ui.button("선택 해제").clicked();
                });
                ui.label(
                    "왼쪽 위 십자 손잡이: 선택·이동 / 파란 사각 손잡이: 크기 / Shift: 비율 유지",
                );
            });
        self.ext.pen.size_draft = Some((self.session.generation, w, h));
        if apply {
            let changed = changed_table_size(&props, w, h);
            if changed.is_empty() {
                self.message = "바뀐 크기가 없습니다.".into();
            } else {
                self.run(
                    "table.properties",
                    json!({"table":a,"props":Value::Object(changed)}),
                );
            }
        }
        if let Some(id) = action {
            self.select_whole_table(a);
            self.ext_dispatch(id, ctx);
        }
        if close {
            self.ext.pen.selected = None;
        }
    }
    fn select_whole_table(&mut self, a: ObjectAddress) {
        if let Some((c, _)) = self.session.entries().into_iter().find(|(c, _)| {
            c.section == a.section
                && c.paragraph == a.paragraph
                && c.cell.is_some_and(|cell| cell.control == a.control)
        }) {
            let result = self.session.select(c, false);
            self.result(result);
        }
        self.ext.table = Default::default();
        self.ext.object = Default::default();
        self.ext.pen.selected = Some(a);
        self.ext.pen.size_draft = None;
        self.session.clear_selection();
    }
    /// Cached editable tables on `page` (page-space boxes); rebuilt after edits.
    fn selection_tables(&mut self, page: u32) -> Vec<SelectionTable> {
        let generation = self.session.generation;
        if self
            .ext
            .pen
            .selection_tables
            .as_ref()
            .is_none_or(|(g, _)| *g != generation)
        {
            self.ext.pen.selection_tables = Some((generation, Default::default()));
        }
        if let Some((_, pages)) = &self.ext.pen.selection_tables
            && let Some(list) = pages.get(&page)
        {
            return list.clone();
        }
        let layout = self.page_layout(page);
        let list: Vec<SelectionTable> = layout["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| {
                if t["type"] != "table" || t.get("headerFooter").is_some() {
                    return None;
                }
                let a = ObjectAddress {
                    section: t["secIdx"].as_u64()? as usize,
                    paragraph: t["paraIdx"].as_u64()? as usize,
                    control: t["controlIdx"].as_u64()? as usize,
                };
                // Nested controls do not share the top-level editing address space.
                self.session.table_layout(a, None).ok()?;
                let f = |k: &str| t[k].as_f64().unwrap_or(0.0) as f32;
                Some((
                    a,
                    Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h"))),
                    t["cells"].as_array().map_or(0, Vec::len),
                ))
            })
            .collect();
        if let Some((_, pages)) = self.ext.pen.selection_tables.as_mut() {
            pages.insert(page, list.clone());
        }
        list
    }
    fn table_selection_canvas(
        &mut self,
        page: u32,
        response: &egui::Response,
        page_rect: Rect,
    ) -> bool {
        if self.ext_modal_open() {
            return false;
        }
        let pointer = response.ctx.input(|i| i.pointer.interact_pos());
        let origin = page_press(response);
        let painter = response.ctx.layer_painter(response.layer_id);
        let zoom = self.zoom;
        let controls: Vec<SelectionTable> = self
            .selection_tables(page)
            .into_iter()
            .map(|(a, r, cells)| {
                (
                    a,
                    Rect::from_min_size(page_rect.min + r.min.to_vec2() * zoom, r.size() * zoom),
                    cells,
                )
            })
            .collect();
        for (a, rect, _) in controls.iter().rev() {
            let grip = table_move_grip(*rect);
            let current = self.session.cursor;
            let active = self.ext.pen.selected == Some(*a);
            let visible = active
                || (current.section == a.section
                    && current.paragraph == a.paragraph
                    && current.cell.is_some_and(|c| c.control == a.control))
                || pointer.is_some_and(|p| rect.expand(24.0).contains(p));
            if !visible {
                continue;
            }
            painter.rect_filled(grip, 2.0, Color32::WHITE);
            painter.rect_stroke(
                grip,
                2.0,
                Stroke::new(1.0, chrome::Tokens::ACCENT),
                egui::StrokeKind::Inside,
            );
            let c = grip.center();
            let col = Stroke::new(1.3, chrome::Tokens::ACCENT);
            for direction in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                painter.arrow(c, direction * 6.0, col);
            }
            if response.hovered() && pointer.is_some_and(|p| grip.contains(p)) {
                response.ctx.set_cursor_icon(egui::CursorIcon::Move);
            }
            if response.clicked() && pointer.is_some_and(|p| grip.contains(p)) {
                self.select_whole_table(*a);
                return true;
            }
            if origin.is_some_and(|p| grip.contains(p)) {
                self.select_whole_table(*a);
                self.ext.pen.moving = Some((page, origin.unwrap(), *rect, self.session.generation));
                break;
            }
            // A click selects the outside border; dragging an unselected grid line still resizes cells.
            if !active && response.clicked() && pointer.is_some_and(|p| on_table_outline(*rect, p))
            {
                self.select_whole_table(*a);
                return true;
            }
        }
        let Some(a) = self.ext.pen.selected else {
            return false;
        };
        let Some((_, rect, page_cells)) = controls.iter().find(|(address, _, _)| *address == a)
        else {
            if response.clicked() {
                self.ext.pen.selected = None;
            }
            return false;
        };
        let rect = *rect;
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.5, chrome::Tokens::ACCENT),
            egui::StrokeKind::Outside,
        );
        let handles = [
            rect.right_center(),
            rect.center_bottom(),
            rect.right_bottom(),
        ];
        let hovered = pointer
            .filter(|_| response.hovered())
            .and_then(|p| handles.iter().position(|h| h.distance(p) < 8.0));
        for h in handles {
            painter.rect_filled(
                Rect::from_center_size(h, Vec2::splat(8.0)),
                1.0,
                chrome::Tokens::ACCENT,
            );
        }
        if let Some(i) = hovered {
            response.ctx.set_cursor_icon(if i == 0 {
                egui::CursorIcon::ResizeHorizontal
            } else if i == 1 {
                egui::CursorIcon::ResizeVertical
            } else {
                egui::CursorIcon::ResizeNwSe
            });
        }
        if self.ext.pen.moving.is_none()
            && let Some(start) = origin
        {
            if let Some(i) = handles.iter().position(|h| h.distance(start) < 8.0) {
                let Ok(info) = self.session.table_layout(a, None) else {
                    return true;
                };
                if Some(*page_cells) != info["cells"].as_array().map(Vec::len) {
                    self.error = Some("쪽에 나뉜 표의 크기는 표/셀 속성에서 조절하세요.".into());
                    return true;
                }
                if let Ok(props) = self.session.table_properties(a) {
                    self.ext.pen.resize = Some((
                        page,
                        i,
                        start,
                        rect,
                        props["tableWidth"].as_f64().unwrap_or(0.0),
                        props["tableHeight"].as_f64().unwrap_or(0.0),
                        self.session.generation,
                    ));
                }
            } else if on_table_outline(rect, start) {
                self.ext.pen.moving = Some((page, start, rect, self.session.generation));
            } else {
                self.ext.pen.selected = None;
                return false;
            }
        }
        if let Some((owner, start, original, generation)) = self.ext.pen.moving {
            if owner != page {
                return true;
            }
            if generation != self.session.generation {
                self.ext.pen.moving = None;
                return true;
            }
            let Some(p) = pointer else { return true };
            let delta = p - start;
            let min = Pos2::new(
                (original.left() + delta.x).clamp(
                    page_rect.left(),
                    (page_rect.right() - original.width()).max(page_rect.left()),
                ),
                (original.top() + delta.y).clamp(
                    page_rect.top(),
                    (page_rect.bottom() - original.height()).max(page_rect.top()),
                ),
            );
            let delta = min - original.min;
            painter.rect_stroke(
                original.translate(delta),
                0.0,
                Stroke::new(2.0, chrome::Tokens::ACCENT),
                egui::StrokeKind::Outside,
            );
            response.ctx.set_cursor_icon(egui::CursorIcon::Move);
            if page_release(response) {
                self.ext.pen.moving = None;
                if delta.length() > 1.0 {
                    self.run("table.move",json!({"table":a,"page":page,"dx_mm":delta.x/self.zoom*25.4/96.0,"dy_mm":delta.y/self.zoom*25.4/96.0}));
                }
            }
            return true;
        }
        if let Some((owner, i, start, original, w, h, generation)) = self.ext.pen.resize {
            if owner != page {
                return true;
            }
            if generation != self.session.generation {
                self.ext.pen.resize = None;
                return true;
            }
            let Some(p) = pointer else { return true };
            let delta = p - start;
            let mut sx = if i == 1 {
                1.0
            } else {
                ((original.width() + delta.x) / original.width()).max(0.02)
            };
            let mut sy = if i == 0 {
                1.0
            } else {
                ((original.height() + delta.y) / original.height()).max(0.02)
            };
            if response.ctx.input(|i| i.modifiers.shift) {
                let scale = if i == 1 { sy } else { sx };
                sx = scale;
                sy = scale;
            }
            let preview = Rect::from_min_size(original.min, original.size() * Vec2::new(sx, sy));
            painter.rect_stroke(
                preview,
                0.0,
                Stroke::new(2.0, chrome::Tokens::ACCENT),
                egui::StrokeKind::Outside,
            );
            if page_release(response) {
                self.ext.pen.resize = None;
                if delta.length() > 1.0 {
                    self.run("table.properties",json!({"table":a,"props":{"tableWidth":(w*f64::from(sx)).round()as u32,"tableHeight":(h*f64::from(sy)).round()as u32}}));
                }
            }
            return true;
        }
        if response.clicked() && hovered.is_none() {
            self.ext.pen.selected = None;
        }
        hovered.is_some()
    }
}

// Read button events directly: native eframe can coalesce press+motion before the
// widget has acquired egui's drag ID. Ownership is checked against the clipped page.
pub(crate) fn page_press(response: &egui::Response) -> Option<Pos2> {
    let start = response.ctx.input(|i| {
        i.events.iter().find_map(|event| match event {
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } => Some(*pos),
            _ => None,
        })
    })?;
    (response.interact_rect.contains(start)
        && response.ctx.layer_id_at(start) == Some(response.layer_id))
    .then_some(start)
}
pub(crate) fn page_release(response: &egui::Response) -> bool {
    response.ctx.input(|i| {
        i.events.iter().any(|event| {
            matches!(
                event,
                Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                }
            )
        })
    })
}

fn table_move_grip(rect: Rect) -> Rect {
    // Leave the document's top-left margin corner visible beside an inline table.
    Rect::from_center_size(rect.left_top() - Vec2::new(32.0, 11.0), Vec2::splat(18.0))
}
fn on_table_outline(rect: Rect, p: Pos2) -> bool {
    rect.expand(3.0).contains(p) && !rect.shrink(3.0).contains(p)
}

// Piecewise mapping preserves real cell boundaries when content expands some rows.
fn axis_coordinate(points: &[(f32, f64)], p: f32) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let i = points
        .partition_point(|(x, _)| *x < p)
        .clamp(1, points.len() - 1);
    let (a, b) = (points[i - 1], points[i]);
    if (b.0 - a.0).abs() < 0.001 {
        return a.1;
    }
    a.1 + f64::from((p - a.0) / (b.0 - a.0)) * (b.1 - a.1)
}
#[cfg(test)]
mod tests {
    #[test]
    fn selection_panel_sends_only_changed_table_dimensions() {
        let props = json!({"tableWidth":42520,"tableHeight":1128});
        let mm = |hu: f64| hu * 25.4 / 7200.0;
        // Unchanged values (as displayed, rounded to 0.01 mm) send nothing.
        assert!(changed_table_size(&props, mm(42520.0), mm(1128.0)).is_empty());
        assert!(changed_table_size(&props, 150.0, 3.98).is_empty());
        let only_width = changed_table_size(&props, 120.0, mm(1128.0));
        assert_eq!(Value::Object(only_width), json!({"tableWidth":34016}));
        let both = changed_table_size(&props, 120.0, 20.0);
        assert_eq!(
            Value::Object(both),
            json!({"tableWidth":34016,"tableHeight":5669})
        );
    }

    use super::*;

    fn fixture() -> HwpApp {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.session
            .run(
                "table.draw_create",
                json!({"width_mm":100,"height_mm":50,"x_mm":30,"y_mm":40}),
            )
            .unwrap();
        app
    }
    fn bounds(app: &HwpApp) -> Rect {
        let v: Value = serde_json::from_str(&app.session.page_control_layout(0).unwrap()).unwrap();
        let t = v["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["type"] == "table")
            .unwrap();
        Rect::from_min_size(
            Pos2::new(50.0, 50.0)
                + Vec2::new(
                    t["x"].as_f64().unwrap() as f32,
                    t["y"].as_f64().unwrap() as f32,
                ) * app.zoom,
            Vec2::new(
                t["w"].as_f64().unwrap() as f32,
                t["h"].as_f64().unwrap() as f32,
            ) * app.zoom,
        )
    }
    fn frame(app: &mut HwpApp, events: Vec<Event>) {
        let ctx = app.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 1400.0))),
                events,
                ..Default::default()
            },
            |ui| {
                let rect =
                    Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(794.0, 1123.0) * app.zoom);
                let response = ui.interact(
                    rect,
                    egui::Id::new("page-test"),
                    egui::Sense::click_and_drag(),
                );
                app.ext_canvas(0, &response, rect);
            },
        );
        output.textures_delta.clear();
    }
    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn drag(app: &mut HwpApp, start: Pos2, end: Pos2) {
        frame(app, vec![]);
        frame(app, vec![Event::PointerMoved(start), button(start, true)]);
        // Jump beyond the drag threshold in one frame, as a fast real pointer does.
        frame(app, vec![Event::PointerMoved(end)]);
        frame(app, vec![button(end, false)]);
        assert!(app.error.is_none(), "{:?}", app.error);
    }

    #[test]
    fn native_coalesced_press_and_motion_resize_cell_without_drag_id() {
        let mut app = fixture();
        let a = app.session.run("table.info", json!({})).unwrap()["table"].clone();
        app.session
            .run("table.pen", json!({"table":a,"start":[50,0],"end":[50,50]}))
            .unwrap();
        let start = bounds(&app).center();
        let end = start + Vec2::new(30.0, 0.0);
        let before = app.session.run("table.info", json!({})).unwrap();
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![Event::PointerMoved(Pos2::new(1000.0, 1300.0))],
        );
        frame(
            &mut app,
            vec![button(start, true), Event::PointerMoved(end)],
        );
        frame(&mut app, vec![button(end, false)]);
        assert!(app.error.is_none(), "{:?}", app.error);
        let after = app.session.run("table.info", json!({})).unwrap();
        assert!(
            after["columnWidths"][0].as_u64().unwrap()
                > before["columnWidths"][0].as_u64().unwrap()
        );
    }
    #[test]
    fn short_drag_batched_in_one_frame() {
        let mut app = fixture();
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        let before = bounds(&app);
        let start = table_move_grip(before).center();
        let end = start + Vec2::new(40.0, 50.0);
        frame(
            &mut app,
            vec![
                Event::PointerMoved(start),
                button(start, true),
                Event::PointerMoved(end),
                button(end, false),
            ],
        );
        assert!((bounds(&app).min - before.min - Vec2::new(40.0, 50.0)).length() < 0.5);
    }
    #[test]
    fn document_view_grip_drag_works_with_scroll_area_and_focus_target() {
        let mut app = fixture();
        fn full_frame(app: &mut HwpApp, events: Vec<Event>) {
            let ctx = app.ctx.clone();
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 916.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.sync_formatting();
                    app.shortcuts(&ctx);
                    app.toolbar(ui);
                    app.status_bar(ui);
                    app.page_view(ui);
                    app.keyboard(&ctx);
                    app.dialogs(&ctx);
                },
            );
            out.textures_delta.clear();
        }
        full_frame(&mut app, vec![]);
        full_frame(&mut app, vec![]);
        let page = app.view_page_rect.unwrap();
        let r = bounds(&app).translate(page.min - Pos2::new(50.0, 50.0));
        let start = table_move_grip(r).center();
        let end = start + Vec2::new(35.0, 45.0);
        let before = bounds(&app);
        full_frame(
            &mut app,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        full_frame(&mut app, vec![Event::PointerMoved(end)]);
        full_frame(&mut app, vec![button(end, false)]);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            (bounds(&app).min - before.min - Vec2::new(35.0, 45.0)).length() < 0.5,
            "{:?} -> {:?}, selected={:?}",
            before,
            bounds(&app),
            app.ext.pen.selected
        );
    }
    #[test]
    fn pointer_grip_selects_moves_and_resizes_in_single_undo_steps() {
        let mut app = fixture();
        let before = bounds(&app);
        let grip = table_move_grip(before).center();
        drag(&mut app, grip, grip + Vec2::new(45.0, 30.0));
        assert!(app.ext.pen.selected.is_some());
        let moved = bounds(&app);
        assert!((moved.min - before.min - Vec2::new(45.0, 30.0)).length() < 0.5);
        drag(
            &mut app,
            moved.right_bottom(),
            moved.right_bottom() + Vec2::new(60.0, 40.0),
        );
        let resized = bounds(&app);
        assert!(
            (resized.size() - moved.size() - Vec2::new(60.0, 40.0)).length() < 0.5,
            "{moved:?} -> {resized:?}"
        );
        app.session.execute(Command::Undo).unwrap();
        assert_eq!(bounds(&app), moved);
        app.session.execute(Command::Undo).unwrap();
        assert_eq!(bounds(&app), before);
    }
    #[test]
    fn fast_drag_on_unselected_cell_boundary_uses_press_origin() {
        let mut app = fixture();
        let a = app.session.run("table.info", json!({})).unwrap()["table"].clone();
        app.session
            .run("table.pen", json!({"table":a,"start":[50,0],"end":[50,50]}))
            .unwrap();
        let rect = bounds(&app);
        let start = rect.center();
        let before = app.session.run("table.info", json!({})).unwrap();
        drag(&mut app, start, start + Vec2::new(30.0, 0.0));
        let after = app.session.run("table.info", json!({})).unwrap();
        assert!(
            after["columnWidths"][0].as_u64().unwrap()
                > before["columnWidths"][0].as_u64().unwrap()
        );
        assert_eq!(bounds(&app).width(), rect.width());
        app.session.execute(Command::Undo).unwrap();
        assert_eq!(
            app.session.run("table.info", json!({})).unwrap()["columnWidths"],
            before["columnWidths"]
        );
    }
    #[test]
    fn content_expanded_row_does_not_move_other_pen_boundaries() {
        let map = [(100.0, 0.0), (180.0, 10.0), (200.0, 20.0)];
        assert_eq!(axis_coordinate(&map, 180.0), 10.0);
        assert_eq!(axis_coordinate(&map, 190.0), 15.0);
        assert_eq!(axis_coordinate(&map, 140.0), 5.0);
    }
}
