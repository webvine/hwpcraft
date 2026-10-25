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
    /// 문서 암호 설정 / 변경·해제 (보안 탭).
    Password {
        change: bool,
        password: String,
        confirm: String,
        remove: bool,
    },
    /// 개인 정보 보호하기: scan results with per-row selection.
    Privacy {
        kinds: [bool; 4],
        mask: String,
        hidden: bool,
        scan: Option<serde_json::Value>,
        selected: Vec<bool>,
    },
    /// 미리 보기: one page rendered through the same renderer as the editor.
    Preview {
        page: u32,
        texture: Option<(u32, u64, egui::TextureHandle, Vec2)>,
    },
    /// 구역 설정.
    Section {
        /// 쪽·그림·표·수식 start numbers (0 = 앞 구역에 이어서).
        numbers: [u16; 4],
        tab_mm: f64,
        hide: [bool; 6],
        whole_document: bool,
    },
    /// 그림 파일로 저장 (PNG).
    SavePicture {
        all_pages: bool,
        scale: f32,
    },
    /// 문서 보기 색 → 사용자 설정.
    ViewColorCustom {
        paper: Color32,
        text: Color32,
    },
    /// 다른 이름으로 저장 → 현재 쪽만 / 쪽 범위.
    SavePages {
        current_only: bool,
        from: u32,
        to: u32,
    },
}

/// Hangul-style Alt key tips: Alt alone shows tab letters, a tab letter shows item letters.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum KeyTips {
    #[default]
    Off,
    Tabs,
    Items,
}

/// Most recently used documents kept in the local settings folder (Hangul shows 9).
pub(crate) const RECENT_LIMIT: usize = 9;

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
    /// 문단 부호 · 조판 부호 · 투명 선 (screen only; re-applied after a document is replaced).
    pub marks: [bool; 3],
    marks_applied: u64,
    /// Recent documents, newest first.
    pub recent: Vec<PathBuf>,
    recent_loaded: bool,
    last_path: Option<PathBuf>,
    pub keytips: KeyTips,
    alt_down: bool,
    alt_alone: bool,
    /// 문서 보기 색 (screen only) and the colours of its 사용자 설정 entry.
    pub view_color: ViewColor,
    pub custom_colors: (Color32, Color32),
    /// 보기 → 그림: off draws only picture frames.
    pub show_pictures: bool,
    /// 맞쪽 보기 request; the continuous page view (core area) lays pages out.
    pub mirror_pages: bool,
    /// Header/footer editor to open next frame (이전/다음 inside the editor).
    pub hf_next: Option<(bool, usize)>,
    /// Selection highlight rectangles per page, for one document generation and selection.
    selection: Option<SelectionHighlight>,
    /// Page redraw queue: the document generation each cached page texture shows.
    pub page_drawn: std::collections::BTreeMap<u32, u64>,
    /// Pass in which a stale page was last redrawn (one stale page per pass).
    redraw_pass: Option<u64>,
    /// Pages the view asked for in the current and the previous pass.
    requested: (
        u64,
        std::collections::BTreeSet<u32>,
        std::collections::BTreeSet<u32>,
    ),
    /// Caret page for one generation, so the queue starts where the user types.
    caret_page: Option<(u64, u32)>,
    /// Print request waiting for its background PDF: file, copies, page count.
    print_after: Option<(PathBuf, u32, usize)>,
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
            marks: [false; 3],
            marks_applied: u64::MAX,
            recent: Vec::new(),
            recent_loaded: false,
            last_path: None,
            keytips: KeyTips::Off,
            alt_down: false,
            alt_alone: false,
            view_color: ViewColor::Light,
            custom_colors: (Color32::WHITE, Color32::BLACK),
            show_pictures: true,
            mirror_pages: false,
            hf_next: None,
            selection: None,
            page_drawn: Default::default(),
            redraw_pass: None,
            requested: (u64::MAX, Default::default(), Default::default()),
            caret_page: None,
            print_after: None,
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
            | "view.rulers"
            | "view.paragraph_marks"
            | "view.control_codes"
            | "view.transparent_lines" => true,
            "page.number" | "page.new_number" | "page.hide" | "page.section_dialog" => body,
            "page.header" | "page.footer" | "page.border_fill" | "page.master" | "file.print"
            | "file.doc_info" | "file.exit" | "page.setup_dialog" | "page.margins"
            | "file.preview" | "file.save_hml" | "file.save_pages" | "security.password"
            | "security.privacy" => true,
            "security.password_change" => self.session.has_password(),
            "page.margins.default"
            | "page.margins.narrow1_hf"
            | "page.margins.narrow2_hf"
            | "page.margins.wide_hf"
            | "page.margins.narrow"
            | "page.margins.wide"
            | "file.insert" => body,
            "page.copy"
            | "page.paste"
            | "file.save_picture"
            | "view.color.light"
            | "view.color.dark"
            | "view.color.highcontrast"
            | "view.color.oldhwp"
            | "view.color.custom"
            | "view.pictures"
            | "view.mirror_pages"
            | "page.header_footer.next"
            | "page.header_footer.prev" => true,
            "page.delete" => self.layout_page_count() > 1,
            "page.section.next" => {
                self.session.cursor.section + 1 < self.session.core.document().sections.len()
            }
            "page.section.prev" => self.session.cursor.section > 0,
            "file.save_block" => body && self.session.has_selection(),
            _ => return None,
        })
    }
    /// Check state of toggle commands shown as check boxes or pressed buttons.
    pub(crate) fn shell_ext_checked(&self, id: &str) -> bool {
        match id {
            "view.status_bar" => self.ext.shell.show_status_bar,
            "view.doc_tabs" => self.ext.shell.show_doc_tabs,
            "view.rulers" => self.show_rulers,
            "view.paragraph_marks" => self.ext.shell.marks[0],
            "view.control_codes" => self.ext.shell.marks[1],
            "view.transparent_lines" => self.ext.shell.marks[2],
            "view.pictures" => self.ext.shell.show_pictures,
            "view.mirror_pages" => self.ext.shell.mirror_pages,
            id if id.starts_with("view.color.") => self.ext.shell.view_color.id() == id,
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
            "view.paragraph_marks" | "view.control_codes" | "view.transparent_lines" => {
                let i = match id {
                    "view.paragraph_marks" => 0,
                    "view.control_codes" => 1,
                    _ => 2,
                };
                self.ext.shell.marks[i] ^= true;
                self.apply_marks();
            }
            "file.save_hml" => self.save_hml(),
            id if id.starts_with("page.margins.") => {
                let preset = &id["page.margins.".len()..];
                if self.run("page.margins.preset", json!({"preset": preset})) {
                    let label = MARGIN_PRESETS
                        .iter()
                        .find(|(cmd, _)| *cmd == id)
                        .map_or(preset, |(_, l)| *l);
                    self.message = format!("쪽 여백: {label}");
                }
            }
            "page.copy" => {
                if self.run("page.copy", json!({})) {
                    self.message =
                        "쪽을 복사했습니다. 쪽 붙이기는 현재 쪽 뒤에 새 쪽으로 넣습니다.".into();
                }
            }
            "page.paste" | "page.delete" | "page.section.next" | "page.section.prev" => {
                self.run(id, json!({}));
            }
            "file.insert" => self.insert_document(),
            "file.save_block" => self.save_block(),
            "view.pictures" => {
                self.ext.shell.show_pictures ^= true;
                self.restyle_pages();
            }
            "view.mirror_pages" => {
                self.ext.shell.mirror_pages ^= true;
                self.message = if self.ext.shell.mirror_pages {
                    "맞쪽 보기: 1쪽은 오른쪽, 이후 두 쪽씩 나란히 보입니다.".into()
                } else {
                    "한 쪽씩 보기".into()
                };
            }
            "view.color.light"
            | "view.color.dark"
            | "view.color.highcontrast"
            | "view.color.oldhwp" => {
                let mode = ViewColor::from_id(id).unwrap_or_default();
                self.ext.shell.view_color = mode;
                self.restyle_pages();
                self.message = format!("문서 보기 색: {} (화면에만 적용)", mode.label());
            }
            "page.header_footer.next" | "page.header_footer.prev" => {
                let (header, apply) = match &self.feature_dialog {
                    Some(super::feature_dialogs::FeatureDialog::HeaderFooter {
                        is_header,
                        apply,
                        ..
                    }) => (*is_header, *apply),
                    _ => (true, 0),
                };
                self.step_header_footer(header, apply, id.ends_with("next"));
            }
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
            "security.password" | "security.password_change" => Dialog::Password {
                change: self.session.has_password(),
                password: String::new(),
                confirm: String::new(),
                remove: false,
            },
            "security.privacy" => Dialog::Privacy {
                kinds: [true; 4],
                mask: "*".into(),
                hidden: true,
                scan: None,
                selected: Vec::new(),
            },
            "file.save_picture" => Dialog::SavePicture {
                all_pages: false,
                scale: 1.0,
            },
            "view.color.custom" => {
                let (paper, text) = self.ext.shell.custom_colors;
                Dialog::ViewColorCustom { paper, text }
            }
            "file.preview" => Dialog::Preview {
                page: self.page,
                texture: None,
            },
            "page.section_dialog" => {
                let v = self.session.run("page.section.get", json!({})).ok()?;
                let n = |k: &str| v[k].as_u64().unwrap_or(0).min(u64::from(u16::MAX)) as u16;
                let b = |k: &str| v[k].as_bool().unwrap_or(false);
                Dialog::Section {
                    numbers: [
                        n("pageNum"),
                        n("pictureNum"),
                        n("tableNum"),
                        n("equationNum"),
                    ],
                    tab_mm: v["defaultTabSpacingMm"].as_f64().unwrap_or(40.0),
                    hide: [
                        b("hideHeader"),
                        b("hideFooter"),
                        b("hideMasterPage"),
                        b("hideBorder"),
                        b("hideFill"),
                        b("hideEmptyLine"),
                    ],
                    whole_document: false,
                }
            }
            "file.save_pages" => Dialog::SavePages {
                current_only: true,
                from: self.page + 1,
                to: self.page + 1,
            },
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
        self.shell_ext_frame();
        let Some(mut dialog) = self.ext.shell.dialog.take() else {
            return;
        };
        let keep = match &mut dialog {
            Dialog::Password {
                change,
                password,
                confirm,
                remove,
            } => self.password_dialog(ctx, *change, password, confirm, remove),
            Dialog::Privacy { .. } => self.privacy_dialog(ctx, &mut dialog),
            Dialog::Preview { page, texture } => self.preview_dialog(ctx, page, texture),
            Dialog::Section {
                numbers,
                tab_mm,
                hide,
                whole_document,
            } => self.section_dialog(ctx, numbers, tab_mm, hide, whole_document),
            Dialog::SavePages {
                current_only,
                from,
                to,
            } => self.save_pages_dialog(ctx, current_only, from, to),
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
            Dialog::SavePicture { all_pages, scale } => {
                self.save_picture_dialog(ctx, all_pages, scale)
            }
            Dialog::ViewColorCustom { paper, text } => {
                self.view_color_custom_dialog(ctx, paper, text)
            }
        };
        if keep && self.ext.shell.dialog.is_none() {
            self.ext.shell.dialog = Some(dialog);
        }
    }

    /// Hangul two-key shortcuts and file/view keys; true when consumed.
    pub(crate) fn shell_ext_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        // COMMAND is Ctrl on Windows/Linux and Cmd on macOS.
        let ctrl = Modifiers::COMMAND;
        if self.keytip_shortcuts(ctx) {
            return true;
        }
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
            "password" => "security.password",
            "privacy" => "security.privacy",
            "preview" => "file.preview",
            "section" => "page.section_dialog",
            "save-pages" => "file.save_pages",
            "hf-fields" => {
                let _ = self.run("page.header", json!({"apply_to": "both", "template": 0}));
                self.open_header_footer_editor(true, 0);
                return true;
            }
            "save-picture" => "file.save_picture",
            "mirror" => {
                self.tab = 2;
                self.ext.shell.mirror_pages = true;
                return true;
            }
            "view-color" => {
                self.tab = 2;
                self.ext.shell.view_color = ViewColor::Dark;
                self.restyle_pages();
                return true;
            }
            "margins" => {
                self.tab = 5;
                self.shell_ext_dispatch("page.margins.narrow1_hf", &self.ctx.clone());
                return true;
            }
            "keytips" => {
                self.tab = 5;
                self.ext.shell.keytips = KeyTips::Items;
                return true;
            }
            "view-marks" => {
                self.tab = 2;
                self.ext.shell.marks = [true, true, true];
                self.apply_marks();
                return true;
            }
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
                if *template == 0 {
                    self.open_header_footer_editor(*header, *apply);
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
        let count = self.layout_page_count();
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
            // Unique per request: an older print file must never be sent by mistake.
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            std::env::temp_dir().join(format!("hwpcraft-print-{}-{stamp}.pdf", std::process::id()))
        };
        !self.start_print_pdf(target, range_name, copies, to_file)
    }
    /// Prepare the printed pages and convert them in the background PDF slot. Printing
    /// to paper waits for the file (`shell_print_when_ready`). True when started.
    pub(crate) fn start_print_pdf(
        &mut self,
        target: PathBuf,
        range: &str,
        copies: u32,
        to_file: bool,
    ) -> bool {
        let job = self.session.print_pdf_job(range, self.page);
        let count = job.as_ref().map_or(0, |(pages, _)| pages.len());
        let prepared = job
            .map(|(_, job)| -> PreparedPdf { (job.page_count(), Box::new(move || job.convert())) });
        if !self.start_shell_pdf(target.clone(), prepared) {
            return false;
        }
        if !to_file {
            self.ext.shell.print_after = Some((target, copies, count));
        }
        true
    }
    /// After the background PDF for a print request is written, hand it to the printer.
    fn shell_print_when_ready(&mut self) {
        if self.pdf.busy() {
            return;
        }
        let Some((target, copies, count)) = self.ext.shell.print_after.take() else {
            return;
        };
        if !target.is_file() {
            // The PDF slot has already reported why the file was not written.
            return;
        }
        match crate::platform::print_pdf(&target, copies) {
            crate::platform::PrintOutcome::Printed => {
                self.message = format!("{count}쪽을 {copies}부 기본 프린터로 보냈습니다.");
            }
            crate::platform::PrintOutcome::Opened => {
                self.message = format!(
                    "{count}쪽 PDF를 기본 보기 프로그램으로 열었습니다. 거기서 인쇄하세요."
                );
            }
            crate::platform::PrintOutcome::Failed => {
                self.error = Some(
                    "PDF 인쇄 프로그램을 찾지 못해 인쇄하지 못했습니다. 'PDF 저장'으로 저장한 뒤 인쇄하세요."
                        .into(),
                );
            }
        }
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
                        row(ui, "형식", f["format"].as_str().unwrap_or("-").to_owned());
                        for (k, key) in [("만든 날짜", "created"), ("고친 날짜", "modified")]
                        {
                            row(ui, k, f[key].as_u64().map_or("-".into(), date));
                        }
                        row(
                            ui,
                            "쪽 · 구역",
                            format!("{}쪽 · {}구역", info["pages"], info["sections"]),
                        );
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
                    let subs: Vec<(String, String)> = info["fontSubstitutions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|p| {
                            Some((p[0].as_str()?.to_owned(), p[1].as_str()?.to_owned()))
                        })
                        .collect();
                    ui.label(RichText::new("글꼴").strong());
                    ui.label(RichText::new("이 컴퓨터에서 대신 쓰는 글꼴").strong());
                    ui.end_row();
                    for f in info["fonts"].as_array().into_iter().flatten() {
                        let name = f.as_str().unwrap_or("");
                        ui.label(name);
                        ui.label(
                            subs.iter()
                                .find(|(n, _)| n == name)
                                .map_or("설치됨", |(_, s)| s.as_str()),
                        );
                        ui.end_row();
                    }
                }
                _ => {
                    row(ui, "포함된 그림 데이터", format!("{}개", info["binData"]));
                    for p in info["pictureData"].as_array().into_iter().flatten() {
                        ui.label(format!(
                            "#{} {}",
                            p["id"],
                            p["extension"].as_str().unwrap_or("")
                        ));
                        ui.label(format!(
                            "{} · {} · 사용 {}곳",
                            p["type"].as_str().unwrap_or(""),
                            p["bytes"]
                                .as_u64()
                                .map_or("크기 모름".to_owned(), |b| format!("{b} 바이트")),
                            p["usedBy"]
                        ));
                        ui.end_row();
                    }
                }
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

