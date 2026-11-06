//! 머리말·꼬리말 편집 화면: a double click on a page's header or footer area enters it,
//! as in Hangul; Esc, a double click on the body or 닫기 leaves. While one is edited the
//! rest of the page is dimmed, the edited area has a dashed frame and a label on the
//! page it was entered from, and the 머리말/꼬리말 tab and a status chip name it. The
//! engine keeps which one is edited and its caret (`Session::hf_*`).
use super::*;
use hwpcraft_engine::{HfApply, HfHit, HfTarget};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

/// The 머리말/꼬리말 context tab (ribbon `TABS`).
pub(crate) const HF_TAB: usize = 13;
const LEAVE_HINT: &str = "Esc나 본문 두 번 누르기로 마칩니다.";

/// Areas of the pages drawn while a header or footer is edited: each lookup builds a
/// page's render tree, so they are kept per (generation, hf revision).
#[derive(Default)]
pub(crate) struct HfShell {
    info: RefCell<(u64, u64, HashMap<u32, Value>)>,
    /// Generation the edited header or footer was last checked against (undo, redo).
    checked: u64,
}

/// `rect` less the `bright` rectangles, as horizontal strips.
fn outside(rect: Rect, bright: &[Rect]) -> Vec<Rect> {
    let bright: Vec<Rect> = bright
        .iter()
        .map(|b| b.intersect(rect))
        .filter(|b| b.is_positive())
        .collect();
    let mut ys = vec![rect.min.y, rect.max.y];
    ys.extend(bright.iter().flat_map(|b| [b.min.y, b.max.y]));
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let mut parts = Vec::new();
    for w in ys.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        let mut covered: Vec<(f32, f32)> = bright
            .iter()
            .filter(|b| b.min.y < y1 && b.max.y > y0)
            .map(|b| (b.min.x, b.max.x))
            .collect();
        covered.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut x = rect.min.x;
        for (from, to) in covered.into_iter().chain([(rect.max.x, rect.max.x)]) {
            if from > x {
                parts.push(Rect::from_min_max(Pos2::new(x, y0), Pos2::new(from, y1)));
            }
            x = x.max(to);
        }
    }
    parts
}

/// 머리말(양쪽), 꼬리말(홀수 쪽), …
pub(crate) fn hf_label(t: HfTarget) -> String {
    let kind = if t.footer { "꼬리말" } else { "머리말" };
    let pages = match t.apply_to {
        HfApply::Both => "양쪽",
        HfApply::Odd => "홀수 쪽",
        HfApply::Even => "짝수 쪽",
    };
    format!("{kind}({pages})")
}

impl HwpApp {
    /// The header and footer areas of `page` (`Session::hf_page_info`), remembered.
    fn hf_info(&self, page: u32) -> Option<Value> {
        let key = (self.session.generation, self.session.hf_revision());
        let mut memo = self.hf.info.borrow_mut();
        if (memo.0, memo.1) != key {
            *memo = (key.0, key.1, HashMap::new());
        }
        if let Some(v) = memo.2.get(&page) {
            return Some(v.clone());
        }
        let mut v = self.session.hf_page_info(page).ok()?;
        // Boxes of the edited header's objects, which may reach out of its area.
        let layout = self
            .session
            .page_control_layout(page)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let objects: Vec<Value> = layout
            .iter()
            .flat_map(|l| l["controls"].as_array().into_iter().flatten())
            .filter(|c| self.session.hf_layout_address(c).is_some())
            .map(|c| json!([c["x"], c["y"], c["w"], c["h"]]))
            .collect();
        v["objects"] = json!(objects);
        memo.2.insert(page, v.clone());
        Some(v)
    }

    /// Edit `hit` from `page`, the caret at page point `at` when given. A page that hides
    /// it says so; one the section lacks is made (one undo step).
    pub(crate) fn hf_open(&mut self, hit: HfHit, page: u32, at: Option<(f64, f64)>) -> bool {
        let label = hf_label(hit.target);
        if hit.hidden {
            self.message = format!("이 쪽은 {label}을 감췄습니다(쪽 ▸ 현재 쪽만 감추기).");
            return false;
        }
        if self.session.is_distribution() {
            self.message = hwpcraft_engine::DISTRIBUTION_READ_ONLY.into();
            return false;
        }
        let r = match at {
            Some((x, y)) if hit.exists => self.session.hf_enter_at(hit.target, page, x, y),
            _ => self.session.hf_enter(hit.target, page),
        };
        if self.result(r).is_none() {
            return false;
        }
        self.hf.checked = self.session.generation;
        // A body object picked before stays out of the ribbon's object tabs, whose
        // commands would reach it.
        self.clear_object_selection();
        // Another page than the one shown scrolls there (이전·다음).
        self.page = page;
        self.tab = HF_TAB;
        self.message = if hit.exists {
            format!("{label} 편집: {LEAVE_HINT}")
        } else {
            format!("{label}을 새로 만들었습니다. {LEAVE_HINT}")
        };
        true
    }

