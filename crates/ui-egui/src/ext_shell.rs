//! Owner: shell/page area. 메뉴·리본·보기·쪽·파일 서비스 화면.
//! Other areas must not edit this file; request changes through the owner.
//! Dialog arrangement follows Hangul-style conventions.
use super::*;
use chrome::Tokens;

/// Hangul's zoom range is 10–500 %; pages are rendered at 1.25× and scaled.
pub(crate) const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.1..=5.0;
/// Fixed chrome around the page view (menu tabs, ribbon, tool bar, ruler, tabs, status bar).
const CHROME_HEIGHT: f32 = 33.0 + 88.0 + 32.0 + 28.0 + 29.0 + 25.0;
/// Same order as `NUMBER_FORMATS` in crates/engine/src/ext_page.rs.
const NUMBER_FORMATS: &[(u8, &str)] = &[
    (0, "1,2,3"),
    (1, "①,②,③"),
    (2, "I,II,III"),
    (3, "i,ii,iii"),
    (4, "A,B,C"),
    (5, "a,b,c"),
    (8, "가,나,다"),
    (10, "ㄱ,ㄴ,ㄷ"),
    (12, "일,이,삼"),
    (13, "一,二,三"),
];
/// 머리말/꼬리말 마당 (RHWP template ids 0–10): name, left, centre, right text.
const TEMPLATES: &[(&str, &str, &str, &str)] = &[
    ("(모양 없음)", "", "", ""),
    ("왼쪽 쪽 번호", "1", "", ""),
    ("가운데 쪽 번호", "", "1", ""),
    ("오른쪽 쪽 번호", "", "", "1"),
    ("쪽 번호 · 파일 이름", "1", "", "파일 이름"),
    ("파일 이름 · 쪽 번호", "파일 이름", "", "1"),
    ("왼쪽 쪽 번호 (굵게·밑줄)", "1", "", ""),
    ("가운데 쪽 번호 (굵게·밑줄)", "", "1", ""),
    ("오른쪽 쪽 번호 (굵게·밑줄)", "", "", "1"),
    ("쪽 번호 · 파일 이름 (굵게·밑줄)", "1", "", "파일 이름"),
    ("파일 이름 · 쪽 번호 (굵게·밑줄)", "파일 이름", "", "1"),
];
const LINE_KINDS: &[(u8, &str)] = &[
    (0, "없음"),
    (1, "실선"),
    (2, "파선"),
    (3, "점선"),
    (4, "일점 쇄선"),
    (5, "이점 쇄선"),
    (6, "긴 파선"),
    (7, "원형 점선"),
    (8, "이중 실선"),
    (9, "얇고 굵은 이중선"),
    (10, "굵고 얇은 이중선"),
    (11, "얇고 굵고 얇은 삼중선"),
    (12, "물결"),
    (13, "이중 물결"),
];
const LINE_WIDTHS: &[f32] = &[
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Line {
    kind: u8,
    width: u8,
    color: Color32,
}
impl Line {
    const NONE: Self = Self {
        kind: 0,
        width: 0,
        color: Color32::BLACK,
    };
}

pub(crate) enum Dialog {
    PageNumber {
        position: &'static str,
        format: u8,
        dash: bool,
        start: u16,
    },
    HeaderFooter {
        header: bool,
        apply: usize,
        template: usize,
    },
    BorderFill {
        tab: usize,
        page_basis: bool,
        spacing: [f64; 4],
        header_inside: bool,
        footer_inside: bool,
        except_first: bool,
        whole_document: bool,
        /// left, right, top, bottom
        sides: [Line; 4],
        pen: Line,
        fill: Option<Color32>,
        fill_area: usize,
    },
    MasterPage {
        kind: usize,
        list: serde_json::Value,
    },
    Print {
        range: usize,
        pages: String,
        copies: u32,
        to_file: bool,
    },
    DocInfo {
        tab: usize,
        info: serde_json::Value,
    },
    NewNumber {
        start: u16,
    },
    Hide {
        flags: [bool; 6],
    },
}

/// Dialog and tool state owned by this module. Stored in `HwpApp::ext.shell`.
pub(crate) struct State {
    pub show_status_bar: bool,
    pub show_doc_tabs: bool,
    pub rulers: super::rulers::RulerCache,
    /// Page rectangle the rulers were last drawn against.
    pub ruler_page: Option<Rect>,
    pub dialog: Option<Dialog>,
    /// First key of a Hangul two-key shortcut (Ctrl+N, Ctrl+G, Ctrl+Q).
    chord: Option<char>,
    /// Selected object kind last frame, to open its context tab once.
    pub object_kind: Option<&'static str>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            show_status_bar: true,
            show_doc_tabs: true,
            rulers: Default::default(),
            ruler_page: None,
            dialog: None,
            chord: None,
            object_kind: None,
        }
    }
}

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}
fn parse_hex(v: &serde_json::Value) -> Option<Color32> {
    let s = v.as_str()?.strip_prefix('#')?;
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

impl HwpApp {
    /// `Some(enabled)` when this module owns the ribbon/menu command id.
    pub(crate) fn shell_ext_enabled(&self, id: &str) -> Option<bool> {
        let body = self.session.cursor.cell.is_none();
        Some(match id {
            "view.zoom_100"
            | "view.zoom_fit_width"
            | "view.zoom_fit_page"
            | "view.zoom_in"
            | "view.zoom_out"
            | "view.status_bar"
            | "view.doc_tabs"
            | "view.rulers" => true,
            "page.number" | "page.new_number" | "page.hide" => body,
            "page.header" | "page.footer" | "page.border_fill" | "page.master" | "file.print"
            | "file.doc_info" | "file.exit" | "page.setup_dialog" | "page.margins" => true,
            _ => return None,
        })
    }
    /// Check state of toggle commands shown as check boxes or pressed buttons.
    pub(crate) fn shell_ext_checked(&self, id: &str) -> bool {
        match id {
            "view.status_bar" => self.ext.shell.show_status_bar,
            "view.doc_tabs" => self.ext.shell.show_doc_tabs,
            "view.rulers" => self.show_rulers,
            _ => false,
        }
    }
    /// Return true when the command id was handled.
    pub(crate) fn shell_ext_dispatch(&mut self, id: &str, ctx: &egui::Context) -> bool {
        match id {
            "view.zoom_100" => self.set_zoom(1.0),
            "view.zoom_fit_width" => self.zoom_to_fit(true),
            "view.zoom_fit_page" => self.zoom_to_fit(false),
            "view.zoom_in" => self.set_zoom(self.zoom + 0.1),
            "view.zoom_out" => self.set_zoom(self.zoom - 0.1),
            "view.status_bar" => self.ext.shell.show_status_bar ^= true,
            "view.doc_tabs" => self.ext.shell.show_doc_tabs ^= true,
            "view.rulers" => self.show_rulers ^= true,
            "page.setup_dialog" | "page.margins" => self.open_page_setup(),
            "file.exit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {
                let Some(dialog) = self.open_shell_dialog(id) else {
                    return false;
                };
                self.ext.shell.dialog = Some(dialog);
            }
        }
        true
    }
    fn open_shell_dialog(&mut self, id: &str) -> Option<Dialog> {
        Some(match id {
            "page.number" => {
                let v = self.session.run("page.number.get", json!({})).ok()?;
                Dialog::PageNumber {
                    position: match v["position"].as_str() {
                        Some("top_left") => "top_left",
                        Some("top_center") => "top_center",
                        Some("top_right") => "top_right",
                        Some("bottom_left") => "bottom_left",
                        Some("bottom_right") => "bottom_right",
                        Some("outside_top") => "outside_top",
                        Some("outside_bottom") => "outside_bottom",
                        Some("inside_top") => "inside_top",
                        Some("inside_bottom") => "inside_bottom",
                        Some("none") => "none",
                        _ => "bottom_center",
                    },
                    format: v["format"].as_u64().unwrap_or(0) as u8,
                    dash: v["dash"].as_bool().unwrap_or(true),
                    start: v["start"].as_u64().unwrap_or(0).max(1) as u16,
                }
            }
            "page.header" | "page.footer" => Dialog::HeaderFooter {
                header: id == "page.header",
                apply: 0,
                template: 0,
            },
            "page.border_fill" => {
                let v = self.session.run("page.border_fill.get", json!({})).ok()?;
                let line = |k: &str| Line {
                    kind: v[k]["type"].as_u64().unwrap_or(0) as u8,
                    width: v[k]["width"].as_u64().unwrap_or(0) as u8,
                    color: parse_hex(&v[k]["color"]).unwrap_or(Color32::BLACK),
                };
                let mm = |i: usize| v["spacingMm"][i].as_f64().unwrap_or(5.0);
                Dialog::BorderFill {
                    tab: 0,
                    page_basis: v["basis"] == "page",
                    spacing: [mm(0), mm(1), mm(2), mm(3)],
                    header_inside: v["headerInside"].as_bool().unwrap_or(false),
                    footer_inside: v["footerInside"].as_bool().unwrap_or(false),
                    except_first: v["hideBorder"].as_bool().unwrap_or(false),
                    whole_document: false,
                    sides: [
                        line("borderLeft"),
                        line("borderRight"),
                        line("borderTop"),
                        line("borderBottom"),
                    ],
                    pen: Line {
                        kind: 1,
                        width: 0,
                        color: Color32::BLACK,
                    },
                    fill: (v["fillType"] == "solid")
                        .then(|| parse_hex(&v["fillColor"]))
                        .flatten(),
                    fill_area: match v["fillArea"].as_str() {
                        Some("page") => 1,
                        Some("border") => 2,
                        _ => 0,
                    },
                }
            }
            "page.master" => Dialog::MasterPage {
                kind: 0,
                list: self
                    .session
                    .run("page.master.list", json!({}))
                    .unwrap_or_default(),
            },
            "file.print" => Dialog::Print {
                range: 0,
                pages: String::new(),
                copies: 1,
                to_file: false,
            },
            "file.doc_info" => Dialog::DocInfo {
                tab: 2,
                info: self.session.run("file.doc_info", json!({})).ok()?,
            },
            "page.new_number" => Dialog::NewNumber { start: 1 },
            "page.hide" => {
                let v = self.session.run("page.hide.get", json!({})).ok()?;
                let b = |k: &str| v[k].as_bool().unwrap_or(false);
                Dialog::Hide {
                    flags: [
                        b("hideHeader"),
                        b("hideFooter"),
                        b("hideMasterPage"),
                        b("hideBorder"),
                        b("hideFill"),
                        b("hidePageNum"),
                    ],
                }
            }
            _ => return None,
        })
    }

    /// Called every frame after the built-in dialogs.
    pub(crate) fn shell_ext_dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.ext.shell.dialog.take() else {
            return;
        };
        let keep = match &mut dialog {
            Dialog::PageNumber {
                position,
                format,
                dash,
                start,
            } => self.page_number_dialog(ctx, position, format, dash, start),
            Dialog::HeaderFooter {
                header,
                apply,
                template,
            } => self.header_footer_dialog(ctx, header, apply, template),
            Dialog::BorderFill { .. } => self.border_fill_dialog(ctx, &mut dialog),
            Dialog::MasterPage { kind, list } => master_page_dialog(ctx, kind, list),
            Dialog::Print {
                range,
                pages,
                copies,
                to_file,
            } => self.print_dialog(ctx, range, pages, copies, to_file),
            Dialog::DocInfo { tab, info } => doc_info_dialog(ctx, tab, info),
            Dialog::NewNumber { start } => self.new_number_dialog(ctx, start),
            Dialog::Hide { flags } => self.hide_dialog(ctx, flags),
        };
        if keep && self.ext.shell.dialog.is_none() {
            self.ext.shell.dialog = Some(dialog);
        }
    }

    /// Hangul two-key shortcuts and file/view keys; true when consumed.
    pub(crate) fn shell_ext_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        let ctrl = Modifiers::CTRL;
        if let Some(prefix) = self.ext.shell.chord {
            let pressed = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => Some(*key),
                    _ => None,
                })
            });
            let Some(key) = pressed else {
                return false;
            };
            ctx.input_mut(|i| i.events.clear());
            self.ext.shell.chord = None;
            self.message.clear();
            let id = match (prefix, key) {
                ('N', Key::P) => "page.number",
                ('N', Key::H) => "page.header",
                ('N', Key::T) => "table.create",
                ('G', Key::C) => "view.control_codes",
                ('G', Key::T) => "view.paragraph_marks",
                ('G', Key::L) => "view.page_outline",
                ('G', Key::Q) => "view.zoom_100",
                ('G', Key::P) => "view.zoom_fit_page",
                ('G', Key::W) => "view.zoom_fit_width",
                ('Q', Key::I) | ('Q', Key::D) => "file.doc_info",
                _ => return true,
            };
            if self.ext_enabled(id) {
                self.ext_dispatch(id, ctx);
            } else if id == "table.create" {
                self.show_table = true;
            } else {
                self.message = format!("{id}: 아직 지원하지 않는 명령입니다.");
            }
            return true;
        }
        for (key, prefix, hint) in [
            (Key::N, 'N', "Ctrl+N, (P 쪽 번호 · H 머리말/꼬리말 · T 표)"),
            (
                Key::G,
                'G',
                "Ctrl+G, (C 조판 부호 · L 쪽 윤곽 · Q 100% · P 쪽 맞춤 · W 폭 맞춤)",
            ),
            (Key::Q, 'Q', "Ctrl+Q, (I 문서 정보)"),
        ] {
            if ctx.input_mut(|i| i.consume_key(ctrl, key)) {
                self.ext.shell.chord = Some(prefix);
                self.message = hint.into();
                return true;
            }
        }
        let alt = Modifiers::ALT;
        let shortcuts: [(Modifiers, Key, &str); 9] = [
            (Modifiers::NONE, Key::F7, "page.setup_dialog"),
            (ctrl, Key::P, "file.print"),
            (alt, Key::P, "file.print"),
            (alt, Key::X, "file.exit"),
            (ctrl, Key::F1, "view.ribbon"),
            (alt, Key::N, "file.new"),
            (alt, Key::O, "file.open"),
            (alt, Key::S, "file.save"),
            (alt, Key::V, "file.save_as"),
        ];
        for (modifiers, key, id) in shortcuts {
            if !ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                continue;
            }
            match id {
                "view.ribbon" => self.show_ribbon ^= true,
                "file.new" => self.request(Pending::New, ctx),
                "file.open" => self.open(ctx),
                "file.save" => {
                    self.save(false, None);
                }
                "file.save_as" => {
                    self.save(true, None);
                }
                _ => {
                    self.shell_ext_dispatch(id, ctx);
                }
            }
            return true;
        }
        false
    }
    /// Start-up `--view` names owned by this module.
    pub(crate) fn shell_ext_view(&mut self, name: &str) -> bool {
        let id = match name {
            "page-number" => "page.number",
            "header-footer" => "page.header",
            "page-border" => "page.border_fill",
            "master-page" => "page.master",
            "print" => "file.print",
            "doc-info" => "file.doc_info",
            "new-number" => "page.new_number",
            "page-hide" => "page.hide",
            "view" | "format" | "security" | "review" | "tools" => {
                self.tab = match name {
                    "view" => 2,
                    "format" => 4,
                    "security" => 6,
                    "review" => 7,
                    _ => 8,
                };
                return true;
            }
            _ => return false,
        };
        let ctx = self.ctx.clone();
        self.shell_ext_dispatch(id, &ctx)
    }
    /// True while a shell dialog must block document typing and shortcuts.
    pub(crate) fn shell_ext_modal_open(&self) -> bool {
        self.ext.shell.dialog.is_some()
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32) {
        // Snap to whole percents like Hangul's zoom box.
        self.zoom = (zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()) * 100.0).round() / 100.0;
    }
    /// 폭 맞춤 / 쪽 맞춤 against the current window and the cursor's page.
    pub(crate) fn zoom_to_fit(&mut self, width_only: bool) {
        let screen = self.ctx.content_rect();
        let side = if self.show_navigation { 220.0 } else { 0.0 };
        let width = screen.width() - side - 28.0 - 15.0 - 40.0;
        let height = screen.height() - CHROME_HEIGHT - 30.0;
        let sizes = self.page_sizes();
        let Some(page) = sizes.get(self.page as usize).or(sizes.first()).copied() else {
            return;
        };
        if page.x <= 0.0 || page.y <= 0.0 {
            return;
        }
        let fit = width / page.x;
        self.set_zoom(if width_only {
            fit
        } else {
            fit.min(height / page.y)
        });
    }

    // ----- 쪽 번호 매기기 (550×500) -----
    fn page_number_dialog(
        &mut self,
        ctx: &egui::Context,
        position: &mut &'static str,
        format: &mut u8,
        dash: &mut bool,
        start: &mut u16,
    ) -> bool {
        let sample = {
            let n = NUMBER_FORMATS
                .iter()
                .find(|(c, _)| c == format)
                .map(|(_, l)| l.split(',').next().unwrap_or("1"))
                .unwrap_or("1")
                .to_owned();
            if *dash { format!("-{n}-") } else { n }
        };
        let button = dialog(
            ctx,
            "쪽 번호 매기기",
            550.0,
            &["넣기(D)", "취소"],
            |ui| {
                group(ui, "번호 위치");
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(390.0, 200.0), egui::Sense::hover());
                let single =
                    Rect::from_min_size(area.min + Vec2::new(14.0, 26.0), Vec2::new(117.0, 149.0));
                let spread =
                    Rect::from_min_size(area.min + Vec2::new(151.0, 26.0), Vec2::new(240.0, 149.0));
                let p = ui.painter();
                p.rect_stroke(
                    single,
                    0.0,
                    Stroke::new(1.0, Color32::BLACK),
                    egui::StrokeKind::Inside,
                );
                p.rect_stroke(
                    spread,
                    0.0,
                    Stroke::new(1.0, Color32::from_gray(150)),
                    egui::StrokeKind::Inside,
                );
                p.line_segment(
                    [spread.center_top(), spread.center_bottom()],
                    Stroke::new(1.0, Color32::from_gray(150)),
                );
                let slot = |pos: &str| -> Option<Pos2> {
                    let (x, top) = match pos {
                        "top_left" => (single.left() + 10.0, true),
                        "top_center" => (single.center().x, true),
                        "top_right" => (single.right() - 10.0, true),
                        "bottom_left" => (single.left() + 10.0, false),
                        "bottom_center" => (single.center().x, false),
                        "bottom_right" => (single.right() - 10.0, false),
                        _ => return None,
                    };
                    Some(Pos2::new(
                        x,
                        if top {
                            single.top() + 12.0
                        } else {
                            single.bottom() - 12.0
                        },
                    ))
                };
                if let Some(at) = slot(position) {
                    p.text(
                        at,
                        egui::Align2::CENTER_CENTER,
                        &sample,
                        egui::FontId::proportional(11.0),
                        Color32::BLACK,
                    );
                }
                let below = single.bottom() + 5.0;
                let radios: [(&str, &'static str, Pos2); 10] = [
                    ("", "top_left", Pos2::new(single.left(), area.top())),
                    (
                        "",
                        "top_center",
                        Pos2::new(single.center().x - 7.0, area.top()),
                    ),
                    (
                        "",
                        "top_right",
                        Pos2::new(single.right() - 14.0, area.top()),
                    ),
                    (
                        "안쪽 위",
                        "inside_top",
                        Pos2::new(spread.left(), area.top()),
                    ),
                    (
                        "바깥쪽 위",
                        "outside_top",
                        Pos2::new(spread.left() + 123.0, area.top()),
                    ),
                    ("", "bottom_left", Pos2::new(single.left(), below)),
                    (
                        "",
                        "bottom_center",
                        Pos2::new(single.center().x - 7.0, below),
                    ),
                    ("", "bottom_right", Pos2::new(single.right() - 14.0, below)),
                    (
                        "안쪽 아래",
                        "inside_bottom",
                        Pos2::new(spread.left(), below),
                    ),
                    (
                        "바깥쪽 아래",
                        "outside_bottom",
                        Pos2::new(spread.left() + 123.0, below),
                    ),
                ];
                for (label, value, at) in radios {
                    let width = if label.is_empty() { 16.0 } else { 100.0 };
                    let r = Rect::from_min_size(at, Vec2::new(width, 18.0));
                    if ui
                        .put(r, egui::RadioButton::new(*position == value, label))
                        .clicked()
                    {
                        *position = value;
                    }
                }
                ui.radio_value(position, "none", "쪽 번호 없음");
                group(ui, "번호 모양");
                ui.horizontal(|ui| {
                    ui.label("번호 모양");
                    let selected = NUMBER_FORMATS
                        .iter()
                        .find(|(c, _)| c == format)
                        .map(|(_, l)| *l)
                        .unwrap_or("1,2,3");
                    egui::ComboBox::from_id_salt("page-number-format")
                        .width(91.0)
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for (code, label) in NUMBER_FORMATS {
                                ui.selectable_value(format, *code, *label);
                            }
                        });
                    ui.checkbox(dash, "줄표 넣기");
                });
                ui.horizontal(|ui| {
                    ui.label("시작 번호");
                    ui.add(egui::DragValue::new(start).range(1..=9999));
                });
                ui.add_space(6.0);
                note(
                    ui,
                    "쪽 번호의 글자 모양은 문서 스타일 \"쪽 번호\"를 따릅니다. 번호 위치는 현재 구역에 적용됩니다.",
                );
            },
        );
        match button {
            Some(0) => !self.run(
                "page.number",
                json!({"position":*position,"format":*format,"dash":*dash,"start":*start}),
            ),
            Some(_) => false,
            None => true,
        }
    }

    // ----- 머리말/꼬리말 (696×491) -----
    fn header_footer_dialog(
        &mut self,
        ctx: &egui::Context,
        header: &mut bool,
        apply: &mut usize,
        template: &mut usize,
    ) -> bool {
        let button = dialog(
            ctx,
            "머리말/꼬리말",
            696.0,
            &["만들기(D)", "취소"],
            |ui| {
                group(ui, "종류");
                ui.horizontal(|ui| {
                    ui.radio_value(header, true, "머리말");
                    ui.add_space(14.0);
                    ui.radio_value(header, false, "꼬리말");
                });
                group(ui, "위치");
                ui.horizontal(|ui| {
                    for (i, label) in ["양쪽", "홀수 쪽", "짝수 쪽"].iter().enumerate() {
                        ui.radio_value(apply, i, *label);
                        ui.add_space(14.0);
                    }
                });
                group(ui, "머리말/꼬리말 마당");
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label("목록");
                        egui::Frame::NONE
                            .stroke(Stroke::new(1.0, Tokens::BORDER))
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("hf-templates")
                                    .max_height(236.0)
                                    .show(ui, |ui| template_list(ui, template));
                            });
                    });
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label("미리 보기");
                        template_preview(ui, *header, *template);
                    });
                });
                note(
                    ui,
                    "(모양 없음)·양쪽을 만들면 이어서 한 줄 내용을 입력합니다.",
                );
            },
        );
        match button {
            Some(0) => {
                let id = if *header {
                    "page.header"
                } else {
                    "page.footer"
                };
                let apply_to = ["both", "odd", "even"][(*apply).min(2)];
                if !self.run(id, json!({"apply_to":apply_to,"template":*template})) {
                    return true;
                }
                if *template == 0 && *apply == 0 {
                    self.feature_dialog =
                        Some(super::feature_dialogs::FeatureDialog::HeaderFooter {
                            is_header: *header,
                            text: String::new(),
                        });
                }
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    // ----- 쪽 테두리/배경 (557×774) -----
    fn border_fill_dialog(&mut self, ctx: &egui::Context, d: &mut Dialog) -> bool {
        let Dialog::BorderFill {
            tab,
            page_basis,
            spacing,
            header_inside,
            footer_inside,
            except_first,
            whole_document,
            sides,
            pen,
            fill,
            fill_area,
        } = d
        else {
            return false;
        };
        let button = dialog(
            ctx,
            "쪽 테두리/배경",
            557.0,
            &["설정(D)", "취소"],
            |ui| {
                chrome::tabs(ui, tab, &["테두리", "배경"]);
                group(ui, "테두리/배경 종류");
                ui.horizontal(|ui| {
                    let _ = ui.radio(true, "양쪽");
                    ui.add_enabled(false, egui::RadioButton::new(false, "홀수 쪽"))
                        .on_disabled_hover_text(
                            "RHWP 문서 모델은 구역마다 쪽 테두리/배경을 하나만 저장합니다.",
                        );
                    ui.add_enabled(false, egui::RadioButton::new(false, "짝수 쪽"));
                });
                if *tab == 0 {
                    border_tab(
                        ui,
                        sides,
                        pen,
                        page_basis,
                        spacing,
                        header_inside,
                        footer_inside,
                        except_first,
                    );
                } else {
                    group(ui, "채우기");
                    let mut on = fill.is_some();
                    let mut color = fill.unwrap_or(Color32::from_rgb(255, 250, 230));
                    ui.horizontal(|ui| {
                        if ui.radio(!on, "색 채우기 없음").clicked() {
                            on = false;
                        }
                        if ui.radio(on, "색").clicked() {
                            on = true;
                        }
                        ui.add_enabled_ui(on, |ui| ui.color_edit_button_srgba(&mut color));
                    });
                    *fill = on.then_some(color);
                    group(ui, "채울 영역");
                    ui.horizontal(|ui| {
                        for (i, l) in ["종이", "쪽", "테두리"].iter().enumerate() {
                            ui.radio_value(fill_area, i, *l);
                        }
                    });
                    note(ui, "그림 채우기와 그러데이션은 아직 지원하지 않습니다.");
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("적용 범위");
                    egui::ComboBox::from_id_salt("border-scope")
                        .width(150.0)
                        .selected_text(if *whole_document {
                            "문서 전체"
                        } else {
                            "현재 구역"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(whole_document, false, "현재 구역");
                            ui.selectable_value(whole_document, true, "문서 전체");
                        });
                });
            },
        );
        match button {
            Some(0) => {
                let line =
                    |l: &Line| json!({"type": l.kind, "width": l.width, "color": hex(l.color)});
                !self.run(
                    "page.border_fill",
                    json!({
                        "basis": if *page_basis { "page" } else { "paper" },
                        "spacingMm": spacing,
                        "headerInside": header_inside,
                        "footerInside": footer_inside,
                        "applyPage": if *except_first { "exceptFirst" } else { "all" },
                        "borders": sides.iter().map(line).collect::<Vec<_>>(),
                        "fill": fill.map(hex),
                        "fillArea": (["paper", "page", "border"][(*fill_area).min(2)]),
                        "wholeDocument": whole_document,
                    }),
                )
            }
            Some(_) => false,
            None => true,
        }
    }

    // ----- 인쇄 (600×806) -----
    fn print_dialog(
        &mut self,
        ctx: &egui::Context,
        range: &mut usize,
        pages: &mut String,
        copies: &mut u32,
        to_file: &mut bool,
    ) -> bool {
        let count = self.session.core.page_count();
        let button = dialog(ctx, "인쇄", 600.0, &["인쇄(D)", "취소"], |ui| {
            chrome::tabs(ui, &mut 0, &["기본"]);
            ui.label("프린터 선택");
            ui.horizontal(|ui| {
                ui.radio_value(to_file, false, "Windows 기본 프린터");
                ui.radio_value(to_file, true, "PDF 저장");
            });
            note(
                ui,
                "선택한 쪽을 PDF로 만든 뒤 Windows 인쇄로 보냅니다. 프린터는 Windows 기본 프린터를 씁니다.",
            );
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(232.0);
                    group(ui, "인쇄 범위");
                    egui::Grid::new("print-range")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.radio_value(range, 0, "모두");
                            ui.end_row();
                            ui.radio_value(range, 1, "현재 쪽");
                            ui.radio_value(range, 3, "현재까지");
                            ui.end_row();
                            ui.add_enabled(false, egui::RadioButton::new(false, "현재 구역"));
                            ui.radio_value(range, 4, "현재부터");
                            ui.end_row();
                            ui.radio_value(range, 2, "일부분");
                            ui.end_row();
                        });
                    ui.add_enabled(
                        *range == 2,
                        egui::TextEdit::singleline(pages)
                            .desired_width(91.0)
                            .hint_text("1,3,5-7"),
                    );
                    ui.label(RichText::new(format!("예) 1,3,5-7 · 전체 {count}쪽")).size(11.0));
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    group(ui, "인쇄 매수");
                    ui.horizontal(|ui| {
                        ui.label("매수");
                        ui.add(egui::DragValue::new(copies).range(1..=99));
                    });
                });
            });
        });
        match button {
            Some(0) => self.print(*range, pages, *copies, *to_file),
            Some(_) => false,
            None => true,
        }
    }
    /// Returns true to keep the dialog open (cancelled save or failure).
    fn print(&mut self, range: usize, pages: &str, copies: u32, to_file: bool) -> bool {
        let range_name = ["all", "current", pages, "to_current", "from_current"][range.min(4)];
        let target = if to_file {
            match rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("인쇄.pdf")
                .save_file()
            {
                Some(p) => p,
                None => return true,
            }
        } else {
            std::env::temp_dir().join(format!("hwpcraft-print-{}.pdf", std::process::id()))
        };
        let r = self.session.run(
            "file.print_pdf",
            json!({"path": target, "pages": range_name, "current": self.page}),
        );
        let Some(done) = self.result(r) else {
            return true;
        };
        let count = done["pages"].as_array().map_or(0, Vec::len);
        if to_file {
            self.message = format!("{count}쪽을 PDF로 저장했습니다 · {}", target.display());
            return false;
        }
        let mut sent = 0;
        for _ in 0..copies {
            // The Windows shell "print" verb of the default PDF handler.
            let status = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    "Start-Process -FilePath $args[0] -Verb Print",
                ])
                .arg(&target)
                .status();
            if status.is_ok_and(|s| s.success()) {
                sent += 1;
            }
        }
        if sent == 0 {
            self.error = Some(
                "Windows에 PDF 인쇄 프로그램이 없어 인쇄하지 못했습니다. 'PDF 저장'으로 저장한 뒤 인쇄하세요."
                    .into(),
            );
            return true;
        }
        self.message = format!("{count}쪽을 {sent}부 인쇄로 보냈습니다.");
        false
    }

    // ----- 새 번호로 시작 / 현재 쪽만 감추기 -----
    fn new_number_dialog(&mut self, ctx: &egui::Context, start: &mut u16) -> bool {
        let button = dialog(
            ctx,
            "새 번호로 시작",
            360.0,
            &["넣기(D)", "취소"],
            |ui| {
                group(ui, "번호 종류");
                let _ = ui.radio(true, "쪽 번호");
                group(ui, "시작 번호");
                ui.horizontal(|ui| {
                    ui.label("시작 번호");
                    ui.add(egui::DragValue::new(start).range(1..=9999));
                });
            },
        );
        match button {
            Some(0) => !self.run("page.new_number", json!({"start":*start})),
            Some(_) => false,
            None => true,
        }
    }
    fn hide_dialog(&mut self, ctx: &egui::Context, flags: &mut [bool; 6]) -> bool {
        let button = dialog(ctx, "감추기", 360.0, &["설정(D)", "취소"], |ui| {
            group(ui, "감출 내용");
            for (flag, label) in
                flags
                    .iter_mut()
                    .zip(["머리말", "꼬리말", "바탕쪽", "테두리", "배경", "쪽 번호"])
            {
                ui.checkbox(flag, label);
            }
            note(ui, "현재 쪽에만 적용됩니다.");
        });
        match button {
            Some(0) => !self.run(
                "page.hide",
                json!({"header":flags[0],"footer":flags[1],"master_page":flags[2],
                    "border":flags[3],"fill":flags[4],"page_number":flags[5]}),
            ),
            Some(_) => false,
            None => true,
        }
    }
}

