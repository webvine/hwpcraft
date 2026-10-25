//! Document zoom; ribbon and dialog sizes stay fixed.
use super::*;

#[derive(Default)]
pub(crate) struct State {
    pub viewport: Option<Rect>,
    pub pages: Vec<(u32, Rect)>,
    remainder: f32,
}

impl HwpApp {
    fn apply_wheel_zoom(&mut self, ctx: &egui::Context, require_ctrl: bool) -> Option<f32> {
        let delta = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } if (!require_ctrl || modifiers.ctrl)
                        && !modifiers.alt
                        && !modifiers.shift =>
                    {
                        let amount = match unit {
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Point => delta.y / 40.0,
                            egui::MouseWheelUnit::Page => delta.y * 10.0,
                        };
                        amount.is_finite().then_some(amount * 0.1)
                    }
                    _ => None,
                })
                .sum::<f32>()
        });
        if delta == 0.0 {
            return None;
        }
        ctx.input_mut(|i| {
            i.events.retain(|event|!matches!(event,Event::MouseWheel{modifiers,..} if (!require_ctrl || modifiers.ctrl) && !modifiers.alt && !modifiers.shift));
            i.smooth_scroll_delta=Vec2::ZERO;
        });
        let old = self.zoom;
        let wanted = (old + self.wheel_zoom.remainder + delta).clamp(0.1, 5.0);
        self.set_zoom(wanted);
        self.wheel_zoom.remainder = wanted - self.zoom;
        if (old - self.zoom).abs() < 0.0001 {
            return None;
        }
        ctx.request_repaint();
        Some(old)
    }
    pub(crate) fn status_wheel_zoom(&mut self, response: &egui::Response) {
        if response.hovered()
            && !self.ext_modal_open()
            && self.error.is_none()
            && self.pending.is_none()
            && !self.show_help
            && self.page_setup.is_none()
            && !self.show_character
            && !self.show_paragraph
            && !self.show_table
            && self.feature_dialog.is_none()
            && !response.ctx.input(|i| i.pointer.any_down())
        {
            let _ = self.apply_wheel_zoom(&response.ctx, false);
        }
    }
    /// Runs before ScrollArea layout, so Ctrl+wheel never also scrolls the document.
    pub(crate) fn document_wheel_zoom(&mut self, ui: &egui::Ui, sizes: &[Vec2]) -> Option<Vec2> {
        if self.ext_modal_open()
            || self.error.is_some()
            || self.pending.is_some()
            || self.show_help
            || self.page_setup.is_some()
            || self.show_character
            || self.show_paragraph
            || self.show_table
            || self.feature_dialog.is_some()
        {
            return None;
        }
        let ctx = ui.ctx();
        let viewport = self.wheel_zoom.viewport?;
        let pointer = ctx.input(|i| i.pointer.hover_pos())?;
        if !viewport.contains(pointer)
            || ctx.layer_id_at(pointer) != Some(ui.layer_id())
            || ctx.input(|i| i.pointer.any_down())
        {
            return None;
        }
        let old = self.apply_wheel_zoom(ctx, true)?;
        // Keep the page point under the mouse stable, unless a scroll limit is reached.
        let (index, page) = self
            .wheel_zoom
            .pages
            .iter()
            .find(|(_, r)| r.contains(pointer))
            .copied()?;
        let logical = (pointer - page.min) / old;
        let page_size = *sizes.get(index as usize)? * self.zoom;
        let top = 15.0
            + sizes
                .iter()
                .take(index as usize)
                .map(|s| s.y * self.zoom + PAGE_GAP)
                .sum::<f32>();
        let left = ((viewport.width() - page_size.x) / 2.0).max(0.0);
        let content = Vec2::new(left, top) + logical * self.zoom;
        Some((content - (pointer - viewport.min)).max(Vec2::ZERO))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(app: &mut HwpApp, events: Vec<Event>) {
        let ctx = app.ctx.clone();
        let mut output = ctx.run_ui(
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
        output.textures_delta.clear();
    }
    fn wheel(delta: f32, ctrl: bool) -> Event {
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(0.0, delta),
            modifiers: egui::Modifiers {
                ctrl,
                command: ctrl,
                ..Default::default()
            },
            phase: egui::TouchPhase::Move,
        }
    }
    fn app() -> HwpApp {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        frame(&mut a, vec![]);
        frame(&mut a, vec![]);
        a
    }

    #[test]
    fn wheel_over_status_percentage_works_without_ctrl() {
        let mut a = app();
        frame(&mut a, vec![Event::PointerMoved(Pos2::new(1470.0, 903.0))]);
        frame(&mut a, vec![wheel(1.0, false)]);
        assert_eq!(a.zoom, 1.0);
        frame(&mut a, vec![wheel(-1.0, false)]);
        assert_eq!(a.zoom, 0.9);
    }
    #[test]
    fn small_pixel_wheel_events_accumulate_and_horizontal_wheel_is_ignored() {
        let mut a = app();
        let p = Pos2::new(a.wheel_zoom.pages[0].1.center().x, 400.0);
        frame(&mut a, vec![Event::PointerMoved(p)]);
        for _ in 0..4 {
            frame(
                &mut a,
                vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, 1.0),
                    modifiers: egui::Modifiers::CTRL,
                    phase: egui::TouchPhase::Move,
                }],
            );
        }
        assert_eq!(a.zoom, 0.91);
        frame(
            &mut a,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(4.0, 0.0),
                modifiers: egui::Modifiers::CTRL,
                phase: egui::TouchPhase::Move,
            }],
        );
        assert_eq!(a.zoom, 0.91);
    }
    #[test]
    fn ctrl_wheel_zooms_document_around_pointer_without_editing_or_ui_scaling() {
        let mut a = app();
        let before = a.wheel_zoom.pages[0].1;
        let p = Pos2::new(before.center().x, before.top() + 320.0);
        let logical = (p - before.min) / a.zoom;
        let generation = a.session.generation;
        let dirty = a.session.dirty();
        frame(&mut a, vec![Event::PointerMoved(p), wheel(1.0, true)]);
        assert_eq!(a.zoom, 1.0);
        frame(&mut a, vec![]);
        let after = a.wheel_zoom.pages[0].1;
        assert!(
            ((p - after.min) / a.zoom - logical).length() < 2.0,
            "{before:?} -> {after:?}"
        );
        assert_eq!(a.ctx.zoom_factor(), 1.0);
        assert_eq!(a.session.generation, generation);
        assert_eq!(a.session.dirty(), dirty);
        frame(&mut a, vec![wheel(-1.0, true)]);
        assert_eq!(a.zoom, 0.9);
    }
    #[test]
    fn ordinary_wheel_scrolls_and_zoom_is_bounded_and_ignores_ribbon_and_modals() {
        let mut a = app();
        let p = a.wheel_zoom.pages[0]
            .1
            .center()
            .min(Pos2::new(800.0, 600.0));
        let y = a.wheel_zoom.pages[0].1.top();
        frame(&mut a, vec![Event::PointerMoved(p), wheel(-4.0, false)]);
        frame(&mut a, vec![]);
        assert_eq!(a.zoom, 0.9);
        assert!(a.wheel_zoom.pages[0].1.top() < y);
        frame(
            &mut a,
            vec![
                Event::PointerMoved(Pos2::new(300.0, 60.0)),
                wheel(1.0, true),
            ],
        );
        assert_eq!(a.zoom, 0.9);
        a.error = Some("modal".into());
        frame(&mut a, vec![Event::PointerMoved(p), wheel(1.0, true)]);
        assert_eq!(a.zoom, 0.9);
        a.error = None;
        frame(&mut a, vec![]);
        frame(&mut a, vec![]);
        frame(&mut a, vec![wheel(1000.0, true)]);
        assert_eq!(a.zoom, 5.0);
        frame(&mut a, vec![wheel(-1000.0, true)]);
        assert_eq!(a.zoom, 0.1);
    }
}
