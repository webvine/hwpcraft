//! Pictures, shapes and text boxes inside the header or footer being edited on the page
//! (머리말·꼬리말 편집): as in Hangul they are picked only there. A press selects one,
//! dragging moves it, its handles resize or rotate it, a double click or Enter opens
//! 개체 속성 and Delete removes it; each change is one undo step. The engine runs the
//! body's object commands on them through addresses with `header_footer`
//! (`Session::hf_layout_address`). A table is picked by its frame (a press inside it
//! is for its cells); the ribbon's picture, shape and table tabs act on what is picked
//! (`hf_object_ribbon`, `hf_in_table`).
use super::*;
use crate::ext_object::{ROTATE, ROTATE_GAP, handle_at, handle_cursor, handle_points, resized};
use hwpcraft_engine::HfTarget;
use serde_json::Value;

/// Page pixels (96 dpi) to HWPUNIT (7200 per inch).
const HU_PER_PX: f32 = 75.0;
/// Why a ribbon object command waits for a picked header object.
pub(crate) const HF_OBJECT_UNSUPPORTED: &str = "머리말·꼬리말 개체에는 아직 쓸 수 없습니다.";

/// Object and table commands that cannot reach a picked header object of `kind` yet.
fn hf_object_blocked(kind: &str, id: &str) -> bool {
    matches!(
        id,
        "object.group" | "object.ungroup" | "object.next" | "object.prev" | "object.select"
    ) || id.starts_with("object.align.")
        || (kind == "table"
            && id.starts_with("table.")
            && !matches!(
                id,
                "table.properties" | "table.cell_border_fill" | "table.delete"
            ))
}
/// How close to a table's frame a press picks the table, in screen pixels.
const TABLE_FRAME: f32 = 4.0;

#[derive(Default)]
pub(crate) struct HfObject {
    selected: Option<Picked>,
    /// The handle being dragged (`None` for a move) and where the drag started, in
    /// page pixels.
    drag: Option<(Option<usize>, Pos2)>,
}

#[derive(Clone)]
struct Picked {
    target: HfTarget,
    address: Value,
    page: u32,
    /// Page pixels.
    bbox: Rect,
    generation: u64,
    /// picture, shape, textbox, equation or table.
    kind: &'static str,
}

