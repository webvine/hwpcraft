//! 개체 이동 안내선 (보기 ▸ 안내선): while a floating object is dragged, its left /
//! centre / right and top / middle / bottom edges snap to the paper's, the body area's and
//! the other objects' on the page within a few screen pixels, and the matching line is
//! drawn across the page, as in Hangul. Alt held moves freely. The move itself stays in
//! the object area (`ext_object`); it asks here for the snapped offset.
use super::*;

/// How close (screen pixels) an edge comes before it snaps.
const SNAP_PX: f32 = 6.0;

/// The page's snap lines: x positions and y positions in page pixels.
#[derive(Clone, Default)]
pub(crate) struct Targets {
    xs: Vec<f32>,
    ys: Vec<f32>,
}

/// Where a moved box snaps: the offset to add to the raw move, and the lines that match.
pub(crate) fn snap(
    bbox: Rect,
    raw: Vec2,
    targets: &Targets,
    tolerance: f32,
) -> (Vec2, Vec<(bool, f32)>) {
    let moved = bbox.translate(raw);
    let best = |edges: [f32; 3], lines: &[f32]| -> Option<(f32, f32)> {
        edges
            .iter()
            .flat_map(|e| lines.iter().map(move |l| (l - e, *l)))
            .filter(|(d, _)| d.abs() <= tolerance)
            .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
    };
    let mut offset = raw;
    let mut guides = Vec::new();
    if let Some((d, x)) = best([moved.left(), moved.center().x, moved.right()], &targets.xs) {
        offset.x += d;
        guides.push((true, x));
    }
    if let Some((d, y)) = best([moved.top(), moved.center().y, moved.bottom()], &targets.ys) {
        offset.y += d;
        guides.push((false, y));
    }
    (offset, guides)
}

impl HwpApp {
    /// The snap lines of `page`: paper edges and centre, body area edges and centre, and
    /// the edges of the page's objects other than `skip` (the ones being moved).
    fn move_targets(&mut self, page: u32, skip: &[Rect]) -> Targets {
        let mut t = Targets::default();
        if let Ok(info) = self.session.page_info(page) {
            let n = |k: &str| info[k].as_f64().unwrap_or(0.0) as f32;
            let (w, h) = (n("width"), n("height"));
            let top = n("marginTop") + n("marginHeader");
            let bottom = h - n("marginBottom") - n("marginFooter");
            let (left, right) = (n("bodyLeft"), w - n("bodyRight"));
            t.xs = vec![0.0, w / 2.0, w, left, (left + right) / 2.0, right];
            t.ys = vec![0.0, h / 2.0, h, top, (top + bottom) / 2.0, bottom];
        }
        let layout = crate::ext_table::cached_page_layout(
            &mut self.ext.shell.move_layout,
            &self.session,
            page,
        );
        for c in layout["controls"].as_array().into_iter().flatten() {
            let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
            let r = Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")));
            if r.width() <= 0.0
                || r.height() <= 0.0
                || skip.iter().any(|s| {
                    s.distance_to_pos(r.center()) < 1.0 && (s.size() - r.size()).length() < 2.0
                })
            {
                continue;
            }
            t.xs.extend([r.left(), r.center().x, r.right()]);
            t.ys.extend([r.top(), r.center().y, r.bottom()]);
        }
        t
    }

    /// The move to show and commit for a floating object at `bbox` dragged by `raw`
    /// (page pixels), with the 개체 이동 안내선 that match; `skip` are the boxes moving
    /// along. Alt held, or the guides switched off, leaves `raw` as it is.
    pub(crate) fn object_move_snapped(
        &mut self,
        page: u32,
        bbox: Rect,
        skip: &[Rect],
        raw: Vec2,
        alt: bool,
    ) -> Vec2 {
        if !self.ext.shell.object_guides || alt {
            self.ext.shell.move_guides.clear();
            return raw;
        }
        if self
            .ext
            .shell
            .move_targets
            .as_ref()
            .is_none_or(|(p, g, _)| *p != page || *g != self.session.generation)
        {
            let mut skip = skip.to_vec();
            skip.push(bbox);
            let targets = self.move_targets(page, &skip);
            self.ext.shell.move_targets = Some((page, self.session.generation, targets));
        }
        let Some((_, _, targets)) = self.ext.shell.move_targets.as_ref() else {
            return raw;
        };
        let (offset, guides) = snap(bbox, raw, targets, SNAP_PX / self.zoom.max(0.1));
        self.ext.shell.move_guides = guides;
        offset
    }

    /// Forget the guides when a move ends.
    pub(crate) fn clear_move_guides(&mut self) {
        self.ext.shell.move_guides.clear();
        self.ext.shell.move_targets = None;
    }

