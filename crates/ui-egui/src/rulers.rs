//! Status bar, document tab bar and fixed rulers in a Hangul-style arrangement.
//! Owner: shell/page area.
use super::icons::Icon;
use super::*;

const BAR: Color32 = Color32::from_rgb(245, 245, 245);
const MARGIN: Color32 = Color32::from_rgb(235, 235, 235);
const TICK: Color32 = Color32::from_rgb(127, 127, 127);
const DIGIT: Color32 = Color32::from_rgb(90, 90, 90);
const RULER: f32 = 28.0;
/// Ruler band inside the 28 px strip (measured: 13 px starting 8 px in).
const BAND: (f32, f32) = (8.0, 21.0);

/// Values the rulers need, refreshed when the document or cursor changes.
#[derive(Clone, Default)]
pub(crate) struct RulerCache {
    key: (u64, usize, usize, Option<hwpcraft_doc::Cell>),
    page: Option<PageSetup>,
    /// Paragraph left margin, right margin and first-line indent in mm.
    indent: (f32, f32, f32),
    chars: usize,
}

impl HwpApp {
    fn ruler_cache(&mut self) -> RulerCache {
        let key = (
            self.session.generation,
            self.session.cursor.section,
            self.session.cursor.paragraph,
            self.session.cursor.cell,
        );
        if self.ext.shell.rulers.key != key || self.ext.shell.rulers.page.is_none() {
            let page = self.session.page_setup().ok();
            // Raw paragraph units are 1/200 pt; 1 pt = 25.4/72 mm.
            let mm = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32 / 200.0 * 25.4 / 72.0;
            let indent = self
                .session
                .paragraph_format()
                .map(|p| {
                    (
                        mm(&p["marginLeft"]),
                        mm(&p["marginRight"]),
                        mm(&p["indent"]),
                    )
                })
                .unwrap_or_default();
            let chars = self
                .session
                .core
                .document()
                .sections
                .iter()
                .flat_map(|s| s.paragraphs.iter())
                .map(|p| p.text.chars().count())
                .sum();
            self.ext.shell.rulers = RulerCache {
                key,
                page,
                indent,
                chars,
            };
        }
        self.ext.shell.rulers.clone()
    }