// ----- 0.13: 보기 표식 · 최근 문서 · 보안 · 미리 보기 · 구역 · 쪽 범위 저장 · Alt 키팁 -----

/// 머리말/꼬리말 편집 창 tokens standing for RHWP fields (`page.header_footer.text`).
pub(crate) const HF_TOKENS: &[(&str, &str)] = &[
    ("[쪽 번호]", "page_number"),
    ("[전체 쪽수]", "total_pages"),
    ("[파일 이름]", "file_name"),
];

/// Split editor text with `HF_TOKENS` into plain text and `(offset, kind)` fields.
pub(crate) fn split_hf_tokens(text: &str) -> (String, Vec<(usize, &'static str)>) {
    let mut plain = String::new();
    let mut fields = Vec::new();
    let mut rest = text;
    let mut offset = 0usize;
    'outer: while !rest.is_empty() {
        for (token, kind) in HF_TOKENS {
            if let Some(after) = rest.strip_prefix(token) {
                fields.push((offset, *kind));
                rest = after;
                continue 'outer;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            plain.push(c);
            offset += 1;
        }
        rest = chars.as_str();
    }
    (plain, fields)
}

/// Inverse of `split_hf_tokens` for the engine's `{text, fields}` answer.
pub(crate) fn join_hf_tokens(text: &str, fields: &[(usize, String)]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        for (offset, kind) in fields {
            if *offset == i
                && let Some((token, _)) = HF_TOKENS.iter().find(|(_, k)| k == kind)
            {
                out.push_str(token);
            }
        }
        out.push(*c);
    }
    for (offset, kind) in fields {
        if *offset >= chars.len()
            && let Some((token, _)) = HF_TOKENS.iter().find(|(_, k)| k == kind)
        {
            out.push_str(token);
        }
    }
    out
}