    /// Draw the matching 개체 이동 안내선 across `page` (`rect` on screen).
    pub(crate) fn paint_move_guides(&self, painter: &egui::Painter, rect: Rect) {
        let stroke = Stroke::new(1.0, Color32::from_rgb(230, 40, 140));
        for &(vertical, at) in &self.ext.shell.move_guides {
            let at = at * self.zoom;
            let line = if vertical {
                [
                    Pos2::new(rect.left() + at, rect.top()),
                    Pos2::new(rect.left() + at, rect.bottom()),
                ]
            } else {
                [
                    Pos2::new(rect.left(), rect.top() + at),
                    Pos2::new(rect.right(), rect.top() + at),
                ]
            };
            painter.line_segment(line, stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets() -> Targets {
        Targets {
            xs: vec![0.0, 400.0, 800.0, 100.0],
            ys: vec![0.0, 500.0, 1000.0],
        }
    }

    /// A box dragged near the page centre snaps its centre onto it; far away it stays.
    #[test]
    fn a_moved_box_snaps_its_nearest_edge_to_a_line() {
        let bbox = Rect::from_min_size(Pos2::new(300.0, 300.0), Vec2::new(100.0, 50.0));
        // Centre x 350 + 47 = 397: 3 px from the page centre 400.
        let (offset, guides) = snap(bbox, Vec2::new(47.0, 0.0), &targets(), 6.0);
        assert_eq!(offset, Vec2::new(50.0, 0.0));
        assert_eq!(guides, [(true, 400.0)]);
        // Left edge 300 - 197 = 103: 3 px from the body edge 100; top 300 + 198 = 498.
        let (offset, guides) = snap(bbox, Vec2::new(-197.0, 198.0), &targets(), 6.0);
        assert_eq!(offset, Vec2::new(-200.0, 200.0));
        assert_eq!(guides, [(true, 100.0), (false, 500.0)]);
        // Nothing within reach: the raw move.
        let (offset, guides) = snap(bbox, Vec2::new(20.0, 20.0), &targets(), 6.0);
        assert_eq!(offset, Vec2::new(20.0, 20.0));
        assert!(guides.is_empty());
    }

    /// Through the page canvas: a rectangle dragged to 3 px left of the page centre lands
    /// centred on it; with the guides off it lands where it was dropped.
    #[test]
    fn a_dragged_shape_snaps_to_the_page_centre() -> anyhow::Result<()> {
        let centre_after = |guides: bool| -> anyhow::Result<f32> {
            let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
            a.ext.shell.object_guides = guides;
            a.session.run(
                "shape.insert",
                serde_json::json!({"kind":"rectangle","width_mm":40.0,"height_mm":20.0,"x_mm":30.0,"y_mm":60.0}),
            )?;
            let ctx = a.ctx.clone();
            let page_rect =
                Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(794.0, 1123.0) * a.zoom);
            let mut time = 0.0;
            let mut frame = |a: &mut HwpApp, events: Vec<egui::Event>| {
                time += 0.05;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 1400.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    let caret = a.session.caret().ok();
                    a.page_canvas(ui, &ctx, 0, page_rect, caret.as_ref(), false);
                });
                out.textures_delta.clear();
            };
            let shape = |a: &HwpApp| -> Option<Rect> {
                let layout = crate::ext_table::cached_page_layout(&mut None, &a.session, 0);
                layout["controls"]
                    .as_array()?
                    .iter()
                    .find(|c| c["type"] == "shape")
                    .map(|c| {
                        let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
                        Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")))
                    })
            };
            let bbox = shape(&a).ok_or_else(|| anyhow::anyhow!("no shape"))?;
            let page_w = a.session.page_info(0)?["width"].as_f64().unwrap_or(0.0) as f32;
            let screen = |p: Pos2| page_rect.min + p.to_vec2() * a.zoom;
            let start = screen(bbox.center());
            // Drop the centre 3 px (page) left of the page centre.
            let target = screen(Pos2::new(page_w / 2.0 - 3.0, bbox.center().y));
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(&mut a, vec![]);
            frame(
                &mut a,
                vec![
                    egui::Event::PointerMoved(start),
                    button(start, true),
                    button(start, false),
                ],
            );
            frame(&mut a, vec![]);
            frame(
                &mut a,
                vec![egui::Event::PointerMoved(start), button(start, true)],
            );
            for i in 1..=8 {
                let p = start + (target - start) * (i as f32 / 8.0);
                frame(&mut a, vec![egui::Event::PointerMoved(p)]);
            }
            frame(&mut a, vec![button(target, false)]);
            frame(&mut a, vec![]);
            anyhow::ensure!(a.error.is_none(), "{:?}", a.error);
            let after = shape(&a).ok_or_else(|| anyhow::anyhow!("no shape after"))?;
            Ok(after.center().x - page_w / 2.0)
        };
        let snapped = centre_after(true)?;
        assert!(snapped.abs() < 0.6, "centred on the page: off by {snapped}");
        let free = centre_after(false)?;
        assert!(
            (free + 3.0).abs() < 1.0,
            "dropped where it was let go: {free}"
        );
        Ok(())
    }
}
