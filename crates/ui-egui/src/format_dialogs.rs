//! Owner: format area. 글자 모양(Alt+L)·문단 모양(Alt+T) 창.
//! Layout follows the Hangul-style character / paragraph format windows.
use super::*;
use serde_json::Value;

pub(super) const LANGUAGES: [&str; 8] = [
    "대표",
    "한글",
    "영문",
    "한자",
    "일어",
    "외국어",
    "기호",
    "사용자",
];
/// Fallback 글꼴 list when the system font database is empty; the platform's Korean
/// font (`hwpcraft_render::default_korean_font`) is put first by `fallback_fonts`.
const FONTS: [&str; 17] = [
    "함초롬바탕",
    "함초롬돋움",
    "바탕",
    "바탕체",
    "돋움",
    "돋움체",
    "굴림",
    "굴림체",
    "궁서",
    "나눔고딕",
    "나눔명조",
    "Arial",
    "Times New Roman",
    "Courier New",
    "Calibri",
    "Cambria",
    "Segoe UI",
];
static FONT_LIST: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
fn fallback_fonts() -> Vec<String> {
    let default = hwpcraft_render::default_korean_font();
    std::iter::once(default)
        .chain(FONTS.iter().copied().filter(|f| *f != default))
        .map(str::to_owned)
        .collect()
}
/// Installed font families (Korean names first), read once: the database holds hundreds.
pub(super) fn load_fonts(renderer: &hwpcraft_render::Renderer) {
    FONT_LIST.get_or_init(|| {
        let names = renderer.font_families();
        if names.is_empty() {
            fallback_fonts()
        } else {
            names
        }
    });
}
pub(super) fn font_names() -> Vec<String> {
    FONT_LIST.get().cloned().unwrap_or_else(fallback_fonts)
}

/// HWP border width indices (표 28), in mm.
pub(super) const BORDER_WIDTHS: [f32; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];
/// HWP border line types (표 27): 0 = none.
pub(super) const LINE_TYPES: [&str; 18] = [
    "없음",
    "실선",
    "파선",
    "점선",
    "일점 쇄선",
    "이점 쇄선",
    "긴 파선",
    "원형 점선",
    "이중 실선",
    "얇고 굵은 이중선",
    "굵고 얇은 이중선",
    "얇고 굵고 얇은 삼중선",
    "물결",
    "이중 물결",
    "두꺼운 3D",
    "두꺼운 3D(반대)",
    "3D 단선",
    "3D 단선(반대)",
];
const PATTERNS: [&str; 7] = ["없음", "가로줄", "세로줄", "역사선", "사선", "십자", "격자"];
const OUTLINES: [&str; 7] = [
    "없음",
    "실선",
    "점선",
    "굵은 실선",
    "파선",
    "일점 쇄선",
    "이점 쇄선",
];
const EMPHASIS: [&str; 7] = ["없음", "●", "○", "ˇ", "˜", "･", "ː"];
const SIDES: [&str; 4] = ["borderLeft", "borderRight", "borderTop", "borderBottom"];
/// 1 mm in HWP units (7200 per inch).
const MM: f64 = 7200.0 / 25.4;

/// WordCraft/DesignCraft pattern: a persistent draft, commit only changed fields.
/// Cancel never mutates the document or toolbar state.
#[derive(Clone)]
pub(super) struct FormatDraft {
    original: Value,
    pub(super) values: Value,
    pub(super) tab: usize,
    language: usize,
    font_points: f64,
    /// Editing a style's shape from the 스타일 창: the patch goes back to that window.
    pub(super) for_style: bool,
    /// Border pen used by the side buttons (type, width index, color).
    pen: (u8, u8, String),
    tab_kind: u8,
    tab_fill: u8,
    tab_position: f64,
    tab_selected: Option<usize>,
    deleted_tabs: Vec<Value>,
    preview: Option<(String, egui::TextureHandle, Vec2)>,
    preview_key: String,
    preview_at: f64,
}
impl FormatDraft {
    pub(super) fn new(values: Value) -> Self {
        let pen = SIDES
            .iter()
            .map(|k| &values[*k])
            .find(|b| b["type"].as_u64().unwrap_or(0) > 0)
            .map(|b| {
                (
                    b["type"].as_u64().unwrap_or(1) as u8,
                    b["width"].as_u64().unwrap_or(0) as u8,
                    b["color"].as_str().unwrap_or("#000000").to_owned(),
                )
            })
            .unwrap_or((1, 0, "#000000".into()));
        Self {
            original: values.clone(),
            values,
            tab: 0,
            language: 0,
            font_points: 10.0,
            for_style: false,
            pen,
            tab_kind: 0,
            tab_fill: 0,
            tab_position: 0.0,
            tab_selected: None,
            deleted_tabs: Vec::new(),
            preview: None,
            preview_key: String::new(),
            preview_at: 0.0,
        }
    }
    pub(super) fn patch(&self) -> Value {
        Value::Object(
            self.values
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, v)| self.original.get(*k) != Some(*v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
}

impl HwpApp {
    pub(super) fn format_dialogs(&mut self, ctx: &egui::Context) {
        for character in [true, false] {
            if !(if character {
                self.show_character
            } else {
                self.show_paragraph
            }) {
                continue;
            }
            let mut draft = if let Some(d) = self.format_draft.take() {
                d
            } else {
                let r = if character {
                    self.session.character_properties()
                } else {
                    self.session.paragraph_format()
                };
                let Some(v) = self.result(r) else {
                    self.show_character = false;
                    self.show_paragraph = false;
                    return;
                };
                let mut d = FormatDraft::new(v);
                d.tab = self.ext.format.initial_tab.take().unwrap_or(0);
                d
            };
            load_fonts(&self.renderer);
            draft.font_points = self
                .session
                .character_properties()
                .ok()
                .and_then(|p| p["fontSize"].as_f64())
                .unwrap_or(1000.0)
                / 100.0;
            self.update_preview(ctx, &mut draft, character);
            let mut apply = false;
            let mut cancel = false;
            let (title, width, body) = if character {
                ("글자 모양", 695.0, 574.0)
            } else {
                ("문단 모양", 553.0, 432.0)
            };
            let r = chrome::modal(if character {
                "char-shape"
            } else {
                "para-shape"
            })
            .show(ctx, |ui| {
                ui.set_width(width - 24.0);
                cancel = chrome::title(ui, title);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(body);
                        egui::Frame::new()
                            .stroke(Stroke::new(1.0, Color32::from_gray(227)))
                            .inner_margin(egui::Margin::symmetric(14, 6))
                            .show(ui, |ui| {
                                ui.set_width(body - 30.0);
                                ui.set_min_height(if character { 440.0 } else { 512.0 });
                                if character {
                                    character_fields(ui, &mut draft);
                                } else {
                                    paragraph_fields(ui, &mut draft);
                                }
                            });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("대화 상자 설정");
                            ui.add_enabled_ui(false, |ui| {
                                egui::ComboBox::from_id_salt("dialog-preset")
                                    .width(body - 150.0)
                                    .selected_text("사용자 지정")
                                    .show_ui(ui, |_| {});
                                let _ = ui.button("구성...");
                            });
                        });
                    });
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        let validation = hwpcraft_engine::validate_format(
                            &without_head(draft.patch()),
                            character,
                        );
                        let (a, c) = chrome::actions(ui, validation.is_ok(), "설정(D)");
                        apply = a;
                        cancel |= c;
                        if let Err(e) = validation {
                            ui.add_space(8.0);
                            ui.colored_label(Color32::DARK_RED, e.to_string());
                        }
                    });
                });
            });
            if apply {
                let patch = draft.patch();
                if draft.for_style {
                    self.ext.format.style_shape_applied(character, patch);
                    cancel = true;
                } else if patch.as_object().is_some_and(|p| p.is_empty()) {
                    cancel = true;
                } else if character {
                    cancel = self.run("format.character", patch);
                } else {
                    let head = head_params(&patch, &draft.values);
                    let format = without_head(patch);
                    let params = match head {
                        Some(head) => json!({"format": format, "head": head}),
                        None => json!({ "format": format }),
                    };
                    cancel = self.run("format.paragraph.apply", params);
                }
            }
            if cancel || r.should_close() {
                if draft.for_style {
                    self.ext.format.style_shape_closed();
                }
                self.show_character = false;
                self.show_paragraph = false;
                self.sync_formatting();
            } else {
                self.format_draft = Some(draft);
            }
        }
    }
    /// Render the preview with RHWP itself so real fonts, 장평, 자간 and spacing show.
    fn update_preview(&mut self, ctx: &egui::Context, d: &mut FormatDraft, character: bool) {
        let key = d.values.to_string();
        let now = ctx.input(|i| i.time);
        if key == d.preview_key {
            return;
        }
        if d.preview.is_some() && now - d.preview_at < 0.15 {
            ctx.request_repaint_after(std::time::Duration::from_millis(160));
            return;
        }
        d.preview_key = key;
        d.preview_at = now;
        let (chars, para) = if character {
            (Some(&d.values), None)
        } else {
            (None, Some(&d.values))
        };
        let Ok(image) = preview_image(&self.renderer, chars, para) else {
            return;
        };
        let size = Vec2::new(image.size[0] as f32, image.size[1] as f32) / 1.5;
        let texture = ctx.load_texture(
            if character {
                "char-preview"
            } else {
                "para-preview"
            },
            image,
            egui::TextureOptions::LINEAR,
        );
        d.preview = Some((d.preview_key.clone(), texture, size));
    }
}