    /// Line within the page and column within the line, from RHWP line segments.
    fn line_and_column(&self) -> (usize, usize) {
        let c = self.session.cursor;
        let Ok(para) = self.session.paragraph(c) else {
            return (1, c.offset + 1);
        };
        // Line segments count UTF-16 units.
        let unit: usize = para.text.chars().take(c.offset).map(char::len_utf16).sum();
        let index = para
            .line_segs
            .iter()
            .rposition(|l| l.text_start as usize <= unit)
            .unwrap_or(0);
        let start = para
            .line_segs
            .get(index)
            .map(|l| l.text_start as usize)
            .unwrap_or(0);
        let column = para
            .text
            .encode_utf16()
            .take(unit)
            .skip(start)
            .count()
            .saturating_add(1);
        if c.cell.is_some() {
            return (index + 1, column);
        }
        // Count lines back to the first line of the page (flag bit 0) or a page jump.
        let mut line = 1;
        let mut previous = para.line_segs.get(index).map(|l| l.vertical_pos);
        let sections = &self.session.core.document().sections;
        let Some(section) = sections.get(c.section) else {
            return (index + 1, column);
        };
        let mut segs = para.line_segs[..=index.min(para.line_segs.len().saturating_sub(1))]
            .iter()
            .rev()
            .chain(
                section.paragraphs[..c.paragraph.min(section.paragraphs.len())]
                    .iter()
                    .rev()
                    .flat_map(|p| p.line_segs.iter().rev()),
            );
        if let Some(first) = segs.next()
            && first.tag & 1 == 0
        {
            for seg in segs {
                if previous.is_some_and(|p| seg.vertical_pos > p) {
                    break;
                }
                line += 1;
                if seg.tag & 1 != 0 {
                    break;
                }
                previous = Some(seg.vertical_pos);
            }
        }
        (line, column)
    }

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let cache = self.ruler_cache();
        if self.ext.shell.show_status_bar {
            egui::Panel::bottom("hwp-status")
                .exact_size(25.0)
                .frame(
                    egui::Frame::NONE
                        .fill(Color32::WHITE)
                        .inner_margin(egui::Margin {
                            left: 10,
                            right: 0,
                            top: 0,
                            bottom: 0,
                        }),
                )
                .show(ui, |ui| self.status_items(ui, &cache));
        }
        if self.ext.shell.show_doc_tabs {
            egui::Panel::bottom("hwp-document-tab")
                .exact_size(29.0)
                .frame(egui::Frame::NONE.fill(BAR))
                .show(ui, |ui| self.document_tabs(ui));
        } else if self.tab != 0 {
            egui::Panel::bottom("hwp-horizontal-scroll")
                .exact_size(super::document_scroll::WIDTH)
                .frame(egui::Frame::NONE.fill(BAR))
                .show(ui, |ui| self.horizontal_scrollbar(ui, ui.max_rect()));
        }
        if self.tab != 0 {
            // The memo pane docks beside the document, outside its scroll rail and rulers.
            self.comments_panel(ui);
            self.vertical_scroll_panel(ui);
        }
        if self.show_rulers && self.tab != 0 {
            let page = self.view_page_rect;
            egui::Panel::top("hwp-hruler")
                .exact_size(RULER)
                .frame(egui::Frame::NONE.fill(BAR))
                .show(ui, |ui| {
                    let corner = Rect::from_min_size(ui.max_rect().min, Vec2::splat(RULER));
                    tab_kind_marker(ui, corner);
                    if let (Some(page), Some(setup)) = (page, cache.page.as_ref()) {
                        self.horizontal_ruler(ui, page, setup, cache.indent);
                    }
                });
            egui::Panel::left("hwp-vruler")
                .exact_size(RULER)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(BAR))
                .show(ui, |ui| {
                    if let (Some(page), Some(setup)) = (page, cache.page.as_ref()) {
                        self.vertical_ruler(ui, page, setup);
                    }
                });
            if self.ext.shell.ruler_page != page {
                self.ext.shell.ruler_page = page;
                ctx.request_repaint();
            }
        }
    }

    fn status_items(&mut self, ui: &mut egui::Ui, cache: &RulerCache) {
        let ctx = ui.ctx().clone();
        let pages = self.layout_page_count().max(1);
        let sections = self.session.core.document().sections.len().max(1);
        let (line, column) = self.line_and_column();
        let page = self
            .session
            .caret()
            .ok()
            .and_then(|v| v["pageIndex"].as_u64())
            .unwrap_or(self.page as u64)
            + 1;
        ui.horizontal_centered(|ui| {
            let full = ui.max_rect();
            let split = (full.right() - 300.0).max(full.left());
            let left = Rect::from_min_max(full.min, Pos2::new(split, full.bottom()));
            let right = Rect::from_min_max(Pos2::new(split, full.top()), full.max);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(left)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(left));
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let cell = |ui: &mut egui::Ui, text: String, tip: &str| {
                        let galley = ui.painter().layout_no_wrap(
                            text,
                            egui::FontId::proportional(12.0),
                            Color32::from_gray(51),
                        );
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(galley.size().x + 14.0, 24.0),
                            egui::Sense::click(),
                        );
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 0.0, Color32::from_gray(238));
                        }
                        ui.painter().galley(
                            rect.center() - galley.size() / 2.0,
                            galley,
                            Color32::from_gray(51),
                        );
                        response.on_hover_text(tip)
                    };
                    if cell(
                        ui,
                        format!("{page}/{pages}쪽"),
                        "쪽 / 전체 쪽 · 클릭하면 쪽 찾아가기",
                    )
                    .clicked()
                    {
                        self.page = (page as u32) % pages;
                    }
                    cell(ui, "1단".into(), "현재 단");
                    cell(ui, format!("{line}줄"), "쪽 안에서의 줄");
                    cell(ui, format!("{column}칸"), "줄 안에서의 칸");
                    cell(ui, format!("{}글자", cache.chars), "문서 전체 글자 수");
                    cell(ui, "문자 입력".into(), "입력 상태");
                    cell(
                        ui,
                        format!("{}/{} 구역", self.session.cursor.section + 1, sections),
                        "현재 구역 / 전체 구역",
                    );
                    let mode = if self.session.overwrite {
                        "수정"
                    } else {
                        "삽입"
                    };
                    if cell(ui, mode.into(), "삽입/수정 전환 (Insert)").clicked() {
                        self.session.overwrite = !self.session.overwrite;
                    }
                    if self.session.has_track_change_records() {
                        ui.add_enabled_ui(false, |ui| {
                            cell(
                                ui,
                                "변경 기록 보존됨(표시 미지원)".into(),
                                "이 문서에는 변경 내용 추적 기록이 있습니다. 저장할 때 그대로 \
                                 보존하지만, 본문에 변경 표시를 그리지는 못합니다.",
                            );
                        });
                    }
                    if !self.message.is_empty() && !start_hint_is_stale(self) {
                        ui.add_space(12.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&self.message)
                                    .color(Color32::from_gray(110))
                                    .size(11.5),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&self.message);
                    }
                },
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(right)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(right));
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let zoom = format!("{:.0}%", self.zoom * 100.0);
                    let response = ui
                        .add_sized([64.0, 24.0], egui::Button::new(zoom).frame(false))
                        .on_hover_text("여기서는 휠만, 문서 위에서는 Ctrl+휠: 확대/축소 (10–500%)");
                    self.status_wheel_zoom(&response);
                    let popup = egui::Id::new("hwp-zoom-menu");
                    if response.clicked() {
                        egui::Popup::toggle_id(&ctx, popup);
                    }
                    egui::Popup::from_response(&response)
                        .id(popup)
                        .open_memory(None)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            for percent in [500, 300, 200, 150, 125, 100, 75, 50, 25] {
                                if ui.button(format!("{percent}%")).clicked() {
                                    self.set_zoom(percent as f32 / 100.0);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("쪽 맞춤").clicked() {
                                self.zoom_to_fit(false);
                                ui.close();
                            }
                            if ui.button("폭 맞춤").clicked() {
                                self.zoom_to_fit(true);
                                ui.close();
                            }
                        });
                    if self
                        .icons
                        .small(ui, Icon::ZoomIn, None, "확대", true, false, false)
                        .clicked()
                    {
                        self.set_zoom(self.zoom + 0.1);
                    }
                    let mut percent = self.zoom * 100.0;
                    ui.spacing_mut().slider_width = 80.0;
                    if ui
                        .add(
                            egui::Slider::new(&mut percent, 10.0..=500.0)
                                .logarithmic(true)
                                .show_value(false),
                        )
                        .on_hover_text("확대/축소 슬라이더")
                        .changed()
                    {
                        self.set_zoom(percent / 100.0);
                    }
                    if self
                        .icons
                        .small(ui, Icon::ZoomOut, None, "축소", true, false, false)
                        .clicked()
                    {
                        self.set_zoom(self.zoom - 0.1);
                    }
                    ui.add_space(8.0);
                    for (icon, tip, id) in [
                        (Icon::FitPage, "쪽 맞춤", "view.zoom_fit_page"),
                        (Icon::FitWidth, "폭 맞춤", "view.zoom_fit_width"),
                        (Icon::PageOutline, "쪽 윤곽", "view.page_outline"),
                    ] {
                        let enabled = self.ext_enabled(id);
                        if self
                            .icons
                            .small(ui, icon, None, tip, enabled, false, false)
                            .clicked()
                        {
                            self.ext_dispatch(id, &ctx);
                        }
                    }
                },
            );
        });
    }

    fn document_tabs(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let pages = self.layout_page_count().max(1);
        let full = ui.max_rect();
        let split = full.left() + (full.width() * 0.49).clamp(360.0, 740.0);
        let tabs = Rect::from_min_max(full.min, Pos2::new(split, full.bottom()));
        let rail = Rect::from_min_max(
            Pos2::new(split + 4.0, full.top()),
            full.max - Vec2::new(super::document_scroll::WIDTH, 0.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(tabs)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(tabs));
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(10.0);
                if self
                    .icons
                    .small(
                        ui,
                        Icon::Windows,
                        None,
                        "작업 창",
                        true,
                        false,
                        self.show_navigation,
                    )
                    .clicked()
                {
                    self.show_navigation = !self.show_navigation;
                }
                ui.add_space(12.0);
                for (label, tip, target) in [
                    ("⏮", "첫 쪽", 0),
                    ("◀", "이전 쪽", self.page.saturating_sub(1)),
                    ("▶", "다음 쪽", (self.page + 1).min(pages - 1)),
                    ("⏭", "마지막 쪽", pages - 1),
                ] {
                    let enabled = target != self.page;
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(
                                RichText::new(label)
                                    .size(10.0)
                                    .color(Color32::from_gray(120)),
                            )
                            .frame(false)
                            .min_size(Vec2::new(24.0, 24.0)),
                        )
                        .on_hover_text(tip)
                        .clicked()
                    {
                        self.page = target;
                    }
                }
                ui.add_space(18.0);
                let name = self
                    .session
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "빈 문서 1".into());
                let color = if self.session.dirty() {
                    Color32::from_rgb(220, 50, 50)
                } else {
                    Color32::from_rgb(29, 120, 220)
                };
                let galley = ui.painter().layout_no_wrap(
                    name.clone(),
                    egui::FontId::proportional(12.5),
                    color,
                );
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(
                        (galley.size().x + 44.0)
                            .max(92.0)
                            .min((ui.available_width() - 36.0).max(40.0)),
                        27.0,
                    ),
                    egui::Sense::click(),
                );
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    Stroke::new(1.0, Color32::from_gray(210)),
                );
                ui.painter_at(rect.shrink2(Vec2::new(8.0, 0.0))).galley(
                    Pos2::new(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
                    galley,
                    color,
                );
                response.on_hover_text(
                    self.session
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or(name),
                );
                ui.add_space(6.0);
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("+").size(15.0).color(Color32::from_gray(110)),
                        )
                        .frame(false),
                    )
                    .on_hover_text("새 탭 (새 문서)")
                    .clicked()
                {
                    self.request(Pending::New, &ctx);
                }
            },
        );
        if self.tab != 0 {
            self.horizontal_scrollbar(ui, rail);
        }
    }

    fn horizontal_ruler(
        &mut self,
        ui: &mut egui::Ui,
        page: Rect,
        setup: &PageSetup,
        indent: (f32, f32, f32),
    ) {
        let strip = ui.max_rect();
        let (width_mm, _) = paper(setup);
        let scale = page.width() / width_mm as f32;
        let band = Rect::from_min_max(
            Pos2::new(page.left(), strip.top() + BAND.0),
            Pos2::new(page.right(), strip.top() + BAND.1),
        );
        let cell = self.ruler_cell_geometry(page);
        let left = cell.as_ref().map_or(
            page.left() + (setup.left_mm + setup.gutter_mm) as f32 * scale,
            |c| c.inner_left(),
        );
        let right = cell
            .as_ref()
            .map_or(page.right() - setup.right_mm as f32 * scale, |c| {
                c.inner_right()
            });
        let painter = ui.painter_at(Rect::from_min_max(
            Pos2::new(strip.left() + RULER, strip.top()),
            strip.max,
        ));
        painter.rect_filled(band, 0.0, MARGIN);
        painter.rect_filled(
            Rect::from_x_y_ranges(left..=right, band.y_range()),
            0.0,
            Color32::WHITE,
        );
        ticks(&painter, band, left, scale, false);
        // Paragraph indent markers: first line on top, left margin below, right margin.
        let first = left + (indent.0 + indent.2) * scale;
        let body_left = left + indent.0 * scale;
        let body_right = right - indent.1 * scale;
        triangle(&painter, Pos2::new(first, band.top()), true);
        triangle(&painter, Pos2::new(body_left, band.bottom()), false);
        painter.rect_stroke(
            Rect::from_center_size(
                Pos2::new(body_left, band.bottom() + 5.5),
                Vec2::new(8.0, 5.0),
            ),
            0.0,
            Stroke::new(1.0, TICK),
            egui::StrokeKind::Inside,
        );
        triangle(&painter, Pos2::new(body_right, band.bottom()), false);
        if let Some(cell) = cell {
            self.cell_margin_ruler(ui, band, cell);
            return;
        }
        // Drag the grey/white boundaries to change page margins.
        let mut changed = None;
        for (side, x) in [(0u8, left), (1u8, right)] {
            let hit = Rect::from_center_size(
                Pos2::new(x, band.center().y),
                Vec2::new(7.0, band.height()),
            );
            let response = ui
                .interact(
                    hit,
                    egui::Id::new(("hwp-hruler-margin", side)),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text(if side == 0 {
                    "왼쪽 여백"
                } else {
                    "오른쪽 여백"
                });
            if let Some(pos) = response.interact_pointer_pos()
                && response.dragged()
            {
                painter.line_segment(
                    [
                        Pos2::new(pos.x, strip.top()),
                        Pos2::new(pos.x, strip.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(46, 105, 220)),
                );
            }
            if response.drag_stopped()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let mut s = setup.clone();
                let mm = ((if side == 0 {
                    pos.x - page.left()
                } else {
                    page.right() - pos.x
                }) / scale)
                    .max(0.0) as f64;
                if side == 0 {
                    s.left_mm = (mm - s.gutter_mm).max(0.0).round();
                } else {
                    s.right_mm = mm.round();
                }
                changed = Some(s);
            }
        }
        if let Some(s) = changed {
            self.apply_ruler_margins(s);
        }
    }

    fn vertical_ruler(&mut self, ui: &mut egui::Ui, page: Rect, setup: &PageSetup) {
        let strip = ui.max_rect();
        let (_, height_mm) = paper(setup);
        let scale = page.height() / height_mm as f32;
        let band = Rect::from_min_max(
            Pos2::new(strip.left() + BAND.0, page.top()),
            Pos2::new(strip.left() + BAND.1, page.bottom()),
        );
        let top = page.top() + (setup.top_mm + setup.header_mm) as f32 * scale;
        let bottom = page.bottom() - (setup.bottom_mm + setup.footer_mm) as f32 * scale;
        let painter = ui.painter_at(strip);
        painter.rect_filled(band, 0.0, MARGIN);
        painter.rect_filled(
            Rect::from_x_y_ranges(band.x_range(), top..=bottom),
            0.0,
            Color32::WHITE,
        );
        ticks(&painter, band, top, scale, true);
        let mut changed = None;
        for (side, y) in [(0u8, top), (1u8, bottom)] {
            let hit =
                Rect::from_center_size(Pos2::new(band.center().x, y), Vec2::new(band.width(), 7.0));
            let response = ui
                .interact(
                    hit,
                    egui::Id::new(("hwp-vruler-margin", side)),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                .on_hover_text(if side == 0 {
                    "위쪽 여백"
                } else {
                    "아래쪽 여백"
                });
            if let Some(pos) = response.interact_pointer_pos()
                && response.dragged()
            {
                painter.line_segment(
                    [
                        Pos2::new(strip.left(), pos.y),
                        Pos2::new(strip.right(), pos.y),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(46, 105, 220)),
                );
            }
            if response.drag_stopped()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let mut s = setup.clone();
                if side == 0 {
                    s.top_mm = (((pos.y - page.top()) / scale) as f64 - s.header_mm)
                        .max(0.0)
                        .round();
                } else {
                    s.bottom_mm = (((page.bottom() - pos.y) / scale) as f64 - s.footer_mm)
                        .max(0.0)
                        .round();
                }
                changed = Some(s);
            }
        }
        if let Some(s) = changed {
            self.apply_ruler_margins(s);
        }
    }

    fn apply_ruler_margins(&mut self, setup: PageSetup) {
        match setup.validate() {
            Ok(()) => self.command(Command::PageSetup(setup)),
            Err(e) => self.error = Some(format!("{e:#}")),
        }
    }
}

