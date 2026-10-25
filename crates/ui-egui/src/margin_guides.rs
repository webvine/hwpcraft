//! Screen-only body corner marks and cell-padding ruler controls.
use super::*;
use hwpcraft_engine::ObjectAddress;
use serde_json::Value;

#[derive(Default)]
pub(crate) struct State {
    drag: Option<(CellGeometry, bool, u64, Cursor)>,
}
#[derive(Clone)]
pub(crate) struct CellGeometry {
    table: ObjectAddress,
    rect: Rect,
    padding: [f32; 4], // mm: left/right/top/bottom
    range: Value,
    scale: f32, // screen px / mm
}
impl CellGeometry {
    pub(crate) fn inner_left(&self) -> f32 {
        self.rect.left() + self.padding[0] * self.scale
    }
    pub(crate) fn inner_right(&self) -> f32 {
        self.rect.right() - self.padding[1] * self.scale
    }
}
impl HwpApp {
    pub(crate) fn paint_margin_corners(&self, ui: &egui::Ui, page: u32, rect: Rect) {
        let Ok(info) = self.session.page_info(page) else {
            return;
        };
        let Some(body) = body_rect(&info) else { return };
        let screen = Rect::from_min_max(
            rect.min + body.min.to_vec2() * self.zoom,
            rect.min + body.max.to_vec2() * self.zoom,
        );
        let painter = ui.painter_at(rect.intersect(ui.clip_rect()));
        for (corner, dir) in [
            (screen.left_top(), Vec2::new(-1.0, -1.0)),
            (screen.right_top(), Vec2::new(1.0, -1.0)),
            (screen.left_bottom(), Vec2::new(-1.0, 1.0)),
            (screen.right_bottom(), Vec2::new(1.0, 1.0)),
        ] {
            let joint = corner + dir * 3.0;
            let stroke = Stroke::new(1.0, Color32::from_gray(198));
            painter.line_segment([joint, joint + Vec2::new(dir.x * 18.0, 0.0)], stroke);
            painter.line_segment([joint, joint + Vec2::new(0.0, dir.y * 18.0)], stroke);
        }
    }
    pub(crate) fn ruler_cell_geometry(&self, page_rect: Rect) -> Option<CellGeometry> {
        let c = self.session.cursor;
        let at = c.cell?;
        let page = self.session.caret().ok()?["pageIndex"].as_u64()? as u32;
        if page != self.page {
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
        let hwpcraft_doc::model::control::Control::Table(table) = p.controls.get(at.control)?
        else {
            return None;
        };
        let cell = table.cells.get(at.index)?;
        let raw = self.session.page_control_layout(page).ok()?;
        let layout: Value = serde_json::from_str(&raw).ok()?;
        let t = layout["controls"].as_array()?.iter().find(|t| {
            t["type"] == "table"
                && t["secIdx"] == c.section
                && t["paraIdx"] == c.paragraph
                && t["controlIdx"] == at.control
        })?;
        let r = t["cells"]
            .as_array()?
            .iter()
            .find(|r| r["row"] == cell.row && r["col"] == cell.col)?;
        let f = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
        let rect = Rect::from_min_size(
            page_rect.min + Vec2::new(f("x"), f("y")) * self.zoom,
            Vec2::new(f("w"), f("h")) * self.zoom,
        );
        let effective=|a:i16,b:i16| if cell.use_cell_padding_axis(a,b){a.max(0)}else{b.max(0)} as f32*25.4/7200.0;
        Some(CellGeometry{
            table:ObjectAddress{section:c.section,paragraph:c.paragraph,control:at.control},rect,
            padding:[effective(cell.padding.left,table.padding.left),effective(cell.padding.right,table.padding.right),effective(cell.padding.top,table.padding.top),effective(cell.padding.bottom,table.padding.bottom)],
            range:self.block_range().unwrap_or_else(||json!({"start_row":cell.row,"end_row":cell.row+cell.row_span-1,"start_col":cell.col,"end_col":cell.col+cell.col_span-1})),
            scale:self.zoom*96.0/25.4,
        })
    }
    pub(crate) fn cell_margin_ruler(&mut self, ui: &egui::Ui, band: Rect, cell: CellGeometry) {
        if self.ext_modal_open() || self.error.is_some() {
            return;
        }
        let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
        let press = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        for (left, x) in [(true, cell.inner_left()), (false, cell.inner_right())] {
            let hit = Rect::from_center_size(
                Pos2::new(x, band.center().y),
                Vec2::new(10.0, band.height() + 6.0),
            )
            .intersect(ui.clip_rect());
            let response = ui
                .interact(
                    hit,
                    egui::Id::new(("cell-padding-ruler", left)),
                    egui::Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text(if left {
                    "셀 왼쪽 안 여백 · 끌어서 조절, 두 번 클릭하면 상세 설정"
                } else {
                    "셀 오른쪽 안 여백 · 끌어서 조절, 두 번 클릭하면 상세 설정"
                });
            ui.painter().line_segment(
                [
                    Pos2::new(x, band.top() + 3.0),
                    Pos2::new(x, band.bottom() - 3.0),
                ],
                Stroke::new(2.0, Color32::from_gray(115)),
            );
            if press
                .is_some_and(|p| hit.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id()))
            {
                self.margin_guides.drag = Some((
                    cell.clone(),
                    left,
                    self.session.generation,
                    self.session.cursor,
                ));
            }
            if response.double_clicked() {
                self.margin_guides.drag = None;
                self.ext_dispatch("table.cell_margin", &ui.ctx().clone());
                return;
            }
        }
        let Some((original, left, generation, cursor)) = self.margin_guides.drag.clone() else {
            return;
        };
        if generation != self.session.generation || cursor != self.session.cursor {
            self.margin_guides.drag = None;
            return;
        }
        let Some(p) = pointer else { return };
        let other = original.padding[usize::from(left)];
        let max = (original.rect.width() / original.scale - other - 1.0).clamp(0.0, 50.0);
        let mm = (if left {
            p.x - original.rect.left()
        } else {
            original.rect.right() - p.x
        } / original.scale)
            .clamp(0.0, max);
        let x = if left {
            original.rect.left() + mm * original.scale
        } else {
            original.rect.right() - mm * original.scale
        };
        ui.painter().line_segment(
            [
                Pos2::new(x, band.top()),
                Pos2::new(x, ui.max_rect().bottom()),
            ],
            Stroke::new(1.0, chrome::Tokens::ACCENT),
        );
        if ui.ctx().input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    Event::PointerButton {
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    }
                )
            })
        }) {
            self.margin_guides.drag = None;
            if (mm - original.padding[usize::from(!left)]).abs() < 0.05 {
                return;
            }
            let key = if left { "paddingLeft" } else { "paddingRight" };
            let mut props = json!({"applyInnerMargin":true});
            props[key] = json!((mm * 7200.0 / 25.4).round() as i32);
            self.run(
                "table.cell_properties",
                json!({"table":original.table,"range":original.range,"props":props}),
            );
        }
    }
}