/// Hangul dialog frame: title bar, content on the left and 80×28 command
/// buttons on the right. Returns the clicked button; closing counts as the last.
pub(crate) fn dialog(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    buttons: &[&str],
    body: impl FnOnce(&mut egui::Ui),
) -> Option<usize> {
    let mut clicked = None;
    let response = chrome::modal(("hwp-shell-dialog", title)).show(ctx, |ui| {
        ui.set_width(width - 24.0);
        if chrome::title(ui, title) {
            clicked = Some(buttons.len() - 1);
        }
        ui.horizontal_top(|ui| {
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, Color32::from_gray(227)))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.set_width(width - 24.0 - 120.0);
                    ui.vertical(body);
                });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                for (i, label) in buttons.iter().enumerate() {
                    let (width, color) = if i == 0 {
                        (1.5, Tokens::ACCENT)
                    } else {
                        (1.0, Tokens::BORDER)
                    };
                    let b = egui::Button::new(*label)
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(width, color))
                        .min_size(Vec2::new(80.0, 28.0));
                    if ui.add(b).clicked() {
                        clicked = Some(i);
                    }
                    ui.add_space(4.0);
                }
            });
        });
    });
    if response.should_close() && clicked.is_none() {
        clicked = Some(buttons.len() - 1);
    }
    clicked
}