/// Displayed paper size in mm (width, height) after orientation.
fn paper(s: &PageSetup) -> (f64, f64) {
    if s.landscape {
        (s.height_mm, s.width_mm)
    } else {
        (s.width_mm, s.height_mm)
    }
}

/// Hangul's ruler: numbered every cm counting outward from the body edge,
/// ticks every 1/8 cm with a longer one at each half centimetre.
fn ticks(painter: &egui::Painter, band: Rect, origin: f32, scale: f32, vertical: bool) {
    let (start, end) = if vertical {
        (band.top(), band.bottom())
    } else {
        (band.left(), band.right())
    };
    let step = scale * 1.25; // 1/8 cm
    if step <= 0.5 {
        return;
    }
    let first = ((start - origin) / step).floor() as i32;
    let last = ((end - origin) / step).ceil() as i32;
    for i in first..=last {
        let at = origin + i as f32 * step;
        if at < start || at > end {
            continue;
        }
        let length = if i % 8 == 0 {
            0.0
        } else if i % 4 == 0 {
            4.0
        } else {
            2.0
        };
        if i % 8 == 0 {
            if i != 0 && step * 8.0 >= 14.0 {
                let cm = (i / 8).abs().to_string();
                let galley = painter.layout_no_wrap(cm, egui::FontId::proportional(9.5), DIGIT);
                let size = galley.size();
                let pos = if vertical {
                    Pos2::new(band.center().x + size.y / 2.0, at - size.x / 2.0)
                } else {
                    Pos2::new(at - size.x / 2.0, band.center().y - size.y / 2.0)
                };
                let mut text = egui::epaint::TextShape::new(pos, galley, DIGIT);
                if vertical {
                    text = text.with_angle(std::f32::consts::FRAC_PI_2);
                }
                painter.add(text);
            }
            continue;
        }
        let stroke = Stroke::new(1.0, TICK);
        if vertical {
            painter.line_segment(
                [
                    Pos2::new(band.right() - 3.0 - length, at),
                    Pos2::new(band.right() - 3.0, at),
                ],
                stroke,
            );
        } else {
            painter.line_segment(
                [
                    Pos2::new(at, band.bottom() - 3.0 - length),
                    Pos2::new(at, band.bottom() - 3.0),
                ],
                stroke,
            );
        }
    }
}