/// Settings folder shared with the engine (`hwpcraft_engine::config_dir`: Windows
/// `%APPDATA%`, macOS `~/Library/Application Support`, Linux `$XDG_CONFIG_HOME`).
/// `None` means settings are not persisted.
pub(crate) fn config_dir() -> Option<PathBuf> {
    hwpcraft_engine::config_dir()
}
fn recent_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("recent.txt"))
}
/// Move `path` to the front, dropping duplicates and anything past `RECENT_LIMIT`.
pub(crate) fn push_recent(list: &mut Vec<PathBuf>, path: &std::path::Path) {
    list.retain(|p| p != path);
    list.insert(0, path.to_owned());
    list.truncate(RECENT_LIMIT);
}
pub(crate) fn parse_recent(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .take(RECENT_LIMIT)
        .collect()
}
pub(crate) fn serialize_recent(list: &[PathBuf]) -> String {
    list.iter()
        .take(RECENT_LIMIT)
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Alt key tips per tab (Hangul-style Alt letters): (tab, letter, command id).
pub(crate) const ALT_KEYS: &[(usize, char, &str)] = &[
    (1, 'T', "edit.cut"),
    (1, 'C', "edit.copy"),
    (1, 'P', "edit.paste"),
    (1, 'A', "format.copy_shape"),
    (1, 'Y', "edit.delete_ctrl_marks"),
    (1, 'L', "format.character_dialog"),
    (1, 'M', "format.paragraph_dialog"),
    (1, 'S', "format.style.dialog"),
    (1, 'G', "page.portrait"),
    (1, 'D', "page.landscape"),
    (1, 'J', "page.margins"),
    (1, 'E', "page.master"),
    (1, 'U', "page.columns"),
    (1, 'I', "insert.picture_dialog"),
    (1, 'B', "table.create"),
    (1, 'R', "insert.chart"),
    (1, 'V', "object.select"),
    (1, 'K', "object.lock"),
    (1, 'F', "edit.find"),
    (1, 'N', "edit.replace"),
    (1, 'O', "edit.sort"),
    (2, 'L', "view.page_outline"),
    (2, 'C', "view.color_mode"),
    (2, 'H', "view.paragraph_marks"),
    (2, 'I', "view.pictures"),
    (2, 'R', "view.control_codes"),
    (2, 'T', "view.transparent_lines"),
    (2, 'F', "view.proof_marks"),
    (2, 'M', "view.memo"),
    (2, 'S', "view.status_bar"),
    (2, 'A', "view.doc_tabs"),
    (2, 'X', "view.rulers"),
    (2, 'G', "view.grid"),
    (2, 'D', "view.guides"),
    (2, 'P', "view.hanja_pronounce"),
    (2, 'W', "view.task_pane"),
    (2, 'B', "view.split"),
    (2, 'O', "view.arrange"),
    (2, 'K', "view.switch_window"),
    (2, 'U', "view.zoom_out"),
    (2, 'N', "view.zoom_in"),
    (2, 'J', "view.zoom_100"),
    (2, 'E', "view.zoom_fit_page"),
    (2, 'V', "view.zoom_fit_width"),
    (3, 'P', "insert.picture_dialog"),
    (3, 'T', "table.create"),
    (3, 'C', "insert.chart"),
    (3, 'W', "insert.textart"),
    (3, 'E', "insert.equation"),
    (3, 'O', "insert.ole"),
    (3, 'L', "insert.form_object"),
    (3, 'K', "insert.field"),
    (3, 'F', "insert.footnote"),
    (3, 'D', "insert.endnote"),
    (3, 'M', "insert.memo"),
    (3, 'R', "review.proof_marks"),
    (3, 'B', "insert.bookmark"),
    (3, 'U', "insert.cross_reference"),
    (3, 'Y', "insert.hyperlink"),
    (3, 'S', "insert.symbol"),
    (3, 'N', "insert.helper"),
    (3, 'H', "insert.hanja"),
    (4, 'Y', "format.style.create"),
    (4, 'T', "format.style.gallery"),
    (4, 'F', "format.character_dialog"),
    (4, 'B', "format.bold"),
    (4, 'I', "format.italic"),
    (4, 'U', "format.underline"),
    (4, 'K', "format.strikethrough"),
    (4, '5', "format.subscript"),
    (4, '6', "format.superscript"),
    (4, 'D', "format.char_border"),
    (4, 'H', "format.highlight"),
    (4, 'C', "format.text_color"),
    (4, 'R', "format.spacing.decrease"),
    (4, 'J', "format.spacing.increase"),
    (4, 'P', "format.paragraph_dialog"),
    (4, 'M', "format.outline.toggle"),
    (4, 'L', "format.bullet.toggle"),
    (4, 'E', "format.picture_bullet"),
    (4, 'N', "format.numbering.toggle"),
    (4, 'V', "format.numbering.restart"),
    (4, 'W', "format.level.increase"),
    (4, 'X', "format.level.decrease"),
    (5, 'J', "page.setup_dialog"),
    (5, 'O', "page.portrait"),
    (5, 'Y', "page.landscape"),
    (5, 'Z', "page.margins"),
    (5, 'C', "page.text_direction"),
    (5, 'F', "page.master"),
    (5, 'H', "page.header"),
    (5, 'T', "page.footer"),
    (5, 'B', "page.border_fill"),
    (5, 'G', "page.number"),
    (5, 'W', "page.new_number"),
    (5, 'I', "page.hide"),
    (5, 'M', "page.line_numbers"),
    (5, 'P', "insert.page_break"),
    (5, 'D', "insert.column_break"),
    (5, 'R', "page.delete"),
    (5, 'U', "page.columns"),
    (5, 'N', "page.column_def_break"),
    (5, 'E', "page.section_dialog"),
    (5, 'S', "page.section_break"),
    (5, 'L', "page.label"),
    (5, 'A', "page.manuscript"),
    (6, 'W', "security.password"),
    (6, 'R', "security.password_change"),
    (6, 'B', "security.distribution"),
    (6, 'I', "security.distribution_edit"),
    (6, 'H', "security.distribution_change"),
    (6, 'T', "security.protect_text"),
    (6, 'P', "security.privacy"),
    (6, 'O', "security.privacy_clear"),
    (6, 'A', "security.privacy_password"),
    (6, 'S', "security.settings"),
    (7, 'R', "review.proof_marks"),
    (7, 'V', "review.history"),
    (7, 'D', "review.compare"),
    (7, 'C', "review.track_changes"),
    (7, 'H', "review.balloons"),
    (7, 'S', "review.show_changes"),
    (7, 'O', "review.view_original"),
    (7, 'U', "review.view_original_marked"),
    (7, 'I', "review.view_final_marked"),
    (7, 'F', "review.view_final"),
    (7, 'A', "review.accept"),
    (7, 'J', "review.reject"),
    (7, 'P', "review.prev"),
    (7, 'N', "review.next"),
    (7, 'M', "insert.memo"),
    (8, 'C', "tools.index"),
    (8, 'T', "tools.toc"),
    (8, 'I', "tools.citation"),
    (8, 'B', "tools.bibliography"),
    (8, 'Y', "tools.bibliography_style"),
    (8, 'F', "tools.bibliography_refresh"),
    (8, 'L', "table.block_calc"),
    (8, 'R', "tools.macro"),
    (8, 'S', "review.spelling"),
    (8, 'Q', "tools.autocorrect"),
    (8, 'A', "tools.dictionary"),
    (8, 'D', "tools.translate"),
    (8, 'P', "tools.presentation"),
    (8, 'K', "tools.keyboard"),
    (8, 'Z', "tools.customize"),
    (8, 'U', "tools.options"),
    (8, 'E', "tools.user_info"),
    (8, 'H', "tools.assets"),
];
/// Letter shown for a command id in the given tab.
pub(crate) fn alt_letter(tab: usize, id: &str) -> Option<char> {
    ALT_KEYS
        .iter()
        .find(|(t, _, i)| *t == tab && *i == id)
        .map(|(_, c, _)| *c)
}

impl HwpApp {
    /// Per-frame bookkeeping: recent documents and view marks that follow document replacement.
    fn shell_ext_frame(&mut self) {
        if !self.ext.shell.recent_loaded {
            self.ext.shell.recent_loaded = true;
            if let Some(text) = recent_file().and_then(|p| std::fs::read_to_string(p).ok()) {
                self.ext.shell.recent = parse_recent(&text);
            }
        }
        if self.session.path != self.ext.shell.last_path {
            self.ext.shell.last_path = self.session.path.clone();
            if let Some(path) = self.session.path.clone() {
                push_recent(&mut self.ext.shell.recent, &path);
                if let Some(file) = recent_file() {
                    if let Some(dir) = file.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(file, serialize_recent(&self.ext.shell.recent));
                }
            }
        }
        // `open`/`reset` replace the core and clear its screen-only flags; they also empty
        // the undo stack, so that is the cheap moment to re-apply what the user turned on.
        if self.ext.shell.marks != [false; 3]
            && self.ext.shell.marks_applied != self.session.generation
            && !self.session.can_undo()
        {
            self.apply_marks();
        }
        self.shell_print_when_ready();
        if let Some((header, apply)) = self.ext.shell.hf_next.take() {
            self.open_header_footer_editor(header, apply);
        }
    }
    /// Push the 보기 check boxes into the engine (screen only; bumps the page cache).
    pub(crate) fn apply_marks(&mut self) {
        let m = self.ext.shell.marks;
        let r = self.session.run(
            "view.marks",
            json!({"paragraphMarks": m[0], "controlCodes": m[1], "transparentBorders": m[2]}),
        );
        self.result(r);
        self.ext.shell.marks_applied = self.session.generation;
    }
    /// Open a recent document (file menu).
    pub(crate) fn open_recent(&mut self, index: usize, ctx: &egui::Context) {
        if let Some(path) = self.ext.shell.recent.get(index).cloned() {
            if path.is_file() {
                self.request(Pending::Open(path), ctx);
            } else {
                self.ext.shell.recent.remove(index);
                self.error = Some(format!("파일을 찾을 수 없습니다: {}", path.display()));
            }
        }
    }
    /// 머리말/꼬리말 editor with field tokens; `apply` is 0 both, 1 odd, 2 even.
    pub(crate) fn open_header_footer_editor(&mut self, is_header: bool, apply: usize) {
        let apply_to = ["both", "odd", "even"][apply.min(2)];
        let r = self.session.run(
            "page.header_footer.text.get",
            json!({"header": is_header, "apply_to": apply_to}),
        );
        let Some(v) = self.result(r) else {
            return;
        };
        let fields: Vec<(usize, String)> = v["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| {
                Some((
                    usize::try_from(f["offset"].as_u64()?).ok()?,
                    f["kind"].as_str()?.to_owned(),
                ))
            })
            .collect();
        let text = join_hf_tokens(v["text"].as_str().unwrap_or(""), &fields);
        self.feature_dialog = Some(super::feature_dialogs::FeatureDialog::HeaderFooter {
            is_header,
            apply,
            text,
        });
    }
    fn save_hml(&mut self) {
        let name = self
            .session
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "새 문서".into());
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("한글 표준 문서 XML (HML)", &["hml"])
            .set_file_name(format!("{name}.hml"))
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("hml");
        }
        if self.run("file.save_hml", json!({"path": path})) {
            self.message = format!("HML로 저장했습니다 · {}", path.display());
        }
    }

    // ----- Alt 키팁 -----
    /// Alt alone toggles tab tips; a tab letter opens its item tips; an item letter runs it.
    /// Alt+letter pressed together switches tabs directly (Alt+F opens the file menu).
    fn keytip_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        let tab_keys: [(Key, usize); 9] = [
            (Key::F, 0),
            (Key::E, 1),
            (Key::U, 2),
            (Key::D, 3),
            (Key::J, 4),
            (Key::W, 5),
            (Key::R, 6),
            (Key::H, 7),
            (Key::K, 8),
        ];
        let (alt_down, key_pressed, clicked) = ctx.input(|i| {
            (
                i.modifiers.alt,
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key { pressed: true, .. } | egui::Event::Text(_)
                    )
                }),
                i.pointer.any_pressed(),
            )
        });
        if alt_down && !self.ext.shell.alt_down {
            self.ext.shell.alt_alone = true;
        }
        if alt_down && key_pressed {
            self.ext.shell.alt_alone = false;
        }
        if !alt_down && self.ext.shell.alt_down && self.ext.shell.alt_alone {
            self.ext.shell.keytips = match self.ext.shell.keytips {
                KeyTips::Off => KeyTips::Tabs,
                _ => KeyTips::Off,
            };
            self.message = if self.ext.shell.keytips == KeyTips::Tabs {
                "Alt 키팁: 메뉴 탭 글자(F E U D J W R H K)를 누르세요. Esc로 닫습니다.".into()
            } else {
                String::new()
            };
        }
        self.ext.shell.alt_down = alt_down;
        if clicked {
            self.ext.shell.keytips = KeyTips::Off;
        }
        for (key, index) in tab_keys {
            if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, key)) {
                self.keytip_tab(index, ctx);
                return true;
            }
        }
        if self.ext.shell.keytips == KeyTips::Off {
            return false;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.ext.shell.keytips = match self.ext.shell.keytips {
                KeyTips::Items => KeyTips::Tabs,
                _ => KeyTips::Off,
            };
            return true;
        }
        let pressed = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.ctrl && !modifiers.command => Some(*key),
                _ => None,
            })
        });
        let Some(key) = pressed else {
            return false;
        };
        let name = key.name();
        let Some(letter) = (name.chars().count() == 1)
            .then(|| name.chars().next())
            .flatten()
        else {
            return false;
        };
        let letter = letter.to_ascii_uppercase();
        // The tip letter must not reach the document.
        ctx.input_mut(|i| i.events.clear());
        match self.ext.shell.keytips {
            KeyTips::Tabs => {
                if let Some((_, index)) = tab_keys
                    .iter()
                    .find(|(k, _)| k.name() == letter.to_string())
                {
                    self.keytip_tab(*index, ctx);
                }
            }
            KeyTips::Items => {
                if let Some((_, _, id)) = ALT_KEYS
                    .iter()
                    .find(|(t, c, _)| *t == self.tab && *c == letter)
                {
                    self.ext.shell.keytips = KeyTips::Off;
                    self.message.clear();
                    self.ribbon_keytip(id, ctx);
                }
            }
            KeyTips::Off => {}
        }
        true
    }
    fn keytip_tab(&mut self, index: usize, ctx: &egui::Context) {
        if index == 0 {
            egui::Popup::toggle_id(ctx, egui::Id::new(("hwp-tab-menu", 0usize)));
            self.ext.shell.keytips = KeyTips::Off;
            self.message.clear();
            return;
        }
        self.tab = index;
        self.show_ribbon = true;
        self.ext.shell.keytips = KeyTips::Items;
        self.message =
            "Alt 키팁: 리본 항목의 글자를 누르세요. Esc는 탭 선택으로 돌아갑니다.".into();
    }

    // ----- 문서 암호 설정 (한글 배치: 암호 · 암호 확인 · 보안 종류) -----
    fn password_dialog(
        &mut self,
        ctx: &egui::Context,
        change: bool,
        password: &mut String,
        confirm: &mut String,
        remove: &mut bool,
    ) -> bool {
        let title = if change {
            "문서 암호 변경/해제"
        } else {
            "문서 암호 설정"
        };
        let button = dialog(ctx, title, 430.0, &["설정(D)", "취소"], |ui| {
            if change {
                group(ui, "현재 상태");
                ui.label("이 문서는 다음 저장부터 암호로 보호됩니다.");
                ui.checkbox(remove, "암호 해제");
            }
            group(ui, "암호");
            egui::Grid::new("doc-password")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("암호");
                    ui.add_enabled(
                        !*remove,
                        egui::TextEdit::singleline(password)
                            .password(true)
                            .desired_width(220.0),
                    );
                    ui.end_row();
                    ui.label("암호 확인");
                    ui.add_enabled(
                        !*remove,
                        egui::TextEdit::singleline(confirm)
                            .password(true)
                            .desired_width(220.0),
                    );
                    ui.end_row();
                });
            note(
                ui,
                "5자 이상 64자 이하. 암호를 잊으면 문서를 열 수 없습니다.",
            );
            group(ui, "보안 종류");
            let _ = ui.radio(true, "보안 수준 높음 (향상된 암호화)");
            ui.add_enabled(
                false,
                egui::RadioButton::new(false, "보안 수준 보통 (호환)"),
            )
            .on_disabled_hover_text(
                "RHWP는 HWP EncryptVersion 4 / HWPX AES-256 한 가지로만 저장합니다.",
            );
            note(
                ui,
                "암호는 HWP/HWPX 저장에만 적용되고 PDF·텍스트 내보내기에는 적용되지 않습니다.",
            );
        });
        match button {
            Some(0) => {
                if *remove {
                    let r = self.session.set_password(None);
                    if self.result(r).is_some() {
                        self.message = "문서 암호를 해제했습니다. 다음 저장부터 적용됩니다.".into();
                        return false;
                    }
                    return true;
                }
                let n = password.chars().count();
                if !(5..=64).contains(&n) {
                    self.error = Some("암호는 5자 이상 64자 이하로 입력하세요.".into());
                    return true;
                }
                if password != confirm {
                    self.error = Some("암호와 암호 확인이 다릅니다.".into());
                    return true;
                }
                let r = self.session.set_password(Some(password.clone()));
                if self.result(r).is_some() {
                    self.message = "문서 암호를 설정했습니다. 다음 저장부터 적용됩니다.".into();
                    false
                } else {
                    true
                }
            }
            Some(_) => false,
            None => true,
        }
    }

    // ----- 개인 정보 보호하기 -----
    fn privacy_dialog(&mut self, ctx: &egui::Context, d: &mut Dialog) -> bool {
        let Dialog::Privacy {
            kinds,
            mask,
            hidden,
            scan,
            selected,
        } = d
        else {
            return false;
        };
        const KINDS: [(&str, &str); 4] = [
            ("ssn", "주민등록번호"),
            ("card", "신용카드 번호"),
            ("phone", "전화번호"),
            ("email", "전자우편 주소"),
        ];
        let mut do_scan = false;
        let mut goto: Option<Cursor> = None;
        let button = dialog(
            ctx,
            "개인 정보 보호하기",
            760.0,
            &["가리기(D)", "닫기"],
            |ui| {
                group(ui, "찾을 개인 정보");
                ui.horizontal_wrapped(|ui| {
                    for (flag, (_, label)) in kinds.iter_mut().zip(KINDS) {
                        ui.checkbox(flag, label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("보호 문자");
                    ui.add(
                        egui::TextEdit::singleline(mask)
                            .desired_width(24.0)
                            .char_limit(1),
                    );
                    ui.add_space(12.0);
                    ui.checkbox(hidden, "숨은 글자(배경색·0pt)도 찾기");
                    ui.add_space(12.0);
                    if ui.button("찾기(F)").clicked() {
                        do_scan = true;
                    }
                });
                note(
                    ui,
                    "주민등록번호는 검증 숫자, 카드 번호는 Luhn 검사를 통과한 값만 찾습니다(RHWP, 오탐 0 우선).",
                );
                group(ui, "찾은 개인 정보");
                match scan {
                    None => {
                        ui.label("찾기를 누르면 문서 전체(본문·표·글상자)를 검사합니다.");
                    }
                    Some(v) => {
                        let findings = v["findings"].as_array().cloned().unwrap_or_default();
                        if findings.is_empty() {
                            ui.label("찾은 개인 정보가 없습니다.");
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("privacy-findings")
                            .max_height(200.0)
                            .show(ui, |ui| {
                                egui::Grid::new("privacy-grid")
                                    .num_columns(5)
                                    .striped(true)
                                    .spacing([14.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label(RichText::new("선택").strong());
                                        ui.label(RichText::new("종류").strong());
                                        ui.label(RichText::new("쪽").strong());
                                        ui.label(RichText::new("내용").strong());
                                        ui.label(RichText::new("가린 결과").strong());
                                        ui.end_row();
                                        for (i, f) in findings.iter().enumerate() {
                                            if let Some(on) = selected.get_mut(i) {
                                                ui.checkbox(on, "");
                                            } else {
                                                ui.label("");
                                            }
                                            ui.label(f["label"].as_str().unwrap_or(""));
                                            ui.label(
                                                f["page"].as_u64().map_or("-".to_owned(), |p| {
                                                    (p + 1).to_string()
                                                }),
                                            );
                                            let raw = f["raw"].as_str().unwrap_or("");
                                            let in_body = f["inBody"].as_bool().unwrap_or(false);
                                            let link = ui.add_enabled(
                                                in_body,
                                                egui::Button::new(raw).frame(false),
                                            );
                                            if link.clicked() {
                                                goto = Some(Cursor {
                                                    section: f["section"].as_u64().unwrap_or(0)
                                                        as usize,
                                                    paragraph: f["paragraph"].as_u64().unwrap_or(0)
                                                        as usize,
                                                    offset: f["charOffset"].as_u64().unwrap_or(0)
                                                        as usize,
                                                    cell: None,
                                                });
                                            }
                                            ui.label(f["masked"].as_str().unwrap_or(""));
                                            ui.end_row();
                                        }
                                    });
                            });
                        let hidden_report = &v["hiddenText"];
                        if let Some(items) = hidden_report["hiddenText"].as_array() {
                            group(ui, "숨은 글자");
                            if items.is_empty() {
                                ui.label("숨은 글자가 없습니다.");
                            } else {
                                ui.label(format!(
                                    "{}곳 · {}자 (배경과 같은 색, 0pt·아주 작은 글자, 쪽 밖 배치)",
                                    items.len(),
                                    hidden_report["hiddenCharCount"]
                                ));
                                egui::ScrollArea::vertical()
                                    .id_salt("privacy-hidden")
                                    .max_height(90.0)
                                    .show(ui, |ui| {
                                        for h in items {
                                            ui.label(format!(
                                                "{} · {}쪽 · \"{}\"",
                                                h["kind"].as_str().unwrap_or(""),
                                                h["page"]
                                                    .as_u64()
                                                    .map_or("-".to_owned(), |p| (p + 1)
                                                        .to_string()),
                                                h["excerpt"].as_str().unwrap_or("")
                                            ));
                                        }
                                    });
                                note(
                                    ui,
                                    "숨은 글자는 가리지 않고 알려만 줍니다. 글자 모양에서 색·크기를 바꾸세요.",
                                );
                            }
                        }
                    }
                }
            },
        );
        if let Some(c) = goto {
            let r = self.session.select(c, false);
            self.result(r);
            self.follow_cursor();
        }
        if do_scan {
            let list: Vec<&str> = kinds
                .iter()
                .zip(KINDS)
                .filter(|(on, _)| **on)
                .map(|(_, (k, _))| k)
                .collect();
            if list.is_empty() {
                self.error = Some("찾을 개인 정보 종류를 하나 이상 선택하세요.".into());
            } else {
                let r = self.session.run(
                    "security.privacy.scan",
                    json!({"kinds": list, "mask": mask, "hiddenText": *hidden}),
                );
                if let Some(v) = self.result(r) {
                    let n = v["findings"].as_array().map_or(0, Vec::len);
                    *selected = vec![true; n];
                    self.message = format!("개인 정보 {n}곳을 찾았습니다.");
                    *scan = Some(v);
                }
            }
            return true;
        }
        match button {
            Some(0) => {
                let Some(v) = scan.as_ref() else {
                    self.error = Some("먼저 찾기를 누르세요.".into());
                    return true;
                };
                let mut items: Vec<serde_json::Value> = Vec::new();
                for (f, on) in v["findings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .zip(selected.iter())
                {
                    if *on && !items.iter().any(|i| i["raw"] == f["raw"]) {
                        items.push(json!({"raw": f["raw"], "masked": f["masked"]}));
                    }
                }
                if items.is_empty() {
                    self.error = Some("가릴 항목을 선택하세요.".into());
                    return true;
                }
                let r = self
                    .session
                    .run("security.privacy.mask", json!({"items": items}));
                if let Some(done) = self.result(r) {
                    self.message = format!(
                        "{}곳을 가렸습니다. 실행 취소로 되돌릴 수 있습니다.",
                        done["replaced"]
                    );
                    *scan = None;
                    selected.clear();
                    self.sync_formatting();
                    self.follow_cursor();
                }
                true
            }
            Some(_) => false,
            None => true,
        }
    }

    // ----- 미리 보기 -----
    fn preview_dialog(
        &mut self,
        ctx: &egui::Context,
        page: &mut u32,
        texture: &mut Option<(u32, u64, egui::TextureHandle, Vec2)>,
    ) -> bool {
        let count = self.layout_page_count().max(1);
        *page = (*page).min(count - 1);
        let sizes = self.page_sizes();
        let size = sizes
            .get(*page as usize)
            .copied()
            .unwrap_or(Vec2::new(794.0, 1123.0));
        let scale = (520.0 / size.y.max(1.0)).clamp(0.25, 1.0);
        let generation = self.session.generation;
        if texture
            .as_ref()
            .is_none_or(|(p, g, _, _)| *p != *page || *g != generation)
        {
            match self
                .session
                .render_page_svg(*page)
                .and_then(|svg| self.renderer.svg(&svg, scale))
            {
                Ok(p) => {
                    let image = egui::ColorImage::from_rgba_premultiplied(
                        [p.width as usize, p.height as usize],
                        &p.rgba,
                    );
                    let handle = ctx.load_texture(
                        format!("preview-{page}-{generation}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    *texture = Some((
                        *page,
                        generation,
                        handle,
                        Vec2::new(p.width as f32, p.height as f32),
                    ));
                }
                Err(e) => {
                    self.error = Some(format!("{e:#}"));
                    return false;
                }
            }
        }
        let mut go: Option<u32> = None;
        let mut print = false;
        let button = dialog(ctx, "미리 보기", 720.0, &["인쇄(D)", "닫기"], |ui| {
            ui.horizontal(|ui| {
                for (label, target) in [
                    ("처음", 0),
                    ("이전", page.saturating_sub(1)),
                    ("다음", (*page + 1).min(count - 1)),
                    ("끝", count - 1),
                ] {
                    if ui.button(label).clicked() {
                        go = Some(target);
                    }
                }
                ui.add_space(12.0);
                ui.label(format!("{} / {} 쪽", *page + 1, count));
                ui.add_space(12.0);
                if ui.button("PDF로 저장").clicked() {
                    print = true;
                }
            });
            ui.add_space(6.0);
            if let Some((_, _, handle, size)) = texture.as_ref() {
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(560.0, 540.0), egui::Sense::hover());
                ui.painter().rect_filled(area, 0.0, Color32::from_gray(235));
                let fit = (area.width() / size.x).min(area.height() / size.y).min(1.0);
                let shown = *size * fit;
                let rect = Rect::from_center_size(area.center(), shown);
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                ui.painter().image(
                    handle.id(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, Color32::from_gray(171)),
                    egui::StrokeKind::Outside,
                );
            }
            note(
                ui,
                "화면과 같은 RHWP 조판으로 그립니다. 인쇄는 인쇄 창으로 이어집니다.",
            );
        });
        if let Some(p) = go {
            *page = p;
        }
        if print || button == Some(0) {
            self.ext.shell.dialog = Some(Dialog::Print {
                range: 0,
                pages: String::new(),
                copies: 1,
                to_file: print,
            });
            return false;
        }
        button.is_none()
    }

    // ----- 구역 설정 -----
    fn section_dialog(
        &mut self,
        ctx: &egui::Context,
        numbers: &mut [u16; 4],
        tab_mm: &mut f64,
        hide: &mut [bool; 6],
        whole_document: &mut bool,
    ) -> bool {
        let button = dialog(ctx, "구역 설정", 520.0, &["설정(D)", "취소"], |ui| {
            group(ui, "번호 시작");
            egui::Grid::new("section-numbers")
                .num_columns(3)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    for (n, label) in
                        numbers
                            .iter_mut()
                            .zip(["쪽 번호", "그림 번호", "표 번호", "수식 번호"])
                    {
                        ui.label(label);
                        let mut restart = *n > 0;
                        if ui.radio(!restart, "앞 구역에 이어서").clicked() {
                            restart = false;
                        }
                        ui.horizontal(|ui| {
                            if ui.radio(restart, "새 번호").clicked() {
                                restart = true;
                            }
                            let mut value = (*n).max(1);
                            ui.add_enabled(
                                restart,
                                egui::DragValue::new(&mut value).range(1..=9999),
                            );
                            *n = if restart { value } else { 0 };
                        });
                        ui.end_row();
                    }
                });
            group(ui, "기본 탭 간격");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(tab_mm)
                        .range(0.0..=100.0)
                        .speed(0.5)
                        .suffix(" mm"),
                );
                note(ui, "탭 설정이 없는 문단에서 Tab 키가 멈추는 간격");
            });
            group(ui, "감추기");
            ui.horizontal_wrapped(|ui| {
                for (flag, label) in hide.iter_mut().zip([
                    "머리말",
                    "꼬리말",
                    "바탕쪽",
                    "쪽 테두리",
                    "쪽 배경",
                    "빈 줄",
                ]) {
                    ui.checkbox(flag, label);
                }
            });
            group(ui, "줄 번호");
            ui.add_enabled(false, egui::Checkbox::new(&mut false, "줄 번호 표시"))
                .on_disabled_hover_text("RHWP에 줄 번호 API가 없어 지원하지 않습니다.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("적용 범위");
                egui::ComboBox::from_id_salt("section-scope")
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
        });
        match button {
            Some(0) => !self.run(
                "page.section",
                json!({
                    "pageNum": numbers[0], "pictureNum": numbers[1],
                    "tableNum": numbers[2], "equationNum": numbers[3],
                    "defaultTabSpacingMm": *tab_mm,
                    "hideHeader": hide[0], "hideFooter": hide[1], "hideMasterPage": hide[2],
                    "hideBorder": hide[3], "hideFill": hide[4], "hideEmptyLine": hide[5],
                    "wholeDocument": *whole_document,
                }),
            ),
            Some(_) => false,
            None => true,
        }
    }

    // ----- 현재 쪽만 / 쪽 범위 저장 -----
    fn save_pages_dialog(
        &mut self,
        ctx: &egui::Context,
        current_only: &mut bool,
        from: &mut u32,
        to: &mut u32,
    ) -> bool {
        let count = self.layout_page_count().max(1);
        let current = self.page + 1;
        let button = dialog(
            ctx,
            "쪽 범위 저장",
            420.0,
            &["저장(D)", "취소"],
            |ui| {
                group(ui, "저장할 쪽");
                ui.radio_value(current_only, true, format!("현재 쪽만 ({current}쪽)"));
                ui.horizontal(|ui| {
                    ui.radio_value(current_only, false, "쪽 범위");
                    ui.add_enabled_ui(!*current_only, |ui| {
                        ui.add(egui::DragValue::new(from).range(1..=count));
                        ui.label("–");
                        ui.add(egui::DragValue::new(to).range(1..=count));
                        ui.label(format!("(전체 {count}쪽)"));
                    });
                });
                note(
                    ui,
                    "선택한 쪽에 걸친 문단만 남겨 새 파일로 저장합니다. 문단 단위로 잘라 쪽 수가 달라질 수 있고, 열려 있는 문서는 바뀌지 않습니다.",
                );
            },
        );
        match button {
            Some(0) => {
                let (a, b) = if *current_only {
                    (current, current)
                } else {
                    (*from, *to)
                };
                let stem = self
                    .session
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "새 문서".into());
                let Some(mut path) = rfd::FileDialog::new()
                    .add_filter("한글 표준 문서 (HWPX)", &["hwpx"])
                    .add_filter("한글 문서 (HWP)", &["hwp"])
                    .add_filter("PDF", &["pdf"])
                    .set_file_name(format!("{stem}-{a}쪽.hwpx"))
                    .save_file()
                else {
                    return true;
                };
                if path.extension().is_none() {
                    path.set_extension("hwpx");
                }
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
                {
                    return !self.start_pages_pdf(path, a, b);
                }
                let r = self
                    .session
                    .run("file.save_pages", json!({"path": path, "from": a, "to": b}));
                match self.result(r) {
                    Some(v) => {
                        self.message =
                            format!("{}쪽을 저장했습니다 · {}", v["pagesAfter"], path.display());
                        false
                    }
                    None => true,
                }
            }
            Some(_) => false,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_moves_to_front_dedupes_and_keeps_nine() {
        let mut list = Vec::new();
        for i in 0..12 {
            push_recent(
                &mut list,
                std::path::Path::new(&format!("C:/docs/{i}.hwpx")),
            );
        }
        assert_eq!(list.len(), RECENT_LIMIT);
        assert_eq!(list[0], PathBuf::from("C:/docs/11.hwpx"));
        push_recent(&mut list, std::path::Path::new("C:/docs/5.hwpx"));
        assert_eq!(list[0], PathBuf::from("C:/docs/5.hwpx"));
        assert_eq!(list.len(), RECENT_LIMIT);
        assert_eq!(list.iter().filter(|p| p.ends_with("5.hwpx")).count(), 1);
        let text = serialize_recent(&list);
        assert_eq!(parse_recent(&text), list);
        assert_eq!(
            parse_recent("\n  \nC:/a.hwp\n"),
            vec![PathBuf::from("C:/a.hwp")]
        );
    }

    #[test]
    fn header_footer_tokens_split_and_join() {
        let (text, fields) = split_hf_tokens("[파일 이름]쪽 [쪽 번호]/[전체 쪽수]");
        assert_eq!(text, "쪽 /");
        assert_eq!(
            fields,
            vec![(0, "file_name"), (2, "page_number"), (3, "total_pages")]
        );
        let back = join_hf_tokens(
            &text,
            &fields
                .iter()
                .map(|(o, k)| (*o, (*k).to_owned()))
                .collect::<Vec<_>>(),
        );
        assert_eq!(back, "[파일 이름]쪽 [쪽 번호]/[전체 쪽수]");
        assert_eq!(split_hf_tokens("그냥 글"), ("그냥 글".to_owned(), vec![]));
        assert_eq!(
            join_hf_tokens("", &[(0, "page_number".into())]),
            "[쪽 번호]"
        );
    }

    #[test]
    fn alt_letters_follow_the_parity_table_without_duplicates_per_tab() {
        assert_eq!(alt_letter(2, "view.paragraph_marks"), Some('H'));
        assert_eq!(alt_letter(5, "page.section_dialog"), Some('E'));
        assert_eq!(alt_letter(6, "security.password"), Some('W'));
        assert_eq!(alt_letter(1, "security.password"), None);
        for tab in 1..=8usize {
            let mut letters: Vec<char> = ALT_KEYS
                .iter()
                .filter(|(t, _, _)| *t == tab)
                .map(|(_, c, _)| *c)
                .collect();
            let n = letters.len();
            letters.sort_unstable();
            letters.dedup();
            assert_eq!(letters.len(), n, "tab {tab} has duplicate Alt letters");
        }
    }
}

