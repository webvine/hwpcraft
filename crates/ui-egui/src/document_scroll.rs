//! Fixed document scroll rails, independent of paper width and zoom.
use super::*;

pub(super) const WIDTH: f32 = 17.0;

#[derive(Default)]
pub(crate) struct State {
    pub pending: [Option<f32>; 2],
    offset: Vec2,
    content: Vec2,
    viewport: Vec2,
    drag: [Option<(f32, f32)>; 2],
    repeat: [f64; 2],
    rails: [Option<Rect>; 2],
}

impl State {
    /// Current scroll position of the document view.
    pub fn offset(&self) -> Vec2 {
        self.offset
    }
    pub fn update<T>(
        &mut self,
        ctx: &egui::Context,
        output: &egui::scroll_area::ScrollAreaOutput<T>,
    ) {
        let viewport = output.inner_rect.size();
        if self.offset != output.state.offset
            || self.content != output.content_size
            || self.viewport != viewport
        {
            ctx.request_repaint();
        }
        self.offset = output.state.offset;
        self.content = output.content_size;
        self.viewport = viewport;
    }
}

fn thumb_geometry(track: Rect, axis: usize, view: f32, content: f32, offset: f32) -> (Rect, f32) {
    let length = track.size()[axis];
    let maximum = (content - view).max(0.0);
    let size = if maximum > 0.0 {
        (length * view / content).max(24.0).min(length)
    } else {
        length
    };
    let travel = (length - size).max(0.0);
    let mut thumb = track;
    thumb.min[axis] += if maximum > 0.0 {
        travel * (offset / maximum).clamp(0.0, 1.0)
    } else {
        0.0
    };
    thumb.max[axis] = thumb.min[axis] + size;
    (thumb, travel)
}