    /// Back to the body.
    pub(crate) fn hf_leave(&mut self) {
        if self.session.hf_target().is_some() {
            self.session.hf_exit();
            self.message = "본문 편집으로 돌아왔습니다.".into();
        }
    }

    /// Pointer input on `page` (drawn at `rect`) for header/footer editing; true when it
    /// took the input. Outside editing only a double click on a header or footer area
    /// is taken (it enters it). While editing every press is: one in the edited area
    /// moves its caret, a double click on the body leaves (the body caret goes there),
    /// a double click on the other area switches to it.
    pub(crate) fn hf_canvas(&mut self, page: u32, response: &egui::Response, rect: Rect) -> bool {
        let at = |s: &Self, p: Pos2| {
            let v = (p - rect.min) / s.zoom;
            (f64::from(v.x), f64::from(v.y))
        };
        let pointer = response.interact_pointer_pos();
        let Some(target) = self.session.hf_target() else {
            let Some(p) = pointer.filter(|_| response.double_clicked()) else {
                return false;
            };
            let (x, y) = at(self, p);
            return match self.session.hf_hit(page, x, y) {
                Ok(Some(hit)) => {
                    self.hf_open(hit, page, Some((x, y)));
                    self.shown_page = page;
                    true
                }
                Ok(None) => false,
                Err(e) => {
                    self.result(Err::<(), _>(e));
                    false
                }
            };
        };
        // Objects in the edited header or footer come first (hf_objects).
        if self.hf_object_canvas(page, response, rect) {
            return true;
        }
        let Some(p) = pointer else {
            return true;
        };
        let (x, y) = at(self, p);
        if response.double_clicked() || response.triple_clicked() {
            match self.session.hf_hit(page, x, y) {
                // As in Hangul: the word, or the paragraph on a triple click.
                Ok(Some(hit)) if hit.target == target => {
                    let r = if response.triple_clicked() {
                        self.session.hf_select_paragraph()
                    } else {
                        self.session.hf_select_word()
                    };
                    self.result(r);
                }
                Ok(Some(hit)) => {
                    self.hf_open(hit, page, Some((x, y)));
                    self.shown_page = page;
                }
                _ => {
                    self.hf_leave();
                    if let Ok(c) = self.session.hit_test(page, x, y) {
                        let r = self.session.select(c, false);
                        self.result(r);
                    }
                }
            }
            return true;
        }
        let extend = response.dragged() && !response.drag_started()
            || response.ctx.input(|i| i.modifiers.shift);
        if response.clicked() || response.drag_started() || response.dragged() {
            let p = if response.drag_started() {
                response
                    .ctx
                    .input(|i| i.pointer.press_origin())
                    .unwrap_or(p)
            } else {
                p
            };
            let (x, y) = at(self, p);
            match self.session.hf_click(page, x, y, extend) {
                Ok(true) => {
                    self.page = page;
                    self.shown_page = page;
                }
                Ok(false) if response.clicked() => {
                    self.message = format!(
                        "{} 편집 중입니다. 본문을 두 번 누르면 마칩니다.",
                        hf_label(target)
                    );
                }
                Ok(false) => {}
                Err(e) => {
                    self.result(Err::<(), _>(e));
                }
            }
        }
        true
    }