/// Group heading with Hangul's trailing rule.
pub(crate) fn group(ui: &mut egui::Ui, label: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong());
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1.0, Color32::from_gray(227)),
        );
    });
    ui.add_space(4.0);
}
pub(crate) fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.0)
            .color(Color32::from_gray(100)),
    );
}

fn template_list(ui: &mut egui::Ui, template: &mut usize) {
    ui.set_width(318.0);
    for (i, (name, l, c, r)) in TEMPLATES.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(314.0, 44.0), egui::Sense::click());
        if *template == i {
            ui.painter().rect_filled(rect, 0.0, Color32::from_gray(222));
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgb(229, 241, 251));
        }
        let font = egui::FontId::proportional(12.5);
        let ink = Color32::from_gray(30);
        if i == 0 {
            ui.painter()
                .text(rect.center(), egui::Align2::CENTER_CENTER, *name, font, ink);
        } else {
            for (text, align, x) in [
                (*l, egui::Align2::LEFT_CENTER, rect.left() + 12.0),
                (*c, egui::Align2::CENTER_CENTER, rect.center().x),
                (*r, egui::Align2::RIGHT_CENTER, rect.right() - 12.0),
            ] {
                let text = if text == "1" { "쪽 번호" } else { text };
                ui.painter().text(
                    Pos2::new(x, rect.center().y),
                    align,
                    text,
                    font.clone(),
                    ink,
                );
            }
        }
        if resp.on_hover_text(*name).clicked() {
            *template = i;
        }
    }
}
fn template_preview(ui: &mut egui::Ui, header: bool, template: usize) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(180.0, 230.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::from_gray(245));
    let page = rect.shrink(8.0);
    ui.painter().rect_filled(page, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(
        page,
        0.0,
        Stroke::new(1.0, Color32::from_gray(160)),
        egui::StrokeKind::Inside,
    );
    if let Some((_, l, c, r)) = TEMPLATES.get(template) {
        let y = if header {
            page.top() + 14.0
        } else {
            page.bottom() - 14.0
        };
        for (text, align, x) in [
            (*l, egui::Align2::LEFT_CENTER, page.left() + 10.0),
            (*c, egui::Align2::CENTER_CENTER, page.center().x),
            (*r, egui::Align2::RIGHT_CENTER, page.right() - 10.0),
        ] {
            ui.painter().text(
                Pos2::new(x, y),
                align,
                text,
                egui::FontId::proportional(9.0),
                Color32::BLACK,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn border_tab(
    ui: &mut egui::Ui,
    sides: &mut [Line; 4],
    pen: &mut Line,
    page_basis: &mut bool,
    spacing: &mut [f64; 4],
    header_inside: &mut bool,
    footer_inside: &mut bool,
    except_first: &mut bool,
) {
    group(ui, "테두리");
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            egui::Grid::new("border-pen")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("종류");
                    let kind = LINE_KINDS
                        .iter()
                        .find(|(k, _)| *k == pen.kind)
                        .map(|(_, l)| *l)
                        .unwrap_or("실선");
                    egui::ComboBox::from_id_salt("border-kind")
                        .width(127.0)
                        .selected_text(kind)
                        .show_ui(ui, |ui| {
                            for (k, l) in LINE_KINDS {
                                ui.selectable_value(&mut pen.kind, *k, *l);
                            }
                        });
                    ui.end_row();
                    ui.label("굵기");
                    egui::ComboBox::from_id_salt("border-width")
                        .width(127.0)
                        .selected_text(format!(
                            "{} mm",
                            LINE_WIDTHS[usize::from(pen.width).min(15)]
                        ))
                        .show_ui(ui, |ui| {
                            for (i, w) in LINE_WIDTHS.iter().enumerate() {
                                ui.selectable_value(&mut pen.width, i as u8, format!("{w} mm"));
                            }
                        });
                    ui.end_row();
                    ui.label("색");
                    ui.color_edit_button_srgba(&mut pen.color);
                    ui.end_row();
                });
            let mut none = sides.iter().all(|s| s.kind == 0);
            if ui.checkbox(&mut none, "테두리 사용 안 함").changed() {
                *sides = if none { [Line::NONE; 4] } else { [*pen; 4] };
            }
            note(ui, "미리 보기 옆 단추로 선을 넣거나 뺍니다.");
        });
        ui.add_space(20.0);
        border_preview(ui, sides, *pen);
    });
    group(ui, "위치");
    ui.horizontal(|ui| {
        ui.radio_value(page_basis, false, "종이 기준");
        ui.radio_value(page_basis, true, "쪽 기준");
    });
    egui::Grid::new("border-spacing")
        .num_columns(4)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for pair in [
                [("왼쪽", 0usize), ("위쪽", 2)],
                [("오른쪽", 1), ("아래쪽", 3)],
            ] {
                for (label, i) in pair {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut spacing[i])
                            .range(0.0..=100.0)
                            .speed(0.1)
                            .suffix(" mm"),
                    );
                }
                ui.end_row();
            }
        });
    ui.checkbox(header_inside, "머리말 포함");
    ui.checkbox(footer_inside, "꼬리말 포함");
    group(ui, "적용 쪽");
    ui.horizontal(|ui| {
        ui.radio_value(except_first, false, "모두");
        ui.radio_value(except_first, true, "첫 쪽 제외");
        ui.add_enabled(false, egui::RadioButton::new(false, "첫 쪽만"));
    });
}

