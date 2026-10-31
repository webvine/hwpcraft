//! 편집 화면 나누기: the same document in two panes (one above the other, or side by
//! side), each with its own scroll position and page. The active pane's view state stays
//! in the usual `HwpApp` fields, so the unsplit view runs exactly as before; the other
//! pane's state waits here and is swapped in only while that pane is drawn. Pressing
//! the mouse in the other pane makes it the active one before the frame is laid out.
use super::*;

/// What one pane remembers while the other pane is active.
#[derive(Default)]
pub(crate) struct PaneState {
    page: u32,
    shown_page: u32,
    view_page_rect: Option<Rect>,
    document_scroll: document_scroll::State,
    wheel_zoom: wheel_zoom::State,
}

pub(crate) struct Split {
    /// Side by side (true) or one above the other (false).
    pub side_by_side: bool,
    /// Share of the first pane, 0.15–0.85.
    pub ratio: f32,
    other: PaneState,
    /// Screen slot (0 = top/left, 1 = bottom/right) holding the active pane.
    active_slot: usize,
    rects: [Option<Rect>; 2],
}

impl Split {
    /// Show page `page` (0-based) in the inactive pane (start-up views and tests).
    pub(crate) fn set_other_page(&mut self, page: u32) {
        self.other.page = page;
    }
}

/// Width of the bar between the panes.
const DIVIDER: f32 = 6.0;

impl HwpApp {
    fn swap_pane(&mut self, other: &mut PaneState) {
        std::mem::swap(&mut self.page, &mut other.page);
        std::mem::swap(&mut self.shown_page, &mut other.shown_page);
        std::mem::swap(&mut self.view_page_rect, &mut other.view_page_rect);
        std::mem::swap(&mut self.document_scroll, &mut other.document_scroll);
        std::mem::swap(&mut self.wheel_zoom, &mut other.wheel_zoom);
    }

    /// Split the view (or change its direction); `None` joins it again. Both panes start
    /// where the active pane is.
    pub(crate) fn set_split(&mut self, side_by_side: Option<bool>) {
        let offset = self.document_scroll.offset();
        match (side_by_side, self.split.as_mut()) {
            (None, None) => {}
            (None, Some(_)) => {
                self.split = None;
                // The single view uses its own scroll state again: keep the place.
                self.document_scroll.pending = [Some(offset.x), Some(offset.y)];
            }
            (Some(side), Some(split)) => split.side_by_side = side,
            (Some(side), None) => {
                let mut other = PaneState {
                    page: self.page,
                    shown_page: self.page,
                    ..Default::default()
                };
                other.document_scroll.pending = [Some(offset.x), Some(offset.y)];
                self.document_scroll.pending = [Some(offset.x), Some(offset.y)];
                self.split = Some(Split {
                    side_by_side: side,
                    ratio: 0.5,
                    other,
                    active_slot: 0,
                    rects: [None; 2],
                });
            }
        }
    }

    /// The document area: one page view, or two panes with a divider.
    pub(crate) fn document_views(&mut self, ui: &mut egui::Ui) {
        let Some(mut split) = self.split.take() else {
            self.pane = 0;
            self.pane_active = true;
            self.page_view(ui);
            return;
        };
        let ctx = ui.ctx().clone();
        // A press in the other pane makes it active before anything is laid out, so the
        // same click places the caret there.
        let inactive = 1 - split.active_slot;
        if let Some(pos) = ctx.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        }) && split.rects[inactive].is_some_and(|r| r.contains(pos))
        {
            self.swap_pane(&mut split.other);
            split.active_slot = inactive;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_gray(245)))
            .show(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                let (a, divider, b) = pane_rects(full, split.side_by_side, split.ratio);
                let bar = ui.interact(divider, ui.id().with("split-divider"), egui::Sense::drag());
                let bar = bar.on_hover_cursor(if split.side_by_side {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                });
                if let Some(pos) = bar.interact_pointer_pos().filter(|_| bar.dragged()) {
                    split.ratio = if split.side_by_side {
                        (pos.x - full.left()) / full.width()
                    } else {
                        (pos.y - full.top()) / full.height()
                    }
                    .clamp(0.15, 0.85);
                }
                ui.painter()
                    .rect_filled(divider, 0.0, Color32::from_gray(205));
                for (slot, rect) in [(0, a), (1, b)] {
                    let active = slot == split.active_slot;
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .id_salt(("document-pane", slot)),
                    );
                    child.set_clip_rect(rect);
                    self.pane = slot;
                    self.pane_active = active;
                    if active {
                        self.page_view(&mut child);
                    } else {
                        // The caret follows only in the active pane.
                        let follow = std::mem::take(&mut self.scroll_to_caret);
                        self.swap_pane(&mut split.other);
                        self.page_view(&mut child);
                        self.swap_pane(&mut split.other);
                        self.scroll_to_caret = follow;
                    }
                    if active {
                        ui.painter().rect_stroke(
                            rect.shrink(0.5),
                            0.0,
                            Stroke::new(1.0, Color32::from_rgb(120, 160, 220)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                split.rects = [Some(a), Some(b)];
            });
        self.pane = split.active_slot;
        self.pane_active = true;
        self.split = Some(split);
    }
}

