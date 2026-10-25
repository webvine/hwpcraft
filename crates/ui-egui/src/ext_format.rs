//! Owner: format area. 서식 대화상자·도구.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Dialogs follow the Hangul-style style / numbering windows.
use super::format_dialogs::{
    FormatDraft, LANGUAGES, combo, group, lbl, paint_align, preview_image, rgb, spin,
};
use super::*;
use egui::{Key, Modifiers};
use serde_json::Value;

/// Hangul 2024 문단 번호 모양 presets (label, level formats, number format codes).
const NUMBERINGS: [(&str, [&str; 7], [u8; 7]); 10] = [
    (
        "1. 가. 1) 가) (1) (가) ①",
        ["^1.", "^2.", "^3)", "^4)", "(^5)", "(^6)", "^7"],
        [0, 8, 0, 8, 0, 8, 1],
    ),
    (
        "(1) (가) (a) 1) 가) a) ①",
        ["(^1)", "(^2)", "(^3)", "^4)", "^5)", "^6)", "^7"],
        [0, 8, 5, 0, 8, 5, 1],
    ),
    (
        "1) 가) a) (1) (가) (a) ①",
        ["^1)", "^2)", "^3)", "(^4)", "(^5)", "(^6)", "^7"],
        [0, 8, 5, 0, 8, 5, 1],
    ),
    (
        "① (ㄱ) (a) 1) ㄱ) a)",
        ["^1", "(^2)", "(^3)", "^4)", "^5)", "^6)", ""],
        [1, 10, 5, 0, 10, 5, 0],
    ),
    (
        "가) a) (1) (가) (a) ①",
        ["^1)", "^2)", "(^3)", "(^4)", "(^5)", "^6", ""],
        [8, 5, 0, 8, 5, 1, 0],
    ),
    (
        "(ㄱ) (1) (a) 1) a) ①",
        ["(^1)", "(^2)", "(^3)", "^4)", "^5)", "^6", ""],
        [10, 0, 5, 0, 5, 1, 0],
    ),
    (
        "I. A. 1. i) a) 1) ①",
        ["^1.", "^2.", "^3.", "^4)", "^5)", "^6)", "^7"],
        [2, 4, 0, 3, 5, 0, 1],
    ),
    (
        "i a. (i) (a) i) a)",
        ["^1", "^2.", "(^3)", "(^4)", "^5)", "^6)", ""],
        [3, 5, 3, 5, 3, 5, 0],
    ),
    (
        "A. 1. 가 (a) (1) (가) ⓐ",
        ["^1.", "^2.", "^3", "(^4)", "(^5)", "(^6)", "^7"],
        [4, 0, 8, 5, 0, 8, 7],
    ),
    (
        "1. 1.1. 1.1.1. 1.1.1.1.",
        [
            "^1.",
            "^1.^2.",
            "^1.^2.^3.",
            "^1.^2.^3.^4.",
            "^1.^2.^3.^4.^5.",
            "^1.^2.^3.^4.^5.^6.",
            "^1.^2.^3.^4.^5.^6.^7.",
        ],
        [0; 7],
    ),
];
/// Bullet characters in Hangul's 글머리표 모양 order (Malgun Gothic covers all of them).
const BULLETS: [char; 16] = [
    '●', '•', '■', '▪', '◆', '◇', '▶', '▷', '○', '□', '★', '☆', '◎', '※', '√', '→',
];

/// First value of HWP number format `code` (표 43) for previews.
fn number_sample(code: u8, n: u32) -> String {
    let pick = |set: &[&str]| set[(n as usize - 1) % set.len()].to_owned();
    match code {
        1 => pick(&["①", "②", "③"]),
        2 => pick(&["I", "II", "III"]),
        3 => pick(&["i", "ii", "iii"]),
        4 => pick(&["A", "B", "C"]),
        5 => pick(&["a", "b", "c"]),
        6 => pick(&["Ⓐ", "Ⓑ", "Ⓒ"]),
        7 => pick(&["ⓐ", "ⓑ", "ⓒ"]),
        8 => pick(&["가", "나", "다"]),
        9 => pick(&["㉮", "㉯", "㉰"]),
        10 => pick(&["ㄱ", "ㄴ", "ㄷ"]),
        11 => pick(&["㉠", "㉡", "㉢"]),
        12 => pick(&["일", "이", "삼"]),
        13 => pick(&["一", "二", "三"]),
        _ => n.to_string(),
    }
}
fn number_text(formats: &[&str; 7], codes: &[u8; 7], level: usize) -> String {
    let mut out = formats[level].to_owned();
    for k in (1..=7).rev() {
        out = out.replace(&format!("^{k}"), &number_sample(codes[k - 1], 1));
    }
    out
}

struct StyleEdit {
    id: Option<usize>,
    name: String,
    english: String,
    character: bool,
    next: usize,
    char_patch: Value,
    para_patch: Value,
}
struct StyleDialog {
    selected: usize,
    edit: Option<StyleEdit>,
    delete: Option<usize>,
    /// The 글자/문단 모양 window is editing this style's shape (`true` = character).
    shape: Option<bool>,
    preview: Option<(String, egui::TextureHandle, Vec2)>,
}
struct NumberingDialog {
    tab: usize,
    bullet: usize,
    numbering: usize,
    restart: usize,
    start: f64,
    previous: usize,
}

/// Dialog and tool state owned by this module. Stored in `HwpApp::ext.format`.
#[derive(Default)]
pub(crate) struct State {
    /// Tab to open the next 글자/문단 모양 window on.
    pub(super) initial_tab: Option<usize>,
    style: Option<StyleDialog>,
    numbering: Option<NumberingDialog>,
    /// Shape edited for the style window, applied on the next frame.
    style_update: Option<(bool, Value)>,
    font_language: usize,
    text_color: Option<String>,
    shade_color: Option<String>,
    highlight: Option<String>,
    copied: Option<(Value, Value)>,
    overlap: Option<OverlapDialog>,
}
/// 글자 겹치기 window: `control` is set when editing the control under the caret.
struct OverlapDialog {
    chars: String,
    border: usize,
    size: f64,
    overlap: bool,
    control: Option<usize>,
}
/// Hangul's 글자 겹치기 테두리 모양 list, in HWP border-type order (0–6).
const OVERLAP_BORDERS: [&str; 7] = [
    "글자끼리 겹치기",
    "원",
    "반전 원",
    "사각형",
    "반전 사각형",
    "삼각형",
    "반전 삼각형",
];
/// One row of Hangul's preset 글자 색 / 음영 / 형광펜 swatches (the engine's measured
/// `COLOR_PRESETS`); returns the clicked preset's `#rrggbb`.
fn preset_row(ui: &mut egui::Ui) -> Option<String> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (_, label, hex) in hwpcraft_engine::COLOR_PRESETS {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
            ui.painter().rect(
                r,
                1.0,
                rgb(hex),
                Stroke::new(1.0, Color32::from_gray(150)),
                egui::StrokeKind::Inside,
            );
            if resp.on_hover_text(label).clicked() {
                out = Some(hex.to_owned());
            }
        }
    });
    out
}
impl State {
    /// Called by the 글자/문단 모양 window in style mode.
    pub(super) fn style_shape_applied(&mut self, character: bool, patch: Value) {
        let Some(dialog) = self.style.as_mut() else {
            return;
        };
        dialog.shape = None;
        if let Some(edit) = dialog.edit.as_mut() {
            let target = if character {
                &mut edit.char_patch
            } else {
                &mut edit.para_patch
            };
            if let (Some(t), Value::Object(p)) = (target.as_object_mut(), patch) {
                t.extend(p);
            }
        } else if patch.as_object().is_some_and(|p| !p.is_empty()) {
            self.style_update = Some((character, patch));
        }
    }
    pub(super) fn style_shape_closed(&mut self) {
        if let Some(d) = self.style.as_mut() {
            d.shape = None;
        }
    }
}

const ENGINE_IDS: &[&str] = &[
    "format.bold",
    "format.italic",
    "format.underline",
    "format.strikethrough",
    "format.superscript",
    "format.subscript",
    "format.normal",
    "format.font_size.increase",
    "format.font_size.decrease",
    "format.ratio.increase",
    "format.ratio.decrease",
    "format.spacing.increase",
    "format.spacing.decrease",
    "format.align.justify",
    "format.align.left",
    "format.align.center",
    "format.align.right",
    "format.align.distribute",
    "format.align.split",
    "format.line_spacing.increase",
    "format.line_spacing.decrease",
    "format.indent.positive",
    "format.indent.negative",
    "format.margin_left.increase",
    "format.margin_left.decrease",
    "format.margin_right.increase",
    "format.margin_right.decrease",
    "format.margin.increase",
    "format.margin.decrease",
    "format.numbering.toggle",
    "format.bullet.toggle",
    "format.outline.toggle",
    "format.level.increase",
    "format.level.decrease",
    "format.superscript.cycle",
    "format.indent.at_caret",
    "format.paragraph.up",
    "format.paragraph.down",
    "format.style.clear_char",
    "format.style.update_from_caret",
];
/// Hangul commands shown on the ribbon that HwpCraft cannot perform yet.
const UNSUPPORTED: &[&str] = &[
    "format.style.gallery",
    "format.drop_cap",
    "format.picture_bullet",
];
const UI_IDS: &[&str] = &[
    "format.character_dialog",
    "format.paragraph_dialog",
    "format.style.dialog",
    "format.style.create",
    "format.copy_shape",
    "format.highlight",
    "format.text_color",
    "format.shade_color",
    "format.font.next",
    "format.font.prev",
    "format.char_overlap.dialog",
    "format.char_border",
    "format.paragraph_border",
    "format.tabs",
    "format.numbering.dialog",
    "format.outline.dialog",
    "format.numbering.restart",
    "format.style",
    "format.font_lang",
    "format.font_face",
    "format.font_size",
    "format.line_spacing",
    "format.align",
];