/// 162×134 preview with 위/아래/왼쪽/오른쪽/모두 buttons, as in Hangul.
fn border_preview(ui: &mut egui::Ui, sides: &mut [Line; 4], pen: Line) {
    let (area, _) = ui.allocate_exact_size(Vec2::new(200.0, 168.0), egui::Sense::hover());
    let page = Rect::from_min_size(area.min, Vec2::new(162.0, 134.0));
    let p = ui.painter();
    p.rect_filled(page, 0.0, Color32::WHITE);
    for corner in [
        page.left_top(),
        page.right_top(),
        page.left_bottom(),
        page.right_bottom(),
    ] {
        p.circle_filled(corner, 1.5, Color32::from_gray(150));
    }
    let inner = page.shrink(14.0);
    let edges = [
        [inner.left_top(), inner.left_bottom()],
        [inner.right_top(), inner.right_bottom()],
        [inner.left_top(), inner.right_top()],
        [inner.left_bottom(), inner.right_bottom()],
    ];
    for (edge, side) in edges.iter().zip(sides.iter()) {
        let stroke = if side.kind > 0 {
            let w = (LINE_WIDTHS[usize::from(side.width).min(15)] * 3.0).max(1.0);
            Stroke::new(w, side.color)
        } else {
            Stroke::new(1.0, Color32::from_gray(225))
        };
        p.line_segment(*edge, stroke);
    }
    let s = Vec2::splat(24.0);
    let buttons: [(Pos2, &str, &[usize]); 5] = [
        (Pos2::new(page.right() + 6.0, page.top()), "위", &[2]),
        (
            Pos2::new(page.right() + 6.0, page.bottom() - 24.0),
            "아래",
            &[3],
        ),
        (Pos2::new(page.left(), page.bottom() + 6.0), "왼쪽", &[0]),
        (
            Pos2::new(page.right() - 24.0, page.bottom() + 6.0),
            "오른쪽",
            &[1],
        ),
        (
            Pos2::new(page.right() + 6.0, page.bottom() + 6.0),
            "모두",
            &[0, 1, 2, 3],
        ),
    ];
    for (at, tip, targets) in buttons {
        let rect = Rect::from_min_size(at, s);
        let r = ui
            .put(rect, egui::Button::new("").min_size(s))
            .on_hover_text(tip);
        let ink = Stroke::new(1.5, Color32::from_gray(80));
        let mark = rect.shrink(6.0);
        let edge = match tip {
            "위" => Some([mark.left_top(), mark.right_top()]),
            "아래" => Some([mark.left_bottom(), mark.right_bottom()]),
            "왼쪽" => Some([mark.left_top(), mark.left_bottom()]),
            "오른쪽" => Some([mark.right_top(), mark.right_bottom()]),
            _ => None,
        };
        match edge {
            Some(edge) => {
                ui.painter().line_segment(edge, ink);
            }
            None => {
                ui.painter()
                    .rect_stroke(mark, 0.0, ink, egui::StrokeKind::Middle);
            }
        }
        if r.clicked() {
            let on = targets.iter().any(|i| sides[*i] != pen);
            for i in targets {
                sides[*i] = if on { pen } else { Line::NONE };
            }
        }
    }
}