fn body_rect(info: &Value) -> Option<Rect> {
    let f = |k: &str| info[k].as_f64().map(|n| n as f32);
    let rect = Rect::from_min_max(
        Pos2::new(f("bodyLeft")?, f("marginTop")? + f("marginHeader")?),
        Pos2::new(
            f("bodyRight")?,
            f("height")? - f("marginBottom")? - f("marginFooter")?,
        ),
    );
    (rect.is_finite() && rect.is_positive()).then_some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(app: &mut HwpApp, events: Vec<Event>) {
        let ctx = app.ctx.clone();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 916.0))),
                events,
                ..Default::default()
            },
            |ui| {
                app.toolbar(ui);
                app.status_bar(ui);
                app.page_view(ui);
                app.dialogs(&ctx);
            },
        );
        out.textures_delta.clear();
    }
    #[test]
    fn ruler_drag_changes_cell_padding_not_page_margin_and_undoes_once() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.session
            .run("table.create", json!({"rows":2,"columns":2}))
            .unwrap();
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let hit = app
            .ctx
            .read_response(egui::Id::new(("cell-padding-ruler", true)))
            .unwrap()
            .rect;
        let before = app.session.page_setup().unwrap();
        let c = app
            .session
            .run("table.cell_properties.get", json!({}))
            .unwrap();
        let start = hit.center();
        let end = start + Vec2::new(18.0, 0.0);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut app,
            vec![button(start, true), Event::PointerMoved(end)],
        );
        frame(&mut app, vec![button(end, false)]);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.session.page_setup().unwrap(), before);
        let after = app
            .session
            .run("table.cell_properties.get", json!({}))
            .unwrap();
        assert_eq!(after["applyInnerMargin"], true);
        assert!(after["paddingLeft"].as_u64().unwrap() > 1000);
        app.session.execute(Command::Undo).unwrap();
        assert_eq!(
            app.session
                .run("table.cell_properties.get", json!({}))
                .unwrap(),
            c
        );
    }
    #[test]
    fn body_corner_geometry_uses_resolved_binding_and_header_footer() {
        let v = json!({"bodyLeft":130,"bodyRight":690,"height":1123,"marginTop":75,"marginHeader":40,"marginBottom":60,"marginFooter":30});
        assert_eq!(
            body_rect(&v),
            Some(Rect::from_min_max(
                Pos2::new(130.0, 115.0),
                Pos2::new(690.0, 1033.0)
            ))
        );
        let mut s = Session::new().unwrap();
        let mut setup = s.page_setup().unwrap();
        setup.left_mm = 25.0;
        setup.right_mm = 18.0;
        setup.gutter_mm = 7.0;
        setup.binding = 1;
        s.execute(Command::PageSetup(setup)).unwrap();
        s.execute(Command::PageBreak).unwrap();
        let mut boxes = Vec::new();
        for page in 0..2 {
            let v: Value =
                serde_json::from_str(&s.core.get_page_info_native(page).unwrap()).unwrap();
            boxes.push(body_rect(&v).unwrap());
        }
        assert!((boxes[0].left() - 32.0 * 96.0 / 25.4).abs() < 0.2);
        assert_ne!(boxes[0].left(), boxes[1].left());
    }
}
