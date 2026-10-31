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
/// 보기 → 격자: screen-only lines every [`GRID_MM`] over the paper, every second one
/// a little darker, like Hangul's line grid. Nothing is saved or printed.
pub(crate) const GRID_MM: f32 = 5.0;

/// Grid line offsets from the paper edge (screen px) for a side of `length` px.
pub(crate) fn grid_offsets(length: f32, zoom: f32) -> Vec<(f32, bool)> {
    let step = GRID_MM / 25.4 * 96.0 * zoom;
    if !(step.is_finite() && step >= 3.0) {
        return Vec::new();
    }
    (1..)
        .map(|n| (n as f32 * step, n % 2 == 0))
        .take_while(|(offset, _)| *offset < length)
        .collect()
}

/// 안내선 on the page `rect`: dashed lines across the paper.
fn paint_guides(ui: &egui::Ui, rect: Rect, zoom: f32, guides: &[(bool, f32)]) {
    let painter = ui.painter_at(rect.intersect(ui.clip_rect()));
    let stroke = Stroke::new(1.0, Color32::from_rgb(0, 160, 200));
    for &(vertical, mm) in guides {
        let at = mm / 25.4 * 96.0 * zoom;
        let line = if vertical {
            let x = rect.left() + at;
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())]
        } else {
            let y = rect.top() + at;
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)]
        };
        painter.add(egui::Shape::dashed_line(&line, stroke, 6.0, 4.0));
    }
}

fn paint_grid(ui: &egui::Ui, rect: Rect, zoom: f32) {
    let painter = ui.painter_at(rect.intersect(ui.clip_rect()));
    let stroke = |major: bool| {
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(70, 120, 200, if major { 70 } else { 34 }),
        )
    };
    for (x, major) in grid_offsets(rect.width(), zoom) {
        let x = rect.left() + x;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke(major),
        );
    }
    for (y, major) in grid_offsets(rect.height(), zoom) {
        let y = rect.top() + y;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke(major),
        );
    }
}

impl HwpApp {
    pub(crate) fn paint_margin_corners(&self, ui: &egui::Ui, page: u32, rect: Rect) {
        if self.ext.shell.show_grid {
            paint_grid(ui, rect, self.zoom);
        }
        if self.ext.shell.show_guides {
            paint_guides(ui, rect, self.zoom, &self.ext.shell.guides);
        }
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

#[cfg(test)]
mod grid_tests {
    use super::*;

    #[test]
    fn the_grid_is_five_millimetres_with_every_second_line_darker() {
        // A4 width at 100 %: 794 px, 5 mm = 18.9 px.
        let lines = grid_offsets(794.0, 1.0);
        assert_eq!(lines.len(), 42);
        assert!((lines[0].0 - 18.9).abs() < 0.05 && !lines[0].1);
        assert!(lines[1].1, "10 mm lines are darker");
        // Too dense to read at tiny zoom: no grid rather than a grey wash.
        assert!(grid_offsets(794.0, 0.1).is_empty());
    }

    #[test]
    fn guides_are_added_shown_and_cleared() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        assert_eq!(a.shell_ext_enabled("view.guides.clear"), Some(false));
        a.shell_ext_dispatch("view.guides.add", &ctx);
        assert!(a.ext.shell.dialog.is_some(), "asks direction and position");
        a.ext.shell.dialog = None;
        a.ext.shell.guides.push((false, 25.0));
        assert!(a.shell_ext_checked("view.guides"));
        assert_eq!(a.shell_ext_enabled("view.guides.clear"), Some(true));
        a.shell_ext_dispatch("view.guides", &ctx);
        assert!(!a.shell_ext_checked("view.guides"), "hidden but kept");
        assert_eq!(a.ext.shell.guides.len(), 1);
        a.shell_ext_dispatch("view.guides.clear", &ctx);
        assert!(a.ext.shell.guides.is_empty());
        Ok(())
    }

    #[test]
    fn the_grid_toggle_is_a_checked_view_command() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        assert_eq!(a.shell_ext_enabled("view.grid"), Some(true));
        assert!(!a.shell_ext_checked("view.grid"));
        assert!(a.shell_ext_dispatch("view.grid", &ctx));
        assert!(a.shell_ext_checked("view.grid") && a.message.contains("5 mm"));
        a.shell_ext_dispatch("view.grid", &ctx);
        assert!(!a.ext.shell.show_grid);
        Ok(())
    }
}