// ----- 바탕쪽 (391×369) -----
fn master_page_dialog(ctx: &egui::Context, kind: &mut usize, list: &serde_json::Value) -> bool {
    let button = dialog(ctx, "바탕쪽", 391.0, &["만들기(D)", "취소"], |ui| {
        group(ui, "종류");
        for (i, label) in [
            "양쪽",
            "홀수 쪽",
            "짝수 쪽",
            "현재 구역의 마지막 쪽",
            "현재 구역 안 임의 쪽",
        ]
        .iter()
        .enumerate()
        {
            ui.radio_value(kind, i, *label);
        }
        group(ui, "속성");
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut false, "기존 바탕쪽과 겹치게 하기"),
        );
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut false, "인쇄할 때 바탕쪽 맨 위로 보내기"),
        );
        group(ui, "현재 구역의 바탕쪽");
        let pages = list["masterPages"].as_array().cloned().unwrap_or_default();
        if pages.is_empty() {
            ui.label("없음");
        }
        for m in pages {
            ui.label(format!(
                "{} · {}",
                m["applyTo"].as_str().unwrap_or("?"),
                m["text"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(40)
                    .collect::<String>()
            ));
        }
        ui.label(
            RichText::new(
                "바탕쪽 만들기·편집은 아직 지원하지 않습니다. 기존 문서의 바탕쪽은 그대로 표시·저장됩니다.",
            )
            .size(11.0)
            .color(Color32::from_rgb(170, 60, 40)),
        );
    });
    // 만들기 stays unavailable: RHWP keeps master pages as render-only data.
    !matches!(button, Some(1))
}

