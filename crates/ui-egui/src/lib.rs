pub const APP_NAME: &str = "HwpCraft";
use anyhow::Result;
use egui::{Color32, Event, ImeEvent, Key, Pos2, Rect, RichText, Stroke, Vec2};
use hwpcraft_engine::{Command, Cursor, PageSetup, Session};
mod autotext_dialog;
mod body_autotext_editor;
mod chrome;
mod clipboard;
mod comments_editor;
mod context_editor;
mod context_editor_v2;
mod document_scroll;
mod ext_core;
mod ext_format;
mod ext_object;
mod ext_shell;
mod ext_table;
mod extensions;
mod feature_dialogs;
mod find_dialog;
mod format_dialogs;
mod hanja_dialog;
mod icons;
#[cfg(test)]
mod input_format_tests;
mod margin_guides;
mod mcp_bridge;
mod nested_edit;
mod object_dialogs;
mod page_setup;
mod pdf_export;
mod platform;
pub use platform::{app_icon, write_icon_png};
mod review_dialogs;
mod ribbon;
mod ribbon_layout;
mod rulers;
mod split_view;
mod symbol_dialog;
mod table_drawing;
mod wheel_zoom;
use hwpcraft_render::Renderer;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

enum Pending {
    New,
    Open(PathBuf),
    Quit,
}
struct PasswordPrompt {
    path: PathBuf,
    password: String,
    error: Option<String>,
}
pub struct HwpApp {
    pub session: Session,
    renderer: Renderer,
    icons: icons::Icons,
    show_navigation: bool,
    show_editor: bool,
    show_rulers: bool,
    show_ribbon: bool,
    show_table: bool,
    show_character: bool,
    format_draft: Option<format_dialogs::FormatDraft>,
    show_paragraph: bool,
    feature_dialog: Option<feature_dialogs::FeatureDialog>,
    format_clipboard: Option<serde_json::Value>,
    /// Asked when an encrypted document is opened without a password.
    password_prompt: Option<PasswordPrompt>,
    /// 문자표 (Ctrl+F10).
    symbol_dialog: Option<symbol_dialog::State>,
    /// 형광펜 overlay rectangles for the current document generation.
    equation_preview: Option<(String, egui::TextureHandle, Vec2)>,
    font_family: String,
    line_spacing: f32,
    line_spacing_type: String,
    alignment: String,
    pub(crate) pages: BTreeMap<u32, (egui::TextureHandle, Vec2)>,
    zoom: f32,
    wheel_zoom: wheel_zoom::State,
    margin_guides: margin_guides::State,
    document_scroll: document_scroll::State,
    tab: usize,
    page: u32,
    message: String,
    error: Option<String>,
    pending: Option<Pending>,
    allow_close: bool,
    /// Background PDF export (see `pdf_export`).
    pdf: pdf_export::PdfSlot,
    find: String,
    replacement: String,
    show_find: bool,
    show_help: bool,
    page_setup: Option<PageSetup>,
    font_size: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    rows: u16,
    columns: u16,
    editor: String,
    editor_cursor: Cursor,
    editor_generation: u64,
    preedit: String,
    frames: u64,
    screenshot: Option<PathBuf>,
    exit_after_screenshot: bool,
    ext: extensions::ExtState,
    /// Kept for start-up views and module hooks that run outside a frame.
    ctx: egui::Context,
    find_state: find_dialog::FindState,
    page_sizes: (u64, Vec<Vec2>),
    /// Page the scroll view last showed; a different `page` means "scroll there".
    shown_page: u32,
    scroll_to_caret: bool,
    /// 편집 화면 나누기 (None: one view).
    split: Option<split_view::Split>,
    /// Pane being drawn (0 when not split) and whether it takes input.
    pane: usize,
    pane_active: bool,
    /// Screen rectangle of the top visible page, for rulers and overlays.
    view_page_rect: Option<Rect>,
}
/// Gap between pages in the continuous view, matching Hangul's page spacing.
const PAGE_GAP: f32 = 14.0;
impl HwpApp {
    /// Select a workspace for startup and reproducible desktop review.
    pub fn set_initial_view(&mut self, name: &str) -> Result<()> {
        self.sync_formatting();
        match name {
            "character" => self.show_character = true,
            "paragraph" => self.show_paragraph = true,
            "body-autotext" => self.open_body_autotext(),
            "table-create" => {
                let ctx = self.ctx.clone();
                self.ext_dispatch("table.create", &ctx);
                if !self.ext_enabled("table.create") {
                    self.show_table = true;
                }
            }
            "equation" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                    object_dialogs::ObjectDialog::equation(),
                ))
            }
            "objects" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                    object_dialogs::ObjectDialog::Objects,
                ))
            }
            "styles" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                    object_dialogs::ObjectDialog::Styles,
                ))
            }
            "symbols" => self.open_symbol_dialog(),
            // symbols-user / -unicode / -hnc / -ks / -emoji / -special / -search:<query>
            name if name.starts_with("symbols-") => {
                self.open_symbol_dialog();
                if let Some(st) = self.symbol_dialog.as_mut() {
                    st.show_view(&name["symbols-".len()..]);
                }
            }
            "shape" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(
                    object_dialogs::ObjectDialog::shape(hwpcraft_engine::ShapeKind::Rectangle),
                ))
            }
            "file" => self.tab = 0,
            "edit" => self.tab = 1,
            "insert" => self.tab = 3,
            "page" => self.tab = 5,
            "table" => {
                if let Some((cursor, _)) = self
                    .session
                    .entries()
                    .into_iter()
                    .find(|(c, _)| c.cell.is_some())
                {
                    self.session.select(cursor, false)?;
                    self.tab = 9;
                } else {
                    anyhow::bail!("표가 있는 문서를 열어 주세요.");
                }
            }
            "page-setup" => self.open_page_setup(),
            "bookmarks" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Bookmarks {
                    name: String::new(),
                })
            }
            "hyperlink" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Link {
                    uri: String::new(),
                    text: String::new(),
                })
            }
            "columns" => {
                self.feature_dialog = Some(feature_dialogs::FeatureDialog::Columns {
                    count: 2,
                    spacing: 8.0,
                })
            }
            "find" => self.open_find(false),
            "replace" => self.open_find(true),
            name if self.ext_view(name) => {}
            _ => anyhow::bail!("알 수 없는 보기: {name}"),
        }
        Ok(())
    }
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        file: Option<&Path>,
        screenshot: Option<PathBuf>,
        exit_after_screenshot: bool,
    ) -> Result<Self> {
        Self::with_context(cc.egui_ctx.clone(), file, screenshot, exit_after_screenshot)
    }
    fn with_context(
        ctx: egui::Context,
        file: Option<&Path>,
        screenshot: Option<PathBuf>,
        exit_after_screenshot: bool,
    ) -> Result<Self> {
        let mut fonts = egui::FontDefinitions::default();
        // The OS Korean font (맑은 고딕 / Apple SD Gothic Neo / Noto Sans CJK KR …).
        if let Some((path, index)) = hwpcraft_render::korean_font_face()
            && let Ok(bytes) = std::fs::read(&path)
        {
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            fonts.font_data.insert("korean".into(), data.into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "korean".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("korean".into());
        }
        // Symbol / emoji fonts widen 문자표 coverage; they sit after the Korean font.
        for (i, (path, index)) in hwpcraft_render::symbol_font_faces().into_iter().enumerate() {
            if let Ok(bytes) = std::fs::read(&path) {
                let mut data = egui::FontData::from_owned(bytes);
                data.index = index;
                let name = format!("symbol-{i}");
                fonts.font_data.insert(name.clone(), data.into());
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push(name);
            }
        }
        ctx.set_fonts(fonts);
        ctx.set_visuals(egui::Visuals::light());
        ctx.style_mut_of(egui::Theme::Light, |s| {
            s.spacing.button_padding = Vec2::new(7.0, 4.0);
            s.spacing.item_spacing = Vec2::new(4.0, 4.0);
            s.text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(12.0));
            s.text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(12.0));
            s.visuals.panel_fill = Color32::WHITE;
            s.visuals.selection.bg_fill = Color32::from_rgb(196, 219, 255);
        });
        let mut session = Session::new()?;
        // An encrypted file given on the command line opens the password prompt
        // instead of failing app start-up.
        let mut deferred_password = None;
        if let Some(path) = file {
            match session.open(path) {
                Ok(()) => {}
                Err(e) if hwpcraft_formats::needs_password(&e) => {
                    deferred_password = Some(path.to_owned());
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Self {
            session,
            renderer: Renderer::default(),
            icons: icons::Icons::new(&ctx),
            show_navigation: false,
            show_editor: false,
            show_rulers: true,
            show_ribbon: true,
            show_table: false,
            show_character: false,
            format_draft: None,
            show_paragraph: false,
            feature_dialog: None,
            format_clipboard: None,
            password_prompt: deferred_password.map(|path| PasswordPrompt {
                path,
                password: String::new(),
                error: None,
            }),
            symbol_dialog: None,
            equation_preview: None,
            font_family: hwpcraft_render::default_korean_font().into(),
            line_spacing: 160.0,
            line_spacing_type: "Percent".into(),
            alignment: "justify".into(),
            pages: BTreeMap::new(),
            zoom: 0.9,
            wheel_zoom: Default::default(),
            margin_guides: Default::default(),
            document_scroll: Default::default(),
            tab: 1,
            page: 0,
            message: "문서를 열거나 흰 쪽을 클릭해 입력하세요.".into(),
            error: None,
            pending: None,
            allow_close: false,
            pdf: pdf_export::PdfSlot::default(),
            find: String::new(),
            replacement: String::new(),
            show_find: false,
            show_help: false,
            page_setup: None,
            font_size: 11.0,
            bold: false,
            italic: false,
            underline: false,
            rows: 3,
            columns: 3,
            editor: String::new(),
            editor_cursor: Cursor::default(),
            editor_generation: u64::MAX,
            preedit: String::new(),
            frames: 0,
            screenshot,
            exit_after_screenshot,
            ext: extensions::ExtState::default(),
            ctx,
            find_state: find_dialog::FindState::default(),
            page_sizes: (u64::MAX, Vec::new()),
            shown_page: 0,
            scroll_to_caret: false,
            split: None,
            pane: 0,
            pane_active: true,
            view_page_rect: None,
        })
    }
    fn result<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                None
            }
        }
    }
    fn command(&mut self, command: Command) {
        if self.nested_command(&command).is_some() {
            return;
        }
        let r = self.session.execute(command);
        self.result(r);
        self.sync_formatting();
        self.follow_cursor();
    }
    fn run(&mut self, id: &str, params: serde_json::Value) -> bool {
        if let Some(success) = self.nested_run(id, &params) {
            return success;
        }
        let result = self.session.run(id, params);
        let success = self.result(result).is_some();
        self.sync_formatting();
        self.follow_cursor();
        success
    }
    fn sync_formatting(&mut self) {
        if self.show_character || self.show_paragraph {
            return;
        }
        if let Ok(properties) = self.session.character_properties() {
            self.bold = properties["bold"].as_bool().unwrap_or(false);
            self.italic = properties["italic"].as_bool().unwrap_or(false);
            self.underline = properties["underline"].as_bool().unwrap_or(false);
            self.font_size = properties["fontSize"].as_f64().unwrap_or(1100.0) as f32 / 100.0;
            self.font_family = properties["fontFamily"]
                .as_str()
                .unwrap_or(hwpcraft_render::default_korean_font())
                .to_owned();
        }
        if let Ok(properties) = self.session.paragraph_format() {
            self.alignment = properties["alignment"]
                .as_str()
                .unwrap_or("justify")
                .to_owned();
            self.line_spacing_type = properties["lineSpacingType"]
                .as_str()
                .unwrap_or("Percent")
                .to_owned();
            self.line_spacing = properties["lineSpacing"].as_f64().unwrap_or(160.0) as f32
                / if self.line_spacing_type == "Percent" {
                    1.0
                } else {
                    100.0
                };
        }
    }
    fn open_page_setup(&mut self) {
        let result = self.session.page_setup();
        self.page_setup = self.result(result);
    }
    fn request(&mut self, action: Pending, ctx: &egui::Context) {
        if self.session.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }
    fn perform(&mut self, action: Pending, ctx: &egui::Context) {
        match action {
            Pending::New => {
                let r = self.session.reset();
                self.result(r);
                self.page = 0;
            }
            Pending::Open(path) => match self.session.open(&path) {
                Ok(()) => {
                    self.page = 0;
                    self.message = format!("열기 완료 · {}", path.display());
                }
                Err(e) if hwpcraft_formats::needs_password(&e) => {
                    self.password_prompt = Some(PasswordPrompt {
                        path,
                        password: String::new(),
                        error: None,
                    });
                }
                Err(e) => {
                    self.result::<()>(Err(e));
                }
            },
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.preedit.clear();
    }
    fn open(&mut self, ctx: &egui::Context) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("한글 문서", &["hwp", "hwpx"])
            .pick_file()
        {
            self.request(Pending::Open(p), ctx);
        }
    }
    fn save(&mut self, save_as: bool, extension: Option<&str>) -> bool {
        let existing = if !save_as && extension.is_none() {
            self.session.path.clone()
        } else {
            None
        };
        let path = existing.or_else(|| {
            let name = self
                .session
                .path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "새 문서".into());
            let dialog = rfd::FileDialog::new()
                .set_file_name(format!("{name}.{}", extension.unwrap_or("hwpx")));
            let dialog = if let Some(ext) = extension {
                dialog.add_filter(ext, &[ext])
            } else {
                dialog
                    .add_filter("한글 표준 문서 (HWPX)", &["hwpx"])
                    .add_filter("한글 문서 (HWP)", &["hwp"])
                    .add_filter("Word 문서 (DOCX)", &["docx"])
            };
            dialog.save_file()
        });
        let Some(mut path) = path else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension(extension.unwrap_or("hwpx"));
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            // Conversion takes minutes on long documents; keep the window live.
            return self.start_pdf_export(path);
        }
        // A document with Hangul track changes asks first (the records cannot be kept).
        if self.confirm_tracked_save(&path) {
            return false;
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            // An export, like PDF: the document keeps its own file and saved state.
            return self.export_docx(&path, false);
        }
        let r = self.session.save(&path);
        if self.result(r).is_some() {
            self.message = format!("저장 완료 · {}", path.display());
            true
        } else {
            false
        }
    }
    fn navigator(&mut self, ui: &mut egui::Ui) {
        // Hangul docks its task panes on the right, leaving the vertical ruler by the page.
        egui::Panel::right("navigator")
            .default_size(220.0)
            .size_range(170.0..=320.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.add_space(10.0);
                ui.heading("문서 탐색");
                ui.label(format!(
                    "{}쪽 · {}개 문단/셀",
                    self.layout_page_count(),
                    self.session.entries().len()
                ));
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("outline")
                    .show(ui, |ui| {
                        for (i, (c, text)) in self.session.entries().into_iter().enumerate() {
                            let selected = c.section == self.session.cursor.section
                                && c.paragraph == self.session.cursor.paragraph
                                && c.cell == self.session.cursor.cell;
                            let label = if text.trim().is_empty() {
                                format!("{}  빈 문단", i + 1)
                            } else {
                                format!("{}  {}", i + 1, text)
                            };
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(label).size(12.0))
                                        .selected(selected)
                                        .wrap(),
                                )
                                .clicked()
                            {
                                let r = self.session.select(c, false);
                                self.result(r);
                                self.follow_cursor();
                            }
                        }
                    });
            });
    }
    fn follow_cursor(&mut self) {
        if let Ok(v) = self.session.caret() {
            self.page = v["pageIndex"].as_u64().unwrap_or(0) as u32;
            self.shown_page = self.page;
            self.scroll_to_caret = true;
        }
    }
    fn paragraph_editor(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("paragraph-editor")
            .default_size(145.0)
            .size_range(100.0..=280.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(if self.session.cursor.cell.is_some() {
                        "표 셀 편집"
                    } else {
                        "문단 편집"
                    });
                    ui.label(
                        RichText::new("한글 입력 · 선택 영역 서식 · Enter로 문단 나누기").weak(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{} 글자", self.session.len()));
                    });
                });
                if self.editor_generation != self.session.generation
                    || self.editor_cursor != self.session.cursor
                {
                    self.editor = self.session.text().to_owned();
                    self.editor_cursor = self.session.cursor;
                    self.editor_generation = self.session.generation;
                }
                let before = self.session.cursor;
                let output = egui::TextEdit::multiline(&mut self.editor)
                    .id(egui::Id::new("paragraph-input"))
                    .font(egui::FontId::proportional(18.0))
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .hint_text("문단의 내용을 입력하세요. 페이지에도 바로 반영됩니다.")
                    .show(ui);
                if output.response.changed() {
                    let text = self.editor.clone();
                    let r = self.session.replace_paragraph_text(&text);
                    self.result(r);
                    self.editor_generation = self.session.generation;
                    if self.session.cursor.paragraph != before.paragraph
                        || self.session.cursor.cell != before.cell
                    {
                        self.editor_generation = u64::MAX;
                    }
                }
                if output.response.has_focus()
                    && self.session.cursor.paragraph == before.paragraph
                    && self.session.cursor.cell == before.cell
                    && let Some(range) = output.cursor_range
                {
                    self.session.cursor.offset = range.primary.index.0.min(self.session.len());
                    self.session.anchor = if range.is_empty() {
                        None
                    } else {
                        Some(range.secondary.index.0.min(self.session.len()))
                    };
                }
                self.editor_cursor = self.session.cursor;
            });
    }
    /// Page sizes in layout pixels, refreshed when the document changes.
    fn layout_page_count(&self) -> u32 {
        self.session.page_count().unwrap_or_default()
    }

    fn page_sizes(&mut self) -> Vec<Vec2> {
        if self.page_sizes.0 != self.session.generation {
            let count = match self.session.page_count() {
                Ok(count) => count,
                Err(error) => {
                    self.message = format!("쪽 조판 실패: {error}");
                    return Vec::new();
                }
            };
            // Each page's size, and its body band for 쪽 윤곽 off.
            let (sizes, bodies) = (0..count)
                .map(|i| {
                    let info = self.session.page_info(i).ok();
                    let num = |k: &str| info.as_ref().and_then(|v| v[k].as_f64()).map(|x| x as f32);
                    let size = num("width")
                        .zip(num("height"))
                        .map_or(Vec2::new(794.0, 1123.0), |(w, h)| Vec2::new(w, h));
                    let edge = |a: &str, b: &str| num(a).unwrap_or(0.0) + num(b).unwrap_or(0.0);
                    let top = edge("marginTop", "marginHeader").min(size.y);
                    let bottom = (size.y - edge("marginBottom", "marginFooter")).max(top);
                    (size, (top, bottom))
                })
                .unzip();
            self.page_sizes = (self.session.generation, sizes);
            self.ext.shell.page_bodies = bodies;
        }
        self.page_sizes.1.clone()
    }
    /// Render a page on demand; only visible pages are rasterized and cached.
    fn page_texture(&mut self, ctx: &egui::Context, index: u32) -> Result<egui::TextureHandle> {
        // Stale pages stay on screen; the shell-page queue redraws one per pass.
        self.shell_page_texture(ctx, index)
    }
    /// Hangul-style continuous view: every page stacked vertically on a grey desk.
    fn page_view(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let sizes = self.page_sizes();
        let count = sizes.len() as u32;
        self.page = self.page.min(count.saturating_sub(1));
        let jump = (self.page != self.shown_page).then_some(self.page);
        let follow = std::mem::take(&mut self.scroll_to_caret);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_gray(245))
                    .inner_margin(0.0),
            )
            .show(ui, |ui| {
                let offset = self.document_wheel_zoom(ui, &sizes);
                self.wheel_zoom.pages.clear();
                let mut scroll = egui::ScrollArea::both()
                    .id_salt("document-scroll")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
                if let Some(x) = self.document_scroll.pending[0].take() {
                    scroll = scroll.horizontal_scroll_offset(x);
                }
                if let Some(y) = self.document_scroll.pending[1].take() {
                    scroll = scroll.vertical_scroll_offset(y);
                }
                if let Some(offset) = offset {
                    scroll = scroll.scroll_offset(offset);
                }
                let output = scroll.show(ui, |ui| {
                    // Focus target shared by all pages; keeps Tab/arrows for editing.
                    // Only the active pane of a split view registers it.
                    let canvas = egui::Id::new("document-canvas");
                    if self.pane_active {
                        ui.interact(
                            ui.clip_rect(),
                            canvas,
                            egui::Sense::focusable_noninteractive(),
                        );
                    }
                    ctx.memory_mut(|m| {
                        m.set_focus_lock_filter(
                            canvas,
                            egui::EventFilter {
                                tab: true,
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                escape: false,
                            },
                        )
                    });
                    // About 15 px between the ruler and the first page.
                    ui.add_space(15.0);
                    // A caret in a nested table cell or text box draws from its own rectangle.
                    self.session.revalidate_nested();
                    let caret = if self.session.nested_caret().is_some() {
                        self.session.nested_caret_rect().ok()
                    } else {
                        self.session.caret().ok()
                    };
                    let mut top_page = None;
                    // 맞쪽 보기 (Hangul's facing pages): page 1 alone on the right, then
                    // even/odd pairs side by side; otherwise one page per row.
                    let mirror = self.ext.shell.mirror_pages;
                    let mut rows: Vec<Vec<u32>> = Vec::new();
                    for index in 0..count {
                        if mirror
                            && index > 0
                            && index % 2 == 0
                            && let Some(row) = rows.last_mut()
                        {
                            row.push(index);
                            continue;
                        }
                        rows.push(vec![index]);
                    }
                    // 쪽 윤곽 off: each page shows only its body band, pages touching.
                    let outline = self.ext.shell.page_outline;
                    if !outline {
                        ui.spacing_mut().item_spacing.y = 0.0;
                    }
                    let band = |app: &Self, index: u32| {
                        let size = sizes[index as usize];
                        match app.ext.shell.page_bodies.get(index as usize) {
                            Some(&(top, bottom)) if !outline => (top, bottom),
                            _ => (0.0, size.y),
                        }
                    };
                    for row in rows {
                        let page_sizes: Vec<egui::Vec2> = row
                            .iter()
                            .map(|i| {
                                let (top, bottom) = band(self, *i);
                                Vec2::new(sizes[*i as usize].x, bottom - top) * self.zoom
                            })
                            .collect();
                        let total_width = page_sizes.iter().map(|s| s.x).sum::<f32>()
                            + PAGE_GAP * (row.len() as f32 - 1.0);
                        // A lone first page in facing view sits where the right page goes.
                        let lone_right = mirror && row.len() == 1 && row[0] == 0 && count > 1;
                        ui.horizontal(|ui| {
                            let mut lead = ((ui.available_width() - total_width) / 2.0).max(0.0);
                            if lone_right {
                                lead = ((ui.available_width() + PAGE_GAP) / 2.0).max(lead);
                            }
                            ui.add_space(lead);
                            for (n, index) in row.iter().copied().enumerate() {
                                if n > 0 {
                                    ui.add_space(PAGE_GAP);
                                }
                                let size = page_sizes[n];
                                let (shown, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                                if jump == Some(index) {
                                    ui.scroll_to_rect(shown, Some(egui::Align::TOP));
                                }
                                // The whole page, placed so its body band lands on `shown`;
                                // with 쪽 윤곽 on the two are the same.
                                let top = band(self, index).0 * self.zoom;
                                let rect = Rect::from_min_size(
                                    shown.min - Vec2::new(0.0, top),
                                    sizes[index as usize] * self.zoom,
                                );
                                let visible = ui.is_rect_visible(shown);
                                if visible {
                                    self.wheel_zoom.pages.push((index, rect));
                                }
                                if visible && top_page.is_none() {
                                    top_page = Some((index, rect));
                                }
                                if visible || jump == Some(index) {
                                    if outline {
                                        self.page_canvas(
                                            ui,
                                            &ctx,
                                            index,
                                            rect,
                                            caret.as_ref(),
                                            follow,
                                        );
                                    } else {
                                        let clip = shown.intersect(ui.clip_rect());
                                        let mut band_ui =
                                            ui.new_child(egui::UiBuilder::new().max_rect(shown));
                                        band_ui.set_clip_rect(clip);
                                        self.page_canvas(
                                            &mut band_ui,
                                            &ctx,
                                            index,
                                            rect,
                                            caret.as_ref(),
                                            follow,
                                        );
                                        if index + 1 < count {
                                            dotted_line(
                                                ui,
                                                shown.left_bottom(),
                                                shown.right_bottom(),
                                            );
                                        }
                                    }
                                }
                            }
                        });
                        if outline {
                            ui.add_space(PAGE_GAP);
                        }
                    }
                    if let Some((index, rect)) = top_page {
                        // Fixed rulers outside the scroll view read this rectangle.
                        self.view_page_rect = Some(rect);
                        if jump.is_none() {
                            self.page = index;
                        }
                    }
                });
                self.wheel_zoom.viewport = Some(output.inner_rect);
                self.document_scroll.update(&ctx, &output);
            });
        self.shown_page = self.page;
    }
    fn page_canvas(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        page: u32,
        rect: Rect,
        caret: Option<&serde_json::Value>,
        follow: bool,
    ) {
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        if self.ext.shell.page_outline {
            ui.painter().rect_stroke(
                rect.expand(1.0),
                0.0,
                // Page frame: #ababab 1 px border, no shadow.
                Stroke::new(1.0, Color32::from_gray(0xab)),
                egui::StrokeKind::Outside,
            );
        }
        match self.page_texture(ctx, page) {
            Ok(texture) => {
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            Err(e) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("쪽을 표시하지 못했습니다: {e}"),
                    egui::FontId::proportional(13.0),
                    Color32::DARK_RED,
                );
            }
        }
        self.paint_margin_corners(ui, page, rect);
        let response = ui.interact(
            rect,
            egui::Id::new(("document-page", self.pane, page)),
            egui::Sense::click_and_drag(),
        );
        // One shared focus target keeps keyboard and IME routing independent of the page.
        let canvas = egui::Id::new("document-canvas");
        if response.clicked() || response.drag_started() || response.secondary_clicked() {
            ctx.memory_mut(|m| m.request_focus(canvas));
        }
        let focused = ctx.memory(|m| m.has_focus(canvas));
        // Selection rectangles are computed per line and cached per page.
        for r in self.selection_rects(page) {
            ui.painter().rect_filled(
                Rect::from_min_size(rect.min + r.min.to_vec2() * self.zoom, r.size() * self.zoom),
                0.0,
                Color32::from_rgba_unmultiplied(66, 133, 244, 65),
            );
        }
        if self.session.nested_caret().is_some() {
            for r in self.session.nested_selection_rects().unwrap_or_default() {
                if r["pageIndex"].as_u64() != Some(u64::from(page)) {
                    continue;
                }
                let n = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32 * self.zoom;
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        rect.min + Vec2::new(n("x"), n("y")),
                        Vec2::new(n("width"), n("height")),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(66, 133, 244, 65),
                );
            }
        }
        // Module canvas modes (e.g. table cell blocks) may take over pointer input.
        if !self.ext_canvas(page, &response, rect) {
            let hit = |s: &Self, p: Pos2| {
                let pos = (p - rect.min) / s.zoom;
                s.session.hit_test(page, pos.x as f64, pos.y as f64)
            };
            if (response.clicked() || response.dragged())
                && let Some(p) = response.interact_pointer_pos()
            {
                let extend = response.dragged() || ctx.input(|i| i.modifiers.shift);
                let pos = (p - rect.min) / self.zoom;
                match hit(self, p) {
                    Ok(c) if !(response.dragged() && self.session.nested_caret().is_some()) => {
                        self.session.clear_nested();
                        let r = self.session.select(c, extend);
                        self.result(r);
                        self.page = page;
                        self.shown_page = page;
                    }
                    Ok(_) => {}
                    // The body caret cannot stand in a nested cell or a text box.
                    Err(e) => {
                        let from_body = response.dragged() && self.session.nested_caret().is_none();
                        if !from_body && self.nested_click(page, pos.x as f64, pos.y as f64, extend)
                        {
                            self.page = page;
                            self.shown_page = page;
                        } else if !response.dragged() {
                            self.result(Err::<(), _>(e));
                        }
                    }
                }
            }
            if self.session.nested_caret().is_some() {
                if response.triple_clicked() || response.double_clicked() {
                    self.nested_multi_click(response.triple_clicked());
                }
            } else if response.triple_clicked() {
                self.session.select_paragraph();
            } else if response.double_clicked() {
                let r = self.session.select_word();
                self.result(r);
            }
            if response.secondary_clicked()
                && !self.session.has_selection()
                && let Some(p) = response.interact_pointer_pos()
            {
                let r = hit(self, p);
                if let Some(c) = self.result(r) {
                    let r = self.session.select(c, false);
                    self.result(r);
                }
            }
        }
        response.context_menu(|ui| self.canvas_context_menu(ui));
        let Some(caret) = caret.filter(|c| c["pageIndex"].as_u64() == Some(page as u64)) else {
            return;
        };
        let x = caret["x"].as_f64().unwrap_or(0.0) as f32 * self.zoom + rect.min.x;
        let y = caret["y"].as_f64().unwrap_or(0.0) as f32 * self.zoom + rect.min.y;
        let height = caret["height"].as_f64().unwrap_or(16.0) as f32 * self.zoom;
        let caret_rect = Rect::from_min_size(
            Pos2::new(x, y),
            Vec2::new(
                if self.session.overwrite {
                    height * 0.5
                } else {
                    2.0
                },
                height,
            ),
        );
        if follow {
            ui.scroll_to_rect(caret_rect.expand(24.0), None);
        }
        if !focused {
            return;
        }
        ui.painter().rect_filled(
            caret_rect,
            0.0,
            if self.session.overwrite {
                Color32::from_rgba_unmultiplied(24, 87, 170, 110)
            } else {
                Color32::from_rgb(24, 87, 170)
            },
        );
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect,
                cursor_rect: caret_rect,
                should_interrupt_composition: false,
            })
        });
        if !self.preedit.is_empty() {
            let galley = ui.painter().layout_no_wrap(
                self.preedit.clone(),
                egui::FontId::proportional(height.max(16.0)),
                Color32::BLACK,
            );
            let r = Rect::from_min_size(Pos2::new(x, y), galley.size());
            ui.painter().rect_filled(r, 0.0, Color32::WHITE);
            ui.painter().galley(r.min, galley, Color32::BLACK);
            ui.painter().line_segment(
                [r.left_bottom(), r.right_bottom()],
                Stroke::new(1.0, Color32::BLUE),
            );
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        let modal = self.pending.is_some()
            || self.error.is_some()
            || self.show_help
            || self.page_setup.is_some()
            || self.show_character
            || self.show_paragraph
            || self.show_table
            || self.feature_dialog.is_some()
            || self.password_prompt.is_some()
            || self.symbol_dialog_open()
            || self.ext_modal_open();
        // A modal dialog takes the keyboard from the page (so Tab walks its own controls)
        // and gives it back when it closes, so typing goes on without a click.
        let canvas = egui::Id::new("document-canvas");
        let was_open = std::mem::replace(&mut self.ext.shell.modal_was_open, modal);
        if modal {
            ctx.memory_mut(|m| {
                if m.has_focus(canvas) {
                    m.surrender_focus(canvas);
                }
            });
            return;
        }
        if was_open {
            ctx.memory_mut(|m| m.request_focus(canvas));
        }
        let events = ctx.input(|i| i.events.clone());
        let canvas = ctx.memory(|m| m.focused() == Some(egui::Id::new("document-canvas")));
        for event in events {
            // egui drops the canvas focus on Esc before this runs; Esc still leaves the
            // nested caret, and the body caret takes the keyboard again.
            if self.session.nested_caret().is_some()
                && matches!(
                    event,
                    Event::Key {
                        key: Key::Escape,
                        pressed: true,
                        ..
                    }
                )
            {
                self.session.clear_nested();
                self.message = "본문 편집으로 돌아왔습니다.".into();
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
                continue;
            }
            if canvas && self.session.nested_caret().is_some() && self.nested_event(ctx, &event) {
                continue;
            }
            match event {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command && canvas => match key {
                    Key::Enter if modifiers.shift => {
                        self.run("insert.column_break", json!({}));
                    }
                    Key::Enter => self.command(Command::PageBreak),
                    Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Home
                    | Key::End
                    | Key::ArrowUp
                    | Key::ArrowDown => {
                        use hwpcraft_engine::Motion;
                        let motion = match key {
                            Key::ArrowLeft => Motion::WordLeft,
                            Key::ArrowRight => Motion::WordRight,
                            Key::ArrowUp => Motion::PrevParagraphStart,
                            Key::ArrowDown => Motion::NextParagraphStart,
                            Key::Home => Motion::DocumentStart,
                            _ => Motion::DocumentEnd,
                        };
                        let r = self.session.move_caret(motion, modifiers.shift);
                        self.result(r);
                        self.follow_cursor();
                    }
                    Key::S => {
                        self.save(modifiers.shift, None);
                    }
                    Key::O => self.open(ctx),
                    Key::N => self.request(Pending::New, ctx),
                    Key::Z => self.command(if modifiers.shift {
                        Command::Redo
                    } else {
                        Command::Undo
                    }),
                    Key::Y => {
                        self.run("text.delete_line", json!({}));
                    }
                    Key::T | Key::Delete => {
                        self.run("text.delete_word", json!({}));
                    }
                    Key::Backspace => {
                        self.run("text.delete_word_back", json!({}));
                    }
                    Key::Space if modifiers.alt => {
                        self.run("insert.nbsp", json!({}));
                    }
                    Key::Minus if modifiers.shift => {
                        self.run("insert.soft_hyphen", json!({}));
                    }
                    Key::F => self.open_find(false),
                    Key::H => self.open_find(true),
                    Key::B => {
                        self.bold = !self.bold;
                        self.command(Command::Character(json!({"bold":self.bold})));
                    }
                    Key::I => {
                        self.italic = !self.italic;
                        self.command(Command::Character(json!({"italic":self.italic})));
                    }
                    Key::U => {
                        self.underline = !self.underline;
                        self.command(Command::Character(json!({"underline":self.underline})));
                    }
                    Key::A if canvas => {
                        let r = self.session.select_all();
                        self.result(r);
                    }
                    _ => {}
                },
                Event::Copy if canvas => {
                    let html = self.session.copy_html().ok().flatten();
                    let r = self.session.copy();
                    if let Some(text) = self.result(r)
                        && !text.is_empty()
                        && !clipboard::set(&text, html.as_deref())
                    {
                        ctx.copy_text(text);
                    }
                }
                Event::Cut if canvas => {
                    let html = self.session.copy_html().ok().flatten();
                    let r = self.session.cut();
                    if let Some(text) = self.result(r)
                        && !text.is_empty()
                        && !clipboard::set(&text, html.as_deref())
                    {
                        ctx.copy_text(text);
                    }
                    self.sync_formatting();
                    self.follow_cursor();
                }
                Event::Paste(t) if canvas => {
                    let html = clipboard::html();
                    let r = self.session.paste_with_html(&t, html.as_deref());
                    self.result(r);
                    self.sync_formatting();
                    self.follow_cursor();
                }
                Event::Text(t) if canvas && self.preedit.is_empty() => self.typed_text(t),
                Event::Ime(ImeEvent::Preedit { text, .. }) if canvas => self.preedit = text,
                Event::Ime(ImeEvent::Commit(text)) if canvas => {
                    self.preedit.clear();
                    self.typed_text(text);
                }
                Event::Ime(ImeEvent::DeleteSurrounding {
                    before_chars,
                    after_chars,
                }) if canvas => {
                    let offset = self.session.cursor.offset;
                    self.session.anchor = Some(offset.saturating_sub(before_chars));
                    self.session.cursor.offset =
                        offset.saturating_add(after_chars).min(self.session.len());
                    self.command(Command::Insert(String::new()));
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if canvas && self.preedit.is_empty() => match key {
                    Key::Tab if self.session.cursor.cell.is_some() => {
                        let r = self.session.move_cell(modifiers.shift);
                        self.result(r);
                        self.follow_cursor();
                    }
                    Key::Tab if !modifiers.any() => {
                        self.run("insert.tab", json!({}));
                    }
                    Key::Enter if modifiers.shift => {
                        self.run("insert.line_break", json!({}));
                    }
                    Key::Enter => self.command(Command::Split),
                    Key::Y if modifiers.alt => {
                        self.run("text.delete_to_line_end", json!({}));
                    }
                    Key::Space if modifiers.alt => {
                        self.run("insert.fixed_width_space", json!({}));
                    }
                    Key::Backspace => self.command(Command::Backspace),
                    Key::Delete => self.command(Command::Delete),
                    Key::Insert if !modifiers.any() => {
                        self.session.overwrite = !self.session.overwrite;
                    }
                    Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Home
                    | Key::End
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::PageUp
                    | Key::PageDown
                        if !modifiers.alt =>
                    {
                        use hwpcraft_engine::Motion;
                        let motion = match key {
                            Key::ArrowLeft => Motion::Left,
                            Key::ArrowRight => Motion::Right,
                            Key::Home => Motion::LineStart,
                            Key::End => Motion::LineEnd,
                            Key::ArrowUp => Motion::Up,
                            Key::ArrowDown => Motion::Down,
                            Key::PageUp => Motion::PageUp,
                            _ => Motion::PageDown,
                        };
                        let r = self.session.move_caret(motion, modifiers.shift);
                        self.result(r);
                        self.follow_cursor();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    // Consume document shortcuts before TextEdit's independent text-only undo stack sees them.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending.is_some()
            || self.error.is_some()
            || self.show_help
            || self.page_setup.is_some()
            || self.show_character
            || self.show_paragraph
            || self.show_table
            || self.feature_dialog.is_some()
            || self.password_prompt.is_some()
            || self.symbol_dialog_open()
            || self.ext_modal_open()
        {
            return;
        }
        if self.ext_keys(ctx) || self.ext_shortcuts(ctx) {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F7)) {
            self.open_page_setup();
            return;
        }
        // Hangul: Ctrl+F10 opens 문자표.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F10)) {
            self.open_symbol_dialog();
            return;
        }
        // Hangul: Alt+G opens 이동 (찾아가기).
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, Key::G)) {
            self.core_ext_dispatch("edit.goto");
            return;
        }
        let canvas = ctx.memory(|m| m.focused() == Some(egui::Id::new("document-canvas")));
        if canvas && self.session.cursor.cell.is_some() && self.preedit.is_empty() {
            let shift = ctx.input(|i| i.modifiers.shift);
            let modifiers = if shift {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            };
            if ctx.input_mut(|i| i.consume_key(modifiers, Key::Tab)) {
                let r = self.session.move_cell(shift);
                self.result(r);
                self.follow_cursor();
                return;
            }
        }
        let focused = ctx.memory(|m| m.focused());
        let auxiliary = ["find-query", "find-replacement"]
            .iter()
            .any(|name| focused == Some(egui::Id::new(name)));
        let keys = [
            Key::S,
            Key::O,
            Key::N,
            Key::Z,
            Key::Y,
            Key::F,
            Key::H,
            Key::B,
            Key::I,
            Key::U,
            Key::Enter,
        ];
        for key in keys {
            if auxiliary && matches!(key, Key::Z | Key::Y | Key::B | Key::I | Key::U | Key::Enter) {
                continue;
            }
            let shift = ctx.input(|i| i.modifiers.shift);
            let modifiers = egui::Modifiers {
                ctrl: true,
                command: true,
                shift,
                ..Default::default()
            };
            if !ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                continue;
            }
            match key {
                Key::S => {
                    self.save(shift, None);
                }
                Key::O => self.open(ctx),
                Key::N => self.request(Pending::New, ctx),
                Key::Z => self.command(if shift { Command::Redo } else { Command::Undo }),
                Key::Y => self.command(Command::Redo),
                Key::F => self.open_find(false),
                Key::H => self.open_find(true),
                Key::Enter => self.command(Command::PageBreak),
                Key::B => {
                    self.bold = !self.bold;
                    self.command(Command::Character(json!({"bold":self.bold})));
                }
                Key::I => {
                    self.italic = !self.italic;
                    self.command(Command::Character(json!({"italic":self.italic})));
                }
                Key::U => {
                    self.underline = !self.underline;
                    self.command(Command::Character(json!({"underline":self.underline})));
                }
                _ => {}
            }
        }
    }
    /// Hangul's 암호 입력 prompt for an encrypted document.
    fn password_prompt_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut prompt) = self.password_prompt.take() else {
            return;
        };
        let mut done = false;
        let mut submit = false;
        chrome::modal("password-prompt").show(ctx, |ui| {
            ui.set_width(380.0);
            if chrome::title(ui, "암호 입력") {
                done = true;
            }
            ui.label(format!(
                "암호가 설정된 문서입니다: {}",
                prompt
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("문서 암호");
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut prompt.password)
                        .password(true)
                        .desired_width(220.0),
                );
                chrome::first_focus(&edit);
                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
            });
            if let Some(error) = &prompt.error {
                ui.colored_label(Color32::from_rgb(200, 40, 40), error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (ok, cancel) = chrome::actions(
                    ui,
                    prompt
                        .password
                        .is_empty()
                        .then_some("문서 암호를 넣으세요."),
                    "확인(D)",
                );
                submit |= ok;
                done |= cancel;
            });
        });
        if submit && !prompt.password.is_empty() {
            match self
                .session
                .open_with_password(&prompt.path, &prompt.password)
            {
                Ok(()) => {
                    self.page = 0;
                    self.message = format!("열기 완료 · {}", prompt.path.display());
                    self.preedit.clear();
                    done = true;
                }
                Err(e) if hwpcraft_formats::wrong_password(&e) => {
                    prompt.error = Some("암호가 일치하지 않습니다. 다시 입력하세요.".into());
                    prompt.password.clear();
                }
                Err(e) => {
                    self.result::<()>(Err(e));
                    done = true;
                }
            }
        }
        if !done {
            self.password_prompt = Some(prompt);
        }
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        self.password_prompt_dialog(ctx);
        self.symbol_dialog(ctx);
        self.page_setup_dialog(ctx);
        self.editing_dialogs(ctx);
        if self.error.is_none() {
            self.feature_dialogs(ctx);
            self.find_dialog(ctx);
            self.ext_dialogs(ctx);
        }
        if self.pending.is_some() {
            // The shared dialog frame: buttons on the right, Enter/Alt+D save, Esc cancels.
            let button = ext_shell::dialog(
                ctx,
                "변경 내용 저장",
                440.0,
                &["저장(D)", "저장 안 함(N)", "취소"],
                |ui| {
                    ui.label("문서에 저장하지 않은 변경 내용이 있습니다. 저장한 뒤 계속할까요?");
                },
            );
            match button {
                Some(0) => {
                    if self.save(false, None)
                        && let Some(action) = self.pending.take()
                    {
                        self.perform(action, ctx);
                    }
                }
                Some(1) => {
                    if let Some(action) = self.pending.take() {
                        self.perform(action, ctx);
                    }
                }
                Some(_) => self.pending = None,
                None => {}
            }
        }
        if let Some(message) = self.error.clone()
            && ext_shell::dialog(
                ctx,
                "작업을 완료하지 못했습니다",
                480.0,
                &["확인(D)"],
                |ui| {
                    ui.label(message);
                },
            )
            .is_some()
        {
            self.error = None;
        }
        if self.show_help {
            egui::Window::new("HwpCraft · 사용 안내").open(&mut self.show_help).default_width(580.0).show(ctx,|ui| {
            ui.heading(format!("HwpCraft {}", hwpcraft_doc::VERSION.trim()));ui.label("Ctrl+마우스 휠: 문서 확대·축소 (10–500%)");ui.label("HWP / HWPX 문서를 로컬에서 편집합니다.");ui.label("문서를 열고 흰 쪽을 클릭해 입력하세요. 아래 문단 편집 패널에서도 한글을 입력하고 텍스트를 선택할 수 있습니다. 왼쪽 목록에서 문단과 표 셀을 선택합니다.");
            ui.separator();ui.label("서식은 선택한 글자에 적용됩니다. 선택 영역이 없으면 현재 문단 전체에 적용됩니다. Ctrl+A는 현재 문단을 선택합니다. 여러 문단에 걸친 선택은 아직 지원하지 않습니다.");
            for (_,label,key) in hwpcraft_engine::COMMANDS.iter().chain(hwpcraft_engine::extension_commands()) {ui.label(format!("{label}    {key}"));}
            ui.separator();ui.label("독립적인 초기 편집기이며 한컴오피스의 모든 기능이나 문서 배치의 완전한 일치를 보장하지 않습니다. 본문 수식·기본 도형·단순 주석과 글상자는 개체 목록에서 편집할 수 있습니다. 중첩 표와 고급 개체 편집에는 한계가 있습니다. 중요한 원본은 다른 이름으로 저장한 뒤 확인하세요.");
            ui.label("구조 참조: WordCraft · 문서 엔진: RHWP 0.8.7 (MIT) · Rust / egui 네이티브 데스크톱");ui.label("계정·서버·Docker 없이 동작하며 문서를 외부로 전송하지 않습니다.");
        });
        }
    }
}
impl eframe::App for HwpApp {
    /// `--view bench-ime` types through the same input events as the keyboard and IME.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        raw_input.events.append(&mut self.ext.shell.bench_events);
    }
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_pdf_export(ctx);
        self.serve_mcp_bridge(ctx);
        if self.hold_close_for_pdf(ctx) {
            // Closing waits for the running PDF export; the dirty check runs then.
        } else if ctx.input(|i| i.viewport().close_requested())
            && self.session.dirty()
            && !self.allow_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_owned()));
        if let Some(path) = dropped {
            self.request(Pending::Open(path), ctx);
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let Event::Screenshot { image, .. } = event
                && let Some(path) = self.screenshot.take()
            {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                let r = image::save_buffer(
                    &path,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .map_err(anyhow::Error::from);
                if self.result(r).is_some() && self.exit_after_screenshot {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Timed only while a bench runs (`--view bench-edit`), with eframe's time for the
        // last frame (ui and painting, not the wait for the screen).
        let mut laps = ext_shell::Laps::new(self.ext.shell.bench_running());
        self.ext.shell.frame_cpu_ms = frame.info().cpu_usage.map(|s| f64::from(s) * 1000.0);
        self.sync_formatting();
        laps.mark();
        self.shortcuts(&ctx);
        laps.mark();
        self.toolbar(ui);
        laps.mark();
        self.status_bar(ui);
        laps.mark();
        if self.show_navigation {
            self.navigator(ui);
        }
        if self.show_editor {
            self.paragraph_editor(ui);
        }
        laps.mark();
        if self.tab == 0 {
            self.file_backstage(ui);
        } else {
            self.document_views(ui);
        }
        laps.mark();
        self.keyboard(&ctx);
        laps.mark();
        self.dialogs(&ctx);
        laps.mark();
        self.ext.shell.laps = laps.done();
        self.frames += 1;
        if self.screenshot.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        let name = self
            .session
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_else(|| "새 문서".into());
        let title = format!(
            "{}{} — HwpCraft {}",
            if self.session.dirty() { "● " } else { "" },
            name,
            hwpcraft_doc::VERSION.trim()
        );
        // Only a changed title goes to the window: setting it is a system call each time.
        if self.ext.shell.window_title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.ext.shell.window_title = title;
        }
    }
}

/// 쪽 윤곽 off: the dotted line Hangul draws where one page's body ends and the next begins.
fn dotted_line(ui: &egui::Ui, from: Pos2, to: Pos2) {
    let stroke = Stroke::new(1.0, Color32::from_gray(150));
    let mut x = from.x;
    while x < to.x {
        let end = (x + 3.0).min(to.x);
        ui.painter()
            .line_segment([Pos2::new(x, from.y), Pos2::new(end, from.y)], stroke);
        x += 6.0;
    }
}