// ----- 0.14: 문서 보기 색 · 그림 자리 표시 · 쪽 여백 사전 값 · 쪽 복사/붙이기/지우기 · 끼워 넣기 · 블록/그림 저장 -----

/// Hangul 문서 보기 색: paper and text colours used on screen only (never saved or printed).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ViewColor {
    #[default]
    Light,
    Dark,
    HighContrast,
    OldHwp,
    Custom,
}
impl ViewColor {
    pub(crate) const ALL: [ViewColor; 5] = [
        ViewColor::Light,
        ViewColor::Dark,
        ViewColor::HighContrast,
        ViewColor::OldHwp,
        ViewColor::Custom,
    ];
    pub(crate) fn id(self) -> &'static str {
        match self {
            ViewColor::Light => "view.color.light",
            ViewColor::Dark => "view.color.dark",
            ViewColor::HighContrast => "view.color.highcontrast",
            ViewColor::OldHwp => "view.color.oldhwp",
            ViewColor::Custom => "view.color.custom",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            ViewColor::Light => "기본",
            ViewColor::Dark => "어둡게",
            ViewColor::HighContrast => "고대비",
            ViewColor::OldHwp => "옛 한글",
            ViewColor::Custom => "사용자 설정",
        }
    }
    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }
    /// `(paper, text)` on screen; `None` leaves the rendered page as it is.
    fn colors(self, custom: (Color32, Color32)) -> Option<(Color32, Color32)> {
        match self {
            ViewColor::Light => None,
            ViewColor::Dark => Some((
                Color32::from_rgb(43, 43, 46),
                Color32::from_rgb(222, 222, 222),
            )),
            ViewColor::HighContrast => Some((Color32::BLACK, Color32::WHITE)),
            ViewColor::OldHwp => Some((Color32::from_rgb(0, 0, 128), Color32::WHITE)),
            ViewColor::Custom => Some(custom),
        }
    }
}