fn triangle(painter: &egui::Painter, tip: Pos2, down: bool) {
    let d = if down { -1.0 } else { 1.0 };
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            tip + Vec2::new(-4.0, 5.0 * d),
            tip + Vec2::new(4.0, 5.0 * d),
        ],
        Color32::WHITE,
        Stroke::new(1.0, TICK),
    ));
}

/// The tab-kind button in the corner where both rulers meet (left tab shown).
fn tab_kind_marker(ui: &egui::Ui, corner: Rect) {
    let c = corner.center();
    let stroke = Stroke::new(1.5, Color32::from_gray(70));
    ui.painter().line_segment(
        [c + Vec2::new(-3.0, -3.0), c + Vec2::new(-3.0, 3.0)],
        stroke,
    );
    ui.painter()
        .line_segment([c + Vec2::new(-3.0, 3.0), c + Vec2::new(4.0, 3.0)], stroke);
}

/// Start-up hint the core view puts in the status line before any document is used.
const START_HINT: &str = "문서를 열거나 흰 쪽을 클릭해 입력하세요.";

/// The start-up hint no longer applies once a file is open or the document has content.
fn start_hint_is_stale(app: &HwpApp) -> bool {
    use hwpcraft_doc::model::control::Control;
    app.message == START_HINT
        && (app.session.path.is_some()
            || app.session.dirty()
            || app.session.core.page_count() > 1
            || app
                .session
                .core
                .document()
                .sections
                .iter()
                .flat_map(|s| &s.paragraphs)
                .any(|p| {
                    !p.text.trim().is_empty()
                        || p.controls
                            .iter()
                            .any(|c| !matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
                }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scroll_rail_stays_beside_the_page_from_the_memo_panes_first_frame() -> anyhow::Result<()>
    {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = app.ctx.clone();
        assert!(app.comments_ext_dispatch("view.memo", &ctx));
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 900.0))),
            ..Default::default()
        };
        // One frame, the first with the pane open, in the app's panel order.
        let mut output = ctx.run_ui(input, |ui| {
            app.status_bar(ui);
            egui::CentralPanel::default().show(ui, |_| {});
        });
        output.textures_delta.clear();
        let rect = |id: &str| {
            egui::containers::panel::PanelState::load(&ctx, egui::Id::new(id))
                .map(|state| state.outer_rect)
        };
        let (Some(memo), Some(rail)) =
            (rect("native-memo-editor"), rect("hwp-document-scroll-rail"))
        else {
            panic!("memo pane and scroll rail are both laid out");
        };
        // The document's scroll rail belongs to the page view, left of the pane.
        assert!(
            rail.max.x <= memo.min.x + 0.5,
            "rail {rail:?} memo {memo:?}"
        );
        assert!(
            memo.max.x >= 1499.0,
            "the pane docks at the window edge: {memo:?}"
        );
        Ok(())
    }

    #[test]
    fn start_hint_hides_once_a_document_is_in_use() -> anyhow::Result<()> {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        app.message = START_HINT.into();
        assert!(
            !start_hint_is_stale(&app),
            "a blank document keeps the hint"
        );
        app.session.execute(Command::Insert("글".into()))?;
        assert!(start_hint_is_stale(&app));
        // Other messages (results, errors) are always shown.
        app.message = "열기 완료".into();
        assert!(!start_hint_is_stale(&app));
        Ok(())
    }
}