/// Only keys the engine accepts for writing; the query adds read-only ones.
pub(super) fn writable(values: &Value, character: bool) -> Value {
    let mut out = serde_json::Map::new();
    for (k, v) in values.as_object().into_iter().flatten() {
        if character && k == "fontFamily" && values.get("fontFamilies").is_some() {
            continue;
        }
        if matches!(k.as_str(), "headType" | "numberingId" | "paraLevel") {
            continue;
        }
        if hwpcraft_engine::validate_format(&json!({ k: v }), character).is_ok() {
            out.insert(k.clone(), v.clone());
        }
    }
    if out.get("superscript") == Some(&json!(true)) {
        out.remove("subscript");
    }
    Value::Object(out)
}
/// Character sample when `para` is `None`; otherwise Hangul's three-paragraph sample
/// with the shapes applied to the middle paragraph.
pub(super) fn preview_image(
    renderer: &hwpcraft_render::Renderer,
    chars: Option<&Value>,
    para: Option<&Value>,
) -> Result<egui::ColorImage> {
    let mut s = Session::new()?;
    let character = para.is_none();
    if let (Some(values), None) = (chars, para) {
        s.execute(Command::Insert("한글Eng123漢字あいう※⊙".into()))?;
        s.run("format.paragraph", json!({"alignment":"left"}))?;
        s.run("format.character", writable(values, true))?;
    } else {
        let before = "이전 문단 ".repeat(18);
        let body = "가나다라AaBbCc12345 ".repeat(9);
        let after = "이후 문단 ".repeat(18);
        s.execute(Command::Insert(format!("{before}\n{body}\n{after}")))?;
        for paragraph in 0..3 {
            s.select(
                Cursor {
                    paragraph,
                    ..Default::default()
                },
                false,
            )?;
            let mut v = match chars {
                Some(c) if paragraph == 1 => writable(c, true),
                _ => json!({}),
            };
            v["fontSize"] = json!(700);
            if paragraph != 1 {
                v["textColor"] = json!("#a0a0a0");
            }
            s.run("format.character", v)?;
        }
        s.select(
            Cursor {
                paragraph: 1,
                ..Default::default()
            },
            false,
        )?;
        if let Some(values) = para {
            s.run("format.paragraph", writable(values, false))?;
        }
    }
    let page = renderer.page(&s.core, 0, 1.5)?;
    let (w, h) = (page.width as usize, page.height as usize);
    let dark = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        page.rgba[i..i + 3].iter().any(|c| *c < 235)
    };
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for y in 0..h {
        for x in 0..w {
            if dark(x, y) {
                let b = bounds.get_or_insert((x, y, x, y));
                *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
    }
    let (x0, y0, x1, y1) = bounds.ok_or_else(|| anyhow::anyhow!("빈 미리 보기"))?;
    // Paragraph previews keep the body's left edge so margins and indents stay visible.
    let (x0, x1) = if character {
        (x0.saturating_sub(3), (x1 + 3).min(w - 1))
    } else {
        (x0, x1)
    };
    let (y0, y1) = (y0.saturating_sub(3), (y1 + 3).min(h - 1));
    let mut pixels = Vec::with_capacity((x1 - x0 + 1) * (y1 - y0 + 1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = (y * w + x) * 4;
            pixels.push(Color32::from_rgba_premultiplied(
                page.rgba[i],
                page.rgba[i + 1],
                page.rgba[i + 2],
                page.rgba[i + 3],
            ));
        }
    }
    Ok(egui::ColorImage::new([x1 - x0 + 1, y1 - y0 + 1], pixels))
}
/// 문단 종류 changes (kind or level) become the `head` of `format.paragraph.apply`.
fn head_params(patch: &Value, values: &Value) -> Option<Value> {
    if patch.get("headType").is_none() && patch.get("paraLevel").is_none() {
        return None;
    }
    let kind = match values["headType"].as_str() {
        Some("Outline") => "outline",
        Some("Number") => "number",
        Some("Bullet") => "bullet",
        _ => "none",
    };
    let mut head = json!({ "kind": kind });
    if let Some(level) = patch.get("paraLevel") {
        head["level"] = level.clone();
    }
    Some(head)
}
fn without_head(mut patch: Value) -> Value {
    if let Some(o) = patch.as_object_mut() {
        o.remove("headType");
        o.remove("numberingId");
        o.remove("paraLevel");
    }
    patch
}

// ---- shared widgets ----

pub(super) fn lbl(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(text);
        },
    );
}
pub(super) fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    chrome::section(ui, title);
    ui.horizontal_top(|ui| {
        ui.add_space(12.0);
        ui.vertical(add);
    });
}
pub(super) fn spin(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    width: f32,
) -> bool {
    ui.add_sized(
        [width, 24.0],
        egui::DragValue::new(value)
            .range(range)
            .speed(if decimals == 0 { 1.0 } else { 0.1 })
            .fixed_decimals(decimals)
            .suffix(suffix),
    )
    .changed()
}
/// Number stored in `v[key]` with `scale` raw units per displayed unit.
#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut egui::Ui,
    v: &mut Value,
    key: &str,
    scale: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: &str,
    width: f32,
) {
    let mut n = v[key].as_f64().unwrap_or(0.0) / scale;
    if spin(ui, &mut n, range, decimals, suffix, width) {
        v[key] = json!((n * scale).round() as i64);
    }
}
fn check(ui: &mut egui::Ui, v: &mut Value, key: &str, label: &str) {
    let mut b = v[key].as_bool().unwrap_or(false);
    if ui.checkbox(&mut b, label).changed() {
        v[key] = json!(b);
    }
}
pub(super) fn combo(
    ui: &mut egui::Ui,
    id: &str,
    selected: &mut usize,
    items: &[&str],
    width: f32,
) -> bool {
    let before = *selected;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(items.get(*selected).copied().unwrap_or(""))
        .show_ui(ui, |ui| {
            for (i, item) in items.iter().enumerate() {
                ui.selectable_value(selected, i, *item);
            }
        });
    before != *selected
}
pub(super) fn rgb(s: &str) -> Color32 {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
pub(super) fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}
/// Hangul's 기본 색 palette rows (theme tints).
const PALETTE: [[u32; 8]; 5] = [
    [
        0x000000, 0xffffff, 0x1f497d, 0x4f81bd, 0xc0504d, 0x9bbb59, 0x8064a2, 0x4bacc6,
    ],
    [
        0x7f7f7f, 0xf2f2f2, 0xc6d9f0, 0xdbe5f1, 0xf2dcdb, 0xebf1dd, 0xe5e0ec, 0xdbeef3,
    ],
    [
        0x595959, 0xd8d8d8, 0x8db3e2, 0xb8cce4, 0xe5b9b7, 0xd7e3bc, 0xccc1d9, 0xb7dde8,
    ],
    [
        0x3f3f3f, 0xbfbfbf, 0x17365d, 0x366092, 0x953734, 0x76923c, 0x5f497a, 0x31859b,
    ],
    [
        0xff0000, 0xffc000, 0xffff00, 0x92d050, 0x00b050, 0x00b0f0, 0x0070c0, 0x7030a0,
    ],
];
/// Swatch button with Hangul-style palette popup. `none` adds a 없음 entry (returns `None`).
pub(super) fn color_pick(
    ui: &mut egui::Ui,
    id: &str,
    color: Option<&str>,
    none: Option<&str>,
    width: f32,
) -> Option<Option<String>> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::click());
    let p = ui.painter();
    p.rect(
        rect,
        2.0,
        Color32::WHITE,
        Stroke::new(1.0, Color32::from_gray(200)),
        egui::StrokeKind::Inside,
    );
    let swatch = Rect::from_min_max(
        rect.min + Vec2::new(3.0, 3.0),
        rect.max - Vec2::new(18.0, 3.0),
    );
    match color {
        Some(c) => {
            p.rect_filled(swatch, 2.0, rgb(c));
        }
        None => {
            p.rect_stroke(
                swatch,
                2.0,
                Stroke::new(1.0, Color32::from_gray(150)),
                egui::StrokeKind::Inside,
            );
            p.text(
                swatch.center(),
                egui::Align2::CENTER_CENTER,
                none.unwrap_or("없음"),
                egui::FontId::proportional(11.0),
                Color32::from_gray(60),
            );
        }
    }
    chevron(p, Pos2::new(rect.right() - 9.0, rect.center().y));
    let mut out = None;
    egui::Popup::from_toggle_button_response(&response)
        .id(egui::Id::new(("color", id)))
        .close_behavior(egui::containers::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            out = palette(ui, color, none);
            if out.is_some() {
                ui.close();
            }
        });
    out
}
/// Hangul-style color palette; `Some(None)` is the 없음 entry.
pub(super) fn palette(
    ui: &mut egui::Ui,
    color: Option<&str>,
    none: Option<&str>,
) -> Option<Option<String>> {
    let mut out = None;
    ui.set_min_width(196.0);
    if let Some(label) = none
        && ui.button(label).clicked()
    {
        out = Some(None);
    }
    for row in PALETTE {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for c in row {
                let c = Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
                ui.painter().rect(
                    r,
                    1.0,
                    c,
                    Stroke::new(1.0, Color32::from_gray(180)),
                    egui::StrokeKind::Inside,
                );
                if resp.on_hover_text(hex(c)).clicked() {
                    out = Some(Some(hex(c)));
                }
            }
        });
    }
    ui.separator();
    let mut c = rgb(color.unwrap_or("#000000"));
    ui.horizontal(|ui| {
        ui.label("다른 색");
        if ui.color_edit_button_srgba(&mut c).changed() {
            out = Some(Some(hex(c)));
        }
    });
    out
}
fn chevron(p: &egui::Painter, c: Pos2) {
    p.line_segment(
        [c + Vec2::new(-3.5, -1.5), c + Vec2::new(0.0, 2.0)],
        Stroke::new(1.0, Color32::from_gray(90)),
    );
    p.line_segment(
        [c + Vec2::new(0.0, 2.0), c + Vec2::new(3.5, -1.5)],
        Stroke::new(1.0, Color32::from_gray(90)),
    );
}
fn color_key(ui: &mut egui::Ui, v: &mut Value, key: &str, width: f32) {
    let current = v[key].as_str().unwrap_or("#000000").to_owned();
    if let Some(Some(c)) = color_pick(ui, key, Some(&current), None, width) {
        v[key] = json!(c);
    }
}
/// Paint a sample of HWP line type `kind` (표 27; 0 = none, drawn as Hangul's red slash).
pub(super) fn paint_line(
    p: &egui::Painter,
    a: Pos2,
    b: Pos2,
    kind: u8,
    width: f32,
    color: Color32,
) {
    let stroke = Stroke::new(width.max(1.0), color);
    let dir = (b - a).normalized();
    let normal = Vec2::new(-dir.y, dir.x);
    let len = (b - a).length();
    let dashes = |p: &egui::Painter, pattern: &[f32], offset: Vec2, stroke: Stroke| {
        let mut t = 0.0;
        let mut i = 0;
        while t < len {
            let seg = pattern[i % pattern.len()];
            if i % 2 == 0 {
                p.line_segment(
                    [a + dir * t + offset, a + dir * (t + seg).min(len) + offset],
                    stroke,
                );
            }
            t += seg;
            i += 1;
        }
    };
    match kind {
        0 => {}
        2 => dashes(p, &[6.0, 3.0], Vec2::ZERO, stroke),
        3 => dashes(p, &[1.5, 2.0], Vec2::ZERO, stroke),
        4 => dashes(p, &[6.0, 2.0, 1.5, 2.0], Vec2::ZERO, stroke),
        5 => dashes(p, &[6.0, 2.0, 1.5, 2.0, 1.5, 2.0], Vec2::ZERO, stroke),
        6 => dashes(p, &[10.0, 4.0], Vec2::ZERO, stroke),
        7 => {
            let mut t = 0.0;
            while t < len {
                p.circle_stroke(a + dir * t, 1.2, Stroke::new(1.0, color));
                t += 4.0;
            }
        }
        8..=11 => {
            let (w1, w2) = match kind {
                9 => (1.0, 2.0),
                10 => (2.0, 1.0),
                _ => (1.0, 1.0),
            };
            p.line_segment([a - normal * 1.5, b - normal * 1.5], Stroke::new(w1, color));
            p.line_segment([a + normal * 1.5, b + normal * 1.5], Stroke::new(w2, color));
            if kind == 11 {
                p.line_segment(
                    [a + normal * 3.5, b + normal * 3.5],
                    Stroke::new(1.0, color),
                );
            }
        }
        12 | 13 => {
            let rows: &[f32] = if kind == 13 { &[-1.5, 1.5] } else { &[0.0] };
            for off in rows {
                let points: Vec<Pos2> = (0..=(len as usize))
                    .map(|t| {
                        let t = t as f32;
                        a + dir * t + normal * (off + (t * 0.8).sin() * 1.5)
                    })
                    .collect();
                p.add(egui::Shape::line(points, Stroke::new(1.0, color)));
            }
        }
        14..=17 => {
            let light = Color32::from_gray(200);
            let (first, second) = if kind.is_multiple_of(2) {
                (color, light)
            } else {
                (light, color)
            };
            p.line_segment([a - normal, b - normal], Stroke::new(width.max(1.0), first));
            p.line_segment(
                [a + normal, b + normal],
                Stroke::new(width.max(1.0), second),
            );
        }
        _ => {
            p.line_segment([a, b], stroke);
        }
    }
}
/// Combo whose entries are drawn line samples (Hangul shows pictures, not names).
pub(super) fn line_combo(
    ui: &mut egui::Ui,
    id: &str,
    selected: &mut u8,
    names: &[&str],
    first: u8,
    width: f32,
) -> bool {
    let before = *selected;
    let sample = |ui: &mut egui::Ui, kind: u8, rect: Rect| {
        let a = Pos2::new(rect.left() + 6.0, rect.center().y);
        let b = Pos2::new(rect.right() - 6.0, rect.center().y);
        if kind == 0 && first == 0 {
            let r = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 12.0, 10.0));
            ui.painter().rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
            ui.painter().line_segment(
                [r.left_bottom(), r.right_top()],
                Stroke::new(1.0, Color32::RED),
            );
        } else {
            paint_line(ui.painter(), a, b, kind, 1.0, Color32::BLACK);
        }
    };
    let r = egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text("")
        .show_ui(ui, |ui| {
            for (i, name) in names.iter().enumerate() {
                let kind = first + i as u8;
                let resp = ui
                    .add_sized(
                        [width - 10.0, 20.0],
                        egui::Button::selectable(*selected == kind, ""),
                    )
                    .on_hover_text(*name);
                sample(ui, kind, resp.rect);
                if resp.clicked() {
                    *selected = kind;
                }
            }
        });
    let mut field = r.response.rect;
    field.max.x -= 18.0;
    sample(ui, *selected, field);
    before != *selected
}