/// Hangul 쪽 여백 gallery entries as ribbon command ids (engine `page.margins.preset`).
pub(crate) const MARGIN_PRESETS: &[(&str, &str)] = &[
    ("page.margins.default", "기본"),
    ("page.margins.narrow1_hf", "좁게1 (머리말/꼬리말 여백 포함)"),
    ("page.margins.narrow2_hf", "좁게2 (머리말/꼬리말 여백 포함)"),
    ("page.margins.wide_hf", "넓게 (머리말/꼬리말 여백 포함)"),
    ("page.margins.narrow", "좁게"),
    ("page.margins.wide", "넓게"),
];

/// Map every grey pixel of a rendered page from white→`paper` and black→`text`; coloured
/// pixels (pictures, highlights, coloured text) are left alone, like Hangul's dark mode.
fn recolor(rgba: &mut [u8], paper: Color32, text: Color32) {
    let lerp = |a: u8, b: u8, t: f32| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
    for px in rgba.as_chunks_mut::<4>().0 {
        let (r, g, b) = (px[0], px[1], px[2]);
        let saturation = r.max(g).max(b) - r.min(g).min(b);
        if saturation > 40 {
            continue;
        }
        let lum = (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0;
        px[0] = lerp(text.r(), paper.r(), lum);
        px[1] = lerp(text.g(), paper.g(), lum);
        px[2] = lerp(text.b(), paper.b(), lum);
    }
}

fn svg_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Replace every `<image>` of a page SVG with a grey frame (Hangul 보기 → 그림 off shows
/// only the picture's place so long documents scroll faster).
pub(crate) fn picture_placeholders(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(start) = rest.find("<image") {
        out.push_str(&rest[..start]);
        let tag_end = match rest[start..].find('>') {
            Some(i) => start + i + 1,
            None => break,
        };
        let tag = &rest[start..tag_end];
        let mut after = tag_end;
        if !tag.ends_with("/>")
            && let Some(close) = rest[tag_end..].find("</image>")
        {
            after = tag_end + close + "</image>".len();
        }
        let mut rect = String::from("<rect");
        for name in ["x", "y", "width", "height", "transform"] {
            if let Some(v) = svg_attr(tag, name) {
                rect.push_str(&format!(" {name}=\"{v}\""));
            }
        }
        rect.push_str(" fill=\"#f2f2f2\" stroke=\"#8c8c8c\" stroke-width=\"1\"/>");
        out.push_str(&rect);
        rest = &rest[after..];
    }
    out.push_str(rest);
    out
}

/// Civil date from UNIX seconds (UTC), for the 문서 정보 general tab.
fn date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (h, m) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}")
}