impl HwpApp {
    /// `Some(enabled)` when this module owns the ribbon/menu command id.
    pub(crate) fn format_ext_enabled(&self, id: &str) -> Option<bool> {
        if UNSUPPORTED.contains(&id) {
            return Some(false);
        }
        if id == "format.copy_shape" {
            return Some(true);
        }
        (ENGINE_IDS.contains(&id) || UI_IDS.contains(&id)).then_some(true)
    }
    /// Return true when the command id was handled.
    pub(crate) fn format_ext_dispatch(&mut self, id: &str, ctx: &egui::Context) -> bool {
        let _ = ctx;
        if ENGINE_IDS.contains(&id) {
            self.run(id, json!({}));
            return true;
        }
        match id {
            "format.character_dialog" => self.open_shape(true, 0),
            "format.paragraph_dialog" => self.open_shape(false, 0),
            "format.char_border" => self.open_shape(true, 2),
            "format.tabs" => self.open_shape(false, 2),
            "format.paragraph_border" => self.open_shape(false, 3),
            "format.style.dialog" | "format.style" => self.open_style(false),
            "format.style.create" => self.open_style(true),
            "format.numbering.dialog" => self.open_numbering(1),
            "format.outline.dialog" => self.open_outline(),
            "format.numbering.restart" => {
                self.run("format.numbering.restart", json!({"mode": "new"}));
            }
            "format.highlight" => {
                let c = self
                    .ext
                    .format
                    .highlight
                    .clone()
                    .unwrap_or("#ffff00".into());
                self.run("format.highlight", json!({ "color": c }));
            }
            "format.text_color" => {
                let c = self
                    .ext
                    .format
                    .text_color
                    .clone()
                    .unwrap_or("#ff0000".into());
                self.run("format.text_color", json!({ "color": c }));
            }
            "format.shade_color" => {
                let c = self
                    .ext
                    .format
                    .shade_color
                    .clone()
                    .unwrap_or("#ffff00".into());
                self.run("format.shade_color", json!({ "color": c }));
            }
            "format.font.next" | "format.font.prev" => {
                format_dialogs::load_fonts(&self.renderer);
                let fonts = format_dialogs::font_names();
                self.run(id, json!({ "fonts": fonts }));
            }
            "format.char_overlap.dialog" => self.open_overlap(),
            "format.copy_shape" => self.copy_shape(),
            _ => return false,
        }
        true
    }
    fn open_shape(&mut self, character: bool, tab: usize) {
        self.format_draft = None;
        self.ext.format.initial_tab = Some(tab);
        self.show_character = character;
        self.show_paragraph = !character;
    }
    fn open_style(&mut self, add: bool) {
        let selected = self
            .session
            .paragraph(self.session.cursor)
            .map(|p| p.style_id as usize)
            .unwrap_or(0);
        let mut d = StyleDialog {
            selected,
            edit: None,
            delete: None,
            shape: None,
            preview: None,
        };
        if add {
            d.edit = Some(self.new_style_edit(None));
        }
        self.ext.format.style = Some(d);
    }
    fn new_style_edit(&mut self, id: Option<usize>) -> StyleEdit {
        let detail = id.and_then(|id| {
            self.session
                .run("format.style.detail", json!({ "id": id }))
                .ok()
        });
        let count = self.session.core.document().doc_info.styles.len();
        StyleEdit {
            id,
            name: detail
                .as_ref()
                .and_then(|d| d["name"].as_str())
                .unwrap_or("새 스타일")
                .to_owned(),
            english: detail
                .as_ref()
                .and_then(|d| d["englishName"].as_str())
                .unwrap_or("")
                .to_owned(),
            character: detail.as_ref().is_some_and(|d| d["type"] == "character"),
            next: detail
                .as_ref()
                .and_then(|d| d["nextStyleId"].as_u64())
                .map(|n| n as usize)
                .unwrap_or(count),
            char_patch: json!({}),
            para_patch: json!({}),
        }
    }
    fn open_numbering(&mut self, tab: usize) {
        let list = self
            .session
            .run("format.numbering.list", json!({}))
            .unwrap_or_default();
        let previous = list["previous"].as_array().map(Vec::len).unwrap_or(0);
        self.ext.format.numbering = Some(NumberingDialog {
            tab,
            bullet: 1,
            numbering: 1,
            restart: if previous > 0 { 0 } else { 2 },
            start: 1.0,
            previous,
        });
    }
    /// 개요 번호 모양 (Ctrl+K,O): tab 3 of the numbering window, preselecting the section's shape.
    fn open_outline(&mut self) {
        let list = self
            .session
            .run("format.numbering.list", json!({}))
            .unwrap_or_default();
        let current = list["outlineId"]
            .as_u64()
            .and_then(|id| list["numberings"].get((id as usize).checked_sub(1)?));
        let (numbering, start) = match current {
            Some(n) => (
                NUMBERINGS
                    .iter()
                    .position(|(_, f, c)| {
                        n["levelFormats"] == json!(f) && n["numberFormats"] == json!(c)
                    })
                    .map_or(0, |i| i + 1),
                n["startNumber"].as_f64().unwrap_or(1.0),
            ),
            None => (1, 1.0),
        };
        self.ext.format.numbering = Some(NumberingDialog {
            tab: 3,
            bullet: 1,
            numbering,
            restart: 2,
            start,
            previous: 0,
        });
    }
    /// 글자 겹치기 window: edits the control at the caret when there is one, else inserts.
    fn open_overlap(&mut self) {
        let list = self
            .session
            .run("format.char_overlap.get", json!({}))
            .unwrap_or_default();
        let offset = self.session.cursor.offset as u64;
        let at_caret = list
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["position"].as_u64() == Some(offset))
            .cloned();
        self.ext.format.overlap = Some(match at_caret {
            Some(c) => OverlapDialog {
                chars: c["chars"].as_str().unwrap_or("").to_owned(),
                border: c["border"].as_u64().unwrap_or(0) as usize,
                size: c["size"].as_f64().unwrap_or(100.0),
                overlap: c["expansion"].as_u64() != Some(0),
                control: c["control"].as_u64().map(|x| x as usize),
            },
            None => OverlapDialog {
                chars: String::new(),
                border: 0,
                size: 100.0,
                overlap: true,
                control: None,
            },
        });
    }
    fn overlap_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.overlap.take() else {
            return;
        };
        let (mut close, mut apply, mut delete) = (false, false, false);
        let editing = d.control.is_some();
        let r = chrome::modal("format-char-overlap").show(ctx, |ui| {
            ui.set_width(380.0);
            close = chrome::title(ui, "글자 겹치기");
            group(ui, "겹쳐 쓸 글자 (최대 9자)", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut d.chars)
                        .desired_width(330.0)
                        .char_limit(9),
                );
            });
            group(ui, "테두리 모양", |ui| {
                combo(ui, "overlap-border", &mut d.border, &OVERLAP_BORDERS, 200.0);
                ui.checkbox(&mut d.overlap, "글자끼리 겹침 (끄면 펼쳐 씀)");
            });
            group(ui, "글자 크기 조절", |ui| {
                ui.horizontal(|ui| {
                    lbl(ui, "크기", 60.0);
                    spin(ui, &mut d.size, 1.0..=127.0, 0, "%", 90.0);
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = !d.chars.trim().is_empty();
                if ui
                    .add_enabled(
                        ok,
                        egui::Button::new(if editing { "고치기(D)" } else { "넣기(D)" })
                            .min_size(Vec2::new(80.0, 26.0)),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if editing
                    && ui
                        .add(egui::Button::new("지우기").min_size(Vec2::new(80.0, 26.0)))
                        .clicked()
                {
                    delete = true;
                }
                if ui
                    .add(egui::Button::new("취소").min_size(Vec2::new(80.0, 26.0)))
                    .clicked()
                {
                    close = true;
                }
            });
        });
        if apply {
            let mut params = json!({
                "chars": d.chars.trim(),
                "border": d.border,
                "size": d.size.round() as i64,
                "expansion": u8::from(d.overlap),
            });
            let id = match d.control {
                Some(c) => {
                    params["control"] = json!(c);
                    "format.char_overlap.update"
                }
                None => "format.char_overlap",
            };
            if self.run(id, params) {
                return;
            }
        } else if delete {
            if let Some(c) = d.control
                && self.run("format.char_overlap.delete", json!({ "control": c }))
            {
                return;
            }
        } else if close || r.should_close() {
            return;
        }
        self.ext.format.overlap = Some(d);
    }
    /// 모양 복사 (Alt+C): without a block it copies the caret's 글자·문단 모양; with a
    /// block it applies both in one undo step (`format.copy_shape.apply`).
    fn copy_shape(&mut self) {
        let has_selection = self.session.has_selection();
        if has_selection && let Some((c, p)) = self.ext.format.copied.clone() {
            let ok = self.run(
                "format.copy_shape.apply",
                json!({
                    "character": format_dialogs::writable(&c, true),
                    "paragraph": format_dialogs::writable(&p, false),
                }),
            );
            if ok {
                self.message = "복사한 글자·문단 모양을 적용했습니다.".into();
            }
            return;
        }
        if let (Ok(c), Ok(p)) = (
            self.session.character_properties(),
            self.session.paragraph_format(),
        ) {
            self.ext.format.copied = Some((c, p));
            self.message =
                "글자·문단 모양을 복사했습니다. 블록을 지정한 뒤 다시 누르면 적용합니다.".into();
        }
    }

    /// Hangul 2024 formatting shortcuts. True when one was handled.
    pub(crate) fn format_ext_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        // Shortcuts belong to the document; text fields keep their own keys.
        let focus = ctx.memory(|m| m.focused());
        if focus.is_some_and(|f| f != egui::Id::new("document-canvas")) {
            return false;
        }
        let ctrl = Modifiers::CTRL;
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
        // More specific modifier sets first: egui matches extra Shift/Alt logically.
        let table: &[(Modifiers, Key, &str)] = &[
            (ctrl_alt, Key::F5, "format.margin_left.decrease"),
            (ctrl_alt, Key::F6, "format.margin_left.increase"),
            (ctrl_alt, Key::F7, "format.margin_right.increase"),
            (ctrl_alt, Key::F8, "format.margin_right.decrease"),
            (ctrl_shift, Key::M, "format.align.justify"),
            (ctrl_shift, Key::L, "format.align.left"),
            (ctrl_shift, Key::R, "format.align.right"),
            (ctrl_shift, Key::C, "format.align.center"),
            (ctrl_shift, Key::T, "format.align.distribute"),
            (ctrl_shift, Key::E, "format.margin_left.decrease"),
            (ctrl_shift, Key::G, "format.margin_left.increase"),
            (ctrl_shift, Key::D, "format.margin_right.increase"),
            (ctrl_shift, Key::F, "format.margin_right.decrease"),
            (ctrl_shift, Key::O, "format.indent.negative"),
            (ctrl_shift, Key::I, "format.indent.positive"),
            (ctrl_shift, Key::Q, "format.line_spacing.decrease"),
            (ctrl_shift, Key::W, "format.line_spacing.increase"),
            (ctrl_shift, Key::Insert, "format.numbering.toggle"),
            (ctrl_shift, Key::Delete, "format.bullet.toggle"),
            (alt_shift, Key::Insert, "restart"),
            (alt_shift, Key::E, "format.font_size.increase"),
            (alt_shift, Key::R, "format.font_size.decrease"),
            (alt_shift, Key::K, "format.ratio.increase"),
            (alt_shift, Key::J, "format.ratio.decrease"),
            (alt_shift, Key::W, "format.spacing.increase"),
            (alt_shift, Key::N, "format.spacing.decrease"),
            (alt_shift, Key::B, "format.bold"),
            (alt_shift, Key::I, "format.italic"),
            (alt_shift, Key::U, "format.underline"),
            (alt_shift, Key::P, "format.superscript"),
            (alt_shift, Key::S, "format.subscript"),
            (alt_shift, Key::C, "format.normal"),
            (alt_shift, Key::Z, "format.line_spacing.increase"),
            (alt_shift, Key::A, "format.line_spacing.decrease"),
            (ctrl, Key::F5, "format.indent.negative"),
            (ctrl, Key::F6, "format.indent.positive"),
            (ctrl, Key::F7, "format.margin.increase"),
            (ctrl, Key::F8, "format.margin.decrease"),
            (ctrl, Key::Insert, "format.outline.toggle"),
            (ctrl, Key::Minus, "format.level.increase"),
            (ctrl, Key::Plus, "format.level.decrease"),
            (ctrl, Key::CloseBracket, "format.font_size.increase"),
            (ctrl, Key::OpenBracket, "format.font_size.decrease"),
            (Modifiers::ALT, Key::L, "format.character_dialog"),
            (Modifiers::ALT, Key::T, "format.paragraph_dialog"),
            (Modifiers::ALT, Key::C, "format.copy_shape"),
            (Modifiers::NONE, Key::F6, "format.style.dialog"),
        ];
        for (modifiers, key, id) in table {
            if !ctx.input_mut(|i| i.consume_key(*modifiers, *key)) {
                continue;
            }
            if modifiers.alt {
                // Alt chords must not also type a letter into the document.
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
            }
            match *id {
                "restart" => {
                    self.run("format.numbering.restart", json!({"mode": "new"}));
                }
                id => {
                    self.format_ext_dispatch(id, ctx);
                }
            }
            return true;
        }
        let digits = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
            Key::Num0,
        ];
        for (i, key) in digits.iter().enumerate() {
            if ctx.input_mut(|input| input.consume_key(ctrl, *key)) {
                self.run("format.style.shortcut", json!({"number": (i + 1) % 10}));
                return true;
            }
        }
        false
    }
    /// `--view` names: format-style, numbering, bullet, tabs, paragraph-border, character-border.
    pub(crate) fn format_ext_view(&mut self, name: &str) -> bool {
        match name {
            "format-style" => self.open_style(false),
            "format-style-add" => self.open_style(true),
            "numbering" => self.open_numbering(1),
            "bullet" => self.open_numbering(0),
            "outline" => self.open_outline(),
            "tabs" => self.open_shape(false, 2),
            "paragraph-ext" => self.open_shape(false, 1),
            "paragraph-border" => self.open_shape(false, 3),
            "character-ext" => self.open_shape(true, 1),
            "character-border" => self.open_shape(true, 2),
            "char-overlap" => self.open_overlap(),
            _ => return false,
        }
        true
    }
    /// A formatting dialog owned by this module is open.
    pub(crate) fn format_ext_modal_open(&self) -> bool {
        self.ext.format.style.is_some()
            || self.ext.format.numbering.is_some()
            || self.ext.format.overlap.is_some()
    }
    /// Called every frame after the built-in dialogs.
    pub(crate) fn format_ext_dialogs(&mut self, ctx: &egui::Context) {
        if let Some((character, patch)) = self.ext.format.style_update.take()
            && let Some(id) = self.ext.format.style.as_ref().map(|d| d.selected)
        {
            let key = if character { "character" } else { "paragraph" };
            self.run("format.style.update", json!({"id": id, key: patch}));
            if let Some(d) = self.ext.format.style.as_mut() {
                d.preview = None;
            }
        }
        self.style_dialog(ctx);
        self.numbering_dialog(ctx);
        self.overlap_dialog(ctx);
    }

    // ---- 스타일 (F6) ----

    fn style_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.style.take() else {
            return;
        };
        // While its 글자/문단 모양 window is open the style window steps aside.
        if d.shape.is_some() {
            self.ext.format.style = Some(d);
            return;
        }
        if let Some(edit) = d.edit.take() {
            let keep = self.style_edit_dialog(ctx, &mut d, edit);
            self.ext.format.style = Some(d);
            let _ = keep;
            return;
        }
        if let Some(id) = d.delete.take() {
            self.style_delete_dialog(ctx, &mut d, id);
            self.ext.format.style = Some(d);
            return;
        }
        let styles = self
            .session
            .run("format.style.list", json!({}))
            .unwrap_or_default();
        let styles = styles.as_array().cloned().unwrap_or_default();
        d.selected = d.selected.min(styles.len().saturating_sub(1));
        let detail = self
            .session
            .run("format.style.detail", json!({"id": d.selected}))
            .unwrap_or_default();
        self.style_preview(ctx, &mut d, &detail);
        let current = self
            .session
            .paragraph(self.session.cursor)
            .map(|p| p.style_id as usize)
            .unwrap_or(0);
        let mut close = false;
        let mut apply = None;
        let r = chrome::modal("format-style").show(ctx, |ui| {
            ui.set_width(763.0);
            close = chrome::title(ui, "스타일");
            ui.horizontal_top(|ui| {
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, Color32::from_gray(227)))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(640.0);
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(314.0);
                                ui.label("스타일 목록");
                                egui::Frame::new()
                                    .stroke(Stroke::new(1.0, Color32::from_gray(200)))
                                    .show(ui, |ui| {
                                        ui.set_min_size(Vec2::new(312.0, 440.0));
                                        ui.horizontal(|ui| {
                                            ui.add_sized(
                                                [192.0, 22.0],
                                                egui::Label::new(RichText::new("이름").weak()),
                                            );
                                            ui.add_sized(
                                                [90.0, 22.0],
                                                egui::Label::new(RichText::new("단축키").weak()),
                                            );
                                        });
                                        egui::ScrollArea::vertical().max_height(410.0).show(
                                            ui,
                                            |ui| {
                                                for (i, s) in styles.iter().enumerate() {
                                                    let character = s["type"] == "character";
                                                    let name = s["name"].as_str().unwrap_or("");
                                                    let shortcut =
                                                        s["shortcut"].as_str().unwrap_or("-");
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        Vec2::new(296.0, 28.0),
                                                        egui::Sense::click(),
                                                    );
                                                    let p = ui.painter();
                                                    if d.selected == i {
                                                        p.rect(
                                                            rect,
                                                            0.0,
                                                            Color32::from_rgb(222, 233, 255),
                                                            Stroke::new(
                                                                1.0,
                                                                chrome::Tokens::ACCENT,
                                                            ),
                                                            egui::StrokeKind::Inside,
                                                        );
                                                    } else if resp.hovered() {
                                                        p.rect_filled(
                                                            rect,
                                                            0.0,
                                                            Color32::from_gray(245),
                                                        );
                                                    }
                                                    let icon = Rect::from_min_size(
                                                        rect.min + Vec2::new(8.0, 7.0),
                                                        Vec2::splat(14.0),
                                                    );
                                                    if character {
                                                        p.text(
                                                            icon.center(),
                                                            egui::Align2::CENTER_CENTER,
                                                            "가",
                                                            egui::FontId::proportional(12.0),
                                                            Color32::from_rgb(46, 105, 220),
                                                        );
                                                    } else {
                                                        paint_align(p, icon, "justify");
                                                    }
                                                    if i == current {
                                                        let red = Stroke::new(
                                                            1.8,
                                                            Color32::from_rgb(200, 40, 40),
                                                        );
                                                        let o = icon.left_bottom()
                                                            + Vec2::new(-2.0, -5.0);
                                                        p.line_segment(
                                                            [o, o + Vec2::new(4.0, 4.0)],
                                                            red,
                                                        );
                                                        p.line_segment(
                                                            [
                                                                o + Vec2::new(4.0, 4.0),
                                                                o + Vec2::new(11.0, -5.0),
                                                            ],
                                                            red,
                                                        );
                                                    }
                                                    p.text(
                                                        Pos2::new(
                                                            rect.left() + 32.0,
                                                            rect.center().y,
                                                        ),
                                                        egui::Align2::LEFT_CENTER,
                                                        name,
                                                        egui::FontId::proportional(13.0),
                                                        Color32::from_gray(25),
                                                    );
                                                    p.text(
                                                        Pos2::new(
                                                            rect.left() + 200.0,
                                                            rect.center().y,
                                                        ),
                                                        egui::Align2::LEFT_CENTER,
                                                        shortcut,
                                                        egui::FontId::proportional(12.0),
                                                        Color32::from_gray(60),
                                                    );
                                                    if resp.clicked() {
                                                        d.selected = i;
                                                        d.preview = None;
                                                    }
                                                    if resp.double_clicked() {
                                                        apply = Some(i);
                                                    }
                                                }
                                            },
                                        );
                                    });
                                ui.horizontal(|ui| {
                                    let small = format_dialogs::icon_button;
                                    if small(ui, "add", "스타일 추가하기", true) {
                                        d.edit = Some(self.new_style_edit(None));
                                    }
                                    if small(ui, "edit", "스타일 편집하기", true) {
                                        d.edit = Some(self.new_style_edit(Some(d.selected)));
                                    }
                                    if small(ui, "delete", "스타일 지우기", d.selected > 0) {
                                        d.delete = Some(0);
                                    }
                                    if small(ui, "replace", "커서 위치의 모양으로 바꾸기", true)
                                        && self.run(
                                            "format.style.update_from_caret",
                                            json!({ "id": d.selected }),
                                        )
                                    {
                                        d.preview = None;
                                    }
                                    ui.add_space(6.0);
                                    // 바탕글 stays first, so the second style cannot move up.
                                    if small(ui, "up", "한 줄 위로 이동하기", d.selected > 1)
                                        && self.run(
                                            "format.style.move",
                                            json!({"id": d.selected, "direction": "up"}),
                                        )
                                    {
                                        d.selected -= 1;
                                        d.preview = None;
                                    }
                                    let can_down = d.selected > 0 && d.selected + 1 < styles.len();
                                    if small(ui, "down", "한 줄 아래로 이동하기", can_down)
                                        && self.run(
                                            "format.style.move",
                                            json!({"id": d.selected, "direction": "down"}),
                                        )
                                    {
                                        d.selected += 1;
                                        d.preview = None;
                                    }
                                    ui.add_space(6.0);
                                    small(ui, "gallery", "스타일마당 (지원하지 않음)", false);
                                    if small(ui, "import", "스타일 가져오기 (JSON)", true)
                                        && let Some(path) = rfd::FileDialog::new()
                                            .add_filter("스타일 JSON", &["json"])
                                            .pick_file()
                                        && self.run(
                                            "format.style.import",
                                            json!({ "path": path.to_string_lossy() }),
                                        )
                                    {
                                        self.message = "스타일을 가져왔습니다.".into();
                                        d.preview = None;
                                    }
                                    if small(ui, "export", "스타일 내보내기 (JSON)", true)
                                        && let Some(path) = rfd::FileDialog::new()
                                            .add_filter("스타일 JSON", &["json"])
                                            .set_file_name("styles.json")
                                            .save_file()
                                        && self.run(
                                            "format.style.export",
                                            json!({ "path": path.to_string_lossy() }),
                                        )
                                    {
                                        self.message = "스타일을 내보냈습니다.".into();
                                    }
                                });
                            });
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                ui.set_width(310.0);
                                ui.label("문단 모양");
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::new(310.0, 100.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect(
                                    rect,
                                    0.0,
                                    Color32::WHITE,
                                    Stroke::new(1.0, Color32::from_gray(215)),
                                    egui::StrokeKind::Inside,
                                );
                                if let Some((_, t, size)) = &d.preview {
                                    let inner = rect.shrink(4.0);
                                    let scale = (inner.width() / size.x)
                                        .min(inner.height() / size.y)
                                        .min(1.0);
                                    let size = *size * scale;
                                    ui.painter().image(
                                        t.id(),
                                        Rect::from_min_size(
                                            Pos2::new(inner.center().x - size.x / 2.0, inner.top()),
                                            size,
                                        ),
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                let para = &detail["paragraph"];
                                let chars = &detail["character"];
                                let pt = |v: &Value, scale: f64| {
                                    format!(
                                        "{} pt",
                                        (v.as_f64().unwrap_or(0.0) / scale * 10.0).round() / 10.0
                                    )
                                };
                                let indent = para["indent"].as_f64().unwrap_or(0.0);
                                let first = if indent > 0.0 {
                                    format!("들여쓰기 {}", pt(&json!(indent), 200.0))
                                } else if indent < 0.0 {
                                    format!("내어쓰기 {}", pt(&json!(-indent), 200.0))
                                } else {
                                    "보통".into()
                                };
                                let align = match para["alignment"].as_str() {
                                    Some("left") => "왼쪽",
                                    Some("right") => "오른쪽",
                                    Some("center") => "가운데",
                                    Some("distribute") => "배분",
                                    Some("split") => "나눔",
                                    _ => "양쪽",
                                };
                                let spacing = if para["lineSpacingType"] == "Percent" {
                                    format!("{} %", para["lineSpacing"].as_i64().unwrap_or(160))
                                } else {
                                    pt(&para["lineSpacing"], 100.0)
                                };
                                let next = styles
                                    .get(detail["nextStyleId"].as_u64().unwrap_or(0) as usize)
                                    .and_then(|s| s["name"].as_str())
                                    .unwrap_or("-")
                                    .to_owned();
                                let info = |ui: &mut egui::Ui, rows: &[(&str, String)]| {
                                    egui::Grid::new(ui.next_auto_id())
                                        .num_columns(4)
                                        .spacing([6.0, 6.0])
                                        .show(ui, |ui| {
                                            for pair in rows.chunks(2) {
                                                for (k, v) in pair {
                                                    lbl(ui, k, 62.0);
                                                    ui.label(format!(": {v}"));
                                                }
                                                ui.end_row();
                                            }
                                        });
                                };
                                let mut edit_shape = None;
                                chrome::section(ui, "문단 모양 정보");
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(250.0);
                                        info(
                                            ui,
                                            &[
                                                ("왼쪽 여백", pt(&para["marginLeft"], 200.0)),
                                                ("첫 줄", first),
                                                ("오른쪽 여백", pt(&para["marginRight"], 200.0)),
                                                ("정렬", align.into()),
                                                ("줄 간격", spacing),
                                                ("다음 스타일", next),
                                            ],
                                        );
                                    });
                                    if ui.button("설정...").clicked() {
                                        edit_shape = Some(false);
                                    }
                                });
                                chrome::section(ui, "글자 모양 정보");
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(250.0);
                                        info(
                                            ui,
                                            &[
                                                (
                                                    "글꼴",
                                                    chars["fontFamily"]
                                                        .as_str()
                                                        .unwrap_or("")
                                                        .into(),
                                                ),
                                                (
                                                    "장평",
                                                    format!(
                                                        "{} %",
                                                        chars["ratios"][0].as_i64().unwrap_or(100)
                                                    ),
                                                ),
                                                ("크기", pt(&chars["fontSize"], 100.0)),
                                                (
                                                    "자간",
                                                    format!(
                                                        "{} %",
                                                        chars["spacings"][0].as_i64().unwrap_or(0)
                                                    ),
                                                ),
                                            ],
                                        );
                                    });
                                    if ui.button("설정...").clicked() {
                                        edit_shape = Some(true);
                                    }
                                });
                                chrome::section(ui, "글머리표/문단 번호 정보");
                                let head = match para["headType"].as_str() {
                                    Some("Outline") => "개요",
                                    Some("Number") => "문단 번호",
                                    Some("Bullet") => "글머리표",
                                    _ => "없음",
                                };
                                ui.horizontal(|ui| {
                                    lbl(ui, "종류", 62.0);
                                    ui.label(format!(": {head}"));
                                });
                                chrome::section(ui, "현재 커서 위치 스타일");
                                ui.horizontal(|ui| {
                                    lbl(ui, "스타일", 62.0);
                                    ui.label(format!(
                                        ": {}",
                                        styles
                                            .get(current)
                                            .and_then(|s| s["name"].as_str())
                                            .unwrap_or("")
                                    ));
                                    if ui.button("글자 스타일 해제").clicked() {
                                        self.run("format.style.clear_char", json!({}));
                                        d.preview = None;
                                    }
                                });
                                if let Some(character) = edit_shape {
                                    let values = if character {
                                        chars.clone()
                                    } else {
                                        para.clone()
                                    };
                                    let mut draft = FormatDraft::new(values);
                                    draft.for_style = true;
                                    self.format_draft = Some(draft);
                                    self.show_character = character;
                                    self.show_paragraph = !character;
                                    d.shape = Some(character);
                                }
                            });
                        });
                    });
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.set_width(80.0);
                    if ui
                        .add(
                            egui::Button::new("설정(D)")
                                .fill(Color32::WHITE)
                                .stroke(Stroke::new(1.5, chrome::Tokens::ACCENT))
                                .min_size(Vec2::new(80.0, 28.0)),
                        )
                        .clicked()
                    {
                        apply = Some(d.selected);
                    }
                });
            });
        });
        if let Some(id) = apply {
            if self.run("style.apply", json!({ "id": id })) {
                return;
            }
        } else if close || (r.should_close() && d.shape.is_none()) {
            return;
        }
        self.ext.format.style = Some(d);
    }
    fn style_preview(&mut self, ctx: &egui::Context, d: &mut StyleDialog, detail: &Value) {
        let key = format!("{}:{}", self.session.generation, detail);
        if d.preview.as_ref().is_some_and(|(k, _, _)| *k == key) {
            return;
        }
        let Ok(image) = preview_image(
            &self.renderer,
            Some(&detail["character"]),
            Some(&detail["paragraph"]),
        ) else {
            return;
        };
        let size = Vec2::new(image.size[0] as f32, image.size[1] as f32) / 1.5;
        let t = ctx.load_texture("style-preview", image, egui::TextureOptions::LINEAR);
        d.preview = Some((key, t, size));
    }
    /// 스타일 추가하기 / 스타일 편집하기 (593×259).
    fn style_edit_dialog(
        &mut self,
        ctx: &egui::Context,
        d: &mut StyleDialog,
        mut edit: StyleEdit,
    ) -> bool {
        let styles = self
            .session
            .run("format.style.list", json!({}))
            .unwrap_or_default();
        let mut names: Vec<String> = styles
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| s["name"].as_str().unwrap_or("").to_owned())
            .collect();
        if edit.id.is_none() {
            names.push(format!("{} (새 스타일)", edit.name));
        }
        let title = if edit.id.is_some() {
            "스타일 편집하기"
        } else {
            "스타일 추가하기"
        };
        let mut close = false;
        let mut apply = false;
        let mut shape = None;
        let r = chrome::modal("format-style-edit").show(ctx, |ui| {
            ui.set_width(569.0);
            close = chrome::title(ui, title);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(460.0);
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(225.0);
                            ui.label("스타일 이름");
                            ui.add_sized([219.0, 24.0], egui::TextEdit::singleline(&mut edit.name));
                            chrome::section(ui, "스타일 종류");
                            ui.add_enabled_ui(edit.id.is_none(), |ui| {
                                ui.horizontal(|ui| {
                                    ui.radio_value(&mut edit.character, false, "문단");
                                    ui.radio_value(&mut edit.character, true, "글자");
                                });
                            });
                        });
                        ui.vertical(|ui| {
                            ui.label("영문 이름");
                            ui.add_sized([219.0, 24.0], egui::TextEdit::singleline(&mut edit.english));
                            ui.label("다음 문단에 적용할 스타일");
                            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                            ui.add_enabled_ui(!edit.character, |ui| {
                                combo(ui, "style-next", &mut edit.next, &refs, 207.0);
                            });
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!edit.character, egui::Button::new("문단 모양...")).clicked() {
                            shape = Some(false);
                        }
                        if ui.button("글자 모양...").clicked() {
                            shape = Some(true);
                        }
                        ui.add_enabled(false, egui::Button::new("글머리표/문단 번호..."))
                            .on_disabled_hover_text("스타일의 번호 모양은 문단 모양 › 확장에서 지정합니다.");
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("스타일 이름은 다르지만 영문 이름이 같은 경우에는 두 스타일을 같은 스타일로 인식합니다.").size(11.5));
                });
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(
                        ui,
                        !edit.name.trim().is_empty(),
                        if edit.id.is_some() { "설정(D)" } else { "추가(D)" },
                    );
                    apply = a;
                    close |= c;
                });
            });
        });
        if let Some(character) = shape {
            // Base values: the edited style, or the caret's shapes for a new style.
            let base = match edit.id {
                Some(id) => self
                    .session
                    .run("format.style.detail", json!({ "id": id }))
                    .ok()
                    .map(|v| v[if character { "character" } else { "paragraph" }].clone()),
                None => if character {
                    self.session.character_properties()
                } else {
                    self.session.paragraph_format()
                }
                .ok(),
            };
            if let Some(mut base) = base {
                let patch = if character {
                    &edit.char_patch
                } else {
                    &edit.para_patch
                };
                if let (Some(b), Some(p)) = (base.as_object_mut(), patch.as_object()) {
                    b.extend(p.clone());
                }
                let mut draft = FormatDraft::new(base);
                draft.for_style = true;
                self.format_draft = Some(draft);
                self.show_character = character;
                self.show_paragraph = !character;
                d.shape = Some(character);
            }
        }
        if apply {
            let mut params = json!({"name": edit.name.trim(), "englishName": edit.english.trim()});
            if !edit.character {
                params["nextStyleId"] = json!(edit.next);
            }
            for (key, patch) in [
                ("character", &edit.char_patch),
                ("paragraph", &edit.para_patch),
            ] {
                if patch.as_object().is_some_and(|p| !p.is_empty()) {
                    params[key] = patch.clone();
                }
            }
            let ok = match edit.id {
                Some(id) => {
                    params["id"] = json!(id);
                    self.run("format.style.update", params)
                }
                None => {
                    params["type"] = json!(if edit.character {
                        "character"
                    } else {
                        "paragraph"
                    });
                    let r = self.session.run("format.style.create", params);
                    match self.result(r) {
                        Some(v) => {
                            d.selected = v["id"].as_u64().unwrap_or(0) as usize;
                            true
                        }
                        None => false,
                    }
                }
            };
            d.preview = None;
            if ok {
                return false;
            }
        }
        if close || r.should_close() {
            return false;
        }
        d.edit = Some(edit);
        true
    }
    fn style_delete_dialog(
        &mut self,
        ctx: &egui::Context,
        d: &mut StyleDialog,
        mut replacement: usize,
    ) {
        let styles = self
            .session
            .run("format.style.list", json!({}))
            .unwrap_or_default();
        let names: Vec<String> = styles
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| s["name"].as_str().unwrap_or("").to_owned())
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let target = names.get(d.selected).cloned().unwrap_or_default();
        let mut close = false;
        let mut apply = false;
        let r = chrome::modal("format-style-delete").show(ctx, |ui| {
            ui.set_width(420.0);
            close = chrome::title(ui, "스타일 지우기");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    ui.label(format!("'{target}' 스타일을 지웁니다."));
                    ui.label("이 스타일을 쓰는 문단은 다음 스타일로 바뀝니다.");
                    ui.horizontal(|ui| {
                        lbl(ui, "대치할 스타일", 70.0);
                        combo(ui, "style-replace", &mut replacement, &refs, 160.0);
                    });
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, replacement != d.selected, "지우기(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply
            && self.run(
                "format.style.delete",
                json!({"id": d.selected, "replacement": replacement}),
            )
        {
            d.selected = replacement.min(d.selected.saturating_sub(1));
            d.preview = None;
            return;
        }
        if !(close || r.should_close()) {
            d.delete = Some(replacement);
        }
    }

    // ---- 글머리표 및 문단 번호 (Ctrl+K,N) ----

    fn numbering_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.numbering.take() else {
            return;
        };
        let mut close = false;
        let mut apply = false;
        let r = chrome::modal("format-numbering").show(ctx, |ui| {
            ui.set_width(558.0);
            close = chrome::title(
                ui,
                if d.tab == 3 {
                    "개요 번호 모양"
                } else {
                    "글머리표 및 문단 번호"
                },
            );
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(461.0);
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0, Color32::from_gray(227)))
                        .inner_margin(egui::Margin::symmetric(14, 6))
                        .show(ui, |ui| {
                            ui.set_width(431.0);
                            ui.set_min_height(470.0);
                            if d.tab < 3 {
                                chrome::tabs(
                                    ui,
                                    &mut d.tab,
                                    &["글머리표", "문단 번호", "그림 글머리표"],
                                );
                            }
                            match d.tab {
                                0 => {
                                    group(ui, "글머리표 모양", |ui| {
                                        let mut items = vec![None];
                                        items.extend(BULLETS.iter().map(|c| Some(*c)));
                                        gallery(
                                            ui,
                                            "bullets",
                                            &mut d.bullet,
                                            items.len(),
                                            3,
                                            |p, r, i| {
                                                if let Some(Some(c)) = items.get(i) {
                                                    for row in 0..2 {
                                                        let y = r.top() + 28.0 + row as f32 * 34.0;
                                                        p.text(
                                                            Pos2::new(r.left() + 18.0, y),
                                                            egui::Align2::CENTER_CENTER,
                                                            c.to_string(),
                                                            egui::FontId::proportional(13.0),
                                                            Color32::BLACK,
                                                        );
                                                        lines(
                                                            p,
                                                            Pos2::new(r.left() + 32.0, y),
                                                            r.width() - 44.0,
                                                            2,
                                                        );
                                                    }
                                                }
                                            },
                                        );
                                    });
                                    group(ui, "확인용 글머리표 모양", |ui| {
                                        ui.label(
                                            RichText::new(
                                                "확인용 글머리표는 아직 지원하지 않습니다.",
                                            )
                                            .weak(),
                                        );
                                    });
                                }
                                1 => {
                                    group(ui, "문단 번호 모양", |ui| {
                                        gallery(
                                            ui,
                                            "numberings",
                                            &mut d.numbering,
                                            NUMBERINGS.len() + 1,
                                            3,
                                            numbering_preview,
                                        );
                                    });
                                    group(ui, "시작 번호 방식", |ui| {
                                        ui.add_enabled(
                                            d.previous > 0,
                                            egui::RadioButton::new(
                                                d.restart == 0,
                                                "앞 번호 목록에 이어",
                                            ),
                                        )
                                        .clicked()
                                        .then(|| d.restart = 0);
                                        ui.add_enabled(
                                            d.previous > 1,
                                            egui::RadioButton::new(
                                                d.restart == 1,
                                                "이전 번호 목록에 이어",
                                            ),
                                        )
                                        .clicked()
                                        .then(|| d.restart = 1);
                                        if ui.radio(d.restart == 2, "새 번호 목록 시작").clicked()
                                        {
                                            d.restart = 2;
                                        }
                                        ui.add_enabled_ui(d.restart == 2, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                lbl(ui, "1수준 시작 번호", 79.0);
                                                spin(ui, &mut d.start, 1.0..=65535.0, 0, "", 91.0);
                                            });
                                        });
                                    });
                                }
                                3 => {
                                    group(ui, "개요 번호 모양", |ui| {
                                        gallery(
                                            ui,
                                            "outline-shapes",
                                            &mut d.numbering,
                                            NUMBERINGS.len() + 1,
                                            3,
                                            numbering_preview,
                                        );
                                    });
                                    group(ui, "시작 번호", |ui| {
                                        ui.horizontal(|ui| {
                                            lbl(ui, "1수준 시작 번호", 79.0);
                                            spin(ui, &mut d.start, 1.0..=65535.0, 0, "", 91.0);
                                        });
                                        ui.label(
                                            RichText::new(
                                                "현재 구역의 개요 문단(개요 1–7)에 적용됩니다.",
                                            )
                                            .weak(),
                                        );
                                    });
                                }
                                _ => {
                                    group(ui, "그림 글머리표 모양", |ui| {
                                        ui.label(
                                            RichText::new(
                                                "그림 글머리표는 아직 지원하지 않습니다.",
                                            )
                                            .weak(),
                                        );
                                    });
                                }
                            }
                        });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_enabled(false, egui::Button::new("사용자 정의..."));
                        format_dialogs::icon_button(ui, "clear", "지우기", false);
                    });
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    let valid = d.tab < 2 || (d.tab == 3 && d.numbering > 0);
                    let (a, c) = chrome::actions(ui, valid, "설정(D)");
                    apply = a;
                    close |= c;
                });
            });
        });
        if apply && d.tab == 3 {
            if let Some((_, formats, codes)) = NUMBERINGS.get(d.numbering.wrapping_sub(1))
                && self.run(
                    "format.outline.shape",
                    json!({"levelFormats": formats, "numberFormats": codes,
                        "startNumber": d.start as u16}),
                )
            {
                return;
            }
        } else if apply {
            let params = if d.tab == 0 {
                match BULLETS.get(d.bullet.wrapping_sub(1)) {
                    Some(c) => json!({"kind": "bullet", "char": c.to_string()}),
                    None => json!({"kind": "none"}),
                }
            } else {
                match NUMBERINGS.get(d.numbering.wrapping_sub(1)) {
                    Some((_, formats, codes)) => {
                        let mut p = json!({"kind": "number", "levelFormats": formats, "numberFormats": codes});
                        match d.restart {
                            0 => p["restart"] = json!({"mode": "continue"}),
                            1 => p["restart"] = json!({"mode": "previous"}),
                            _ if d.previous > 0 || d.start != 1.0 => {
                                p["restart"] = json!({"mode": "new", "start": d.start as u32});
                            }
                            _ => {}
                        }
                        p
                    }
                    None => json!({"kind": "none"}),
                }
            };
            if self.run("format.numbering.apply", params) {
                return;
            }
        }
        if !(close || r.should_close()) {
            self.ext.format.numbering = Some(d);
        }
    }

    // ---- 서식 도구줄 / 리본 위젯 ----

    /// Combo boxes and split buttons drawn by this module. `None` = draw an icon button.
    /// Called from `ribbon.rs` (owner shell/page area) for the 서식 tab and 서식 도구줄.
    pub(crate) fn format_ext_widget(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        size: Vec2,
    ) -> Option<egui::Response> {
        format_dialogs::load_fonts(&self.renderer);
        let chars = self.session.character_properties().unwrap_or_default();
        let para = self.session.paragraph_format().unwrap_or_default();
        Some(match id {
            "format.style" => {
                let styles = self
                    .session
                    .run("format.style.list", json!({}))
                    .unwrap_or_default();
                let current = styles
                    .as_array()
                    .and_then(|a| a.iter().find(|s| s["current"] == true))
                    .and_then(|s| s["name"].as_str())
                    .unwrap_or("바탕글")
                    .to_owned();
                let mut pick = None;
                let r = hwp_combo(ui, "tb-style", size, Prefix::Style, &current, |ui| {
                    for s in styles.as_array().into_iter().flatten() {
                        let label = format!(
                            "{}    {}",
                            s["name"].as_str().unwrap_or(""),
                            s["shortcut"].as_str().unwrap_or("")
                        );
                        if ui.selectable_label(s["current"] == true, label).clicked() {
                            pick = s["id"].as_u64();
                        }
                    }
                    ui.separator();
                    if ui.button("스타일...  F6").clicked() {
                        pick = Some(u64::MAX);
                    }
                });
                match pick {
                    Some(u64::MAX) => self.open_style(false),
                    Some(id) => {
                        self.run("style.apply", json!({ "id": id }));
                    }
                    None => {}
                }
                r.on_hover_text("스타일 (F6)")
            }
            "format.font_lang" => {
                let mut lang = self.ext.format.font_language;
                let r = hwp_combo(ui, "tb-lang", size, Prefix::None, LANGUAGES[lang], |ui| {
                    for (i, l) in LANGUAGES.iter().enumerate() {
                        ui.selectable_value(&mut lang, i, *l);
                    }
                });
                self.ext.format.font_language = lang;
                r.on_hover_text("글꼴 언어")
            }
            "format.font_face" => {
                let lang = self.ext.format.font_language;
                let mut fonts: Vec<Value> = chars["fontFamilies"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let current = if lang == 0 {
                    chars["fontFamily"].as_str()
                } else {
                    fonts.get(lang - 1).and_then(Value::as_str)
                }
                .unwrap_or("함초롬바탕")
                .to_owned();
                let mut pick: Option<String> = None;
                let all = format_dialogs::font_names();
                let r = hwp_combo(ui, "tb-font", size, Prefix::Font, &current, |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show_rows(
                        ui,
                        20.0,
                        all.len(),
                        |ui, rows| {
                            for name in &all[rows] {
                                if ui
                                    .selectable_label(
                                        *name == current,
                                        RichText::new(name.as_str()).size(13.0),
                                    )
                                    .clicked()
                                {
                                    pick = Some(name.clone());
                                }
                            }
                        },
                    );
                });
                if let Some(font) = pick.filter(|f| *f != current) {
                    let patch = if lang == 0 || fonts.len() != 7 {
                        json!({ "fontFamily": font })
                    } else {
                        fonts[lang - 1] = json!(font);
                        json!({ "fontFamilies": fonts })
                    };
                    self.run("format.character", patch);
                }
                r.on_hover_text("글꼴")
            }
            "format.font_size" => {
                let mut pt = chars["fontSize"].as_f64().unwrap_or(1000.0) / 100.0;
                let (changed, r) = hwp_spin(
                    ui,
                    "tb-size",
                    size,
                    Spin {
                        value: &mut pt,
                        range: 1.0..=4096.0,
                        decimals: 1,
                        unit: "pt",
                        step: 1.0,
                        prefix: Prefix::None,
                        presets: &[
                            8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0,
                            32.0, 36.0, 48.0, 72.0,
                        ],
                    },
                );
                if changed {
                    self.run(
                        "format.character",
                        json!({"fontSize": (pt * 100.0).round() as i64}),
                    );
                }
                r.on_hover_text("글자 크기")
            }
            "format.line_spacing" => {
                let percent = para["lineSpacingType"] == "Percent";
                let scale = if percent { 1.0 } else { 100.0 };
                let mut value = para["lineSpacing"].as_f64().unwrap_or(160.0) / scale;
                let presets: &[f64] = if percent {
                    &[100.0, 130.0, 150.0, 160.0, 180.0, 200.0, 250.0, 300.0]
                } else {
                    &[]
                };
                let (changed, r) = hwp_spin(
                    ui,
                    "tb-line-spacing",
                    size,
                    Spin {
                        value: &mut value,
                        range: 0.0..=500.0,
                        decimals: if percent { 0 } else { 1 },
                        unit: if percent { "%" } else { "pt" },
                        step: if percent { 10.0 } else { 1.0 },
                        prefix: Prefix::LineSpacing,
                        presets,
                    },
                );
                if changed {
                    self.run(
                        "format.paragraph",
                        json!({"lineSpacing": (value * scale).round() as i64}),
                    );
                }
                r.on_hover_text("줄 간격")
            }
            "format.underline"
            | "format.strikethrough"
            | "format.text_color"
            | "format.shade_color"
            | "format.highlight"
            | "format.char_border" => self.split_button(ui, id, size, &chars),
            "format.align" => {
                let align = para["alignment"].as_str().unwrap_or("justify").to_owned();
                let r = big_button(ui, size, "문단 정렬", |p, r| paint_align(p, r, &align));
                egui::Popup::from_toggle_button_response(&r).show(|ui| {
                    for (a, label) in [
                        ("justify", "양쪽 정렬"),
                        ("left", "왼쪽 정렬"),
                        ("center", "가운데 정렬"),
                        ("right", "오른쪽 정렬"),
                        ("distribute", "배분 정렬"),
                        ("split", "나눔 정렬"),
                    ] {
                        if ui.selectable_label(align == a, label).clicked() {
                            self.run(&format!("format.align.{a}"), json!({}));
                        }
                    }
                });
                r
            }
            "format.outline.toggle" | "format.bullet.toggle" | "format.numbering.toggle" => {
                self.numbering_split(ui, id, size, &para)
            }
            _ => return None,
        })
    }
    fn split_button(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        size: Vec2,
        chars: &Value,
    ) -> egui::Response {
        let on = match id {
            "format.underline" => chars["underline"] == true,
            "format.strikethrough" => chars["strikethrough"] == true,
            "format.char_border" => chars["borderLeft"]["type"].as_u64().unwrap_or(0) > 0,
            _ => false,
        };
        let text_color = self
            .ext
            .format
            .text_color
            .clone()
            .unwrap_or("#ff0000".into());
        let shade = self
            .ext
            .format
            .shade_color
            .clone()
            .unwrap_or("#ffff00".into());
        let highlight = self
            .ext
            .format
            .highlight
            .clone()
            .unwrap_or("#ffff00".into());
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let main = Rect::from_min_max(rect.min, Pos2::new(rect.right() - 14.0, rect.bottom()));
        let arrow = Rect::from_min_max(Pos2::new(rect.right() - 14.0, rect.top()), rect.max);
        let main_resp = ui.interact(main, egui::Id::new((id, "main")), egui::Sense::click());
        let arrow_resp = ui.interact(arrow, egui::Id::new((id, "arrow")), egui::Sense::click());
        let p = ui.painter();
        if on || main_resp.hovered() || arrow_resp.hovered() {
            p.rect(
                rect,
                2.0,
                if on {
                    Color32::from_rgb(222, 233, 255)
                } else {
                    Color32::from_gray(240)
                },
                Stroke::new(1.0, Color32::from_gray(205)),
                egui::StrokeKind::Inside,
            );
        }
        // Same 18 px '가' glyphs as the other 서식 도구줄 icons (icons.rs, owner 04).
        let icon = match id {
            "format.underline" => super::icons::Icon::Underline,
            "format.strikethrough" => super::icons::Icon::Strike,
            "format.text_color" | "format.shade_color" => super::icons::Icon::TextColor,
            "format.highlight" => super::icons::Icon::Highlight,
            _ => super::icons::Icon::CharBorder,
        };
        let glyph = Rect::from_center_size(main.center(), Vec2::splat(18.0));
        if id == "format.shade_color" {
            // 음영: the glyph sits on its shade color, as on Hangul's button.
            ui.painter()
                .rect_filled(glyph.shrink(1.0), 1.0, rgb(&shade));
        }
        self.icons.paint(ui, glyph, icon, true);
        // The color bar shows the color the button will apply, as in Hangul.
        let bar = match id {
            "format.text_color" => Some(&text_color),
            "format.highlight" => Some(&highlight),
            _ => None,
        };
        if let Some(color) = bar {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(glyph.left() + 1.0, glyph.bottom() - 3.0),
                    Vec2::new(glyph.width() - 2.0, 3.0),
                ),
                0.0,
                rgb(color),
            );
        }
        thin_chevron(ui.painter(), arrow.center());
        if main_resp.clicked() {
            match id {
                "format.underline" | "format.strikethrough" | "format.char_border" => {
                    self.run(id, json!({}));
                }
                id => {
                    self.format_ext_dispatch(id, ui.ctx());
                }
            }
        }
        egui::Popup::from_toggle_button_response(&arrow_resp)
            .id(egui::Id::new((id, "popup")))
            .show(|ui| match id {
                "format.text_color" => {
                    ui.label("글자 색");
                    let pick = preset_row(ui).or_else(|| color_menu(ui, "", None).flatten());
                    if let Some(c) = pick {
                        self.ext.format.text_color = Some(c.clone());
                        self.run("format.text_color", json!({ "color": c }));
                    }
                }
                "format.shade_color" => {
                    ui.label("음영 색");
                    let pick = preset_row(ui)
                        .map(Some)
                        .or_else(|| color_menu(ui, "", Some("음영 없음")));
                    if let Some(c) = pick {
                        self.ext.format.shade_color = c.clone();
                        self.run("format.shade_color", json!({ "color": c }));
                    }
                }
                "format.highlight" => {
                    ui.label("형광펜");
                    let pick = preset_row(ui)
                        .map(Some)
                        .or_else(|| color_menu(ui, "", Some("색 없음")));
                    if let Some(c) = pick {
                        self.ext.format.highlight = c.clone();
                        self.run("format.highlight", json!({ "color": c }));
                    }
                }
                "format.underline" | "format.strikethrough" => {
                    let key = if id == "format.underline" {
                        "underlineShape"
                    } else {
                        "strikeShape"
                    };
                    for (i, name) in format_dialogs::LINE_TYPES[1..12].iter().enumerate() {
                        let resp = ui
                            .add_sized([150.0, 20.0], egui::Button::selectable(false, ""))
                            .on_hover_text(*name);
                        format_dialogs::paint_line(
                            ui.painter(),
                            Pos2::new(resp.rect.left() + 8.0, resp.rect.center().y),
                            Pos2::new(resp.rect.right() - 8.0, resp.rect.center().y),
                            i as u8 + 1,
                            1.0,
                            Color32::BLACK,
                        );
                        if resp.clicked() {
                            let mut patch = json!({ key: i });
                            if id == "format.underline" {
                                patch["underline"] = json!(true);
                                patch["underlineType"] = json!("Bottom");
                            } else {
                                patch["strikethrough"] = json!(true);
                            }
                            self.run("format.character", patch);
                        }
                    }
                    ui.separator();
                    if ui
                        .button(if id == "format.underline" {
                            "밑줄 없음"
                        } else {
                            "취소선 없음"
                        })
                        .clicked()
                    {
                        self.run(id, json!({"value": false}));
                    }
                }
                _ => {
                    if ui.button("테두리 없음").clicked() {
                        self.run("format.char_border", json!({ "value": false }));
                    }
                    if ui.button("바깥쪽 테두리").clicked() {
                        self.run(
                            "format.char_border",
                            json!({"type": 1, "width": 0, "color": "#000000"}),
                        );
                    }
                    ui.label(RichText::new("선 종류").weak());
                    for (i, name) in format_dialogs::LINE_TYPES[1..12].iter().enumerate() {
                        let resp = ui
                            .add_sized([150.0, 20.0], egui::Button::selectable(false, ""))
                            .on_hover_text(*name);
                        format_dialogs::paint_line(
                            ui.painter(),
                            Pos2::new(resp.rect.left() + 8.0, resp.rect.center().y),
                            Pos2::new(resp.rect.right() - 8.0, resp.rect.center().y),
                            i as u8 + 1,
                            1.0,
                            Color32::BLACK,
                        );
                        if resp.clicked() {
                            self.run("format.char_border", json!({ "type": i + 1 }));
                        }
                    }
                    ui.label(RichText::new("테두리 색").weak());
                    if let Some(c) = preset_row(ui) {
                        self.run("format.char_border", json!({ "color": c }));
                    }
                    ui.separator();
                    if ui.button("글자 테두리/배경...").clicked() {
                        self.open_shape(true, 2);
                    }
                }
            });
        let tip = match id {
            "format.underline" => "밑줄 (Ctrl+U)",
            "format.strikethrough" => "취소선",
            "format.text_color" => "글자 색",
            "format.shade_color" => "글자 음영 색",
            "format.highlight" => "형광펜",
            _ => "글자 테두리",
        };
        response.union(main_resp).on_hover_text(tip)
    }
    fn numbering_split(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        size: Vec2,
        para: &Value,
    ) -> egui::Response {
        let kind = match id {
            "format.outline.toggle" => "Outline",
            "format.bullet.toggle" => "Bullet",
            _ => "Number",
        };
        let on = para["headType"] == kind;
        let label = match kind {
            "Outline" => "개요",
            "Bullet" => "글머리표",
            _ => "문단 번호",
        };
        let draw = |p: &egui::Painter, r: Rect| {
            for row in 0..3 {
                let y = r.top() + 3.0 + row as f32 * (r.height() - 6.0) / 2.0;
                let mark = match kind {
                    "Bullet" => "•".to_owned(),
                    "Outline" => ["1.", "가.", "1)"][row].to_owned(),
                    _ => format!("{}.", row + 1),
                };
                p.text(
                    Pos2::new(r.left() + 1.0, y),
                    egui::Align2::LEFT_CENTER,
                    mark,
                    egui::FontId::proportional(8.0),
                    Color32::from_gray(40),
                );
                let indent = if kind == "Outline" {
                    row as f32 * 2.0
                } else {
                    0.0
                };
                p.line_segment(
                    [
                        Pos2::new(r.left() + 9.0 + indent, y),
                        Pos2::new(r.right(), y),
                    ],
                    Stroke::new(1.0, Color32::from_gray(90)),
                );
            }
        };
        let response = if size.y > 40.0 {
            big_button(ui, size, label, draw)
        } else {
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            if on || response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    if on {
                        Color32::from_rgb(222, 233, 255)
                    } else {
                        Color32::from_gray(240)
                    },
                );
            }
            draw(
                ui.painter(),
                Rect::from_min_size(
                    rect.min + Vec2::new(4.0, 4.0),
                    Vec2::new(size.x - 18.0, size.y - 8.0),
                ),
            );
            ui.painter().text(
                Pos2::new(rect.right() - 7.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "∨",
                egui::FontId::proportional(9.0),
                Color32::from_gray(90),
            );
            response
        };
        // Left part toggles; the arrow (right 14 px) opens the shape gallery.
        let arrow = response.interact_pointer_pos().is_some_and(|p| {
            p.x > response.rect.right() - 14.0 || size.y > 40.0 && p.y > response.rect.top() + 40.0
        });
        if response.clicked() && !arrow {
            self.run(id, json!({}));
        }
        egui::Popup::from_response(&response)
            .id(egui::Id::new((id, "gallery")))
            .open_memory((response.clicked() && arrow).then_some(egui::containers::SetOpenCommand::Toggle))
            .show(|ui| {
                ui.set_width(250.0);
                if kind == "Bullet" {
                    ui.horizontal_wrapped(|ui| {
                        for c in BULLETS {
                            if ui.add_sized([28.0, 28.0], egui::Button::new(RichText::new(c.to_string()).size(14.0))).clicked() {
                                self.run("format.numbering.apply", json!({"kind": "bullet", "char": c.to_string()}));
                            }
                        }
                    });
                } else if kind == "Number" {
                    for (name, formats, codes) in NUMBERINGS {
                        if ui.selectable_label(false, name).clicked() {
                            self.run("format.numbering.apply", json!({"kind": "number", "levelFormats": formats, "numberFormats": codes}));
                        }
                    }
                } else {
                    for level in 0..7u8 {
                        if ui.selectable_label(on && para["paraLevel"] == level, format!("개요 {} 수준", level + 1)).clicked() {
                            self.run("format.numbering.apply", json!({"kind": "outline", "level": level}));
                        }
                    }
                }
                ui.separator();
                if ui.button(format!("{label} 해제")).clicked() {
                    self.run("format.numbering.apply", json!({"kind": "none"}));
                }
                if kind != "Outline" && ui.button("글머리표 및 문단 번호...").clicked() {
                    self.open_numbering(if kind == "Bullet" { 0 } else { 1 });
                }
                if kind == "Outline" && ui.button("개요 번호 모양...").clicked() {
                    self.open_outline();
                }
            });
        response.on_hover_text(match kind {
            "Outline" => "개요 적용/해제 (Ctrl+Insert)",
            "Bullet" => "글머리표 적용/해제 (Ctrl+Shift+Delete)",
            _ => "문단 번호 적용/해제 (Ctrl+Shift+Insert)",
        })
    }
}