// ----- 문서 정보 (620×520) -----
fn doc_info_dialog(ctx: &egui::Context, tab: &mut usize, info: &serde_json::Value) -> bool {
    let button = dialog(ctx, "문서 정보", 620.0, &["확인(D)", "취소"], |ui| {
        chrome::tabs(
            ui,
            tab,
            &["일반", "문서 요약", "문서 통계", "글꼴 정보", "그림 정보"],
        );
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(k);
            ui.label(v);
            ui.end_row();
        };
        egui::Grid::new(("doc-info", *tab))
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| match tab {
                0 => match info["file"].as_object() {
                    Some(f) => {
                        row(ui, "파일 위치", f["path"].as_str().unwrap_or("").to_owned());
                        row(ui, "크기", format!("{} 바이트", f["bytes"]));
                    }
                    None => row(ui, "파일", "아직 저장하지 않은 문서".into()),
                },
                1 => {
                    for k in ["제목", "주제", "지은이", "날짜", "키워드", "설명"] {
                        ui.label(k);
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut String::new()).desired_width(360.0),
                        );
                        ui.end_row();
                    }
                }
                2 => {
                    for (k, key) in [
                        ("쪽", "pages"),
                        ("구역", "sections"),
                        ("문단", "paragraphs"),
                        ("줄", "lines"),
                        ("글자 (공백 포함)", "characters"),
                        ("글자 (공백 제외)", "charactersNoSpaces"),
                        ("낱말", "words"),
                        ("표", "tables"),
                        ("그림", "pictures"),
                        ("그리기 개체", "shapes"),
                        ("수식", "equations"),
                        ("각주·미주", "notes"),
                    ] {
                        row(ui, k, info[key].to_string());
                    }
                }
                3 => {
                    for f in info["fonts"].as_array().into_iter().flatten() {
                        ui.label(f.as_str().unwrap_or(""));
                        ui.end_row();
                    }
                }
                _ => row(ui, "포함된 그림 데이터", format!("{}개", info["binData"])),
            });
        if *tab == 1 {
            ui.label(
                RichText::new("문서 요약 정보는 RHWP에서 읽기·저장하지 않아 바꿀 수 없습니다.")
                    .size(11.0)
                    .color(Color32::from_rgb(170, 60, 40)),
            );
        }
    });
    button.is_none()
}