    /// While a header or footer is edited: dim `page` outside the edited kind's area and,
    /// on the page it was entered from, frame and label that area and draw the caret
    /// and selection.
    pub(crate) fn hf_paint(&self, painter: &egui::Painter, page: u32, rect: Rect) {
        let Some(target) = self.session.hf_target() else {
            return;
        };
        let Some(info) = self.hf_info(page) else {
            return;
        };
        let side = &info[if target.footer { "footer" } else { "header" }];
        let r = |i: usize| side["rect"][i].as_f64().unwrap_or(0.0) as f32 * self.zoom;
        let area = Rect::from_min_size(rect.min + Vec2::new(r(0), r(1)), Vec2::new(r(2), r(3)))
            .intersect(rect);
        let dim = Color32::from_rgba_unmultiplied(255, 255, 255, 150);
        // A page that shows another header (the even one while the odd one is edited)
        // or hides it is dimmed whole.
        let shown = serde_json::from_value::<HfTarget>(side["target"].clone()).ok();
        if shown != Some(target) || side["hidden"] == true {
            painter.rect_filled(rect, 0.0, dim);
            return;
        }
        // The area and the edited header's objects (which may reach out of it) stay
        // bright.
        let mut bright = vec![area];
        for o in info["objects"].as_array().into_iter().flatten() {
            let f = |i: usize| o[i].as_f64().unwrap_or(0.0) as f32 * self.zoom;
            bright.push(Rect::from_min_size(
                rect.min + Vec2::new(f(0), f(1)),
                Vec2::new(f(2), f(3)),
            ));
        }
        for part in outside(rect, &bright) {
            painter.rect_filled(part, 0.0, dim);
        }
        self.hf_object_paint(painter, page, rect);
        if self.session.hf_page() != Some(page) {
            return;
        }
        let blue = Color32::from_rgb(40, 110, 220);
        let corners = [
            area.left_top(),
            area.right_top(),
            area.right_bottom(),
            area.left_bottom(),
            area.left_top(),
        ];
        painter.extend(egui::Shape::dashed_line(
            &corners,
            egui::Stroke::new(1.0, blue),
            5.0,
            3.0,
        ));
        // The label sits outside the area, towards the page edge, so it covers no text.
        let galley = painter.layout_no_wrap(
            hf_label(target),
            egui::FontId::proportional(11.0),
            Color32::WHITE,
        );
        let size = galley.size() + Vec2::new(8.0, 2.0);
        let min = if target.footer {
            Pos2::new(area.min.x, area.max.y)
        } else {
            Pos2::new(area.min.x, area.min.y - size.y)
        };
        let tag = Rect::from_min_size(min, size);
        painter.rect_filled(tag, 2.0, blue);
        painter.galley(tag.min + Vec2::new(4.0, 1.0), galley, Color32::WHITE);
        let n = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0) as f32 * self.zoom;
        for s in self
            .session
            .hf_selection_rects(page)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
        {
            if s["pageIndex"]
                .as_u64()
                .is_some_and(|p| p != u64::from(page))
            {
                continue;
            }
            painter.rect_filled(
                Rect::from_min_size(
                    rect.min + Vec2::new(n(&s, "x"), n(&s, "y")),
                    Vec2::new(n(&s, "width"), n(&s, "height")),
                ),
                0.0,
                Color32::from_rgba_unmultiplied(66, 133, 244, 65),
            );
        }
        if let Ok(c) = self.session.hf_caret_rect()
            && c["pageIndex"].as_u64() == Some(u64::from(page))
        {
            let top = rect.min + Vec2::new(n(&c, "x"), n(&c, "y"));
            let height = n(&c, "height").max(4.0);
            painter.line_segment(
                [top, top + Vec2::new(0.0, height)],
                egui::Stroke::new(1.5, Color32::BLACK),
            );
            // The body caret, which tells the input method where typing goes, is hidden
            // here: egui turns the input method on (and puts its candidate window by the
            // caret) only in frames that report one, so the header caret does.
            let ctx = painter.ctx();
            if ctx.memory(|m| m.has_focus(egui::Id::new("document-canvas"))) {
                let caret = Rect::from_min_size(top, Vec2::new(2.0, height));
                ctx.output_mut(|o| {
                    o.ime = Some(egui::output::IMEOutput {
                        purpose: egui::IMEPurpose::Normal,
                        rect,
                        cursor_rect: caret,
                        should_interrupt_composition: false,
                    })
                });
                // The syllable being composed, at the caret as in the body.
                if !self.preedit.is_empty() {
                    let galley = painter.layout_no_wrap(
                        self.preedit.clone(),
                        egui::FontId::proportional(height.max(16.0)),
                        Color32::BLACK,
                    );
                    let r = Rect::from_min_size(top, galley.size());
                    painter.rect_filled(r, 0.0, Color32::WHITE);
                    painter.galley(r.min, galley, Color32::BLACK);
                    painter.line_segment(
                        [r.left_bottom(), r.right_bottom()],
                        egui::Stroke::new(1.0, Color32::BLUE),
                    );
                }
            }
        }
    }

    /// Once a frame: after undo, redo or another command the edited header or footer
    /// may be gone (leave) or shorter (the caret is kept inside).
    pub(crate) fn hf_frame(&mut self) {
        if self.session.hf_target().is_some() && self.hf.checked != self.session.generation {
            self.hf.checked = self.session.generation;
            self.session.revalidate_hf();
            if self.session.hf_target().is_none() {
                self.message = "편집하던 머리말/꼬리말이 없어져 본문으로 돌아왔습니다.".into();
            } else if let (Some(t), Some(page)) = (self.session.hf_target(), self.session.hf_page())
                && self
                    .session
                    .hf_page_target(page, t.footer)
                    .is_ok_and(|hit| hit.target == t && hit.hidden)
            {
                self.session.hf_exit();
                self.message = format!("이 쪽은 {}을 감춰 본문으로 돌아왔습니다.", hf_label(t));
            }
        }
    }

    /// The body caret onto `page` (its middle), so a command for the caret's page
    /// (현재 쪽만 감추기) acts on the page the header or footer was entered from.
    fn hf_body_caret_to(&mut self, page: u32) {
        let Ok(info) = self.session.page_info(page) else {
            return;
        };
        let f = |k: &str| info[k].as_f64().unwrap_or(0.0) / 2.0;
        if let Ok(c) = self.session.hit_test(page, f("width"), f("height")) {
            let r = self.session.select(c, false);
            self.result(r);
        }
    }

    /// A key, text or IME event while a header or footer is edited: Esc leaves; nothing
    /// reaches the body caret. True when it was taken.
    pub(crate) fn hf_key_event(&mut self, event: &egui::Event) -> bool {
        if self.session.hf_target().is_none() {
            return false;
        }
        match event {
            egui::Event::Key {
                key: egui::Key::Escape,
                pressed: true,
                ..
            } => {
                // A selected header object is let go first.
                if !self.hf_object_deselect() {
                    self.hf_leave();
                }
                true
            }
            // Typing edits the header (hf_edit); keys it leaves alone stay out of
            // the body too.
            egui::Event::Key { .. }
            | egui::Event::Text(_)
            | egui::Event::Ime(_)
            | egui::Event::Paste(_)
            | egui::Event::Cut
            | egui::Event::Copy => {
                if !self.hf_object_key(event) {
                    let ctx = self.ctx.clone();
                    self.hf_event(&ctx, event);
                }
                true
            }
            _ => false,
        }
    }

    /// The status bar chip while a header or footer is edited (pressing it leaves).
    pub(crate) fn hf_status_chip(&mut self, ui: &mut egui::Ui) {
        let Some(target) = self.session.hf_target() else {
            return;
        };
        if rulers::status_chip(
            ui,
            &format!("{} 편집", hf_label(target)),
            Color32::from_rgb(40, 110, 220),
            Color32::from_rgb(232, 240, 253),
        )
        .on_hover_text(format!("{LEAVE_HINT} 누르면 본문 편집으로 돌아갑니다."))
        .clicked()
        {
            self.hf_leave();
        }
    }

    /// The 머리말/꼬리말 tab's commands: `Some(enabled)` for its ids.
    pub(crate) fn hf_ext_enabled(&self, id: &str) -> Option<bool> {
        let editing = self.session.hf_target().is_some();
        match id {
            "page.hf.close" | "page.hf.prev" | "page.hf.next" | "page.hf.switch" => Some(editing),
            "page.hf.both" | "page.hf.odd" | "page.hf.even" => Some(editing),
            _ => None,
        }
    }

    pub(crate) fn hf_ext_checked(&self, id: &str) -> bool {
        let Some(t) = self.session.hf_target() else {
            return false;
        };
        matches!(
            (id, t.apply_to),
            ("page.hf.both", HfApply::Both)
                | ("page.hf.odd", HfApply::Odd)
                | ("page.hf.even", HfApply::Even)
        )
    }

    /// Run a 머리말/꼬리말 tab command; false for other ids.
    pub(crate) fn hf_ext_dispatch(&mut self, id: &str) -> bool {
        let (Some(t), Some(page)) = (self.session.hf_target(), self.session.hf_page()) else {
            return id.starts_with("page.hf.");
        };
        match id {
            "page.hf.close" => self.hf_leave(),
            "page.hf.prev" | "page.hf.next" => self.hf_step(t, page, id.ends_with("next")),
            "page.hf.switch" => {
                let r = self.session.hf_page_target(page, !t.footer);
                if let Some(hit) = self.result(r) {
                    self.hf_open(hit, page, None);
                }
            }
            "page.hide" => {
                self.hf_body_caret_to(page);
                return false;
            }
            "page.hf.both" | "page.hf.odd" | "page.hf.even" => {
                let apply_to = match id {
                    "page.hf.odd" => HfApply::Odd,
                    "page.hf.even" => HfApply::Even,
                    _ => HfApply::Both,
                };
                self.hf_apply(t, page, apply_to);
            }
            _ => return false,
        }
        true
    }

    /// 이전·다음: the same kind (header or footer) on the page before or after, whichever
    /// header or footer that page shows; pages that hide it are passed over.
    fn hf_step(&mut self, t: HfTarget, page: u32, forward: bool) {
        let count = self.session.page_count().unwrap_or(1);
        let mut p = page;
        loop {
            p = match (forward, p) {
                (true, p) if p + 1 < count => p + 1,
                (false, p) if p > 0 => p - 1,
                _ => {
                    self.message = if forward {
                        "마지막 쪽입니다."
                    } else {
                        "첫 쪽입니다."
                    }
                    .into();
                    return;
                }
            };
            match self.session.hf_page_target(p, t.footer) {
                Ok(hit) if hit.hidden => continue,
                Ok(hit) => {
                    self.hf_open(hit, p, None);
                    return;
                }
                Err(e) => {
                    self.result(Err::<(), _>(e));
                    return;
                }
            }
        }
    }

    /// 양쪽·홀수 쪽·짝수 쪽: edit the section's header (footer) for those pages, made if
    /// missing, from the nearest page that shows it.
    fn hf_apply(&mut self, t: HfTarget, page: u32, apply_to: HfApply) {
        if t.apply_to == apply_to {
            return;
        }
        let want = HfTarget { apply_to, ..t };
        let count = self.session.page_count().unwrap_or(1);
        // Pages of the section by distance from the current one: the first that shows
        // the wanted header (once made) is where it is edited.
        let mut order: Vec<u32> = (0..count).collect();
        order.sort_by_key(|p| p.abs_diff(page));
        let generation = self.session.generation;
        let r = self.session.hf_enter(want, page);
        if self.result(r).is_none() {
            return;
        }
        let made = self.session.generation != generation;
        for p in order {
            if let Ok(hit) = self.session.hf_page_target(p, t.footer)
                && hit.target == want
                && !hit.hidden
            {
                self.hf_open(hit, p, None);
                return;
            }
        }
        // No page shows it (a one-page document has no even page): one just made is
        // taken back.
        self.session.hf_exit();
        if made {
            let r = self.session.execute(hwpcraft_engine::Command::Undo);
            self.result(r);
        }
        let r = self.session.hf_enter(t, page);
        self.result(r);
        self.message = format!("{}을 보여 주는 쪽이 없습니다.", hf_label(want));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of page 0 drawn at (50, 50), `seconds` after the previous one, with
    /// header/footer editing taking the pointer as the document view lets it.
    fn frame(a: &mut HwpApp, events: Vec<egui::Event>, seconds: f64) {
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

    fn double_click(a: &mut HwpApp, p: Pos2) {
        frame(a, vec![egui::Event::PointerMoved(p), button(p, true)], 1.0);
        frame(a, vec![button(p, false)], 0.05);
        frame(a, vec![button(p, true)], 0.05);
        frame(a, vec![button(p, false)], 0.05);
    }

    /// Canvas point at the middle of page 0's header (footer) area.
    fn area_centre(a: &HwpApp, footer: bool) -> Pos2 {
        let info = a.session.hf_page_info(0).unwrap();
        let r = &info[if footer { "footer" } else { "header" }]["rect"];
        let f = |i: usize| r[i].as_f64().unwrap() as f32;
        Pos2::new(50.0, 50.0) + Vec2::new(f(0) + f(2) / 2.0, f(1) + f(3) / 2.0) * a.zoom
    }

    fn app() -> HwpApp {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        a.session
            .run("text.insert", json!({"text": "본문 첫 문단"}))
            .unwrap();
        a
    }

    fn escape() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// 머리말 영역을 두 번 누르면 그 머리말 편집으로 들어가 상황 탭이 열리고, 편집 중에는
    /// 키와 글이 본문에 들어가지 않으며, Esc로 본문으로 돌아옵니다.
    #[test]
    fn a_double_click_on_the_header_enters_it_and_esc_leaves() {
        let mut a = app();
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let generation = a.session.generation;
        let p = area_centre(&a, false);
        double_click(&mut a, p);
        let t = a.session.hf_target().expect("editing the header");
        assert!(!t.footer && t.apply_to == HfApply::Both, "{t:?}");
        assert_eq!(a.tab, HF_TAB);
        assert_eq!(a.session.generation, generation, "entering is no edit");
        assert!(a.message.contains("머리말(양쪽)"), "{}", a.message);
        let body = a.session.text().to_owned();
        assert!(a.hf_key_event(&egui::Event::Text("가".into())));
        assert_eq!(a.session.text(), body);
        assert!(a.hf_key_event(&escape()));
        assert!(a.session.hf_target().is_none());
        assert!(!a.hf_key_event(&egui::Event::Text("가".into())));
    }

    /// 편집 중인 머리말에서 친 글은 머리말에 들어가고, 그 안을 두 번 누르면 낱말, 세 번
    /// 누르면 문단이 선택됩니다.
    #[test]
    fn typing_and_multi_clicks_in_the_edited_header() {
        let mut a = app();
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let p = area_centre(&a, false);
        double_click(&mut a, p);
        assert!(a.session.hf_target().is_some());
        for text in ["보고서", " ", "머리말"] {
            assert!(a.hf_key_event(&egui::Event::Text(text.into())));
        }
        let got = a
            .session
            .run("page.header_footer.text.get", json!({"header": true}))
            .unwrap();
        assert_eq!(got["text"], "보고서 머리말", "{got}");
        // The first word, from a double click on it.
        let caret = a.session.hf_caret_rect().unwrap();
        let start = a.session.hf_page_info(0).unwrap()["header"]["rect"][0]
            .as_f64()
            .unwrap();
        let x = start + 6.0;
        let y = caret["y"].as_f64().unwrap() + caret["height"].as_f64().unwrap() / 2.0;
        let at = Pos2::new(50.0, 50.0) + Vec2::new(x as f32, y as f32) * a.zoom;
        double_click(&mut a, at);
        assert!(a.session.hf_target().is_some(), "still editing");
        assert_eq!(a.session.hf_selected_text().unwrap(), "보고서");
        frame(&mut a, vec![button(at, true)], 0.05);
        frame(&mut a, vec![button(at, false)], 0.05);
        assert_eq!(a.session.hf_selected_text().unwrap(), "보고서 머리말");
    }

    /// 흐리게 칠할 곳은 쪽에서 밝게 둘 사각형들(편집 중인 영역, 그 머리말의 개체)을 뺀
    /// 나머지입니다. 영역 밖으로 나간 개체도 밝게 남습니다.
    #[test]
    fn dimming_leaves_the_area_and_its_objects_bright() {
        let page = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 200.0));
        let area = Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(90.0, 30.0));
        // A logo sticking out to the right and below.
        let logo = Rect::from_min_max(Pos2::new(80.0, 20.0), Pos2::new(98.0, 40.0));
        let parts = outside(page, &[area, logo]);
        let overlap = |a: Rect, b: Rect| a.intersect(b).is_positive();
        assert!(
            parts
                .iter()
                .all(|p| !overlap(*p, area) && !overlap(*p, logo))
        );
        let sum: f32 = parts.iter().map(|p| p.area()).sum();
        let union = area.area() + logo.area() - area.intersect(logo).area();
        assert!((sum - (page.area() - union)).abs() < 0.01, "{sum}");
        assert_eq!(outside(page, &[]), vec![page]);
    }

    /// 머리말 편집 중에는 본문 캐럿이 표 안이어도 표 탭과 셀 여백 눈금자가 나오지
    /// 않습니다(본문 표가 대상이 되므로).
    #[test]
    fn body_table_tabs_and_cell_ruler_wait_while_a_header_is_edited() {
        let mut a = app();
        a.session
            .run("insert.table", json!({"rows": 2, "columns": 2}))
            .unwrap();
        a.session
            .select(
                hwpcraft_engine::Cursor {
                    section: 0,
                    paragraph: 0,
                    offset: 0,
                    cell: Some(hwpcraft_doc::Cell {
                        control: 0,
                        index: 0,
                        paragraph: 0,
                    }),
                },
                false,
            )
            .or_else(|_| {
                a.session.select(
                    hwpcraft_engine::Cursor {
                        section: 0,
                        paragraph: 1,
                        offset: 0,
                        cell: Some(hwpcraft_doc::Cell {
                            control: 0,
                            index: 0,
                            paragraph: 0,
                        }),
                    },
                    false,
                )
            })
            .unwrap();
        let page = Rect::from_min_size(Pos2::ZERO, Vec2::splat(2000.0));
        assert!(a.body_in_table() && a.ruler_cell_geometry(page).is_some());
        let hit = a.session.hf_page_target(0, false).unwrap();
        assert!(a.hf_open(hit, 0, None));
        assert!(!a.body_in_table());
        assert!(a.ruler_cell_geometry(page).is_none());
        a.hf_leave();
        assert!(a.body_in_table() && a.ruler_cell_geometry(page).is_some());
    }

    /// 머리말 편집 중에도 한글 입력기가 켜지도록 머리말 캐럿 자리를 입력기 출력으로
    /// 알립니다(본문 캐럿은 그리지 않으므로). 쪽에 포커스가 없으면 알리지 않습니다.
    #[test]
    fn the_input_method_follows_the_header_caret() {
        let mut a = app();
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let p = area_centre(&a, false);
        double_click(&mut a, p);
        assert!(a.session.hf_target().is_some());
        let ime = |a: &mut HwpApp, focused: bool| {
            let ctx = a.ctx.clone();
            let time = ctx.input(|i| i.time) + 0.05;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 1400.0))),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    let canvas = egui::Id::new("document-canvas");
                    if focused {
                        ui.memory_mut(|m| m.request_focus(canvas));
                    } else {
                        ui.memory_mut(|m| m.surrender_focus(canvas));
                    }
                    let rect = Rect::from_min_size(
                        Pos2::new(50.0, 50.0),
                        Vec2::new(794.0, 1123.0) * a.zoom,
                    );
                    a.preedit = "ㅎ".into();
                    a.hf_paint(ui.painter(), 0, rect);
                },
            );
            out.textures_delta.clear();
            out.platform_output.ime
        };
        // Focus asked for in one frame holds from the next.
        ime(&mut a, true);
        let out = ime(&mut a, true).expect("the input method is told where the caret is");
        let info = a.session.hf_page_info(0).unwrap();
        let r = &info["header"]["rect"];
        let f = |i: usize| r[i].as_f64().unwrap() as f32 * a.zoom;
        let area = Rect::from_min_size(Pos2::new(50.0 + f(0), 50.0 + f(1)), Vec2::new(f(2), f(3)));
        assert!(
            area.expand(2.0).contains(out.cursor_rect.center()),
            "{:?} in {area:?}",
            out.cursor_rect
        );
        ime(&mut a, false);
        assert!(
            ime(&mut a, false).is_none(),
            "not without the keyboard focus"
        );
    }

    /// 꼬리말이 없는 쪽 아래 영역을 두 번 누르면 꼬리말을 만들어 들어가고(실행 취소 한 번),
    /// 실행 취소로 꼬리말이 없어지면 본문으로 돌아옵니다.
    #[test]
    fn an_empty_footer_area_makes_the_footer_and_undo_leaves_it() {
        let mut a = app();
        let p = area_centre(&a, true);
        let generation = a.session.generation;
        double_click(&mut a, p);
        let t = a.session.hf_target().expect("editing the new footer");
        assert!(t.footer);
        assert!(a.session.generation > generation, "the footer was made");
        assert!(a.message.contains("새로 만들었습니다"), "{}", a.message);
        a.session.execute(hwpcraft_engine::Command::Undo).unwrap();
        a.hf_frame();
        assert!(a.session.hf_target().is_none());
        assert!(a.message.contains("없어져"), "{}", a.message);
    }

    /// 편집 중 본문을 한 번 누르면 머물고(안내만), 두 번 누르면 본문으로 돌아갑니다.
    /// 다른 영역(꼬리말)을 두 번 누르면 그쪽 편집으로 바뀝니다.
    #[test]
    fn body_and_other_area_double_clicks_while_editing() {
        let mut a = app();
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        a.session
            .run("page.footer", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let p = area_centre(&a, false);
        double_click(&mut a, p);
        assert!(a.session.hf_target().is_some_and(|t| !t.footer));
        let p = area_centre(&a, true);
        double_click(&mut a, p);
        assert!(a.session.hf_target().is_some_and(|t| t.footer), "switched");
        let body = Pos2::new(50.0, 50.0) + Vec2::new(400.0, 500.0) * a.zoom;
        frame(
            &mut a,
            vec![egui::Event::PointerMoved(body), button(body, true)],
            1.0,
        );
        frame(&mut a, vec![button(body, false)], 0.05);
        assert!(a.session.hf_target().is_some(), "one press stays");
        assert!(a.message.contains("두 번 누르면"), "{}", a.message);
        frame(&mut a, vec![button(body, true)], 0.05);
        frame(&mut a, vec![button(body, false)], 0.05);
        assert!(a.session.hf_target().is_none(), "a double click leaves");
    }

    /// 머리말/꼬리말 편집 중의 붙이기는 복사한 그림을 본문에 넣지 않습니다(닫으면 다시
    /// 넣음).
    #[test]
    fn a_copied_picture_is_not_pasted_into_the_body_while_editing() {
        use crate::paste_picture_tests::{fake_clipboard, pictures};
        let mut a = app();
        let hit = a.session.hf_page_target(0, false).unwrap();
        assert!(a.hf_open(hit, 0, None));
        fake_clipboard(None, true).unwrap();
        assert!(!a.paste_picture(None));
        assert_eq!(pictures(&a), 0);
        a.hf_leave();
        assert!(a.paste_picture(None));
        assert_eq!(pictures(&a), 1);
    }

    /// 한 쪽 문서에서 짝수 쪽 머리말을 고르면 보여 주는 쪽이 없다고 알리고, 그 때문에
    /// 만든 짝수 쪽 머리말은 되돌려 남기지 않습니다.
    #[test]
    fn an_even_header_no_page_shows_is_not_left_behind() {
        let mut a = app();
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let hit = a.session.hf_page_target(0, false).unwrap();
        assert!(a.hf_open(hit, 0, None));
        let count = |a: &mut HwpApp| {
            a.session.run("page.header_footer.list", json!({})).unwrap()["items"]
                .as_array()
                .map_or(0, Vec::len)
        };
        assert_eq!(count(&mut a), 1);
        assert!(a.hf_ext_dispatch("page.hf.even"));
        assert!(
            a.message.contains("보여 주는 쪽이 없습니다"),
            "{}",
            a.message
        );
        assert_eq!(count(&mut a), 1, "the even header was taken back");
        let t = a.session.hf_target().expect("still editing");
        assert_eq!(t.apply_to, HfApply::Both);
    }

    /// 현재 쪽만 감추기는 머리말에 들어간 쪽에 걸리도록 본문 캐럿을 그 쪽으로 옮기고,
    /// 그 쪽 머리말이 감춰지면 본문으로 돌아옵니다.
    #[test]
    fn hiding_acts_on_the_entered_page_and_leaves() {
        let mut a = app();
        a.session
            .execute(hwpcraft_engine::Command::PageBreak)
            .unwrap();
        a.session
            .run("text.insert", json!({"text": "둘째 쪽"}))
            .unwrap();
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let first = hwpcraft_engine::Cursor::default();
        a.session.select(first, false).unwrap();
        let hit = a.session.hf_page_target(1, false).unwrap();
        assert!(a.hf_open(hit, 1, None));
        assert!(!a.hf_ext_dispatch("page.hide"), "the dialog still opens");
        assert_eq!(a.session.cursor.paragraph, 1, "the caret is on page 2");
        a.session
            .run(
                "page.hide",
                json!({"header": true, "footer": false, "master_page": false,
                       "border": false, "fill": false, "page_number": false}),
            )
            .unwrap();
        a.hf_frame();
        assert!(a.session.hf_target().is_none());
        assert!(a.message.contains("감춰"), "{}", a.message);
        assert!(a.session.hf_page_target(0, false).is_ok_and(|h| !h.hidden));
    }

    /// 상황 탭: 바꾸기는 같은 쪽의 꼬리말로 옮기고(없으면 만듦), 한 쪽 문서에서 다음은
    /// 마지막 쪽이라고 알려 주고, 닫기는 본문으로 돌아갑니다. 편집 중이 아니면 회색입니다.
    #[test]
    fn the_header_footer_tab_switches_steps_and_closes() {
        let mut a = app();
        assert_eq!(a.hf_ext_enabled("page.hf.close"), Some(false));
        a.session
            .run("page.header", json!({"apply_to": "both", "template": 0}))
            .unwrap();
        let p = area_centre(&a, false);
        double_click(&mut a, p);
        assert_eq!(a.hf_ext_enabled("page.hf.close"), Some(true));
        assert!(a.hf_ext_checked("page.hf.both") && !a.hf_ext_checked("page.hf.odd"));
        assert!(a.hf_ext_dispatch("page.hf.switch"));
        assert!(a.session.hf_target().is_some_and(|t| t.footer));
        assert!(a.hf_ext_dispatch("page.hf.next"));
        assert_eq!(a.message, "마지막 쪽입니다.");
        assert!(a.hf_ext_dispatch("page.hf.close"));
        assert!(a.session.hf_target().is_none());
        assert!(!a.hf_ext_dispatch("view.zoom"));
    }
}