/// Small 24×24 tool button with a painted icon (no emoji font is bundled).
pub(super) fn icon_button(ui: &mut egui::Ui, kind: &str, tip: &str, enabled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());
    let response = if enabled {
        response
    } else {
        response.on_hover_cursor(egui::CursorIcon::Default)
    };
    let p = ui.painter();
    p.rect(
        rect,
        2.0,
        if enabled && response.hovered() {
            Color32::from_gray(238)
        } else {
            Color32::WHITE
        },
        Stroke::new(1.0, Color32::from_gray(200)),
        egui::StrokeKind::Inside,
    );
    let ink = if enabled {
        Color32::from_rgb(46, 105, 220)
    } else {
        Color32::from_gray(190)
    };
    let s = Stroke::new(1.4, ink);
    let c = rect.center();
    let v = |x: f32, y: f32| c + Vec2::new(x, y);
    match kind {
        "add" => {
            p.line_segment([v(-6.0, 0.0), v(6.0, 0.0)], s);
            p.line_segment([v(0.0, -6.0), v(0.0, 6.0)], s);
        }
        "edit" => {
            p.line_segment(
                [v(-5.0, 5.0), v(4.0, -4.0)],
                Stroke::new(2.5, Color32::from_gray(if enabled { 90 } else { 190 })),
            );
            p.line_segment([v(-6.0, 6.0), v(-4.0, 4.0)], s);
        }
        "delete" | "clear" => {
            let red = if enabled {
                Color32::from_rgb(210, 50, 50)
            } else {
                Color32::from_gray(200)
            };
            p.line_segment([v(-5.0, -5.0), v(5.0, 5.0)], Stroke::new(1.5, red));
            p.line_segment([v(-5.0, 5.0), v(5.0, -5.0)], Stroke::new(1.5, red));
        }
        "replace" => {
            p.line_segment([v(-6.0, -2.0), v(5.0, -2.0)], s);
            p.line_segment([v(5.0, -2.0), v(2.0, -5.0)], s);
            p.line_segment([v(6.0, 3.0), v(-5.0, 3.0)], s);
            p.line_segment([v(-5.0, 3.0), v(-2.0, 6.0)], s);
        }
        "up" | "down" => {
            let d = if kind == "up" { -1.0 } else { 1.0 };
            p.line_segment([v(0.0, -6.0 * d), v(0.0, 6.0 * d)], s);
            p.line_segment([v(0.0, 6.0 * d), v(-4.0, 2.0 * d)], s);
            p.line_segment([v(0.0, 6.0 * d), v(4.0, 2.0 * d)], s);
        }
        "gallery" => {
            for i in 0..3 {
                for j in 0..2 {
                    p.rect_stroke(
                        Rect::from_min_size(
                            v(-7.0 + i as f32 * 5.0, -5.0 + j as f32 * 5.5),
                            Vec2::splat(4.0),
                        ),
                        0.0,
                        Stroke::new(1.0, ink),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        "import" => {
            p.rect_stroke(
                Rect::from_min_max(v(-7.0, -3.0), v(7.0, 6.0)),
                1.0,
                s,
                egui::StrokeKind::Inside,
            );
            p.line_segment([v(-7.0, -3.0), v(-5.0, -6.0)], s);
            p.line_segment([v(-5.0, -6.0), v(0.0, -6.0)], s);
        }
        _ => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(13.0)),
                1.0,
                s,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(Rect::from_min_max(v(-3.5, -6.5), v(3.5, -2.0)), 0.0, ink);
        }
    }
    let response = response.on_hover_text(tip);
    enabled && response.clicked()
}

// ---- 글자 모양 ----

fn language_metric(
    ui: &mut egui::Ui,
    d: &mut FormatDraft,
    key: &str,
    default: i64,
    range: std::ops::RangeInclusive<f64>,
) {
    let mut values = d.values[key]
        .as_array()
        .filter(|a| a.len() == 7)
        .cloned()
        .unwrap_or_else(|| vec![json!(default); 7]);
    let index = d.language.saturating_sub(1).min(6);
    let mut n = values[index].as_f64().unwrap_or(default as f64);
    if spin(ui, &mut n, range, 0, " %", 91.0) {
        if d.language == 0 {
            values.fill(json!(n.round() as i64));
        } else {
            values[index] = json!(n.round() as i64);
        }
        d.values[key] = json!(values);
    }
}
fn font_box(ui: &mut egui::Ui, d: &mut FormatDraft, width: f32) {
    let index = d.language.saturating_sub(1).min(6);
    let mut fonts = d.values["fontFamilies"]
        .as_array()
        .filter(|a| a.len() == 7)
        .cloned()
        .unwrap_or_else(|| vec![d.values["fontFamily"].clone(); 7]);
    let mut font = fonts[index].as_str().unwrap_or("함초롬바탕").to_owned();
    let before = font.clone();
    font_combo(ui, "char-font", &mut font, width);
    if font != before {
        if d.language == 0 {
            fonts.fill(json!(font));
        } else {
            fonts[index] = json!(font);
        }
        d.values["fontFamilies"] = json!(fonts);
    }
}
/// Editable font name with a drop-down of known fonts, as in Hangul's 글꼴 콤보.
pub(super) fn font_combo(ui: &mut egui::Ui, id: &str, font: &mut String, width: f32) -> bool {
    let before = font.clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let edit = ui.add_sized(
            [width - 20.0, 24.0],
            egui::TextEdit::singleline(font)
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional)),
        );
        let (rect, response) = ui.allocate_exact_size(Vec2::new(20.0, 24.0), egui::Sense::click());
        ui.painter().rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, Color32::from_gray(200)),
            egui::StrokeKind::Inside,
        );
        chevron(ui.painter(), rect.center());
        egui::Popup::from_toggle_button_response(&response)
            .id(egui::Id::new(("font-list", id)))
            .show(|ui| {
                ui.set_min_width(width);
                // Typing narrows the list, as in Hangul's 글꼴 콤보.
                let all = font_names();
                let query = font.to_lowercase();
                let filtered: Vec<&String> = if all.contains(font) {
                    all.iter().collect()
                } else {
                    all.iter()
                        .filter(|n| n.to_lowercase().contains(&query))
                        .collect()
                };
                egui::ScrollArea::vertical().max_height(300.0).show_rows(
                    ui,
                    20.0,
                    filtered.len(),
                    |ui, rows| {
                        for name in &filtered[rows] {
                            if ui
                                .selectable_label(
                                    *font == **name,
                                    RichText::new(name.as_str()).size(13.0),
                                )
                                .clicked()
                            {
                                *font = (*name).clone();
                            }
                        }
                    },
                );
            });
        let _ = edit;
    });
    *font != before && !font.trim().is_empty()
}
fn attribute_button(ui: &mut egui::Ui, kind: &str, on: bool, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::click());
    let p = ui.painter();
    p.rect(
        rect,
        3.0,
        if on {
            Color32::from_rgb(222, 233, 255)
        } else if response.hovered() {
            Color32::from_gray(245)
        } else {
            Color32::WHITE
        },
        Stroke::new(
            1.0,
            if on {
                chrome::Tokens::ACCENT
            } else {
                Color32::from_gray(200)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let font = |size: f32| egui::FontId::proportional(size);
    let glyph = |p: &egui::Painter, at: Pos2, size: f32, color: Color32| {
        p.text(at, egui::Align2::CENTER_CENTER, "가", font(size), color)
    };
    let ink = Color32::from_gray(40);
    match kind {
        "bold" => {
            glyph(p, c, 17.0, ink);
            glyph(p, c + Vec2::new(0.6, 0.0), 17.0, ink);
        }
        "italic" => {
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "가",
                egui::FontId::proportional(17.0),
                ink,
            );
            p.line_segment(
                [c + Vec2::new(-7.0, 8.0), c + Vec2::new(7.0, -8.0)],
                Stroke::new(0.0, ink),
            );
        }
        "underline" => {
            glyph(p, c - Vec2::new(0.0, 2.0), 16.0, ink);
            p.line_segment(
                [c + Vec2::new(-8.0, 9.0), c + Vec2::new(8.0, 9.0)],
                Stroke::new(1.0, ink),
            );
        }
        "strikethrough" => {
            glyph(p, c, 16.0, ink);
            p.line_segment(
                [c + Vec2::new(-9.0, 0.0), c + Vec2::new(9.0, 0.0)],
                Stroke::new(1.0, ink),
            );
        }
        "outline" => {
            for off in [(-0.8, 0.0), (0.8, 0.0), (0.0, -0.8), (0.0, 0.8)] {
                glyph(p, c + Vec2::new(off.0, off.1), 17.0, ink);
            }
            glyph(p, c, 17.0, Color32::WHITE);
        }
        "shadow" => {
            glyph(p, c + Vec2::new(1.5, 1.5), 17.0, Color32::from_gray(170));
            glyph(p, c, 17.0, ink);
        }
        "emboss" => {
            glyph(p, c + Vec2::new(-0.8, -0.8), 17.0, Color32::from_gray(150));
            glyph(p, c, 17.0, Color32::from_gray(225));
        }
        "engrave" => {
            glyph(p, c + Vec2::new(0.8, 0.8), 17.0, Color32::from_gray(150));
            glyph(p, c, 17.0, Color32::from_gray(225));
        }
        "superscript" | "subscript" => {
            let dash = Stroke::new(1.0, Color32::from_gray(120));
            let r = Rect::from_center_size(c + Vec2::new(-3.0, 2.0), Vec2::splat(15.0));
            p.rect_stroke(r, 0.0, dash, egui::StrokeKind::Inside);
            let small = if kind == "superscript" {
                c + Vec2::new(6.0, -6.0)
            } else {
                c + Vec2::new(6.0, 8.0)
            };
            glyph(p, small, 10.0, ink);
        }
        _ => {
            glyph(p, c, 17.0, ink);
        }
    }
    response.on_hover_text(tip).clicked()
}
fn character_fields(ui: &mut egui::Ui, d: &mut FormatDraft) {
    chrome::tabs(ui, &mut d.tab, &["기본", "확장", "테두리/배경"]);
    ui.spacing_mut().item_spacing.y = 6.0;
    match d.tab {
        0 => character_basic(ui, d),
        1 => character_extended(ui, d),
        _ => {
            border_fields(ui, d, None);
            fill_fields(ui, &mut d.values);
        }
    }
    if d.tab != 2 {
        preview_box(ui, d, if d.tab == 0 { 134.0 } else { 101.0 }, true);
    }
}
fn character_basic(ui: &mut egui::Ui, d: &mut FormatDraft) {
    ui.horizontal(|ui| {
        lbl(ui, "기준 크기", 53.0);
        field(
            ui,
            &mut d.values,
            "fontSize",
            100.0,
            1.0..=4096.0,
            1,
            " pt",
            106.0,
        );
    });
    group(ui, "언어별 설정", |ui| {
        ui.horizontal(|ui| {
            lbl(ui, "언어", 53.0);
            combo(ui, "char-language", &mut d.language, &LANGUAGES, 79.0);
            ui.add_space(16.0);
            lbl(ui, "글꼴", 24.0);
            font_box(ui, d, 324.0);
        });
        ui.horizontal(|ui| {
            lbl(ui, "상대크기", 53.0);
            language_metric(ui, d, "relativeSizes", 100, 10.0..=250.0);
            ui.add_space(16.0);
            lbl(ui, "장평", 24.0);
            language_metric(ui, d, "ratios", 100, 50.0..=200.0);
        });
        ui.horizontal(|ui| {
            lbl(ui, "글자 위치", 53.0);
            language_metric(ui, d, "charOffsets", 0, -100.0..=100.0);
            ui.add_space(16.0);
            lbl(ui, "자간", 24.0);
            language_metric(ui, d, "spacings", 0, -50.0..=50.0);
        });
    });
    group(ui, "속성", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (key, kind, tip) in [
                ("bold", "bold", "진하게"),
                ("italic", "italic", "기울임"),
                ("underline", "underline", "밑줄"),
                ("strikethrough", "strikethrough", "취소선"),
                ("outlineType", "outline", "외곽선"),
                ("shadowType", "shadow", "그림자"),
                ("emboss", "emboss", "양각"),
                ("engrave", "engrave", "음각"),
                ("superscript", "superscript", "위 첨자"),
                ("subscript", "subscript", "아래 첨자"),
                ("", "normal", "보통 모양"),
            ] {
                if matches!(kind, "superscript" | "normal") {
                    ui.add_space(4.0);
                }
                let numeric = matches!(key, "outlineType" | "shadowType");
                let on = if numeric {
                    d.values[key].as_i64().unwrap_or(0) > 0
                } else {
                    d.values[key].as_bool().unwrap_or(false)
                };
                if !attribute_button(ui, kind, on, tip) {
                    continue;
                }
                let v = &mut d.values;
                match kind {
                    "normal" => {
                        for k in [
                            "bold",
                            "italic",
                            "underline",
                            "strikethrough",
                            "emboss",
                            "engrave",
                            "superscript",
                            "subscript",
                        ] {
                            v[k] = json!(false);
                        }
                        v["underlineType"] = json!("None");
                        v["outlineType"] = json!(0);
                        v["shadowType"] = json!(0);
                    }
                    _ if numeric => v[key] = json!(i64::from(!on)),
                    _ => {
                        v[key] = json!(!on);
                        if key == "underline" {
                            v["underlineType"] = json!(if on { "None" } else { "Bottom" });
                        }
                        // Hangul keeps these pairs exclusive.
                        for (a, b) in [
                            ("superscript", "subscript"),
                            ("subscript", "superscript"),
                            ("emboss", "engrave"),
                            ("engrave", "emboss"),
                        ] {
                            if key == a && !on {
                                v[b] = json!(false);
                            }
                        }
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            lbl(ui, "글자 색", 36.0);
            color_key(ui, &mut d.values, "textColor", 91.0);
        });
        ui.horizontal(|ui| {
            lbl(ui, "음영 색", 36.0);
            let shade = d.values["shadeColor"]
                .as_str()
                .unwrap_or("#ffffff")
                .to_owned();
            let none = shade.eq_ignore_ascii_case("#ffffff");
            if let Some(c) = color_pick(
                ui,
                "shadeColor",
                (!none).then_some(&shade),
                Some("음영 색"),
                91.0,
            ) {
                d.values["shadeColor"] = json!(c.unwrap_or_else(|| "#ffffff".into()));
            }
        });
    });
}
fn character_extended(ui: &mut egui::Ui, d: &mut FormatDraft) {
    group(ui, "그림자", |ui| {
        let mut kind = d.values["shadowType"].as_u64().unwrap_or(0) as u8;
        ui.horizontal(|ui| {
            for (i, label) in ["없음", "비연속", "연속"].iter().enumerate() {
                if ui.radio_value(&mut kind, i as u8, *label).changed() {
                    d.values["shadowType"] = json!(kind);
                }
                ui.add_space(12.0);
            }
        });
        ui.add_enabled_ui(kind > 0, |ui| {
            ui.horizontal(|ui| {
                lbl(ui, "X 방향", 32.0);
                field(
                    ui,
                    &mut d.values,
                    "shadowOffsetX",
                    1.0,
                    -100.0..=100.0,
                    0,
                    " %",
                    91.0,
                );
                ui.add_space(16.0);
                lbl(ui, "색", 14.0);
                color_key(ui, &mut d.values, "shadowColor", 91.0);
            });
            ui.horizontal(|ui| {
                lbl(ui, "Y 방향", 32.0);
                field(
                    ui,
                    &mut d.values,
                    "shadowOffsetY",
                    1.0,
                    -100.0..=100.0,
                    0,
                    " %",
                    91.0,
                );
            });
        });
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(258.0);
            group(ui, "밑줄", |ui| {
                let mut position = match d.values["underlineType"].as_str() {
                    Some("Bottom") => 1,
                    Some("Top") => 2,
                    _ => 0,
                };
                ui.horizontal(|ui| {
                    lbl(ui, "위치", 22.0);
                    if combo(
                        ui,
                        "underline-pos",
                        &mut position,
                        &["없음", "아래쪽", "위쪽"],
                        79.0,
                    ) {
                        d.values["underlineType"] = json!(["None", "Bottom", "Top"][position]);
                        d.values["underline"] = json!(position > 0);
                    }
                });
                ui.add_enabled_ui(position > 0, |ui| {
                    ui.horizontal(|ui| {
                        lbl(ui, "모양", 22.0);
                        let mut shape = d.values["underlineShape"].as_u64().unwrap_or(0) as u8 + 1;
                        if line_combo(
                            ui,
                            "underline-shape",
                            &mut shape,
                            &LINE_TYPES[1..12],
                            1,
                            79.0,
                        ) {
                            d.values["underlineShape"] = json!(shape - 1);
                        }
                    });
                    ui.horizontal(|ui| {
                        lbl(ui, "색", 22.0);
                        color_key(ui, &mut d.values, "underlineColor", 91.0);
                    });
                });
            });
        });
        ui.vertical(|ui| {
            ui.set_width(258.0);
            group(ui, "취소선", |ui| {
                let on = d.values["strikethrough"].as_bool().unwrap_or(false);
                let mut shape = if on {
                    d.values["strikeShape"].as_u64().unwrap_or(0) as u8 + 1
                } else {
                    0
                };
                ui.horizontal(|ui| {
                    lbl(ui, "모양", 22.0);
                    if line_combo(ui, "strike-shape", &mut shape, &LINE_TYPES[..12], 0, 79.0) {
                        d.values["strikethrough"] = json!(shape > 0);
                        if shape > 0 {
                            d.values["strikeShape"] = json!(shape - 1);
                        }
                    }
                });
                ui.add_enabled_ui(shape > 0, |ui| {
                    ui.horizontal(|ui| {
                        lbl(ui, "색", 22.0);
                        color_key(ui, &mut d.values, "strikeColor", 91.0);
                    });
                });
            });
        });
    });
    group(ui, "기타", |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(268.0);
                ui.horizontal(|ui| {
                    lbl(ui, "외곽선", 33.0);
                    let mut outline = d.values["outlineType"].as_u64().unwrap_or(0) as usize;
                    if combo(ui, "outline", &mut outline, &OUTLINES, 79.0) {
                        d.values["outlineType"] = json!(outline);
                    }
                });
                ui.horizontal(|ui| {
                    lbl(ui, "강조점", 33.0);
                    let mut dot = d.values["emphasisDot"].as_u64().unwrap_or(0) as usize;
                    if combo(ui, "emphasis", &mut dot, &EMPHASIS, 79.0) {
                        d.values["emphasisDot"] = json!(dot);
                    }
                });
            });
            ui.vertical(|ui| {
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut false, "글꼴에 어울리는 빈칸"),
                )
                .on_disabled_hover_text("RHWP 모델에서 편집을 지원하지 않습니다.");
                check(ui, &mut d.values, "kerning", "커닝");
            });
        });
    });
}
fn preview_box(ui: &mut egui::Ui, d: &FormatDraft, height: f32, character: bool) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::hover(),
    );
    ui.painter().rect(
        rect,
        0.0,
        Color32::WHITE,
        Stroke::new(1.0, Color32::from_gray(215)),
        egui::StrokeKind::Inside,
    );
    if let Some((_, texture, size)) = &d.preview {
        let inner = rect.shrink(6.0);
        let scale = (inner.width() / size.x)
            .min(inner.height() / size.y)
            .min(1.0);
        let size = *size * scale;
        // Hangul anchors the character sample at the bottom-left, paragraphs at the top.
        let min = if character {
            Pos2::new(inner.left(), inner.bottom() - size.y)
        } else {
            Pos2::new(inner.center().x - size.x / 2.0, inner.top())
        };
        ui.painter().image(
            texture.id(),
            Rect::from_min_size(min, size),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

// ---- 테두리/배경 (shared by both windows) ----

fn border_fields(ui: &mut egui::Ui, d: &mut FormatDraft, paragraph: Option<()>) {
    group(ui, "테두리", |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(if paragraph.is_some() { 182.0 } else { 324.0 });
                let mut changed = false;
                ui.horizontal(|ui| {
                    lbl(ui, "종류", 22.0);
                    changed |= line_combo(ui, "border-kind", &mut d.pen.0, &LINE_TYPES, 0, 115.0);
                });
                ui.horizontal(|ui| {
                    lbl(ui, "굵기", 22.0);
                    let mut w = d.pen.1 as usize;
                    let names: Vec<String> =
                        BORDER_WIDTHS.iter().map(|w| format!("{w}mm")).collect();
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    if combo(ui, "border-width", &mut w, &names, 115.0) {
                        d.pen.1 = w as u8;
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    lbl(ui, "색", 22.0);
                    if let Some(Some(c)) =
                        color_pick(ui, "border-color", Some(&d.pen.2), None, 127.0)
                    {
                        d.pen.2 = c;
                        changed = true;
                    }
                });
                if paragraph.is_some() {
                    check(ui, &mut d.values, "borderConnect", "문단 테두리 연결");
                    let id = egui::Id::new("border-live");
                    let mut live = ui.data(|m| m.get_temp(id)).unwrap_or(true);
                    if ui.checkbox(&mut live, "선 모양 바로 적용").changed() {
                        ui.data_mut(|m| m.insert_temp(id, live));
                    }
                    // Hangul re-pens the sides that already have a line.
                    if changed && live {
                        let pen = pen_value(&d.pen);
                        for side in SIDES {
                            if d.values[side]["type"].as_u64().unwrap_or(0) > 0 {
                                d.values[side] = pen.clone();
                            }
                        }
                    }
                }
            });
            border_preview(ui, d);
        });
    });
}
fn pen_value(pen: &(u8, u8, String)) -> Value {
    json!({"type": pen.0, "width": pen.1, "color": pen.2})
}
fn border_preview(ui: &mut egui::Ui, d: &mut FormatDraft) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(192.0, 164.0), egui::Sense::hover());
        let frame = Rect::from_min_size(rect.min, Vec2::new(162.0, 134.0));
        let p = ui.painter();
        p.rect_filled(frame, 0.0, Color32::WHITE);
        p.rect_stroke(
            frame,
            0.0,
            Stroke::new(1.0, Color32::from_gray(200)),
            egui::StrokeKind::Inside,
        );
        let inner = frame.shrink(16.0);
        if d.values["fillType"] == "solid" {
            p.rect_filled(
                inner,
                0.0,
                rgb(d.values["fillColor"].as_str().unwrap_or("#ffffff")),
            );
        }
        for (side, (a, b)) in SIDES.iter().zip([
            (inner.left_top(), inner.left_bottom()),
            (inner.right_top(), inner.right_bottom()),
            (inner.left_top(), inner.right_top()),
            (inner.left_bottom(), inner.right_bottom()),
        ]) {
            let line = &d.values[*side];
            let kind = line["type"].as_u64().unwrap_or(0) as u8;
            if kind == 0 {
                p.line_segment([a, b], Stroke::new(1.0, Color32::from_gray(225)));
            } else {
                let w = BORDER_WIDTHS[(line["width"].as_u64().unwrap_or(0) as usize).min(15)] * 2.0;
                paint_line(
                    p,
                    a,
                    b,
                    kind,
                    w,
                    rgb(line["color"].as_str().unwrap_or("#000000")),
                );
            }
        }
        let button = |ui: &mut egui::Ui, at: Pos2, tip: &str, sides: &[&str]| {
            let r = Rect::from_min_size(at, Vec2::splat(24.0));
            let resp = ui.interact(r, egui::Id::new(("border-side", tip)), egui::Sense::click());
            ui.painter().rect(
                r,
                2.0,
                if resp.hovered() {
                    Color32::from_gray(240)
                } else {
                    Color32::WHITE
                },
                Stroke::new(1.0, Color32::from_gray(190)),
                egui::StrokeKind::Inside,
            );
            let g = r.shrink(5.0);
            for s in sides {
                let (a, b) = match *s {
                    "borderLeft" => (g.left_top(), g.left_bottom()),
                    "borderRight" => (g.right_top(), g.right_bottom()),
                    "borderTop" => (g.left_top(), g.right_top()),
                    _ => (g.left_bottom(), g.right_bottom()),
                };
                ui.painter()
                    .line_segment([a, b], Stroke::new(1.5, chrome::Tokens::ACCENT));
            }
            resp.on_hover_text(tip).clicked()
        };
        let x = frame.right() + 6.0;
        let toggle = |sides: &[&str], d: &mut FormatDraft| {
            let pen = pen_value(&d.pen);
            let all = sides.iter().all(|s| d.values[*s] == pen);
            for s in sides {
                d.values[*s] = if all {
                    json!({"type": 0, "width": 0, "color": "#000000"})
                } else {
                    pen.clone()
                };
            }
        };
        if button(ui, Pos2::new(x, frame.top()), "위", &["borderTop"]) {
            toggle(&["borderTop"], d);
        }
        if button(
            ui,
            Pos2::new(x, frame.bottom() - 24.0),
            "아래",
            &["borderBottom"],
        ) {
            toggle(&["borderBottom"], d);
        }
        let y = frame.bottom() + 6.0;
        if button(ui, Pos2::new(frame.left(), y), "왼쪽", &["borderLeft"]) {
            toggle(&["borderLeft"], d);
        }
        if button(
            ui,
            Pos2::new(frame.right() - 24.0, y),
            "오른쪽",
            &["borderRight"],
        ) {
            toggle(&["borderRight"], d);
        }
        if button(ui, Pos2::new(x, y), "모두", &SIDES) {
            toggle(&SIDES, d);
        }
    });
}
fn fill_fields(ui: &mut egui::Ui, v: &mut Value) {
    group(ui, "배경", |ui| {
        let solid = v["fillType"] == "solid";
        ui.horizontal(|ui| {
            lbl(ui, "면 색", 47.0);
            let color = v["fillColor"].as_str().unwrap_or("#ffffff").to_owned();
            if let Some(c) = color_pick(ui, "fill", solid.then_some(&color), Some("면 색"), 91.0)
            {
                match c {
                    Some(c) => {
                        v["fillType"] = json!("solid");
                        v["fillColor"] = json!(c);
                    }
                    None => v["fillType"] = json!("none"),
                }
            }
        });
        ui.add_enabled_ui(solid, |ui| {
            ui.horizontal(|ui| {
                lbl(ui, "무늬 색", 47.0);
                color_key(ui, v, "patternColor", 91.0);
            });
            ui.horizontal(|ui| {
                lbl(ui, "무늬 모양", 47.0);
                let mut pattern = v["patternType"].as_i64().unwrap_or(0).clamp(0, 6) as usize;
                if combo(ui, "pattern", &mut pattern, &PATTERNS, 79.0) {
                    v["patternType"] = json!(pattern);
                }
            });
        });
    });
}