impl HwpApp {
    /// Objects of the edited header or footer laid out on `page`: address, box and
    /// whether it is a table.
    pub(crate) fn hf_objects_on(&self, page: u32) -> Vec<(Value, Rect, bool)> {
        let Some(layout) = self
            .session
            .page_control_layout(page)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        else {
            return vec![];
        };
        layout["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| {
                matches!(
                    c["type"].as_str(),
                    Some("image" | "shape" | "line" | "group" | "equation" | "table")
                )
            })
            .filter_map(|c| {
                let address = self.session.hf_layout_address(c)?;
                let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
                Some((
                    address,
                    Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h"))),
                    c["type"] == "table",
                ))
            })
            .collect()
    }

    /// The topmost object of the edited header or footer under page point `p`: a
    /// drawing anywhere in its box, a table only on its frame.
    fn hf_object_at(&self, page: u32, p: Pos2) -> Option<(Value, Rect, bool)> {
        let frame = TABLE_FRAME / self.zoom;
        self.hf_objects_on(page)
            .into_iter()
            .rev()
            .find(|(_, r, table)| {
                r.expand(frame).contains(p) && !(*table && r.shrink(frame).contains(p))
            })
    }

    fn hf_object_pick(&mut self, page: u32, address: Value, bbox: Rect, table: bool) {
        let Some(target) = self.session.hf_target() else {
            return;
        };
        let kind = if table {
            "table"
        } else {
            let r = self
                .session
                .run("object.properties.get", json!({"address": address}));
            match r.ok().as_ref().and_then(|v| v["kind"].as_str()) {
                Some("picture") => "picture",
                Some("textbox") => "textbox",
                Some("equation") => "equation",
                _ => "shape",
            }
        };
        self.hf_object.selected = Some(Picked {
            target,
            address,
            page,
            bbox,
            generation: self.session.generation,
            kind,
        });
    }

    /// The picked header or footer object's address, for the ribbon's commands.
    pub(crate) fn hf_object_address(&self) -> Option<Value> {
        let s = self.hf_object.selected.as_ref()?;
        (self.session.hf_target() == Some(s.target)).then(|| s.address.clone())
    }

    /// The picked header or footer object's kind: picture, shape, textbox, equation or
    /// table.
    pub(crate) fn hf_object_kind(&self) -> Option<&'static str> {
        let s = self.hf_object.selected.as_ref()?;
        (self.session.hf_target() == Some(s.target)).then_some(s.kind)
    }

    /// Whether a ribbon command of the object or table module is usable while a header
    /// or footer object is picked: off for those that cannot reach it yet, on for the
    /// rest (they act on it). `None` when nothing is picked.
    pub(crate) fn hf_object_enabled(&self, id: &str) -> Option<bool> {
        let kind = self.hf_object_kind()?;
        Some(!hf_object_blocked(kind, id))
    }

    /// A header or footer table is picked: the ribbon's table tabs act on it.
    pub(crate) fn hf_in_table(&self) -> bool {
        self.hf_object_kind() == Some("table")
    }

    /// 개체 속성 (or 표 속성 for a table) of the picked header object.
    fn hf_object_open(&mut self, s: &Picked) {
        if s.kind == "table" {
            self.open_table_properties_at(s.address.clone());
        } else {
            self.open_object_properties_at(s.address.clone());
        }
    }

    /// Ribbon object and table commands for a picked header or footer object; `None`
    /// when nothing is picked or the command is left to the shared object commands
    /// (which take `object_target`).
    pub(crate) fn hf_object_ribbon(&mut self, id: &str) -> Option<bool> {
        let s = self.hf_object_current()?;
        let a = s.address.clone();
        let props = |p: Value| json!({"address": a, "props": p});
        match id {
            "object.properties" | "table.properties" | "table.cell_border_fill" => {
                self.hf_object_open(&s);
            }
            "object.delete" | "table.delete" => {
                self.hf_object_deselect();
                self.run("object.delete", a.clone());
            }
            "object.lock" => {
                self.run("object.lock", json!({"address": a}));
            }
            "object.caption" => {
                self.run(
                    "object.properties",
                    props(json!({"hasCaption": true, "captionDirection": "Bottom"})),
                );
            }
            "object.placement.inline" => {
                self.run("object.properties", props(json!({"treatAsChar": true})));
            }
            "object.placement.square"
            | "object.placement.top_bottom"
            | "object.placement.front"
            | "object.placement.behind" => {
                let wrap = match id {
                    "object.placement.square" => "Square",
                    "object.placement.top_bottom" => "TopAndBottom",
                    "object.placement.front" => "InFrontOfText",
                    _ => "BehindText",
                };
                self.run(
                    "object.properties",
                    props(json!({"treatAsChar": false, "textWrap": wrap})),
                );
            }
            "object.rotate.left" | "object.rotate.right" => {
                let r = self
                    .session
                    .run("object.properties.get", json!({"address": a}));
                if let Some(v) = self.result(r) {
                    let step = if id.ends_with("left") { -90 } else { 90 };
                    let angle =
                        (v["props"]["rotationAngle"].as_i64().unwrap_or(0) + step).rem_euclid(360);
                    self.run("object.properties", props(json!({"rotationAngle": angle})));
                }
            }
            "object.flip.horizontal" | "object.flip.vertical" => {
                let horizontal = id.ends_with("horizontal");
                if s.kind == "picture" {
                    let r = self
                        .session
                        .run("object.properties.get", json!({"address": a}));
                    if let Some(v) = self.result(r) {
                        let key = if horizontal { "horzFlip" } else { "vertFlip" };
                        let on = v["props"][key] != true;
                        self.run("object.properties", props(json!({ (key): on })));
                    }
                } else {
                    let axis = if horizontal { "horizontal" } else { "vertical" };
                    self.run("object.flip", json!({"address": a, "axis": axis}));
                }
            }
            _ if id.starts_with("object.z_order.") => {
                let mode = id.trim_start_matches("object.z_order.");
                self.run("object.z_order", json!({"address": a, "mode": mode}));
            }
            // A picked table takes only its properties and deletion for now.
            _ if hf_object_blocked(s.kind, id) => {
                self.message = HF_OBJECT_UNSUPPORTED.into();
            }
            _ => return None,
        }
        Some(true)
    }

    /// The selection, while it still belongs to the edited header or footer; after an
    /// edit (or undo) its box is read again, and it is dropped when the object is gone.
    fn hf_object_current(&mut self) -> Option<Picked> {
        let s = self.hf_object.selected.clone()?;
        if self.session.hf_target() != Some(s.target) {
            self.hf_object.selected = None;
            return None;
        }
        if s.generation == self.session.generation {
            return Some(s);
        }
        let found = self
            .hf_objects_on(s.page)
            .into_iter()
            .find(|(a, _, _)| *a == s.address);
        self.hf_object.selected = found.map(|(address, bbox, _)| Picked {
            address,
            bbox,
            generation: self.session.generation,
            ..s
        });
        self.hf_object.selected.clone()
    }

    /// Drop the header object selection; true when there was one (Esc deselects
    /// before it leaves the header).
    pub(crate) fn hf_object_deselect(&mut self) -> bool {
        self.hf_object.drag = None;
        self.hf_object.selected.take().is_some()
    }

    /// Pointer input on `page` (drawn at `rect`) for objects of the edited header or
    /// footer; true when it took the input (the caret is left alone).
    pub(crate) fn hf_object_canvas(
        &mut self,
        page: u32,
        response: &egui::Response,
        rect: Rect,
    ) -> bool {
        let zoom = self.zoom;
        let to_page = |p: Pos2| Pos2::new((p.x - rect.min.x) / zoom, (p.y - rect.min.y) / zoom);
        let selected = self.hf_object_current().filter(|s| s.page == page);
        if let (Some(s), Some(h)) = (&selected, response.hover_pos().map(to_page))
            && let Some(handle) = handle_at(s.bbox, h, zoom)
        {
            response.ctx.set_cursor_icon(handle_cursor(handle));
        }
        let Some(p) = response.interact_pointer_pos().map(to_page) else {
            return false;
        };
        if response.drag_started() {
            let origin = response
                .ctx
                .input(|i| i.pointer.press_origin())
                .map(to_page)
                .unwrap_or(p);
            if let Some(s) = &selected {
                if let Some(h) =
                    handle_at(s.bbox, origin, zoom).filter(|h| *h != ROTATE || s.kind != "table")
                {
                    self.hf_object.drag = Some((Some(h), origin));
                    return true;
                }
                if s.bbox.expand(3.0).contains(origin) {
                    self.hf_object.drag = Some((None, origin));
                    return true;
                }
            }
            if let Some((address, bbox, table)) = self.hf_object_at(page, origin) {
                self.hf_object_pick(page, address, bbox, table);
                self.hf_object.drag = Some((None, origin));
                return true;
            }
            self.hf_object.selected = None;
            return false;
        }
        if let Some((handle, start)) = self.hf_object.drag {
            if response.drag_stopped() {
                self.hf_object.drag = None;
                let keep_ratio = response.ctx.input(|i| i.modifiers.shift);
                self.hf_object_commit(handle, start, p, keep_ratio);
            }
            return true;
        }
        if response.double_clicked()
            && let Some(s) = &selected
            && s.bbox.expand(3.0).contains(p)
            && (s.kind != "table" || !s.bbox.shrink(TABLE_FRAME / zoom).contains(p))
        {
            self.hf_object_open(s);
            return true;
        }
        if response.clicked() {
            if let Some((address, bbox, table)) = self.hf_object_at(page, p) {
                self.hf_object_pick(page, address, bbox, table);
                return true;
            }
            self.hf_object.selected = None;
        }
        false
    }

    /// Apply a finished drag: a move by `end - start`, or the handle's new box or
    /// angle, as one 개체 속성 change.
    fn hf_object_commit(
        &mut self,
        handle: Option<usize>,
        start: Pos2,
        end: Pos2,
        keep_ratio: bool,
    ) {
        let Some(s) = self.hf_object.selected.clone() else {
            return;
        };
        let r = self
            .session
            .run("object.properties.get", json!({"address": s.address}));
        let Some(current) = self.result(r) else {
            return;
        };
        let p = &current["props"];
        let offset = |key: &str, by: f32| {
            json!(p[key].as_i64().unwrap_or(0) + (by * HU_PER_PX).round() as i64)
        };
        let props = match handle {
            Some(ROTATE) => {
                let v = end - s.bbox.center();
                let angle = v.x.atan2(-v.y).to_degrees().rem_euclid(360.0).round() as i64 % 360;
                json!({"rotationAngle": angle})
            }
            Some(h) => {
                let r = resized(s.bbox, h, end, keep_ratio);
                // A table's size spreads over its columns and rows.
                let (w, hgt) = if s.kind == "table" {
                    ("tableWidth", "tableHeight")
                } else {
                    ("width", "height")
                };
                let mut props = json!({
                    w: (r.width() * HU_PER_PX).round() as i64,
                    hgt: (r.height() * HU_PER_PX).round() as i64,
                });
                let (dx, dy) = (r.min.x - s.bbox.min.x, r.min.y - s.bbox.min.y);
                if p["treatAsChar"] != true && (dx.abs() > 0.1 || dy.abs() > 0.1) {
                    props["horzOffset"] = offset("horzOffset", dx);
                    props["vertOffset"] = offset("vertOffset", dy);
                }
                props
            }
            None => {
                let d = end - start;
                if d.length() <= 1.0 {
                    return;
                }
                if p["treatAsChar"] == true {
                    self.message =
                        "글자처럼 취급한 개체는 머리말 글 안에서 옮겨지지 않습니다.".into();
                    return;
                }
                json!({"horzOffset": offset("horzOffset", d.x), "vertOffset": offset("vertOffset", d.y)})
            }
        };
        self.run(
            "object.properties",
            json!({"address": s.address, "props": props}),
        );
    }

    /// Keys while a header object is selected: Delete / Backspace remove it, Enter opens
    /// 개체 속성. True when taken; other keys drop the selection and go on to the text.
    pub(crate) fn hf_object_key(&mut self, event: &egui::Event) -> bool {
        let Some(s) = self.hf_object_current() else {
            return false;
        };
        match event {
            egui::Event::Key {
                key: egui::Key::Delete | egui::Key::Backspace,
                pressed: true,
                ..
            } => {
                self.hf_object_deselect();
                self.run("object.delete", s.address);
                true
            }
            egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                ..
            } => {
                self.hf_object_open(&s);
                true
            }
            egui::Event::Key { pressed: true, .. } | egui::Event::Text(_) => {
                self.hf_object_deselect();
                false
            }
            _ => false,
        }
    }

    /// The selected header object's frame and handles on `page`, and the box a drag
    /// would give.
    pub(crate) fn hf_object_paint(&self, painter: &egui::Painter, page: u32, rect: Rect) {
        let Some(s) = self.hf_object.selected.as_ref() else {
            return;
        };
        if s.page != page || self.session.hf_target() != Some(s.target) {
            return;
        }
        let to_screen = |p: Pos2| rect.min + p.to_vec2() * self.zoom;
        let blue = Color32::from_rgb(40, 110, 220);
        let frame = Rect::from_min_max(to_screen(s.bbox.min), to_screen(s.bbox.max));
        painter.rect_stroke(
            frame,
            0.0,
            egui::Stroke::new(1.0, blue),
            egui::StrokeKind::Middle,
        );
        for h in handle_points(frame) {
            let knob = Rect::from_center_size(h, Vec2::splat(7.0));
            painter.rect_filled(knob, 0.0, Color32::WHITE);
            painter.rect_stroke(
                knob,
                0.0,
                egui::Stroke::new(1.0, blue),
                egui::StrokeKind::Middle,
            );
        }
        if s.kind != "table" {
            let top = frame.center_top();
            let knob = top - Vec2::new(0.0, ROTATE_GAP);
            painter.line_segment([top, knob], egui::Stroke::new(1.0, blue));
            painter.circle(knob, 4.0, Color32::WHITE, egui::Stroke::new(1.0, blue));
        }
        let Some((handle, start)) = self.hf_object.drag else {
            return;
        };
        let Some(pointer) = painter.ctx().pointer_latest_pos().map(|p| {
            Pos2::new(
                (p.x - rect.min.x) / self.zoom,
                (p.y - rect.min.y) / self.zoom,
            )
        }) else {
            return;
        };
        let preview = match handle {
            Some(ROTATE) => return,
            Some(h) => resized(
                s.bbox,
                h,
                pointer,
                painter.ctx().input(|i| i.modifiers.shift),
            ),
            None => s.bbox.translate(pointer - start),
        };
        painter.rect_stroke(
            Rect::from_min_max(to_screen(preview.min), to_screen(preview.max)),
            0.0,
            egui::Stroke::new(1.0, blue.gamma_multiply(0.6)),
            egui::StrokeKind::Middle,
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use hwpcraft_doc::model::control::Control;
    use serde_json::json;

    /// A document whose header has a floating picture at control 0 of its second
    /// paragraph and whose body paragraph 1 has another one with the same numbers.
    /// A folder of its own for one test document (tests run in parallel).
    fn scratch_dir(name: &str) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("hwpcraft-hf-{name}-{}-{n}", std::process::id()))
    }

    pub(crate) fn header_picture() -> HwpApp {
        let dir = scratch_dir("objects");
        std::fs::create_dir_all(&dir).unwrap();
        let logo = dir.join("logo.png");
        image::RgbImage::from_pixel(60, 30, image::Rgb([68, 114, 196]))
            .save(&logo)
            .unwrap();
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let s = &mut a.session;
        s.execute(hwpcraft_engine::Command::Insert("본문 첫 문단".into()))
            .unwrap();
        s.execute(hwpcraft_engine::Command::Split).unwrap();
        s.run(
            "picture.insert",
            json!({"path": logo, "width_mm": 30.0, "treat_as_char": false}),
        )
        .unwrap();
        s.run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let doc = s.core.document_mut();
        let picture = doc.sections[0].paragraphs[1].clone();
        let header = doc.sections[0]
            .paragraphs
            .iter_mut()
            .flat_map(|p| p.controls.iter_mut())
            .find_map(|c| match c {
                Control::Header(h) => Some(&mut h.paragraphs),
                _ => None,
            })
            .unwrap();
        header.truncate(1);
        header.push(picture);
        let path = dir.join("header.hwpx");
        s.save(&path).unwrap();
        s.open(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let hit = a.session.hf_page_target(0, false).unwrap();
        assert!(a.hf_open(hit, 0, None));
        a
    }

    /// One frame of page 0 drawn at (50, 50), `seconds` after the previous one.
    pub(crate) fn frame(a: &mut HwpApp, events: Vec<egui::Event>, seconds: f64) {
        let ctx = a.ctx.clone();
        let time = ctx.input(|i| i.time) + seconds;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 1400.0))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                let rect =
                    Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(794.0, 1123.0) * a.zoom);
                let response = ui.interact(
                    rect,
                    egui::Id::new("page-test"),
                    egui::Sense::click_and_drag(),
                );
                a.hf_canvas(0, &response, rect);
                a.hf_paint(ui.painter(), 0, rect);
            },
        );
        out.textures_delta.clear();
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn drag(a: &mut HwpApp, from: Pos2, to: Pos2) {
        frame(
            a,
            vec![egui::Event::PointerMoved(from), button(from, true)],
            1.0,
        );
        for i in 1..=5 {
            let p = from + (to - from) * (i as f32 / 5.0);
            frame(a, vec![egui::Event::PointerMoved(p)], 0.05);
        }
        frame(a, vec![button(to, false)], 0.05);
    }

    pub(crate) fn click(a: &mut HwpApp, p: Pos2) {
        frame(a, vec![egui::Event::PointerMoved(p), button(p, true)], 1.0);
        frame(a, vec![button(p, false)], 0.05);
    }

    /// The header picture's box on page 0 (page pixels), if it is there.
    pub(crate) fn header_box(a: &HwpApp) -> Option<Rect> {
        a.hf_objects_on(0)
            .into_iter()
            .find(|(_, _, table)| !table)
            .map(|(_, r, _)| r)
    }

    pub(crate) fn screen(zoom: f32, p: Pos2) -> Pos2 {
        Pos2::new(50.0, 50.0) + p.to_vec2() * zoom
    }

    fn body_picture_x(a: &HwpApp) -> f64 {
        let p = a
            .session
            .object_properties(hwpcraft_engine::ObjectAddress {
                section: 0,
                paragraph: 1,
                control: 0,
            })
            .unwrap();
        p["props"]["horzOffset"].as_f64().unwrap()
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// In the edited header a press picks its picture, a drag moves it, a handle
    /// resizes it, Delete removes it (each one undo step), and Esc lets the picture
    /// go before it leaves the header. The body picture with the same numbers stays.
    #[test]
    fn header_pictures_are_picked_moved_resized_and_deleted() {
        let mut a = header_picture();
        let z = a.zoom;
        // egui needs the page widget from a previous frame before it takes a press.
        frame(&mut a, vec![], 0.0);
        let body_x = body_picture_x(&a);
        let b = header_box(&a).expect("the header picture is laid out");
        click(&mut a, screen(z, b.center()));
        assert!(a.hf_object.selected.is_some(), "{}", a.message);
        // Move 40 px right.
        let to = b.center() + Vec2::new(40.0, 0.0);
        drag(&mut a, screen(z, b.center()), screen(z, to));
        let moved = header_box(&a).unwrap();
        assert!(
            (moved.min.x - b.min.x - 40.0).abs() < 1.5,
            "{b:?} -> {moved:?}"
        );
        assert_eq!(body_picture_x(&a), body_x);
        // Resize from the right-bottom handle by 30 px.
        let corner = moved.right_bottom();
        drag(
            &mut a,
            screen(z, corner),
            screen(z, corner + Vec2::new(30.0, 0.0)),
        );
        let resized = header_box(&a).unwrap();
        assert!(
            (resized.width() - moved.width() - 30.0).abs() < 1.5,
            "{resized:?}"
        );
        // Esc lets the picture go and keeps editing; Delete needs it picked again.
        assert!(a.hf_key_event(&key(egui::Key::Escape)));
        assert!(a.hf_object.selected.is_none());
        assert!(a.session.hf_target().is_some());
        click(&mut a, screen(z, resized.center()));
        assert!(a.hf_key_event(&key(egui::Key::Delete)));
        assert!(header_box(&a).is_none(), "deleted");
        assert_eq!(body_picture_x(&a), body_x, "the body picture stays");
        a.session.execute(hwpcraft_engine::Command::Undo).unwrap();
        assert_eq!(header_box(&a), Some(resized));
        a.session.execute(hwpcraft_engine::Command::Undo).unwrap();
        a.session.execute(hwpcraft_engine::Command::Undo).unwrap();
        let back = header_box(&a).unwrap();
        assert!((back.min.x - b.min.x).abs() < 0.5 && (back.width() - b.width()).abs() < 0.5);
        // Esc without a picked picture leaves the header.
        assert!(a.hf_key_event(&key(egui::Key::Escape)));
        assert!(a.session.hf_target().is_none());
    }

    /// A header with a 2×2 table in its second paragraph, entered on page 0.
    pub(crate) fn header_table() -> HwpApp {
        let dir = scratch_dir("table");
        std::fs::create_dir_all(&dir).unwrap();
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let s = &mut a.session;
        s.execute(hwpcraft_engine::Command::Insert("본문".into()))
            .unwrap();
        s.execute(hwpcraft_engine::Command::Split).unwrap();
        s.execute(hwpcraft_engine::Command::Table {
            rows: 2,
            columns: 2,
        })
        .unwrap();
        s.run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let doc = s.core.document_mut();
        let table = doc.sections[0]
            .paragraphs
            .iter()
            .position(|p| p.controls.iter().any(|c| matches!(c, Control::Table(_))))
            .unwrap();
        let moved = doc.sections[0].paragraphs.remove(table);
        let header = doc.sections[0]
            .paragraphs
            .iter_mut()
            .flat_map(|p| p.controls.iter_mut())
            .find_map(|c| match c {
                Control::Header(h) => Some(&mut h.paragraphs),
                _ => None,
            })
            .unwrap();
        header.truncate(1);
        header.push(moved);
        let path = dir.join("header-table.hwpx");
        s.save(&path).unwrap();
        s.open(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let hit = a.session.hf_page_target(0, false).unwrap();
        assert!(a.hf_open(hit, 0, None));
        a
    }

    /// A picked header picture is what the ribbon's picture tab acts on: its kind
    /// shows the tab, 오른쪽으로 회전 turns it, and commands that cannot reach header
    /// objects yet say so instead of acting on a body object.
    #[test]
    fn the_ribbon_picture_tab_acts_on_the_picked_header_picture() {
        let mut a = header_picture();
        let z = a.zoom;
        frame(&mut a, vec![], 0.0);
        assert_eq!(a.object_ext_selected_kind(), None);
        let b = header_box(&a).unwrap();
        click(&mut a, screen(z, b.center()));
        assert_eq!(a.object_ext_selected_kind(), Some("picture"));
        let ctx = a.ctx.clone();
        assert!(a.object_ext_dispatch("object.rotate.right", &ctx));
        let address = a.hf_object_address().unwrap();
        let p = a
            .session
            .run("object.properties.get", json!({"address": address}))
            .unwrap();
        assert_eq!(p["props"]["rotationAngle"], 90, "{p}");
        let body = a
            .session
            .object_properties(hwpcraft_engine::ObjectAddress {
                section: 0,
                paragraph: 1,
                control: 0,
            })
            .unwrap();
        assert_eq!(body["props"]["rotationAngle"], 0, "the body picture stays");
        assert!(a.object_ext_dispatch("object.group", &ctx));
        assert_eq!(a.message, HF_OBJECT_UNSUPPORTED);
    }

    /// A header table is picked by its frame (a press inside is for its cells): the
    /// table tabs then act on it, a double click on the frame opens 표 속성, and the
    /// ribbon's 표 지우기 removes it in one undo step.
    #[test]
    fn a_header_table_is_picked_by_its_frame() {
        let mut a = header_table();
        let z = a.zoom;
        frame(&mut a, vec![], 0.0);
        let (_, b, _) = a
            .hf_objects_on(0)
            .into_iter()
            .find(|(_, _, table)| *table)
            .expect("the header table is laid out");
        // Inside: not picked.
        click(&mut a, screen(z, b.center()));
        assert!(!a.hf_in_table());
        // On the left frame: picked.
        let edge = Pos2::new(b.min.x, b.center().y);
        click(&mut a, screen(z, edge));
        assert!(a.hf_in_table(), "{}", a.message);
        assert_eq!(a.object_ext_selected_kind(), None);
        assert_eq!(a.table_ext_enabled("table.properties"), Some(true));
        // The right-bottom handle narrows it by 40 px.
        let corner = b.right_bottom();
        drag(
            &mut a,
            screen(z, corner),
            screen(z, corner - Vec2::new(40.0, 0.0)),
        );
        let (_, narrower, _) = a
            .hf_objects_on(0)
            .into_iter()
            .find(|(_, _, table)| *table)
            .unwrap();
        assert!(
            (b.width() - narrower.width() - 40.0).abs() < 2.0,
            "{b:?} -> {narrower:?}"
        );
        assert!(a.hf_in_table(), "still picked");
        // A double click on the frame opens 표 속성.
        click(&mut a, screen(z, edge));
        frame(&mut a, vec![button(screen(z, edge), true)], 0.05);
        frame(&mut a, vec![button(screen(z, edge), false)], 0.05);
        assert!(a.table_properties_dialog_open(), "{:?}", a.error);
        let ctx = a.ctx.clone();
        assert!(a.table_ext_dispatch("table.delete", &ctx));
        assert!(
            !a.hf_objects_on(0).iter().any(|(_, _, table)| *table),
            "the header table is gone"
        );
        a.session.execute(hwpcraft_engine::Command::Undo).unwrap();
        assert!(a.hf_objects_on(0).iter().any(|(_, _, table)| *table));
    }

    fn body_table_rows(a: &HwpApp) -> Vec<u16> {
        a.session.core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .filter_map(|c| match c {
                Control::Table(t) => Some(t.row_count),
                _ => None,
            })
            .collect()
    }

    /// The object tabs' buttons are on for a picked header picture, except the ones
    /// that cannot reach it yet.
    #[test]
    fn object_tab_buttons_follow_the_picked_header_picture() {
        let mut a = header_picture();
        let z = a.zoom;
        frame(&mut a, vec![], 0.0);
        let b = header_box(&a).unwrap();
        click(&mut a, screen(z, b.center()));
        assert_eq!(a.object_ext_enabled("object.rotate.right"), Some(true));
        assert_eq!(a.object_ext_enabled("object.properties"), Some(true));
        assert_eq!(a.object_ext_enabled("object.group"), Some(false));
        assert_eq!(a.object_ext_enabled("object.align.left"), Some(false));
        // Ids of other modules stay theirs.
        assert_eq!(a.object_ext_enabled("file.save_pages"), None);
    }

    /// With the body caret in a body table, a picked header table never lets the
    /// ribbon's row and column edits reach the body table, and the table tab's cell
    /// commands are off for it.
    #[test]
    fn row_edits_never_reach_the_body_table_while_a_header_table_is_picked() {
        let mut a = header_table();
        a.hf_leave();
        a.session
            .execute(hwpcraft_engine::Command::Table {
                rows: 3,
                columns: 2,
            })
            .unwrap();
        // The body caret into the body table's first cell.
        let (paragraph, control) = a.session.core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(pi, p)| {
                p.controls
                    .iter()
                    .position(|c| matches!(c, Control::Table(_)))
                    .map(|ci| (pi, ci))
            })
            .unwrap();
        a.session
            .select(
                hwpcraft_engine::Cursor {
                    section: 0,
                    paragraph,
                    offset: 0,
                    cell: Some(hwpcraft_engine::Cell {
                        control,
                        index: 0,
                        paragraph: 0,
                    }),
                },
                false,
            )
            .unwrap();
        assert!(
            a.session.cursor.cell.is_some(),
            "the body caret is in the body table"
        );
        let rows = body_table_rows(&a);
        let hit = a.session.hf_page_target(0, false).unwrap();
        assert!(a.hf_open(hit, 0, None));
        let z = a.zoom;
        frame(&mut a, vec![], 0.0);
        let (_, b, _) = a
            .hf_objects_on(0)
            .into_iter()
            .find(|(_, _, table)| *table)
            .unwrap();
        click(&mut a, screen(z, Pos2::new(b.min.x, b.center().y)));
        assert!(a.hf_in_table());
        assert!(!a.run(
            "table.edit",
            json!({"action": hwpcraft_engine::TableEdit::DeleteRow})
        ));
        assert_eq!(a.message, HF_OBJECT_UNSUPPORTED);
        assert_eq!(body_table_rows(&a), rows, "the body table keeps its rows");
        assert_eq!(a.table_ext_enabled("table.split"), Some(false));
        assert_eq!(a.table_ext_enabled("table.properties"), Some(true));
        assert_eq!(a.table_ext_enabled("table.delete"), Some(true));
    }
}