fn with_page_suffix(path: &std::path::Path, n: usize) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());
    path.with_file_name(format!("{stem}-{n:03}.png"))
}

impl HwpApp {
    /// Screen colour mode or 그림 changed: the core view re-renders pages through
    /// `shell_page_image` once its cache is empty.
    fn restyle_pages(&mut self) {
        self.pages.clear();
    }

    /// The page bitmap with the screen colour mode and picture placeholders applied; the
    /// core view's `page_texture` renders every page through this.
    pub(crate) fn shell_page_image(
        &self,
        index: u32,
        scale: f32,
    ) -> anyhow::Result<egui::ColorImage> {
        let mut svg = self.session.render_page_svg(index)?;
        if !self.ext.shell.show_pictures {
            svg = picture_placeholders(&svg);
        }
        let page = self.renderer.svg(&svg, scale)?;
        let mut rgba = page.rgba;
        if let Some((paper, text)) = self
            .ext
            .shell
            .view_color
            .colors(self.ext.shell.custom_colors)
        {
            recolor(&mut rgba, paper, text);
        }
        Ok(egui::ColorImage::from_rgba_premultiplied(
            [page.width as usize, page.height as usize],
            &rgba,
        ))
    }

    fn view_color_custom_dialog(
        &mut self,
        ctx: &egui::Context,
        paper: &mut Color32,
        text: &mut Color32,
    ) -> bool {
        let button = dialog(
            ctx,
            "문서 보기 색 사용자 설정",
            380.0,
            &["설정(D)", "취소"],
            |ui| {
                group(ui, "화면 색");
                ui.horizontal(|ui| {
                    ui.label("바탕 색");
                    ui.color_edit_button_srgba(paper);
                    ui.add_space(16.0);
                    ui.label("글자 색");
                    ui.color_edit_button_srgba(text);
                });
                note(
                    ui,
                    "화면에만 적용되며 문서·인쇄·PDF에는 영향이 없습니다. 색이 있는 글자와 그림은 그대로 보입니다.",
                );
            },
        );
        match button {
            Some(0) => {
                self.ext.shell.custom_colors = (*paper, *text);
                self.ext.shell.view_color = ViewColor::Custom;
                self.restyle_pages();
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    fn save_picture_dialog(
        &mut self,
        ctx: &egui::Context,
        all_pages: &mut bool,
        scale: &mut f32,
    ) -> bool {
        let count = self.layout_page_count().max(1);
        let current = self.page + 1;
        let button = dialog(
            ctx,
            "그림 파일로 저장",
            440.0,
            &["저장(D)", "취소"],
            |ui| {
                group(ui, "저장할 쪽");
                ui.radio_value(all_pages, false, format!("현재 쪽만 ({current}쪽)"));
                ui.radio_value(all_pages, true, format!("모든 쪽 ({count}쪽)"));
                group(ui, "해상도");
                ui.horizontal(|ui| {
                    for (s, label) in [(1.0, "96 dpi"), (1.5, "144 dpi"), (2.0, "192 dpi")] {
                        ui.radio_value(scale, s, label);
                    }
                });
                note(
                    ui,
                    "PNG로 저장합니다. 여러 쪽은 파일 이름 뒤에 -001, -002가 붙습니다.",
                );
            },
        );
        match button {
            Some(0) => !self.save_picture(*all_pages, *scale),
            Some(_) => false,
            None => true,
        }
    }
    /// Returns true when the pictures were written.
    fn save_picture(&mut self, all_pages: bool, scale: f32) -> bool {
        let stem = self
            .session
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "새 문서".into());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG 그림", &["png"])
            .set_file_name(format!("{stem}.png"))
            .save_file()
        else {
            return false;
        };
        let pages: Vec<u32> = if all_pages {
            (0..self.layout_page_count()).collect()
        } else {
            vec![self.page]
        };
        let mut written = 0usize;
        for (n, index) in pages.iter().enumerate() {
            let target = if pages.len() == 1 {
                path.clone()
            } else {
                with_page_suffix(&path, n + 1)
            };
            let r = self
                .session
                .render_page_svg(*index)
                .and_then(|svg| self.renderer.svg(&svg, scale.clamp(0.25, 2.0)))
                .and_then(|p| {
                    image::save_buffer(&target, &p.rgba, p.width, p.height, image::ColorType::Rgba8)
                        .map_err(|e| anyhow::anyhow!("{}: {e}", target.display()))
                });
            if self.result(r).is_none() {
                return written > 0;
            }
            written += 1;
        }
        self.message = format!(
            "{written}쪽을 그림 파일로 저장했습니다 · {}",
            path.display()
        );
        true
    }

    /// 문서 끼워 넣기: pick an HWP/HWPX and merge it at the caret.
    fn insert_document(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("한글 문서", &["hwp", "hwpx"])
            .pick_file()
        else {
            return;
        };
        if self.run("file.insert", json!({"path": path})) {
            self.message = format!("끼워 넣기 완료 · {}", path.display());
        } else if self.error.as_deref().is_some_and(|e| e.contains("암호")) {
            self.error = Some("암호가 걸린 문서는 암호를 푼 사본을 끼워 넣으세요.".into());
        }
    }

    /// 블록 저장: the selection goes to a new file.
    fn save_block(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("HWPX 문서", &["hwpx"])
            .add_filter("HWP 문서", &["hwp"])
            .add_filter("PDF", &["pdf"])
            .add_filter("텍스트", &["txt"])
            .set_file_name("블록.hwpx")
            .save_file()
        else {
            return;
        };
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            self.start_block_pdf(path);
            return;
        }
        let r = self.session.run("file.save_block", json!({"path": path}));
        if let Some(v) = self.result(r) {
            self.message = format!(
                "블록 {}문단을 저장했습니다 · {}",
                v["paragraphs"],
                path.display()
            );
        }
    }

    /// 머리말/꼬리말 편집 창의 이전/다음: open the neighbouring header/footer of the section.
    pub(crate) fn step_header_footer(&mut self, is_header: bool, apply: usize, forward: bool) {
        let r = self.session.run("page.header_footer.list", json!({}));
        let Some(list) = self.result(r) else {
            return;
        };
        // RHWP apply codes: 0 both, 1 even, 2 odd; the editor uses 0 both, 1 odd, 2 even.
        let items: Vec<(bool, usize)> = list["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| {
                let apply = match i["applyTo"].as_u64()? {
                    1 => 2,
                    2 => 1,
                    _ => 0,
                };
                Some((i["isHeader"].as_bool()?, apply))
            })
            .collect();
        if items.is_empty() {
            self.message = "이 구역에는 머리말/꼬리말이 없습니다.".into();
            return;
        }
        let next = match items.iter().position(|&x| x == (is_header, apply)) {
            Some(i) if forward => (i + 1) % items.len(),
            Some(i) => (i + items.len() - 1) % items.len(),
            None => 0,
        };
        self.ext.shell.hf_next = Some(items[next]);
    }
}

