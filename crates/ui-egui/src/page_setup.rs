//! 편집 용지 (F7) dialog in a Hangul-style arrangement. Owner: shell/page area.
use super::ext_shell::{dialog, group, note};
use super::*;

/// Hangul's 용지 종류 list (name, width mm, height mm).
const PAPERS: &[(&str, f64, f64)] = &[
    ("A3(국배 2배판)", 297.0, 420.0),
    ("A4(국배판)", 210.0, 297.0),
    ("A5(국판)", 148.0, 210.0),
    ("B4(타블로이드)", 257.0, 364.0),
    ("B5(46배판)", 182.0, 257.0),
    ("Letter", 215.9, 279.4),
    ("Legal", 215.9, 355.6),
    ("신국판", 152.0, 225.0),
];

impl HwpApp {
    pub(super) fn page_setup_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut setup) = self.page_setup.clone() else {
            return;
        };
        let valid = setup.validate();
        let button = dialog(ctx, "편집 용지", 661.0, &["설정(D)", "취소"], |ui| {
            let mut tab = 0;
            chrome::tabs(ui, &mut tab, &["기본"]);
            group(ui, "용지 종류");
            ui.horizontal(|ui| {
                ui.label("종류");
                let current = PAPERS
                    .iter()
                    .find(|(_, w, h)| {
                        (setup.width_mm - w).abs() < 0.1 && (setup.height_mm - h).abs() < 0.1
                    })
                    .map(|(n, w, h)| format!("{n} [{w} x {h} mm]"))
                    .unwrap_or_else(|| "사용자 정의".into());
                egui::ComboBox::from_id_salt("paper-kind")
                    .width(362.0)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (name, w, h) in PAPERS {
                            if ui
                                .selectable_label(false, format!("{name} [{w} x {h} mm]"))
                                .clicked()
                            {
                                setup.width_mm = *w;
                                setup.height_mm = *h;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                metric(ui, "폭", &mut setup.width_mm, 50.0..=1000.0);
                ui.add_space(12.0);
                metric(ui, "길이", &mut setup.height_mm, 50.0..=1000.0);
            });
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(204.0);
                    group(ui, "용지 방향");
                    ui.horizontal(|ui| {
                        for (landscape, label) in [(false, "세로"), (true, "가로")] {
                            if tile(ui, label, setup.landscape == landscape, |p, r| {
                                page_glyph(p, r, landscape)
                            }) {
                                setup.landscape = landscape;
                            }
                        }
                    });
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    group(ui, "제본");
                    ui.horizontal(|ui| {
                        for (binding, label) in [(0u8, "한쪽"), (1, "맞쪽"), (2, "위로")] {
                            if tile(ui, label, setup.binding == binding, |p, r| {
                                binding_glyph(p, r, binding)
                            }) {
                                setup.binding = binding;
                            }
                        }
                    });
                });
            });
            group(ui, "용지 여백");
            margins(ui, &mut setup);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("적용 범위");
                egui::ComboBox::from_id_salt("page-scope")
                    .width(150.0)
                    .selected_text(format!("현재 구역 ({})", self.session.cursor.section + 1))
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, "현재 구역");
                    });
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut false, "현재 설정 값을 새 문서에 적용"),
                );
            });
            if let Err(e) = &valid {
                ui.colored_label(Color32::DARK_RED, e.to_string());
            }
            note(ui, "줄 격자 탭은 아직 지원하지 않습니다.");
        });
        match button {
            Some(0) if valid.is_ok() => {
                let result = self.session.execute(Command::PageSetup(setup.clone()));
                if self.result(result).is_some() {
                    self.page_setup = None;
                    self.follow_cursor();
                } else {
                    self.page_setup = Some(setup);
                }
            }
            Some(0) => self.page_setup = Some(setup),
            Some(_) => self.page_setup = None,
            None => self.page_setup = Some(setup),
        }
    }
}

fn metric(ui: &mut egui::Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.label(label);
    ui.add_sized(
        [91.0, 24.0],
        egui::DragValue::new(value)
            .range(range)
            .speed(0.5)
            .max_decimals(1)
            .suffix(" mm"),
    );
}