/// The two pane rectangles and the divider between them.
pub(crate) fn pane_rects(full: Rect, side_by_side: bool, ratio: f32) -> (Rect, Rect, Rect) {
    let ratio = ratio.clamp(0.15, 0.85);
    if side_by_side {
        let x = full.left() + (full.width() - DIVIDER) * ratio;
        (
            Rect::from_min_max(full.min, Pos2::new(x, full.bottom())),
            Rect::from_min_max(
                Pos2::new(x, full.top()),
                Pos2::new(x + DIVIDER, full.bottom()),
            ),
            Rect::from_min_max(Pos2::new(x + DIVIDER, full.top()), full.max),
        )
    } else {
        let y = full.top() + (full.height() - DIVIDER) * ratio;
        (
            Rect::from_min_max(full.min, Pos2::new(full.right(), y)),
            Rect::from_min_max(
                Pos2::new(full.left(), y),
                Pos2::new(full.right(), y + DIVIDER),
            ),
            Rect::from_min_max(Pos2::new(full.left(), y + DIVIDER), full.max),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(a: &mut HwpApp, events: Vec<egui::Event>) {
        let ctx = a.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                a.page_sizes();
                a.document_views(ui);
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn panes_split_the_area_around_a_divider() {
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 600.0));
        let (a, bar, b) = pane_rects(full, false, 0.5);
        assert_eq!((a.top(), b.bottom()), (0.0, 600.0));
        assert_eq!((bar.height(), a.height()), (DIVIDER, b.height()));
        let (a, _, b) = pane_rects(full, true, 0.9);
        assert!(
            (a.width() / (a.width() + b.width()) - 0.85).abs() < 0.01,
            "clamped"
        );
    }

    #[test]
    fn a_press_in_the_other_pane_makes_it_active_and_joining_keeps_one_view() -> anyhow::Result<()>
    {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        for i in 0..3 {
            if i > 0 {
                a.session.execute(Command::PageBreak)?;
            }
            a.session.execute(Command::Insert(format!("{}쪽", i + 1)))?;
        }
        assert!(a.shell_ext_enabled("view.split.none") == Some(false));
        a.shell_ext_dispatch("view.split", &a.ctx.clone());
        assert!(a.shell_ext_checked("view.split"));
        frame(&mut a, vec![]);
        frame(&mut a, vec![]);
        let split = a.split.as_ref().expect("split");
        let lower = split.rects[1].expect("second pane");
        assert_eq!(split.active_slot, 0);
        // The lower pane shows another place of the document.
        if let Some(split) = a.split.as_mut() {
            split.other.page = 2;
        }
        let press = |pressed| egui::Event::PointerButton {
            pos: lower.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut a,
            vec![egui::Event::PointerMoved(lower.center()), press(true)],
        );
        let split = a.split.as_ref().expect("split");
        assert_eq!(split.active_slot, 1, "the lower pane is active now");
        assert_eq!(split.other.page, 0, "the upper pane's state waits");
        frame(&mut a, vec![press(false)]);
        a.shell_ext_dispatch("view.split.none", &a.ctx.clone());
        assert!(a.split.is_none());
        frame(&mut a, vec![]);
        Ok(())
    }
}