// ----- 선택 영역 강조: 줄 단위로 합친 사각형, 문서 세대·선택·쪽마다 한 번 계산 -----

/// Caret probes allowed for one page's highlight. Each probe costs a caret query
/// in RHWP (about 1.4 ms on a 20-page exam paper), so a page stays well under a second.
const SELECTION_PROBES_PER_PAGE: usize = 600;

type SelectionKey = (u64, Cursor, Option<usize>, Option<Cursor>);

/// Selection highlight cache: one entry per page, valid for one generation and selection.
pub(crate) struct SelectionHighlight {
    key: SelectionKey,
    pages: std::collections::HashMap<u32, Vec<Rect>>,
}

#[derive(Clone, Copy)]
struct Probe {
    page: u32,
    x: f32,
    y: f32,
    height: f32,
}

fn probe(session: &Session, at: Cursor, offset: usize) -> Option<Probe> {
    let v = session.caret_at(Cursor { offset, ..at }).ok()?;
    Some(Probe {
        page: u32::try_from(v["pageIndex"].as_u64()?).ok()?,
        x: v["x"].as_f64()? as f32,
        y: v["y"].as_f64()? as f32,
        height: v["height"].as_f64().unwrap_or(16.0) as f32,
    })
}

/// Rectangles (page coordinates, unzoomed) covering `start..end` of one paragraph on
/// `page`. Each laid-out line becomes one rectangle; lines are found by bisection, so a
/// line costs a few caret queries instead of two per character. A character whose end
/// caret moves to the next line gets the same short tail the per-character drawing used.
fn paragraph_selection_rects(
    session: &Session,
    at: Cursor,
    range: std::ops::Range<usize>,
    page: u32,
    budget: &mut usize,
    out: &mut Vec<Rect>,
) {
    let mut memo: std::collections::HashMap<usize, Option<Probe>> = Default::default();
    let mut caret = |offset: usize, budget: &mut usize| -> Option<Probe> {
        if let Some(p) = memo.get(&offset) {
            return *p;
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let p = probe(session, at, offset);
        memo.insert(offset, p);
        p
    };
    let mut stack = vec![(range.start, range.end)];
    while let Some((s, e)) = stack.pop() {
        if s >= e {
            continue;
        }
        let (Some(a), b) = (caret(s, budget), caret(e, budget)) else {
            continue;
        };
        if let Some(b) = b
            && a.page == b.page
            && (b.y - a.y).abs() < 1.0
        {
            if a.page == page {
                out.push(Rect::from_min_size(
                    Pos2::new(a.x, a.y),
                    Vec2::new((b.x - a.x).max(2.0), a.height),
                ));
            }
            continue;
        }
        if e - s == 1 {
            if a.page == page {
                out.push(Rect::from_min_size(
                    Pos2::new(a.x, a.y),
                    Vec2::new(a.height * 0.6, a.height),
                ));
            }
            continue;
        }
        // Whole span on other pages: nothing of it can land on `page`.
        if let Some(b) = b
            && (a.page.max(b.page) < page || a.page.min(b.page) > page)
        {
            continue;
        }
        let mid = s + (e - s) / 2;
        stack.push((mid, e));
        stack.push((s, mid));
    }
}

/// Whether paragraph `at` can draw on `page`: RHWP's pagination gives the first page
/// of each paragraph, so a paragraph spans from its first page to the next one's.
fn paragraph_may_touch(session: &Session, at: Cursor, next: Option<Cursor>, page: u32) -> bool {
    let first = |c: Cursor| -> Option<u32> {
        let v: serde_json::Value = serde_json::from_str(
            &session
                .core
                .get_page_of_position_native(c.section, c.paragraph)
                .ok()?,
        )
        .ok()?;
        u32::try_from(v["page"].as_u64()?).ok()
    };
    let Some(start) = first(at) else {
        return true;
    };
    let end = next.and_then(first).unwrap_or(u32::MAX).max(start);
    // One page of slack: the canvas may show a projected layout (page fields,
    // nested tables) whose page breaks differ slightly from the stored pagination.
    start.saturating_sub(1) <= page && page <= end.saturating_add(1)
}

pub(crate) fn selection_rects_for_page(session: &Session, page: u32) -> Vec<Rect> {
    let Ok(parts) = session.selected_paragraphs() else {
        return Vec::new();
    };
    let mut budget = SELECTION_PROBES_PER_PAGE;
    let mut out = Vec::new();
    for (i, (at, range)) in parts.iter().enumerate() {
        if range.is_empty() || budget == 0 {
            continue;
        }
        let next = parts.get(i + 1).map(|(c, _)| *c);
        if at.cell.is_none() && !paragraph_may_touch(session, *at, next, page) {
            continue;
        }
        paragraph_selection_rects(session, *at, range.clone(), page, &mut budget, &mut out);
    }
    merge_line_rects(out)
}

/// Join touching pieces of one line: bisection splits a line at midpoints, not at its ends.
fn merge_line_rects(mut rects: Vec<Rect>) -> Vec<Rect> {
    rects.sort_by(|a, b| {
        (a.min.y, a.min.x)
            .partial_cmp(&(b.min.y, b.min.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<Rect> = Vec::with_capacity(rects.len());
    for r in rects {
        if let Some(last) = out.last_mut()
            && (last.min.y - r.min.y).abs() < 0.5
            && (last.height() - r.height()).abs() < 0.5
            && r.min.x <= last.max.x + 0.5
        {
            *last = last.union(r);
            continue;
        }
        out.push(r);
    }
    out
}

impl HwpApp {
    /// Selection highlight for one page in page coordinates (multiply by zoom).
    /// Computed once per document generation, selection and page.
    pub(crate) fn selection_rects(&mut self, page: u32) -> Vec<Rect> {
        if !self.session.has_selection() {
            self.ext.shell.selection = None;
            return Vec::new();
        }
        let key = (
            self.session.generation,
            self.session.cursor,
            self.session.anchor,
            self.session.span_anchor,
        );
        let cache = match &mut self.ext.shell.selection {
            Some(cache) if cache.key == key => cache,
            slot => slot.insert(SelectionHighlight {
                key,
                pages: Default::default(),
            }),
        };
        cache
            .pages
            .entry(page)
            .or_insert_with(|| selection_rects_for_page(&self.session, page))
            .clone()
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn per_character(session: &Session, page: u32) -> Vec<Rect> {
        let mut out = Vec::new();
        for (at, range) in session.selected_paragraphs().unwrap() {
            for offset in range {
                let (Some(a), b) = (probe(session, at, offset), probe(session, at, offset + 1))
                else {
                    continue;
                };
                if a.page != page {
                    continue;
                }
                let same = b.is_some_and(|b| (b.y - a.y).abs() < 1.0);
                let w = if same {
                    (b.map_or(a.x, |b| b.x) - a.x).max(2.0)
                } else {
                    a.height * 0.6
                };
                out.push(Rect::from_min_size(
                    Pos2::new(a.x, a.y),
                    Vec2::new(w, a.height),
                ));
            }
        }
        out
    }

    #[test]
    fn merged_selection_covers_every_character_with_few_rectangles() -> anyhow::Result<()> {
        let mut s = Session::new()?;
        let long = "가나다라마바사아자차카타파하 ".repeat(12);
        for i in 0..3 {
            s.execute(Command::Insert(format!("{i}번 문단 {long}")))?;
            s.execute(Command::Split)?;
        }
        s.execute(Command::PageBreak)?;
        s.execute(Command::Insert("둘째 쪽".into()))?;
        s.select_all()?;
        let naive = per_character(&s, 0);
        let merged = selection_rects_for_page(&s, 0);
        assert!(naive.len() > 400, "{}", naive.len());
        assert!(
            merged.len() < naive.len() / 20,
            "{} vs {}",
            merged.len(),
            naive.len()
        );
        for r in &naive {
            assert!(
                merged.iter().any(|m| m.expand(0.6).contains_rect(*r)),
                "{r:?} not covered"
            );
        }
        // Second page: only its own paragraph, nothing from page one.
        let second = selection_rects_for_page(&s, 1);
        assert_eq!(second.len(), 1, "{second:?}");
        Ok(())
    }

    #[test]
    fn selection_cache_follows_generation_and_selection() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session.execute(Command::Insert("하나 둘 셋".into()))?;
        assert!(a.selection_rects(0).is_empty());
        a.session.select_all()?;
        let all = a.selection_rects(0);
        assert_eq!(all.len(), 1);
        a.session.select(
            Cursor {
                offset: 2,
                ..Default::default()
            },
            false,
        )?;
        a.session.select(
            Cursor {
                offset: 4,
                ..Default::default()
            },
            true,
        )?;
        let part = a.selection_rects(0);
        assert_eq!(part.len(), 1);
        assert!(part[0].width() < all[0].width());
        Ok(())
    }
}

// ----- 쪽 다시 그리기 대기열: 편집 뒤 이전 그림을 두고 한 프레임에 한 쪽씩 -----

impl HwpApp {
    /// Texture for page `index`, used by the canvas instead of clearing every page on
    /// each edit. A page drawn for an older document generation stays on screen and at
    /// most one such page is redrawn per pass, the caret's page first; the rest follow
    /// on the next passes (a repaint is requested). A page that has no texture yet, for
    /// example one scrolled into view, is always drawn at once.
    pub(crate) fn shell_page_texture(
        &mut self,
        ctx: &egui::Context,
        index: u32,
    ) -> anyhow::Result<egui::TextureHandle> {
        let generation = self.session.generation;
        let pass = ctx.cumulative_pass_nr();
        let requested = &mut self.ext.shell.requested;
        if requested.0 != pass {
            requested.2 = std::mem::take(&mut requested.1);
            requested.0 = pass;
        }
        requested.1.insert(index);
        let drawn = self.ext.shell.page_drawn.get(&index).copied();
        let cached = self.pages.get(&index).map(|(texture, _)| texture.clone());
        let stale = match (&cached, drawn) {
            (Some(texture), Some(at)) if at == generation => return Ok(texture.clone()),
            (Some(texture), Some(_)) => {
                if !self.may_redraw_page(pass, index, generation) {
                    ctx.request_repaint();
                    return Ok(texture.clone());
                }
                true
            }
            _ => false,
        };
        // The shell-page area applies the document view colour and picture
        // placeholder options on top of the renderer's bitmap.
        let image = self.shell_page_image(index, 1.25)?;
        let size = Vec2::new(image.size[0] as f32 / 1.25, image.size[1] as f32 / 1.25);
        let texture = ctx.load_texture(
            format!("page-{index}-{generation}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        if !self.pages.contains_key(&index)
            && self.pages.len() >= 8
            && let Some(far) = self.pages.keys().copied().max_by_key(|k| k.abs_diff(index))
        {
            self.pages.remove(&far);
            self.ext.shell.page_drawn.remove(&far);
        }
        self.pages.insert(index, (texture.clone(), size));
        self.ext.shell.page_drawn.insert(index, generation);
        if stale {
            self.ext.shell.redraw_pass = Some(pass);
            if self
                .pages
                .iter()
                .any(|(page, _)| self.ext.shell.page_drawn.get(page) != Some(&generation))
            {
                ctx.request_repaint();
            }
        }
        Ok(texture)
    }

    /// One stale page per pass; while the caret's page is visible and stale, it goes first.
    fn may_redraw_page(&mut self, pass: u64, index: u32, generation: u64) -> bool {
        if self.ext.shell.redraw_pass == Some(pass) {
            return false;
        }
        let Some(caret) = self.redraw_caret_page(generation) else {
            return true;
        };
        if caret == index {
            return true;
        }
        let caret_stale = self.pages.contains_key(&caret)
            && self.ext.shell.page_drawn.get(&caret) != Some(&generation);
        let (_, now, before) = &self.ext.shell.requested;
        let caret_visible = now.contains(&caret) || before.contains(&caret);
        !(caret_stale && caret_visible)
    }

    fn redraw_caret_page(&mut self, generation: u64) -> Option<u32> {
        if let Some((at, page)) = self.ext.shell.caret_page
            && at == generation
        {
            return Some(page);
        }
        let page = u32::try_from(self.session.caret().ok()?["pageIndex"].as_u64()?).ok()?;
        self.ext.shell.caret_page = Some((generation, page));
        Some(page)
    }
}

#[cfg(test)]
mod redraw_tests {
    use super::*;

    fn app_with_pages(n: usize) -> anyhow::Result<HwpApp> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        for i in 0..n {
            if i > 0 {
                a.session.execute(Command::PageBreak)?;
            }
            a.session
                .execute(Command::Insert(format!("{}쪽 본문", i + 1)))?;
        }
        Ok(a)
    }

    /// One pass of the view asking for `pages` in order, as the canvas does.
    fn pass(a: &mut HwpApp, pages: &[u32]) {
        let ctx = a.ctx.clone();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            for page in pages {
                let _ = a.shell_page_texture(&ctx, *page).unwrap();
            }
        });
        output.textures_delta.clear();
    }

    fn current(a: &HwpApp) -> Vec<bool> {
        let g = a.session.generation;
        (0..4)
            .map(|p| a.ext.shell.page_drawn.get(&p) == Some(&g))
            .collect()
    }

    #[test]
    fn edit_redraws_the_caret_page_first_then_one_page_per_pass() -> anyhow::Result<()> {
        let mut a = app_with_pages(4)?;
        let visible = [0, 1, 2, 3];
        pass(&mut a, &visible);
        assert_eq!(
            current(&a),
            [true; 4],
            "first view draws every page at once"
        );
        // Type on the third page.
        let at = a
            .session
            .cursors()
            .into_iter()
            .find(|c| {
                a.session
                    .paragraph(*c)
                    .is_ok_and(|p| p.text.starts_with("3쪽"))
            })
            .ok_or_else(|| anyhow::anyhow!("third page paragraph"))?;
        a.session.select(at, false)?;
        a.session.execute(Command::Insert("가".into()))?;
        pass(&mut a, &visible);
        assert_eq!(current(&a), [false, false, true, false], "caret page first");
        pass(&mut a, &visible);
        assert_eq!(current(&a).iter().filter(|c| **c).count(), 2);
        pass(&mut a, &visible);
        pass(&mut a, &visible);
        assert_eq!(
            current(&a),
            [true; 4],
            "every page current within the visible count"
        );
        Ok(())
    }

    #[test]
    fn a_page_without_a_texture_is_drawn_immediately() -> anyhow::Result<()> {
        let mut a = app_with_pages(4)?;
        pass(&mut a, &[0, 1, 2]);
        a.session.select(Cursor::default(), false)?;
        a.session.execute(Command::Insert("가".into()))?;
        // Page 3 scrolls into view in the same pass that redraws the caret page.
        pass(&mut a, &[0, 1, 2, 3]);
        let now = current(&a);
        assert!(now[0], "caret page redrawn");
        assert!(now[3], "newly visible page drawn at once");
        assert!(!now[1] && !now[2], "other stale pages wait: {now:?}");
        Ok(())
    }

    #[test]
    fn a_stale_caret_page_out_of_view_does_not_block_the_others() -> anyhow::Result<()> {
        let mut a = app_with_pages(4)?;
        pass(&mut a, &[0, 1, 2, 3]);
        a.session.select(Cursor::default(), false)?;
        a.session.execute(Command::Insert("가".into()))?;
        // The view has moved away from the caret's first page.
        pass(&mut a, &[2, 3]);
        pass(&mut a, &[2, 3]);
        pass(&mut a, &[2, 3]);
        let now = current(&a);
        assert!(now[2] && now[3], "{now:?}");
        assert!(
            !now[0],
            "the caret page is redrawn only when it is shown again"
        );
        Ok(())
    }
}

// ----- PDF 저장을 백그라운드로: 인쇄 → PDF·쪽 범위 저장·블록 저장 (파일 메뉴와 같은 PdfSlot) -----

/// Pages and the conversion of a prepared PDF (`Session::pdf_job` and friends).
type PreparedPdf = (usize, Box<dyn FnOnce() -> anyhow::Result<Vec<u8>> + Send>);

impl HwpApp {
    /// Hand a prepared PDF to the shared background slot (one export at a time), so the
    /// slow font layout and embedding never run on the UI thread. True when started.
    fn start_shell_pdf(&mut self, path: PathBuf, prepared: anyhow::Result<PreparedPdf>) -> bool {
        // Checked before `prepared` is used: a busy slot should not cost the page
        // preparation (seconds on long documents) only to be refused afterwards.
        if self.pdf.busy() {
            self.error =
                Some("이전 PDF 저장이 아직 진행 중입니다. 끝난 뒤 다시 저장해 주세요.".into());
            return false;
        }
        let Some((pages, convert)) = self.result(prepared) else {
            return false;
        };
        // Same status line as the file menu, refreshed with the elapsed seconds.
        self.begin_pdf_export(pages, path, convert)
    }
    /// 쪽 범위 저장 as PDF (1-based, inclusive), in the background. True when started.
    pub(crate) fn start_pages_pdf(&mut self, path: PathBuf, from: u32, to: u32) -> bool {
        let prepared = self.session.page_count().and_then(|count| {
            if from == 0 || to < from || to > count {
                anyhow::bail!("쪽 범위는 1–{count} 안에서 작은 번호부터 적으세요: {from}–{to}");
            }
            let job = self
                .session
                .pdf_job(&((from - 1)..to).collect::<Vec<_>>())?;
            Ok((job.page_count(), Box::new(move || job.convert()) as Box<_>))
        });
        self.start_shell_pdf(path, prepared)
    }
    /// 블록 저장 as PDF, in the background. True when started.
    pub(crate) fn start_block_pdf(&mut self, path: PathBuf) -> bool {
        let prepared = self
            .session
            .save_block_pdf_job()
            .map(|job| -> PreparedPdf { (job.page_count(), Box::new(move || job.convert())) });
        self.start_shell_pdf(path, prepared)
    }
}

#[cfg(test)]
mod pdf_tests {
    use super::*;

    fn three_pages() -> anyhow::Result<HwpApp> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session.execute(Command::Insert("1쪽".into()))?;
        a.session.execute(Command::PageBreak)?;
        a.session.execute(Command::Insert("2쪽 블록".into()))?;
        a.session.execute(Command::PageBreak)?;
        a.session.execute(Command::Insert("3쪽".into()))?;
        Ok(a)
    }

    /// Frames keep running while the worker converts; returns once it has finished.
    fn wait(a: &mut HwpApp) {
        let ctx = a.ctx.clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while a.pdf.busy() {
            assert!(
                std::time::Instant::now() < deadline,
                "PDF export did not finish"
            );
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
                a.poll_pdf_export(&ctx);
                a.shell_print_when_ready();
            });
            output.textures_delta.clear();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn is_pdf(path: &std::path::Path) -> bool {
        std::fs::read(path).is_ok_and(|b| b.starts_with(b"%PDF"))
    }

    #[test]
    fn print_page_range_and_block_pdfs_are_written_in_the_background() -> anyhow::Result<()> {
        let dir = tempfile_dir("bg-pdf")?;
        let mut a = three_pages()?;
        let print = dir.join("print.pdf");
        assert!(a.start_print_pdf(print.clone(), "2-3", 1, true));
        assert!(
            a.message.contains("초 경과"),
            "shared live progress line: {}",
            a.message
        );
        wait(&mut a);
        assert!(is_pdf(&print));
        assert!(a.message.contains("PDF 저장 완료"), "{}", a.message);
        let pages = dir.join("pages.pdf");
        assert!(a.start_pages_pdf(pages.clone(), 1, 2));
        wait(&mut a);
        assert!(is_pdf(&pages));
        assert!(
            !a.start_pages_pdf(dir.join("bad.pdf"), 2, 9),
            "bad range refused"
        );
        a.error = None;
        let at = a
            .session
            .cursors()
            .into_iter()
            .find(|c| {
                a.session
                    .paragraph(*c)
                    .is_ok_and(|p| p.text.contains("블록"))
            })
            .ok_or_else(|| anyhow::anyhow!("block"))?;
        a.session.select(Cursor { offset: 3, ..at }, false)?;
        a.session.select(Cursor { offset: 5, ..at }, true)?;
        let block = dir.join("block.pdf");
        assert!(a.start_block_pdf(block.clone()));
        wait(&mut a);
        assert!(is_pdf(&block));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_second_pdf_waits_for_the_first_and_printing_waits_for_its_file() -> anyhow::Result<()> {
        let dir = tempfile_dir("bg-pdf-busy")?;
        let mut a = three_pages()?;
        // A slow export holds the single slot.
        let slow = dir.join("slow.pdf");
        a.pdf.start(1, slow.clone(), || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            Ok(b"%PDF-1.7 slow".to_vec())
        })?;
        assert!(!a.start_pages_pdf(dir.join("second.pdf"), 1, 1));
        assert!(a.error.as_deref().is_some_and(|e| e.contains("진행 중")));
        a.error = None;
        // A print request whose file never appears is dropped, never sent.
        a.ext.shell.print_after = Some((dir.join("missing.pdf"), 1, 1));
        a.shell_print_when_ready();
        assert!(
            a.ext.shell.print_after.is_some(),
            "held while the slot is busy"
        );
        wait(&mut a);
        assert!(is_pdf(&slow));
        assert!(
            a.ext.shell.print_after.is_none(),
            "dropped once the slot is free"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    fn tempfile_dir(name: &str) -> anyhow::Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("hwpcraft-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}