impl HwpApp {
    pub(super) fn vertical_scroll_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("hwp-document-scroll-rail")
            .exact_size(WIDTH)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(Color32::from_gray(242)))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let rail = Rect::from_min_max(rect.min, rect.max - Vec2::new(0.0, WIDTH * 2.0));
                self.document_scrollbar(ui, rail, 1);
                for (i, tip, page) in [
                    (0, "이전 쪽", self.page.saturating_sub(1)),
                    (
                        1,
                        "다음 쪽",
                        (self.page + 1).min(self.layout_page_count().saturating_sub(1)),
                    ),
                ] {
                    let r = Rect::from_min_size(
                        Pos2::new(rect.left(), rail.bottom() + i as f32 * WIDTH),
                        Vec2::splat(WIDTH),
                    );
                    let response = ui
                        .interact(r, ui.id().with(("page-scroll", i)), egui::Sense::click())
                        .on_hover_text(tip);
                    if response.hovered() {
                        ui.painter().rect_filled(r, 0.0, Color32::from_gray(225));
                    }
                    // Paint double chevrons instead of relying on font glyph coverage.
                    let sign = if i == 0 { -1.0 } else { 1.0 };
                    for dy in [-2.0, 2.0] {
                        let center = r.center() + Vec2::new(0.0, dy);
                        let stroke = Stroke::new(1.0, Color32::from_gray(125));
                        ui.painter().line_segment(
                            [
                                center + Vec2::new(-3.0, -sign * 1.5),
                                center + Vec2::new(0.0, sign * 1.5),
                            ],
                            stroke,
                        );
                        ui.painter().line_segment(
                            [
                                center + Vec2::new(0.0, sign * 1.5),
                                center + Vec2::new(3.0, -sign * 1.5),
                            ],
                            stroke,
                        );
                    }
                    if response.clicked() {
                        self.page = page;
                    }
                }
            });
    }

    pub(super) fn horizontal_scrollbar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let r = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), WIDTH));
        self.document_scrollbar(ui, r, 0);
    }

    fn document_scrollbar(&mut self, ui: &mut egui::Ui, rect: Rect, axis: usize) {
        self.document_scroll.rails[axis] = Some(rect);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(240));
        let mut track = rect;
        track.min[axis] += WIDTH;
        track.max[axis] -= WIDTH;
        if track.size()[axis] <= 0.0 {
            return;
        }
        let state = &mut self.document_scroll;
        let max = (state.content[axis] - state.viewport[axis]).max(0.0);
        let offset = state.pending[axis].unwrap_or(state.offset[axis]);
        let (thumb, travel) = thumb_geometry(
            track,
            axis,
            state.viewport[axis],
            state.content[axis],
            offset,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("document-rail", axis)),
            egui::Sense::click_and_drag(),
        );
        let press = ui.input(|i| {
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
        let owns_press = press.filter(|p| {
            rect.contains(*p)
                && ui.clip_rect().contains(*p)
                && ui.ctx().layer_id_at(*p) == Some(ui.layer_id())
        });
        let mut target = None;
        if max > 0.0 {
            if let Some(pos) = owns_press {
                if thumb.contains(pos) {
                    state.drag[axis] = Some((pos[axis], offset));
                } else {
                    let step = if track.contains(pos) {
                        state.viewport[axis] * 0.9
                    } else {
                        40.0
                    };
                    target = Some(
                        offset
                            + if pos[axis] < thumb.min[axis] {
                                -step
                            } else {
                                step
                            },
                    );
                    state.repeat[axis] = ui.input(|i| i.time) + 0.4;
                }
            }
            if let Some((start, initial)) = state.drag[axis] {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                    && travel > 0.0
                {
                    target = Some(initial + (pos[axis] - start) * max / travel);
                }
            } else if response.is_pointer_button_down_on() && owns_press.is_none() {
                let now = ui.input(|i| i.time);
                if now >= state.repeat[axis]
                    && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                    && !thumb.contains(pos)
                {
                    let step = if track.contains(pos) {
                        state.viewport[axis] * 0.9
                    } else {
                        40.0
                    };
                    target = Some(
                        offset
                            + if pos[axis] < thumb.min[axis] {
                                -step
                            } else {
                                step
                            },
                    );
                    state.repeat[axis] = now + 0.06;
                }
                ui.ctx().request_repaint_after(Duration::from_millis(60));
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            state.drag[axis] = None;
        }
        if let Some(target) = target {
            state.pending[axis] = Some(target.clamp(0.0, max));
            ui.ctx().request_repaint();
        }
        let ink = if max > 0.0 {
            Color32::from_gray(125)
        } else {
            Color32::from_gray(190)
        };
        for end in [false, true] {
            let mut center = rect.center();
            center[axis] = if end {
                rect.max[axis] - WIDTH / 2.0
            } else {
                rect.min[axis] + WIDTH / 2.0
            };
            let sign = if end { 1.0 } else { -1.0 };
            let mut a = center;
            let mut b = center;
            let mut c = center;
            a[axis] -= sign * 2.0;
            a[1 - axis] -= 3.0;
            b[axis] += sign * 1.5;
            c[axis] -= sign * 2.0;
            c[1 - axis] += 3.0;
            painter.line_segment([a, b], Stroke::new(1.0, ink));
            painter.line_segment([b, c], Stroke::new(1.0, ink));
        }
        if max > 0.0 {
            painter.rect_filled(
                thumb.shrink(2.0),
                0.0,
                Color32::from_gray(if response.hovered() { 180 } else { 202 }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(a: &mut HwpApp, size: Vec2, events: Vec<Event>) {
        let ctx = a.ctx.clone();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                a.toolbar(ui);
                a.status_bar(ui);
                a.page_view(ui);
            },
        );
        out.textures_delta.clear();
    }
    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    fn app(size: Vec2) -> HwpApp {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        frame(&mut a, size, vec![]);
        frame(&mut a, size, vec![]);
        a
    }
    #[test]
    fn rails_stay_at_window_edges_with_long_names_and_zoom() {
        let size = Vec2::new(1280.0, 980.0);
        let mut a = app(size);
        a.session.path = Some(PathBuf::from(format!(
            "{}.hwpx",
            "매우 긴 문서 이름".repeat(50)
        )));
        a.message = "문서 열기 완료: 긴 경로".repeat(100);
        for zoom in [0.5, 1.0, 2.0] {
            a.zoom = zoom;
            frame(&mut a, size, vec![]);
            frame(&mut a, size, vec![]);
            let v = a.document_scroll.rails[1].unwrap();
            let h = a.document_scroll.rails[0].unwrap();
            let viewport = a.wheel_zoom.viewport.unwrap();
            assert!((v.right() - size.x).abs() < 2.0, "{v:?}");
            assert!(
                (viewport.right() - v.left()).abs() < 2.0,
                "{viewport:?} {v:?}"
            );
            assert!(h.top() >= viewport.bottom());
            assert!(h.width() > size.x * 0.4);
            assert!(h.right() <= v.left() + 1.0);
        }
        // Long messages must not displace the right-aligned zoom control.
        frame(
            &mut a,
            size,
            vec![Event::PointerMoved(Pos2::new(size.x - 30.0, size.y - 12.0))],
        );
        frame(
            &mut a,
            size,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, 1.0),
                modifiers: Default::default(),
                phase: egui::TouchPhase::Move,
            }],
        );
        assert!((a.zoom - 2.1).abs() < 0.001);
    }
    #[test]
    fn rail_drag_moves_page_on_both_axes_without_editing() {
        let size = Vec2::new(1280.0, 980.0);
        let mut a = app(size);
        a.zoom = 2.0;
        frame(&mut a, size, vec![]);
        frame(&mut a, size, vec![]);
        let generation = a.session.generation;
        for axis in [0, 1] {
            let mut track = a.document_scroll.rails[axis].unwrap();
            track.min[axis] += WIDTH;
            track.max[axis] -= WIDTH;
            let (thumb, travel) = thumb_geometry(
                track,
                axis,
                a.document_scroll.viewport[axis],
                a.document_scroll.content[axis],
                a.document_scroll.offset[axis],
            );
            let from = thumb.center();
            let mut to = from;
            to[axis] += travel * 0.4;
            let before = a.view_page_rect.unwrap();
            // Windows can coalesce mouse down and motion into one frame.
            frame(
                &mut a,
                size,
                vec![
                    Event::PointerMoved(from),
                    button(from, true),
                    Event::PointerMoved(to),
                ],
            );
            frame(&mut a, size, vec![button(to, false)]);
            frame(&mut a, size, vec![]);
            assert!(a.document_scroll.offset[axis] > 20.0, "axis={axis}");
            assert!(a.view_page_rect.unwrap().min[axis] < before.min[axis] - 20.0);
        }
        assert_eq!(a.session.generation, generation);
        assert!(!a.session.dirty());
    }
    #[test]
    fn arrow_track_and_hidden_tabs_scroll_with_clamping() {
        let size = Vec2::new(1280.0, 980.0);
        let mut a = app(size);
        a.ext.shell.show_doc_tabs = false;
        frame(&mut a, size, vec![]);
        frame(&mut a, size, vec![]);
        assert!(a.document_scroll.rails[0].unwrap().width() > size.x - 2.0);
        let rail = a.document_scroll.rails[1].unwrap();
        let down = Pos2::new(rail.center().x, rail.bottom() - WIDTH / 2.0);
        frame(
            &mut a,
            size,
            vec![Event::PointerMoved(down), button(down, true)],
        );
        frame(&mut a, size, vec![button(down, false)]);
        assert!((a.document_scroll.offset.y - 40.0).abs() < 1.0);
        let track_bottom = down - Vec2::new(0.0, WIDTH);
        frame(
            &mut a,
            size,
            vec![
                Event::PointerMoved(track_bottom),
                button(track_bottom, true),
            ],
        );
        frame(&mut a, size, vec![button(track_bottom, false)]);
        let max = (a.document_scroll.content.y - a.document_scroll.viewport.y).max(0.0);
        assert!((a.document_scroll.offset.y - max).abs() < 1.0);
        a.zoom = 0.25;
        frame(&mut a, size, vec![]);
        frame(&mut a, size, vec![]);
        assert_eq!(a.document_scroll.offset, Vec2::ZERO);
    }
}