// ---- 문단 모양 ----

fn paragraph_fields(ui: &mut egui::Ui, d: &mut FormatDraft) {
    chrome::tabs(ui, &mut d.tab, &["기본", "확장", "탭 설정", "테두리/배경"]);
    ui.spacing_mut().item_spacing.y = 6.0;
    match d.tab {
        0 => {
            paragraph_basic(ui, d);
            preview_box(ui, d, 120.0, false);
        }
        1 => paragraph_extended(ui, d),
        2 => paragraph_tabs(ui, d),
        _ => {
            border_fields(ui, d, Some(()));
            fill_fields(ui, &mut d.values);
            border_spacing(ui, &mut d.values);
        }
    }
}
fn align_button(ui: &mut egui::Ui, align: &str, on: bool, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::click());
    let p = ui.painter();
    p.rect(
        rect,
        3.0,
        if on {
            Color32::from_rgb(222, 233, 255)
        } else {
            Color32::WHITE
        },
        Stroke::new(
            if on { 1.5 } else { 1.0 },
            if on {
                chrome::Tokens::ACCENT
            } else {
                Color32::from_gray(200)
            },
        ),
        egui::StrokeKind::Inside,
    );
    paint_align(p, rect.shrink(9.0), align);
    response.on_hover_text(tip).clicked()
}
/// Alignment glyph: six text lines laid out like Hangul's icons.
pub(super) fn paint_align(p: &egui::Painter, r: Rect, align: &str) {
    let stroke = Stroke::new(1.0, Color32::from_gray(60));
    for row in 0..6 {
        let y = r.top() + 1.0 + row as f32 * (r.height() - 2.0) / 5.0;
        let full = r.width();
        let short = full * 0.62;
        let width = match align {
            "justify" | "split" | "distribute" => full,
            _ if row % 2 == 1 => short,
            _ => full,
        };
        let x = match align {
            "center" => r.center().x - width / 2.0,
            "right" => r.right() - width,
            _ => r.left(),
        };
        if align == "split" && row % 2 == 1 {
            // 나눔: spaced words on the last line of each pair.
            for i in 0..4 {
                let x = r.left() + i as f32 * full / 3.6;
                p.line_segment([Pos2::new(x, y), Pos2::new(x + full / 6.0, y)], stroke);
            }
            continue;
        }
        if align == "distribute" && row == 5 {
            for i in 0..5 {
                let x = r.left() + i as f32 * full / 4.4;
                p.line_segment([Pos2::new(x, y), Pos2::new(x + 2.0, y)], stroke);
            }
            continue;
        }
        p.line_segment([Pos2::new(x, y), Pos2::new(x + width, y)], stroke);
    }
}
fn paragraph_basic(ui: &mut egui::Ui, d: &mut FormatDraft) {
    group(ui, "일반", |ui| {
        ui.horizontal(|ui| {
            lbl(ui, "정렬", 36.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            for (align, tip) in [
                ("justify", "양쪽 정렬"),
                ("left", "왼쪽 정렬"),
                ("center", "가운데 정렬"),
                ("right", "오른쪽 정렬"),
                ("distribute", "배분 정렬"),
                ("split", "나눔 정렬"),
            ] {
                if align_button(ui, align, d.values["alignment"] == align, tip) {
                    d.values["alignment"] = json!(align);
                }
            }
        });
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(160.0);
            group(ui, "여백", |ui| {
                ui.horizontal(|ui| {
                    lbl(ui, "왼쪽", 33.0);
                    field(
                        ui,
                        &mut d.values,
                        "marginLeft",
                        200.0,
                        0.0..=500.0,
                        1,
                        " pt",
                        91.0,
                    );
                });
                ui.horizontal(|ui| {
                    lbl(ui, "오른쪽", 33.0);
                    field(
                        ui,
                        &mut d.values,
                        "marginRight",
                        200.0,
                        0.0..=500.0,
                        1,
                        " pt",
                        91.0,
                    );
                });
            });
        });
        ui.add_space(8.0);
        ui.vertical(|ui| {
            group(ui, "첫 줄", |ui| {
                let indent = d.values["indent"].as_f64().unwrap_or(0.0);
                let mut kind = if indent > 0.0 {
                    1
                } else if indent < 0.0 {
                    2
                } else {
                    0
                };
                let mut amount = indent.abs() / 200.0;
                let before = kind;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.radio_value(&mut kind, 0, "보통");
                        ui.radio_value(&mut kind, 1, "들여쓰기");
                        ui.radio_value(&mut kind, 2, "내어쓰기");
                    });
                    ui.vertical(|ui| {
                        ui.add_space(26.0);
                        ui.add_enabled_ui(kind > 0, |ui| {
                            if spin(ui, &mut amount, 0.0..=500.0, 1, " pt", 91.0) || kind != before
                            {
                                if kind != before && amount == 0.0 {
                                    amount = 10.0;
                                }
                                let sign = match kind {
                                    1 => 1.0,
                                    2 => -1.0,
                                    _ => 0.0,
                                };
                                d.values["indent"] = json!((sign * amount * 200.0).round() as i64);
                            }
                        });
                    });
                });
                if kind == 0 && before != 0 {
                    d.values["indent"] = json!(0);
                }
            });
        });
    });
    group(ui, "간격", |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(195.0);
                ui.horizontal(|ui| {
                    lbl(ui, "줄 간격", 36.0);
                    let types = ["Percent", "Fixed", "SpaceOnly", "Minimum"];
                    let mut index = types
                        .iter()
                        .position(|t| d.values["lineSpacingType"] == *t)
                        .unwrap_or(0);
                    let old_percent = index == 0;
                    if combo(
                        ui,
                        "line-spacing-type",
                        &mut index,
                        &["글자에 따라", "고정 값", "여백만 지정", "최소"],
                        123.0,
                    ) {
                        d.values["lineSpacingType"] = json!(types[index]);
                        let percent = index == 0;
                        if percent != old_percent {
                            let raw = d.values["lineSpacing"].as_f64().unwrap_or(160.0);
                            d.values["lineSpacing"] = json!(if percent {
                                (raw / d.font_points).round().clamp(50.0, 500.0) as i64
                            } else {
                                (raw * d.font_points).round().clamp(100.0, 20000.0) as i64
                            });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    let percent =
                        d.values["lineSpacingType"].as_str().unwrap_or("Percent") == "Percent";
                    field(
                        ui,
                        &mut d.values,
                        "lineSpacing",
                        if percent { 1.0 } else { 100.0 },
                        if percent { 0.0..=500.0 } else { 0.0..=2000.0 },
                        if percent { 0 } else { 1 },
                        if percent { " %" } else { " pt" },
                        91.0,
                    );
                });
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    lbl(ui, "문단 위", 47.0);
                    field(
                        ui,
                        &mut d.values,
                        "spacingBefore",
                        100.0,
                        0.0..=2000.0,
                        1,
                        " pt",
                        91.0,
                    );
                });
                ui.horizontal(|ui| {
                    lbl(ui, "문단 아래", 47.0);
                    field(
                        ui,
                        &mut d.values,
                        "spacingAfter",
                        100.0,
                        0.0..=2000.0,
                        1,
                        " pt",
                        91.0,
                    );
                });
            });
        });
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut false, "편집 용지의 줄 격자 사용"),
        )
        .on_disabled_hover_text("줄 격자는 아직 지원하지 않습니다.");
    });
    group(ui, "줄 나눔 기준", |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(195.0);
                ui.horizontal(|ui| {
                    lbl(ui, "한글 단위", 47.0);
                    let mut k = d.values["koreanBreakUnit"].as_u64().unwrap_or(0) as usize;
                    if combo(ui, "korean-break", &mut k, &["어절", "글자"], 112.0) {
                        d.values["koreanBreakUnit"] = json!(k);
                    }
                });
                ui.horizontal(|ui| {
                    lbl(ui, "영어 단위", 47.0);
                    let mut e = d.values["englishBreakUnit"].as_u64().unwrap_or(0) as usize;
                    if combo(
                        ui,
                        "english-break",
                        &mut e,
                        &["단어", "하이픈", "글자"],
                        112.0,
                    ) {
                        d.values["englishBreakUnit"] = json!(e);
                    }
                });
            });
            ui.horizontal(|ui| {
                lbl(ui, "최소 공백", 47.0);
                ui.add_enabled_ui(false, |ui| {
                    let mut min = 100.0;
                    spin(ui, &mut min, 25.0..=100.0, 0, " %", 91.0);
                })
                .response
                .on_disabled_hover_text("최소 공백은 아직 지원하지 않습니다.");
            });
        });
    });
}
fn paragraph_extended(ui: &mut egui::Ui, d: &mut FormatDraft) {
    ui.add_enabled_ui(!d.for_style, |ui| {
        group(ui, "문단 종류", |ui| {
            let kinds = ["None", "Outline", "Number", "Bullet"];
            let mut kind = kinds
                .iter()
                .position(|k| d.values["headType"] == *k)
                .unwrap_or(0);
            let mut level = d.values["paraLevel"].as_u64().unwrap_or(0) as usize;
            let levels: Vec<String> = (1..=7).map(|l| format!("{l} 수준")).collect();
            let levels: Vec<&str> = levels.iter().map(String::as_str).collect();
            for (i, label) in ["없음", "개요 문단", "번호 문단", "글머리표 문단"]
                .iter()
                .enumerate()
            {
                ui.horizontal(|ui| {
                    ui.set_min_width(200.0);
                    if ui.radio_value(&mut kind, i, *label).changed() {
                        d.values["headType"] = json!(kinds[kind]);
                    }
                    if (i == 1 || i == 2) && kind == i {
                        ui.add_space(10.0);
                        if combo(ui, &format!("level-{i}"), &mut level, &levels, 79.0) {
                            d.values["paraLevel"] = json!(level);
                        }
                    }
                });
            }
        });
    });
    group(ui, "기타", |ui| {
        for (key, label) in [
            ("widowOrphan", "외톨이줄 보호"),
            ("keepWithNext", "다음 문단과 함께"),
            ("keepLines", "문단 보호"),
            ("pageBreakBefore", "문단 앞에서 항상 쪽 나눔"),
            ("fontLineHeight", "글꼴에 어울리는 줄 높이"),
            ("singleLine", "한 줄로 입력"),
            ("autoSpaceKrEn", "한글과 영어 간격을 자동 조절"),
            ("autoSpaceKrNum", "한글과 숫자 간격을 자동 조절"),
        ] {
            check(ui, &mut d.values, key, label);
        }
        ui.horizontal(|ui| {
            lbl(ui, "세로 정렬", 47.0);
            let mut v = d.values["verticalAlign"].as_u64().unwrap_or(0) as usize;
            if combo(
                ui,
                "vertical-align",
                &mut v,
                &["글꼴 기준", "위쪽", "가운데", "아래쪽"],
                79.0,
            ) {
                d.values["verticalAlign"] = json!(v);
            }
        });
    });
}
fn tab_kind_name(kind: u64) -> &'static str {
    ["왼쪽", "오른쪽", "가운데", "소수점"]
        .get(kind as usize)
        .copied()
        .unwrap_or("왼쪽")
}
fn paragraph_tabs(ui: &mut egui::Ui, d: &mut FormatDraft) {
    let mut stops: Vec<Value> = d.values["tabStops"].as_array().cloned().unwrap_or_default();
    let mut changed = false;
    group(ui, "탭 종류", |ui| {
        ui.horizontal(|ui| {
            for (i, label) in ["왼쪽", "오른쪽", "가운데", "소수점"].iter().enumerate() {
                ui.radio_value(&mut d.tab_kind, i as u8, *label);
            }
        });
        ui.horizontal(|ui| {
            lbl(ui, "채울 모양", 47.0);
            line_combo(ui, "tab-fill", &mut d.tab_fill, &LINE_TYPES[..12], 0, 79.0);
        });
        ui.horizontal(|ui| {
            lbl(ui, "탭 위치", 47.0);
            spin(ui, &mut d.tab_position, 0.0..=5000.0, 1, " pt", 91.0);
            if ui.button("추가").clicked() {
                let position = (d.tab_position * 200.0).round() as i64;
                stops.retain(|s| s["position"].as_i64() != Some(position));
                stops.push(json!({"position": position, "type": d.tab_kind, "fill": d.tab_fill}));
                stops.sort_by_key(|s| s["position"].as_i64().unwrap_or(0));
                changed = true;
            }
        });
    });
    group(ui, "탭 목록", |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label("탭 목록");
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                    .show(ui, |ui| {
                        ui.set_min_size(Vec2::new(184.0, 205.0));
                        egui::Grid::new("tab-list")
                            .num_columns(2)
                            .min_col_width(80.0)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(RichText::new("위치").weak());
                                ui.label(RichText::new("종류").weak());
                                ui.end_row();
                                for (i, s) in stops.iter().enumerate() {
                                    let pt = s["position"].as_f64().unwrap_or(0.0) / 200.0;
                                    if ui
                                        .selectable_label(
                                            d.tab_selected == Some(i),
                                            format!("{pt:.1} pt"),
                                        )
                                        .clicked()
                                    {
                                        d.tab_selected = Some(i);
                                        d.tab_position = pt;
                                        d.tab_kind = s["type"].as_u64().unwrap_or(0) as u8;
                                        d.tab_fill = s["fill"].as_u64().unwrap_or(0) as u8;
                                    }
                                    ui.label(tab_kind_name(s["type"].as_u64().unwrap_or(0)));
                                    ui.end_row();
                                }
                            });
                    });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(d.tab_selected.is_some(), egui::Button::new("지우기"))
                        .clicked()
                        && let Some(i) = d.tab_selected.take()
                        && i < stops.len()
                    {
                        d.deleted_tabs.push(stops.remove(i));
                        changed = true;
                    }
                    if ui
                        .add_enabled(!stops.is_empty(), egui::Button::new("모두 지우기"))
                        .clicked()
                    {
                        d.deleted_tabs.append(&mut stops);
                        d.tab_selected = None;
                        changed = true;
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label("지운 탭 목록");
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                    .show(ui, |ui| {
                        ui.set_min_size(Vec2::new(184.0, 205.0));
                        for s in &d.deleted_tabs {
                            let pt = s["position"].as_f64().unwrap_or(0.0) / 200.0;
                            ui.label(format!(
                                "{pt:.1} pt  {}",
                                tab_kind_name(s["type"].as_u64().unwrap_or(0))
                            ));
                        }
                    });
            });
        });
    });
    if changed {
        d.values["tabStops"] = json!(stops);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            group(ui, "자동 탭", |ui| {
                check(ui, &mut d.values, "tabAutoLeft", "내어쓰기용 자동 탭");
                check(ui, &mut d.values, "tabAutoRight", "문단 오른쪽 끝 자동 탭");
            });
        });
        ui.vertical(|ui| {
            group(ui, "기본 탭", |ui| {
                ui.horizontal(|ui| {
                    lbl(ui, "구역 기본 탭 간격", 86.0);
                    // Stored at 200 units per pt like tab positions (Hangul shows 8000 as 40 pt).
                    let mut pt = d.values["defaultTabSpacing"].as_f64().unwrap_or(8000.0) / 200.0;
                    ui.add_enabled_ui(false, |ui| spin(ui, &mut pt, 0.0..=1000.0, 1, " pt", 91.0));
                });
                ui.add_enabled(false, egui::Button::new("변경..."))
                    .on_disabled_hover_text("구역 기본 탭 간격 변경은 아직 지원하지 않습니다.");
            });
        });
    });
}
fn border_spacing(ui: &mut egui::Ui, v: &mut Value) {
    group(ui, "간격", |ui| {
        let mut values: Vec<f64> = (0..4)
            .map(|i| v["borderSpacing"][i].as_f64().unwrap_or(0.0) / MM)
            .collect();
        let mut changed = false;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                for (i, label) in [(0, "왼쪽"), (1, "오른쪽")] {
                    ui.horizontal(|ui| {
                        lbl(ui, label, 33.0);
                        changed |= spin(ui, &mut values[i], 0.0..=100.0, 2, " mm", 91.0);
                    });
                }
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                for (i, label) in [(2, "위쪽"), (3, "아래쪽")] {
                    ui.horizontal(|ui| {
                        lbl(ui, label, 33.0);
                        changed |= spin(ui, &mut values[i], 0.0..=100.0, 2, " mm", 91.0);
                    });
                }
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    lbl(ui, "모두", 24.0);
                    let mut all = values[0];
                    if spin(ui, &mut all, 0.0..=100.0, 2, " mm", 64.0) {
                        values.fill(all);
                        changed = true;
                    }
                });
            });
        });
        if changed {
            v["borderSpacing"] = json!(
                values
                    .iter()
                    .map(|x| (x * MM).round() as i64)
                    .collect::<Vec<_>>()
            );
        }
        check(ui, v, "borderIgnoreMargin", "문단 여백 무시");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_dialog_and_single_field_preserve_mixed_fonts() -> anyhow::Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("AB".into()))?;
        s.cursor.offset = 1;
        s.anchor = Some(0);
        s.run("format.character", json!({"fontFamily":"Arial"}))?;
        s.cursor.offset = 2;
        s.anchor = Some(1);
        s.run("format.character", json!({"fontFamily":"Courier New"}))?;
        s.cursor.offset = 2;
        s.anchor = Some(0);
        let mut draft = FormatDraft::new(s.character_properties()?);
        assert_eq!(draft.patch(), json!({}));
        draft.values["bold"] = json!(true);
        s.run("format.character", draft.patch())?;
        s.select(Cursor::default(), false)?;
        assert_eq!(s.character_properties()?["fontFamily"], "Arial");
        assert_eq!(s.character_properties()?["bold"], true);
        s.cursor.offset = 2;
        s.anchor = Some(1);
        assert_eq!(s.character_properties()?["fontFamily"], "Courier New");
        assert_eq!(s.character_properties()?["bold"], true);
        Ok(())
    }
    #[test]
    fn query_values_filter_to_writable_keys() -> anyhow::Result<()> {
        let s = Session::new()?;
        for character in [true, false] {
            let v = if character {
                s.character_properties()?
            } else {
                s.paragraph_format()?
            };
            let w = writable(&v, character);
            hwpcraft_engine::validate_format(&w, character)?;
            assert!(w.as_object().is_some_and(|o| o.len() > 10));
        }
        Ok(())
    }
    #[test]
    fn paragraph_head_patch_is_split_for_one_undo_step() {
        let values = json!({"alignment":"center","headType":"Number","paraLevel":2});
        let patch = values.clone();
        assert_eq!(
            head_params(&patch, &values),
            Some(json!({"kind":"number","level":2}))
        );
        assert_eq!(without_head(patch), json!({"alignment":"center"}));
        assert_eq!(head_params(&json!({"indent": 10}), &values), None);
        // A level-only change keeps the paragraph's kind.
        assert_eq!(
            head_params(
                &json!({"paraLevel": 3}),
                &json!({"headType":"Outline","paraLevel":3})
            ),
            Some(json!({"kind":"outline","level":3}))
        );
    }
}