/// Gallery cell for `NUMBERINGS[i - 1]`: levels 1–4 with text lines.
fn numbering_preview(p: &egui::Painter, r: Rect, i: usize) {
    let Some((_, formats, codes)) = NUMBERINGS.get(i.wrapping_sub(1)) else {
        return;
    };
    for level in 0..4 {
        let y = r.top() + 16.0 + level as f32 * 21.0;
        let g = p.text(
            Pos2::new(r.left() + 10.0, y),
            egui::Align2::LEFT_CENTER,
            number_text(formats, codes, level),
            egui::FontId::proportional(11.5),
            Color32::BLACK,
        );
        lines(
            p,
            Pos2::new(g.right() + 4.0, y),
            r.right() - g.right() - 14.0,
            1,
        );
    }
}
/// Hangul 서식 도구줄 field border.
const FIELD_BORDER: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
#[derive(Clone, Copy, PartialEq)]
enum Prefix {
    None,
    Style,
    Font,
    LineSpacing,
}
/// Small icon at the left of a toolbar field (style lines, TrueType 'T', line spacing).
/// Returns where the field text starts.
fn paint_prefix(p: &egui::Painter, rect: Rect, prefix: Prefix) -> f32 {
    let icon = Rect::from_center_size(
        Pos2::new(rect.left() + 11.0, rect.center().y),
        Vec2::splat(13.0),
    );
    match prefix {
        Prefix::None => return 6.0,
        Prefix::Style => paint_align(p, icon, "justify"),
        Prefix::Font => {
            p.text(
                icon.center() + Vec2::new(-1.5, -1.0),
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(12.0),
                Color32::from_gray(150),
            );
            p.text(
                icon.center() + Vec2::new(1.5, 1.0),
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(12.0),
                Color32::from_rgb(40, 90, 170),
            );
        }
        Prefix::LineSpacing => {
            let ink = Stroke::new(1.0, Color32::from_gray(70));
            let x = icon.left() + 2.0;
            p.line_segment([Pos2::new(x, icon.top()), Pos2::new(x, icon.bottom())], ink);
            for a in [-2.5, 2.5] {
                p.line_segment(
                    [Pos2::new(x, icon.top()), Pos2::new(x + a, icon.top() + 2.5)],
                    ink,
                );
                p.line_segment(
                    [
                        Pos2::new(x, icon.bottom()),
                        Pos2::new(x + a, icon.bottom() - 2.5),
                    ],
                    ink,
                );
            }
            for i in 0..4 {
                let y = icon.top() + 1.5 + i as f32 * 3.3;
                p.line_segment(
                    [Pos2::new(x + 4.0, y), Pos2::new(icon.right() + 1.0, y)],
                    Stroke::new(1.0, Color32::from_gray(110)),
                );
            }
        }
    }
    24.0
}
fn field_frame(p: &egui::Painter, rect: Rect, hovered: bool) {
    p.rect(
        rect,
        0.0,
        Color32::WHITE,
        Stroke::new(
            1.0,
            if hovered {
                chrome::Tokens::ACCENT
            } else {
                FIELD_BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
}
fn thin_chevron(p: &egui::Painter, c: Pos2) {
    let s = Stroke::new(1.0, Color32::from_gray(90));
    p.line_segment([c + Vec2::new(-3.5, -1.5), c + Vec2::new(0.0, 2.0)], s);
    p.line_segment([c + Vec2::new(0.0, 2.0), c + Vec2::new(3.5, -1.5)], s);
}
/// Hangul-style toolbar combo: white field, 1 px #c8c8c8 border, optional icon, thin ∨.
fn hwp_combo(
    ui: &mut egui::Ui,
    id: &str,
    size: Vec2,
    prefix: Prefix,
    text: &str,
    list: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter();
    field_frame(p, rect, response.hovered());
    let left = paint_prefix(p, rect, prefix);
    let clip = Rect::from_min_max(
        Pos2::new(rect.left() + left, rect.top()),
        Pos2::new(rect.right() - 18.0, rect.bottom()),
    );
    p.with_clip_rect(clip).text(
        Pos2::new(clip.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.5),
        Color32::from_gray(25),
    );
    thin_chevron(p, Pos2::new(rect.right() - 9.0, rect.center().y));
    egui::Popup::from_toggle_button_response(&response)
        .id(egui::Id::new(("tb-combo", id)))
        .show(|ui| {
            ui.set_min_width(size.x);
            list(ui);
        });
    response
}
struct Spin<'a> {
    value: &'a mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    unit: &'a str,
    step: f64,
    prefix: Prefix,
    presets: &'a [f64],
}
/// Hangul-style spin field: editable value, unit, 12 px up/down buttons. Clicking the
/// unit opens the preset list. Returns whether a new value was committed.
fn hwp_spin(ui: &mut egui::Ui, id: &str, size: Vec2, spin: Spin) -> (bool, egui::Response) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter().clone();
    field_frame(&p, rect, response.hovered());
    let left = paint_prefix(&p, rect, spin.prefix);
    let buttons = Rect::from_min_max(Pos2::new(rect.right() - 13.0, rect.top()), rect.max);
    let unit = Rect::from_min_max(
        Pos2::new(buttons.left() - 22.0, rect.top()),
        Pos2::new(buttons.left(), rect.bottom()),
    );
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + left, rect.top() + 3.0),
        Pos2::new(unit.left(), rect.bottom() - 3.0),
    );
    let decimals = spin.decimals;
    let format = |v: f64| format!("{v:.decimals$}");
    let edit_id = egui::Id::new(("tb-spin-text", id));
    let mut text: String = ui
        .data_mut(|d| d.get_temp(edit_id))
        .unwrap_or_else(|| format(*spin.value));
    // A child Ui keeps the parent's layout cursor after the whole field.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let edit = child.add_sized(
        text_rect.size(),
        egui::TextEdit::singleline(&mut text)
            .frame(egui::Frame::NONE)
            .font(egui::FontId::proportional(12.5)),
    );
    let mut changed = false;
    if edit.has_focus() {
        ui.data_mut(|d| d.insert_temp(edit_id, text.clone()));
    }
    if edit.lost_focus() {
        if let Ok(v) = text
            .trim()
            .trim_end_matches(spin.unit)
            .trim()
            .parse::<f64>()
        {
            let v = v.clamp(*spin.range.start(), *spin.range.end());
            if v != *spin.value {
                *spin.value = v;
                changed = true;
            }
        }
        ui.data_mut(|d| d.remove::<String>(edit_id));
    }
    p.text(
        unit.center(),
        egui::Align2::CENTER_CENTER,
        spin.unit,
        egui::FontId::proportional(12.0),
        Color32::from_gray(60),
    );
    let unit_resp = ui.interact(
        unit,
        egui::Id::new(("tb-spin-unit", id)),
        egui::Sense::click(),
    );
    for (i, sign) in [(0, 1.0f64), (1, -1.0)] {
        let r = Rect::from_min_size(
            Pos2::new(
                buttons.left(),
                buttons.top() + i as f32 * buttons.height() / 2.0,
            ),
            Vec2::new(buttons.width(), buttons.height() / 2.0),
        );
        let resp = ui.interact(r, egui::Id::new(("tb-spin", id, i)), egui::Sense::click());
        if resp.hovered() {
            p.rect_filled(r, 0.0, Color32::from_gray(232));
        }
        let c = r.center();
        let s = Stroke::new(1.0, Color32::from_gray(80));
        let d = 1.8 * sign as f32;
        p.line_segment([c + Vec2::new(-3.0, d), c + Vec2::new(0.0, -d)], s);
        p.line_segment([c + Vec2::new(0.0, -d), c + Vec2::new(3.0, d)], s);
        if resp.clicked() {
            *spin.value =
                (*spin.value + sign * spin.step).clamp(*spin.range.start(), *spin.range.end());
            changed = true;
        }
    }
    p.line_segment(
        [
            Pos2::new(buttons.left(), rect.top() + 1.0),
            Pos2::new(buttons.left(), rect.bottom() - 1.0),
        ],
        Stroke::new(1.0, FIELD_BORDER),
    );
    if !spin.presets.is_empty() {
        egui::Popup::from_toggle_button_response(&unit_resp)
            .id(egui::Id::new(("tb-spin-list", id)))
            .show(|ui| {
                ui.set_min_width(size.x);
                for v in spin.presets {
                    if ui
                        .selectable_label(
                            *spin.value == *v,
                            format!("{} {}", format(*v), spin.unit),
                        )
                        .clicked()
                    {
                        *spin.value = *v;
                        changed = true;
                    }
                }
            });
    }
    (changed, response.union(edit))
}
fn lines(p: &egui::Painter, at: Pos2, width: f32, count: usize) {
    for i in 0..count {
        let y = at.y + i as f32 * 8.0;
        p.line_segment(
            [Pos2::new(at.x, y), Pos2::new(at.x + width, y)],
            Stroke::new(1.5, Color32::from_gray(110)),
        );
    }
}
/// Hangul's 96×96 shape grid: item 0 is (없음), drawn as a crossed box.
fn gallery(
    ui: &mut egui::Ui,
    id: &str,
    selected: &mut usize,
    count: usize,
    rows_visible: usize,
    paint: impl Fn(&egui::Painter, Rect, usize),
) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(96.0 * rows_visible as f32 + 8.0)
        .show(ui, |ui| {
            egui::Grid::new(id).spacing([0.0, 0.0]).show(ui, |ui| {
                for i in 0..count {
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::splat(96.0), egui::Sense::click());
                    let p = ui.painter();
                    if *selected == i {
                        p.rect(
                            rect.shrink(1.0),
                            0.0,
                            Color32::from_gray(220),
                            Stroke::new(1.0, Color32::from_gray(150)),
                            egui::StrokeKind::Inside,
                        );
                    } else if resp.hovered() {
                        p.rect_filled(rect.shrink(1.0), 0.0, Color32::from_gray(242));
                    }
                    if i == 0 {
                        let r = Rect::from_center_size(rect.center(), Vec2::splat(34.0));
                        p.rect_stroke(
                            r,
                            3.0,
                            Stroke::new(1.5, Color32::from_gray(110)),
                            egui::StrokeKind::Inside,
                        );
                        p.line_segment(
                            [r.left_bottom(), r.right_top()],
                            Stroke::new(2.0, Color32::from_rgb(190, 40, 40)),
                        );
                    } else {
                        paint(p, rect, i);
                    }
                    if resp.clicked() {
                        *selected = i;
                    }
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
        });
}
fn big_button(
    ui: &mut egui::Ui,
    size: Vec2,
    label: &str,
    icon: impl FnOnce(&egui::Painter, Rect),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 2.0, Color32::from_gray(240));
    }
    icon(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.center().x, rect.top() + 20.0),
            Vec2::new(24.0, 22.0),
        ),
    );
    ui.painter().text(
        Pos2::new(rect.center().x, rect.top() + 48.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        Color32::from_gray(30),
    );
    ui.painter().text(
        Pos2::new(rect.center().x, rect.top() + 62.0),
        egui::Align2::CENTER_CENTER,
        "∨",
        egui::FontId::proportional(9.0),
        Color32::from_gray(90),
    );
    response
}
fn color_menu(ui: &mut egui::Ui, title: &str, none: Option<&str>) -> Option<Option<String>> {
    ui.label(title);
    format_dialogs::palette(ui, None, none)
}