/// 84×64 picture button (용지 방향 / 제본).
fn tile(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    glyph: impl FnOnce(&egui::Painter, Rect),
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(84.0, 64.0), egui::Sense::click());
    let icon = Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 22.0), Vec2::splat(42.0));
    let p = ui.painter();
    let fill = if selected {
        Color32::from_gray(222)
    } else if response.hovered() {
        Color32::from_rgb(229, 241, 251)
    } else {
        Color32::WHITE
    };
    p.rect_filled(icon, 0.0, fill);
    p.rect_stroke(
        icon,
        0.0,
        Stroke::new(1.0, Color32::from_gray(if selected { 120 } else { 200 })),
        egui::StrokeKind::Inside,
    );
    glyph(p, icon.shrink(8.0));
    p.text(
        rect.center_bottom() - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        if selected {
            chrome::Tokens::ACCENT
        } else {
            Color32::from_gray(40)
        },
    );
    response.clicked()
}
fn page_glyph(p: &egui::Painter, r: Rect, landscape: bool) {
    let size = if landscape {
        Vec2::new(r.width(), r.height() * 0.72)
    } else {
        Vec2::new(r.width() * 0.72, r.height())
    };
    let page = Rect::from_center_size(r.center(), size);
    p.rect_stroke(
        page,
        0.0,
        Stroke::new(1.0, Color32::from_gray(80)),
        egui::StrokeKind::Inside,
    );
    for i in 1..5 {
        let y = page.top() + page.height() * i as f32 / 5.0;
        p.line_segment(
            [
                Pos2::new(page.left() + 4.0, y),
                Pos2::new(page.right() - 4.0, y),
            ],
            Stroke::new(1.0, Color32::from_gray(120)),
        );
    }
}
fn binding_glyph(p: &egui::Painter, r: Rect, binding: u8) {
    let blue = Color32::from_rgb(35, 103, 255);
    let stroke = Stroke::new(1.0, Color32::from_gray(80));
    match binding {
        1 => {
            let half = r.width() / 2.0;
            for i in 0..2 {
                let page = Rect::from_min_size(
                    r.min + Vec2::new(i as f32 * half, 4.0),
                    Vec2::new(half, r.height() - 8.0),
                );
                p.rect_stroke(page, 0.0, stroke, egui::StrokeKind::Inside);
            }
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "가나",
                egui::FontId::proportional(10.0),
                blue,
            );
        }
        _ => {
            let page = Rect::from_center_size(r.center(), Vec2::new(r.width() * 0.72, r.height()));
            p.rect_stroke(page, 0.0, stroke, egui::StrokeKind::Inside);
            let edge = if binding == 2 {
                [
                    page.left_top() + Vec2::new(3.0, 3.0),
                    page.right_top() + Vec2::new(-3.0, 3.0),
                ]
            } else {
                [
                    page.left_top() + Vec2::new(3.0, 3.0),
                    page.left_bottom() + Vec2::new(3.0, -3.0),
                ]
            };
            p.line_segment(edge, Stroke::new(1.0, blue));
            p.text(
                page.center(),
                egui::Align2::CENTER_CENTER,
                "가",
                egui::FontId::proportional(13.0),
                blue,
            );
        }
    }
}

/// Margins arranged around the preview like Hangul: 위쪽/머리말 above,
/// 왼쪽/제본 left, 오른쪽 right, 꼬리말/아래쪽 below.
fn margins(ui: &mut egui::Ui, s: &mut PageSetup) {
    let range = 0.0..=500.0;
    ui.vertical_centered(|ui| {
        for (label, value) in [("위쪽", &mut s.top_mm), ("머리말", &mut s.header_mm)] {
            ui.horizontal(|ui| {
                ui.add_space(170.0);
                metric(ui, label, value, range.clone());
            });
        }
    });
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(150.0);
            ui.add_space(14.0);
            for (label, value) in [("왼쪽", &mut s.left_mm), ("제본", &mut s.gutter_mm)] {
                ui.label(label);
                ui.add_sized(
                    [91.0, 24.0],
                    egui::DragValue::new(value)
                        .range(range.clone())
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" mm"),
                );
                ui.add_space(6.0);
            }
        });
        preview(ui, s);
        ui.vertical(|ui| {
            ui.add_space(14.0);
            ui.label("오른쪽");
            ui.add_sized(
                [91.0, 24.0],
                egui::DragValue::new(&mut s.right_mm)
                    .range(range.clone())
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" mm"),
            );
        });
    });
    for (label, value) in [("꼬리말", &mut s.footer_mm), ("아래쪽", &mut s.bottom_mm)] {
        ui.horizontal(|ui| {
            ui.add_space(170.0);
            metric(ui, label, value, range.clone());
        });
    }
}
fn preview(ui: &mut egui::Ui, s: &PageSetup) {
    let (outer, _) = ui.allocate_exact_size(Vec2::new(150.0, 150.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(outer, 0.0, Color32::from_gray(245));
    let (w, h) = if s.landscape {
        (s.height_mm, s.width_mm)
    } else {
        (s.width_mm, s.height_mm)
    };
    let scale = (130.0 / w).min(130.0 / h) as f32;
    let r = Rect::from_center_size(
        outer.center(),
        Vec2::new(w as f32 * scale, h as f32 * scale),
    );
    ui.painter().rect_filled(r, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(60)),
        egui::StrokeKind::Inside,
    );
    let body = Rect::from_min_max(
        r.min
            + Vec2::new(
                (s.left_mm + s.gutter_mm) as f32 * scale,
                (s.top_mm + s.header_mm) as f32 * scale,
            ),
        r.max
            - Vec2::new(
                s.right_mm as f32 * scale,
                (s.bottom_mm + s.footer_mm) as f32 * scale,
            ),
    );
    if body.is_positive() {
        let lines = ((body.height() / 4.0) as usize).clamp(2, 14);
        for i in 0..lines {
            let y = body.top() + 2.0 + i as f32 * 4.0;
            let right = if i % 4 == 3 {
                body.center().x
            } else {
                body.right()
            };
            ui.painter().line_segment(
                [Pos2::new(body.left(), y), Pos2::new(right, y)],
                Stroke::new(1.5, Color32::from_gray(40)),
            );
        }
    }
}
