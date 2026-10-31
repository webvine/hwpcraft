//! Owner: shell/page area. 메뉴·리본·보기·쪽·파일 서비스 화면.
//! Other areas must not edit this file; request changes through the owner.
//! Dialog arrangement follows Hangul-style conventions.
use super::*;
use chrome::Tokens;

mod autocorrect;
mod autosave;
mod bench;
mod links;
pub(crate) use bench::Laps;
mod hanja_view;
mod macros;
mod raster;
mod settings;
mod windows;
pub(crate) use macros::MOUSE_NOTE;

/// Hangul's zoom range is 10–500 %; pages are rasterised at a step near the zoom
/// (`raster_scale`) and the texture is scaled to the exact zoom.
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
        /// 양쪽 바탕쪽 글 being edited.
        text: String,
        /// `page.master.object.list` of the current section.
        objects: serde_json::Value,
        /// The picture or shape 넣기 makes.
        object: MasterObject,
    },
    Print {
        range: usize,
        pages: String,
        copies: u32,
        to_file: bool,
        /// 인쇄 상세: odd/even, reverse, collate, 모아 찍기 (copies come from `copies`).
        layout: hwpcraft_engine::PrintLayout,
        /// 워터마크 글 (empty: none).
        watermark: String,
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
    Preview(Box<PreviewState>),
    /// 구역 설정.
    Section {
        /// 쪽·그림·표·수식 start numbers (0 = 앞 구역에 이어서).
        numbers: [u16; 4],
        tab_mm: f64,
        hide: [bool; 6],
        whole_document: bool,
        /// 줄 번호 표시 as opened and as now (sent only when changed).
        line_numbers: [bool; 2],
    },
    /// 줄 번호 설정.
    LineNumbers(LineNumberSettings),
    /// 문서 이력 관리: `review.history.list` as read when the dialog opened.
    History(serde_json::Value),
    /// 창 전환: the other windows when the dialog opened.
    SwitchWindow {
        peers: Vec<windows::Peer>,
        selected: usize,
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
    /// 라벨: preset index and the label grid in millimetres.
    Label {
        preset: usize,
        rows: u32,
        columns: u32,
        width: f64,
        height: f64,
        gap: f64,
    },
    /// 개인 정보 바꾸기: the name being edited.
    UserInfo(String),
    /// 빠른 교정: the list being edited.
    AutoCorrect(Box<autocorrect::AutoCorrectDraft>),
    /// 변경 추적 문서 저장 / 그림 효과가 있는 문서 저장: the file the user asked to save,
    /// whether Hangul track changes would be merged, and how many picture effects an
    /// .hwp save would drop.
    TrackedSave {
        path: PathBuf,
        tracked: bool,
        effects: usize,
    },
    /// 단 정의 나누기: columns from the caret on.
    ColumnBreak {
        count: u16,
        spacing: f64,
    },
    /// 환경 설정.
    Options(Box<settings::Settings>),
    /// 사용자 설정 → 단축키.
    Shortcuts(Box<settings::ShortcutDraft>),
    /// 문서마당: the user's templates.
    Templates {
        list: Vec<serde_json::Value>,
        selected: usize,
        /// Name for saving the current document as a template.
        name: String,
    },
    /// 문서 복구: copies left by a window that ended unexpectedly.
    Recover {
        orphans: Vec<autosave::Orphan>,
        selected: usize,
    },
    /// 안내선 넣기: direction and distance from the paper edge.
    Guide {
        vertical: bool,
        mm: f32,
    },
    /// 매크로 정의 끝: name the recording (cancel discards it).
    MacroName {
        name: String,
        steps: Vec<hwpcraft_engine::MacroStep>,
    },
    /// 매크로 실행: saved macros, run (with repeat) or delete.
    Macros {
        list: Vec<hwpcraft_engine::SavedMacro>,
        selected: usize,
        repeat: u32,
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

/// Common A4 label sheets: (name, rows, columns, width mm, height mm, gap between
/// columns mm). The last entry is 사용자 지정.
const LABEL_PRESETS: &[(&str, u32, u32, f64, f64, f64)] = &[
    ("2칸 × 8줄 (99.1 × 34 mm)", 8, 2, 99.1, 34.0, 2.5),
    ("2칸 × 7줄 (99.1 × 38.1 mm)", 7, 2, 99.1, 38.1, 2.5),
    ("2칸 × 5줄 (99.1 × 57 mm)", 5, 2, 99.1, 57.0, 2.5),
    ("3칸 × 7줄 (63.5 × 38.1 mm)", 7, 3, 63.5, 38.1, 2.5),
    ("3칸 × 8줄 (70 × 37 mm)", 8, 3, 70.0, 37.0, 0.0),
    ("사용자 지정", 8, 2, 99.1, 34.0, 2.5),
];

/// 줄 번호 설정 dialog state; `restart` is the stored value (1 page, 2 section, 3 continue).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineNumberSettings {
    pub show: bool,
    pub start: u32,
    pub count_by: u32,
    pub distance_mm: f64,
    pub restart: u8,
    pub all_sections: bool,
}

/// 미리 보기: the sheet shown, the print settings tried there and the sheet bitmap
/// (sheet, document generation, settings key, texture, size).
#[derive(Default)]
pub(crate) struct PreviewState {
    pub sheet: usize,
    pub sheets: usize,
    pub layout: hwpcraft_engine::PrintLayout,
    pub watermark: String,
    pub texture: Option<(usize, u64, String, egui::TextureHandle, Vec2)>,
}

/// Most recently used documents kept in the local settings folder (Hangul shows 9).
pub(crate) const RECENT_LIMIT: usize = 9;

/// Dialog and tool state owned by this module. Stored in `HwpApp::ext.shell`.
impl State {
    /// A `--view bench*` run is going (frames are timed part by part).
    pub(crate) fn bench_running(&self) -> bool {
        self.bench.is_some()
    }
}
pub(crate) struct State {
    pub show_status_bar: bool,
    /// 보기 → 지원하지 않는 명령: show unsupported commands (grey) in the ribbon.
    pub show_unsupported: bool,
    pub show_doc_tabs: bool,
    pub rulers: super::rulers::RulerCache,
    /// Page rectangle the rulers were last drawn against.
    pub ruler_page: Option<Rect>,
    pub dialog: Option<Dialog>,
    /// First key of a Hangul two-key shortcut (Ctrl+N, Ctrl+G, Ctrl+K, Ctrl+Q).
    chord: Option<char>,
    /// Command the last two-key shortcut resolved to (tests compare it with the list).
    pub last_chord: Option<&'static str>,
    /// Resolve two-key shortcuts without running them (tests: some open file pickers).
    pub chords_record_only: bool,
    /// `--view tooltip-<command id>`: keep that ribbon command's tooltip open (captures).
    pub pinned_tip: Option<String>,
    /// `--view bench`: scripted scroll/zoom/resize run that logs frame times.
    bench: Option<bench::Bench>,
    /// Page rasterizations since the bench last looked (full pages and bands).
    rasters: u32,
    /// Page texture pixels uploaded since the bench last looked.
    uploaded: usize,
    /// The last pass split into its parts while a bench runs (`bench::Laps`).
    pub laps: [f64; 8],
    /// The window title last sent (sent again only when it changes).
    pub window_title: String,
    /// eframe's time for the last whole frame, ui and painting (bench log).
    pub frame_cpu_ms: Option<f64>,
    /// The document generation last seen by the prefetch and when it changed (None: the
    /// first one seen), so pages out of view wait while the user is typing.
    edit_seen: (u64, Option<std::time::Instant>),
    /// Milliseconds spent making page SVG and rasterizing it, for the bench log.
    raster_ms: std::cell::Cell<[f64; 2]>,
    /// Rasterize new pages and zoom steps on worker threads (off in unit tests, which
    /// expect a page bitmap in the same pass; `raster_tests` turns it on).
    pub async_raster: bool,
    raster: Option<raster::Rasterizer>,
    /// Pass in which a page SVG was made for a worker (one per pass).
    raster_svg_pass: Option<u64>,
    /// 1×1 paper-coloured texture shown while a new page is being rasterized.
    blank: Option<(Option<(Color32, Color32)>, egui::TextureHandle)>,
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
    /// A modal dialog was open last frame (the page gets the keyboard back when it closes).
    pub modal_was_open: bool,
    /// keytips-folded capture: the key tip pressed once the narrow ribbon has been drawn.
    view_keytip: Option<&'static str>,
    /// fit-more-* captures: the popup opened once the narrow window has been drawn.
    view_popup: Option<egui::Id>,
    /// Ribbon group widths as last drawn: (tab, unsupported hidden, group, level) → px.
    pub ribbon_widths: std::collections::HashMap<(usize, bool, usize, u8), f32>,
    /// Width of the 지원 안 함 chip (and its note) per (tab, unsupported hidden).
    pub ribbon_chip: std::collections::HashMap<(usize, bool), f32>,
    /// Right edge the ribbon drew to, and the right edge it had (tests and captures).
    pub ribbon_fit: (f32, f32),
    /// Right edge of the menu tabs, and left edge of the search box beside them.
    pub tabs_fit: (f32, f32),
    /// 서식 도구 줄 piece widths as last drawn: (piece, narrowed) → px.
    pub format_widths: std::collections::HashMap<(usize, bool), f32>,
    /// Right edge the 서식 도구 줄 drew to, its right edge, and how many pieces went behind ».
    pub format_fit: (f32, f32, usize),
    alt_down: bool,
    alt_alone: bool,
    /// 문서 보기 색 (screen only) and the colours of its 사용자 설정 entry.
    pub view_color: ViewColor,
    pub custom_colors: (Color32, Color32),
    /// 보기 → 그림: off draws only picture frames.
    pub show_pictures: bool,
    /// 보기 → 격자 (screen only).
    pub show_grid: bool,
    /// 쪽 윤곽 (Ctrl+G,L): false shows only the body text of each page, stacked with a
    /// dotted line between pages (no margins, header, footer or page frame).
    pub page_outline: bool,
    /// Body band of each page as (top, bottom) in page units, for 쪽 윤곽 off.
    pub page_bodies: Vec<(f32, f32)>,
    /// 환경 설정 and user shortcuts (view.txt).
    pub settings: settings::Settings,
    /// 자동 복구: this window's lock and copy (see `autosave`).
    recovery: Option<autosave::Recovery>,
    /// Recovery was set up or deliberately skipped (unit tests, screenshots).
    recovery_started: bool,
    /// `--view bench-ime`: input events for the next frame (see `bench`).
    pub bench_events: Vec<egui::Event>,
    /// 보기 → 한자 발음 표시 (screen only, see `hanja_view`).
    pub hanja_readings: bool,
    /// 창 배열·창 전환: this window's place in the window list (see `windows`).
    windows: Option<windows::Windows>,
    windows_started: bool,
    /// 보기 → 안내선: (vertical, millimetres from the paper's left/top edge).
    pub guides: Vec<(bool, f32)>,
    pub show_guides: bool,
    /// 맞쪽 보기 request; the continuous page view (core area) lays pages out.
    pub mirror_pages: bool,
    /// Header/footer editor to open next frame (이전/다음 inside the editor).
    pub hf_next: Option<(bool, usize)>,
    /// Selection highlight rectangles per page, for one document generation and selection.
    selection: Option<SelectionHighlight>,
    /// Page redraw queue: the document generation each cached page texture shows.
    pub page_drawn: std::collections::BTreeMap<u32, u64>,
    /// Raster scale (hundredths) each cached page texture was drawn at.
    page_scale: std::collections::BTreeMap<u32, u16>,
    /// The caret paragraph as it appears in its page texture, for band redraws.
    caret_extent: Option<CaretExtent>,
    /// Debug builds only: the CPU copy of each page texture, to verify band redraws
    /// against a full render of the whole page.
    page_images: std::collections::BTreeMap<u32, egui::ColorImage>,
    /// Band redraws done and band redraws rejected by the debug check (tests read these).
    pub band_redraws: u32,
    pub band_rejected: u32,
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
    /// 인쇄 상세 for the next PDF preparation started by the print dialog.
    next_print_layout: Option<hwpcraft_engine::PrintLayout>,
    /// A PDF whose pages are being prepared a slice per frame.
    pdf_preparing: Option<PdfPreparing>,
    /// Preparation time per frame before the first restart (tests use zero: one page).
    pdf_budget: std::time::Duration,
}
impl Default for State {
    fn default() -> Self {
        Self {
            show_status_bar: true,
            show_unsupported: false,
            show_doc_tabs: true,
            rulers: Default::default(),
            ruler_page: None,
            dialog: None,
            chord: None,
            last_chord: None,
            chords_record_only: false,
            pinned_tip: None,
            bench: None,
            uploaded: 0,
            laps: [0.0; 8],
            window_title: String::new(),
            frame_cpu_ms: None,
            edit_seen: (u64::MAX, None),
            rasters: 0,
            raster_ms: Default::default(),
            async_raster: !cfg!(test),
            raster: None,
            raster_svg_pass: None,
            blank: None,
            object_kind: None,
            marks: [false; 3],
            marks_applied: u64::MAX,
            recent: Vec::new(),
            recent_loaded: false,
            last_path: None,
            keytips: KeyTips::Off,
            modal_was_open: false,
            view_keytip: None,
            view_popup: None,
            ribbon_widths: Default::default(),
            ribbon_chip: Default::default(),
            ribbon_fit: (0.0, 0.0),
            tabs_fit: (0.0, 0.0),
            format_widths: Default::default(),
            format_fit: (0.0, 0.0, 0),
            alt_down: false,
            alt_alone: false,
            view_color: ViewColor::Light,
            custom_colors: (Color32::WHITE, Color32::BLACK),
            show_pictures: true,
            show_grid: false,
            page_outline: true,
            page_bodies: Vec::new(),
            guides: Vec::new(),
            recovery: None,
            recovery_started: cfg!(test),
            windows: None,
            windows_started: false,
            hanja_readings: false,
            bench_events: Vec::new(),
            settings: Default::default(),
            show_guides: true,
            mirror_pages: false,
            hf_next: None,
            selection: None,
            page_drawn: Default::default(),
            page_scale: Default::default(),
            caret_extent: None,
            page_images: Default::default(),
            band_redraws: 0,
            band_rejected: 0,
            redraw_pass: None,
            requested: (u64::MAX, Default::default(), Default::default()),
            caret_page: None,
            print_after: None,
            next_print_layout: None,
            pdf_preparing: None,
            pdf_budget: PDF_PREPARE_BUDGET,
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
            | "view.link_next"
            | "view.link_prev"
            | "view.paragraph_marks"
            | "view.control_codes"
            | "view.transparent_lines"
            | "view.grid"
            | "view.page_outline"
            | "view.split"
            | "view.split.side"
            | "view.new_window"
            | "view.open_in_new_window"
            | "view.arrange"
            | "view.arrange.side"
            | "view.arrange.stacked"
            | "view.switch_window"
            | "review.history"
            | "view.hanja_pronounce"
            | "view.guides"
            | "view.guides.add"
            | "file.template"
            | "tools.options"
            | "tools.user_info"
            | "tools.autocorrect"
            | "tools.customize"
            | "page.label" => true,
            "page.column_def_break" | "page.section_break" => body,
            "view.guides.clear" => !self.ext.shell.guides.is_empty(),
            "view.split.none" => self.split.is_some(),
            "page.number" | "page.new_number" | "page.hide" | "page.section_dialog" => body,
            "page.line_numbers.toggle"
            | "page.line_numbers.dialog"
            | "page.line_numbers.skip_paragraph" => body,
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
            id if id.starts_with("macro.") => return self.macro_enabled(id),
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
            "view.grid" => self.ext.shell.show_grid,
            "page.line_numbers.toggle" => self.line_numbers_shown(),
            "view.hanja_pronounce" => self.ext.shell.hanja_readings,
            "page.line_numbers.skip_paragraph" => self.line_number_skipped(),
            "view.page_outline" => self.ext.shell.page_outline,
            "view.guides" => self.ext.shell.show_guides,
            "view.split" => self.split.as_ref().is_some_and(|s| !s.side_by_side),
            "view.split.side" => self.split.as_ref().is_some_and(|s| s.side_by_side),
            "view.mirror_pages" => self.ext.shell.mirror_pages,
            id if id.starts_with("view.color.") => self.ext.shell.view_color.id() == id,
            _ => false,
        }
    }
    /// Return true when the command id was handled.
    pub(crate) fn shell_ext_dispatch(&mut self, id: &str, ctx: &egui::Context) -> bool {
        if id.starts_with("macro.") {
            return self.macro_dispatch(id);
        }
        match id {
            "page.line_numbers.skip_paragraph" => {
                let skip = !self.line_number_skipped();
                if self.run("page.line_numbers.skip", json!({ "skip": skip })) {
                    self.message = if skip {
                        "이 문단은 줄 번호를 건너뜁니다. HWPX로 저장할 때만 남고 HWP로 저장하면 사라집니다.".into()
                    } else {
                        "이 문단에 다시 줄 번호를 매깁니다.".into()
                    };
                }
            }
            "page.line_numbers.toggle" => {
                let show = !self.line_numbers_shown();
                if self.run("page.line_numbers", json!({ "show": show })) {
                    self.message = if show {
                        "이 구역에 줄 번호를 보입니다. 설정은 쪽 → 줄 번호 → 줄 번호 설정에서 바꿉니다.".into()
                    } else {
                        "이 구역의 줄 번호를 감췄습니다.".into()
                    };
                }
            }
            "view.zoom_100" => self.set_zoom(1.0),
            "view.zoom_fit_width" => self.zoom_to_fit(true),
            "view.zoom_fit_page" => self.zoom_to_fit(false),
            "view.zoom_in" => self.set_zoom(self.zoom + 0.1),
            "view.zoom_out" => self.set_zoom(self.zoom - 0.1),
            "view.status_bar" => self.ext.shell.show_status_bar ^= true,
            "view.guides" => self.ext.shell.show_guides ^= true,
            "view.guides.clear" => {
                self.ext.shell.guides.clear();
                self.message = "안내선을 모두 지웠습니다.".into();
            }
            "view.split" => self.set_split(Some(false)),
            "view.split.side" => self.set_split(Some(true)),
            "view.split.none" => self.set_split(None),
            "view.arrange" => self.arrange_windows(ctx, windows::Arrangement::Cascade),
            "view.arrange.side" => self.arrange_windows(ctx, windows::Arrangement::SideBySide),
            "view.arrange.stacked" => self.arrange_windows(ctx, windows::Arrangement::Stacked),
            "view.hanja_pronounce" => {
                self.ext.shell.hanja_readings = !self.ext.shell.hanja_readings;
                self.restyle_pages();
                self.message = if self.ext.shell.hanja_readings {
                    "한자 위에 한글 발음을 작게 보입니다(화면에만, 인쇄·PDF는 그대로).".into()
                } else {
                    "한자 발음 표시를 껐습니다.".into()
                };
            }
            "review.history" => match self.session.run("review.history.list", json!({})) {
                Ok(list) => self.ext.shell.dialog = Some(Dialog::History(list)),
                Err(e) => self.message = format!("문서 이력을 읽지 못했습니다: {e}"),
            },
            "view.switch_window" => {
                let peers = self.other_windows();
                if peers.is_empty() {
                    self.message = "다른 HwpCraft 창이 없습니다. 보기 → 새 창으로 엽니다.".into();
                } else {
                    self.ext.shell.dialog = Some(Dialog::SwitchWindow { peers, selected: 0 });
                }
            }
            "view.new_window" => self.open_new_window(None),
            "view.open_in_new_window" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("한글 문서", &["hwp", "hwpx"])
                    .pick_file()
                {
                    self.open_new_window(Some(path));
                }
            }
            "view.page_outline" => {
                self.ext.shell.page_outline ^= true;
                self.scroll_to_caret = true;
                self.message = if self.ext.shell.page_outline {
                    "쪽 윤곽을 보입니다.".into()
                } else {
                    "쪽 윤곽을 껐습니다: 본문만 이어서 보이고, 쪽 경계는 점선입니다. 머리말·꼬리말·쪽 번호는 보이지 않습니다.".into()
                };
            }
            "view.grid" => {
                self.ext.shell.show_grid ^= true;
                self.message = if self.ext.shell.show_grid {
                    "격자를 5 mm 간격으로 보입니다. 화면에만 보이고 인쇄·저장되지 않습니다.".into()
                } else {
                    "격자를 감췄습니다.".into()
                };
            }
            "view.doc_tabs" => self.ext.shell.show_doc_tabs ^= true,
            "view.link_next" | "view.link_prev" => {
                let step = if id == "view.link_next" {
                    "hyperlink.next"
                } else {
                    "hyperlink.prev"
                };
                self.run(step, json!({}));
            }
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
            "page.section_break" => {
                if self.run(id, json!({})) {
                    let n = self.session.cursor.section + 1;
                    self.message = format!(
                        "구역 나누기: 여기서부터 {n}구역입니다. 쪽 설정과 바탕쪽을 이어받았습니다."
                    );
                }
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
            "page.master" => {
                let current = self
                    .session
                    .run("page.master.get", json!({}))
                    .unwrap_or_default();
                Dialog::MasterPage {
                    kind: 0,
                    list: self
                        .session
                        .run("page.master.list", json!({}))
                        .unwrap_or_default(),
                    text: current["text"].as_str().unwrap_or("").to_owned(),
                    objects: self
                        .session
                        .run("page.master.object.list", json!({}))
                        .unwrap_or_default(),
                    object: MasterObject::default(),
                }
            }
            "file.print" => Dialog::Print {
                range: 0,
                pages: String::new(),
                copies: 1,
                to_file: false,
                layout: Default::default(),
                watermark: String::new(),
            },
            "file.doc_info" => Dialog::DocInfo {
                tab: 2,
                info: self.session.run("file.doc_info", json!({})).ok()?,
            },
            "page.new_number" => Dialog::NewNumber { start: 1 },
            "file.template" => Dialog::Templates {
                list: self.template_list(),
                selected: 0,
                name: self
                    .session
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            },
            "tools.options" => Dialog::Options(Box::new(settings::Settings {
                show_unsupported: self.ext.shell.show_unsupported,
                ..self.ext.shell.settings.clone()
            })),
            "tools.customize" => Dialog::Shortcuts(Box::default()),
            "tools.user_info" => Dialog::UserInfo(self.ext.shell.settings.user_name.clone()),
            "tools.autocorrect" => Dialog::AutoCorrect(Box::new(autocorrect::AutoCorrectDraft {
                enabled: self.ext.shell.settings.autocorrect_enabled,
                entries: self.ext.shell.settings.autocorrect.clone(),
                ..Default::default()
            })),
            "page.column_def_break" => Dialog::ColumnBreak {
                count: 2,
                spacing: 8.0,
            },
            "page.label" => {
                let (_, rows, columns, width, height, gap) = LABEL_PRESETS[0];
                Dialog::Label {
                    preset: 0,
                    rows,
                    columns,
                    width,
                    height,
                    gap,
                }
            }
            "view.guides.add" => Dialog::Guide {
                vertical: true,
                mm: 30.0,
            },
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
            "file.preview" => Dialog::Preview(Box::new(PreviewState {
                sheet: self.page as usize,
                ..Default::default()
            })),
            "page.line_numbers.dialog" => {
                let v = self.session.run("page.line_numbers.get", json!({})).ok()?;
                let n = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
                Dialog::LineNumbers(LineNumberSettings {
                    show: true,
                    start: n("start").max(1),
                    count_by: n("countBy").clamp(1, 100),
                    distance_mm: v["distanceMm"].as_f64().unwrap_or(0.0),
                    restart: match n("restart") {
                        1 | 2 => n("restart") as u8,
                        _ => 3,
                    },
                    all_sections: false,
                })
            }
            "page.section_dialog" => {
                let v = self.session.run("page.section.get", json!({})).ok()?;
                let shown = self.line_numbers_shown();
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
                    line_numbers: [shown; 2],
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
        self.prefetch_pages(ctx);
        // Captures and benchmarks run on a fixed script: no recovery prompt or copies.
        if self.screenshot.is_some() || self.ext.shell.bench.is_some() {
            self.ext.shell.recovery_started = true;
        }
        self.autosave_frame(ctx);
        self.windows_frame(ctx);
        self.bench_frame(ctx);
        self.poll_pdf_prepare(ctx);
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
            Dialog::Preview(preview) => self.preview_dialog(ctx, preview),
            Dialog::Section {
                numbers,
                tab_mm,
                hide,
                whole_document,
                line_numbers,
            } => self.section_dialog(ctx, numbers, tab_mm, hide, whole_document, line_numbers),
            Dialog::LineNumbers(settings) => self.line_numbers_dialog(ctx, settings),
            Dialog::History(list) => self.history_dialog(ctx, list),
            Dialog::SwitchWindow { peers, selected } => {
                self.switch_window_dialog(ctx, peers, selected)
            }
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
            Dialog::MasterPage {
                kind,
                list,
                text,
                objects,
                object,
            } => self.master_page_dialog(ctx, kind, list, text, objects, object),
            Dialog::Print {
                range,
                pages,
                copies,
                to_file,
                layout,
                watermark,
            } => self.print_dialog(ctx, range, pages, copies, to_file, layout, watermark),
            Dialog::DocInfo { tab, info } => doc_info_dialog(ctx, tab, info),
            Dialog::NewNumber { start } => self.new_number_dialog(ctx, start),
            Dialog::Hide { flags } => self.hide_dialog(ctx, flags),
            Dialog::SavePicture { all_pages, scale } => {
                self.save_picture_dialog(ctx, all_pages, scale)
            }
            Dialog::ViewColorCustom { paper, text } => {
                self.view_color_custom_dialog(ctx, paper, text)
            }
            Dialog::MacroName { name, steps } => self.macro_name_dialog(ctx, name, steps),
            Dialog::Guide { vertical, mm } => self.guide_dialog(ctx, vertical, mm),
            Dialog::Options(draft) => self.options_dialog(ctx, draft),
            Dialog::UserInfo(name) => self.user_info_dialog(ctx, name),
            Dialog::AutoCorrect(draft) => self.autocorrect_dialog(ctx, draft),
            Dialog::TrackedSave {
                path,
                tracked,
                effects,
            } => self.tracked_save_dialog(ctx, path, *tracked, *effects),
            Dialog::ColumnBreak { count, spacing } => {
                let button = self::dialog(
                    ctx,
                    "단 정의 나누기",
                    380.0,
                    &["나누기(D)", "취소"],
                    |ui| {
                        group(ui, "여기부터");
                        ui.horizontal(|ui| {
                            ui.label("단 수");
                            ui.add(egui::DragValue::new(count).range(1..=6));
                            ui.label("단 사이");
                            ui.add(
                                egui::DragValue::new(spacing)
                                    .range(0.0..=50.0)
                                    .speed(0.5)
                                    .suffix(" mm"),
                            );
                        });
                        notice(
                            ui,
                            "캐럿에서 문단을 나누고, 그 문단부터 새 단 설정을 씁니다. 예: 제목은 1단, 본문은 2단.",
                        );
                    },
                );
                match button {
                    Some(0) => {
                        let r = self.session.run(
                            "page.column_def_break",
                            json!({ "count": *count, "spacing_mm": *spacing }),
                        );
                        self.result(r).is_none()
                    }
                    Some(_) => false,
                    None => true,
                }
            }
            Dialog::Label {
                preset,
                rows,
                columns,
                width,
                height,
                gap,
            } => self.label_dialog(ctx, preset, rows, columns, width, height, gap),
            Dialog::Shortcuts(draft) => self.shortcuts_dialog(ctx, draft),
            Dialog::Templates {
                list,
                selected,
                name,
            } => self.templates_dialog(ctx, list, selected, name),
            Dialog::Recover { orphans, selected } => self.recover_dialog(ctx, orphans, selected),
            Dialog::Macros {
                list,
                selected,
                repeat,
            } => self.macros_dialog(ctx, list, selected, repeat),
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
        // 문서마당 (Ctrl+Alt+N).
        if ctx.input_mut(|i| i.consume_key(ctrl | Modifiers::ALT, Key::N)) {
            ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
            self.shell_ext_dispatch("file.template", ctx);
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
            let Some(&(_, _, id)) = CHORDS.iter().find(|(p, k, _)| *p == prefix && *k == key)
            else {
                return true;
            };
            self.ext.shell.last_chord = Some(id);
            if self.ext.shell.chords_record_only {
                return true;
            }
            // The ribbon's own entry runs it (enabled check and reason included);
            // dialogs that have no ribbon button go to their module.
            if !self.run_ribbon_command(id, ctx) {
                if self.ext_enabled(id) {
                    self.ext_dispatch(id, ctx);
                } else {
                    self.message = format!("{id}: 지금은 쓸 수 없는 명령입니다.");
                }
            }
            return true;
        }
        for (key, prefix, hint) in [
            (
                Key::N,
                'N',
                "Ctrl+N, (P 쪽 번호 · H 머리말/꼬리말 · T 표 · A 표 나누기 · Z 표 붙이기 · \
                 I 그림 · N 각주 · E 미주 · M 수식)",
            ),
            (
                Key::G,
                'G',
                "Ctrl+G, (C 조판 부호 · T 문단 부호 · L 쪽 윤곽 · Q 100% · P 쪽 맞춤 · W 폭 맞춤)",
            ),
            (
                Key::K,
                'K',
                "Ctrl+K, (H 하이퍼링크 · E 누름틀 · N 문단 번호 · O 개요 번호)",
            ),
            (Key::Q, 'Q', "Ctrl+Q, (I 문서 정보 · R 다음 하이퍼링크)"),
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
        if let Some(id) = name.strip_prefix("tooltip-") {
            let Some(tab) = self.ribbon_tab_of(id) else {
                return false;
            };
            self.tab = tab;
            self.ext.shell.pinned_tip = Some(id.to_owned());
            return true;
        }
        // 매크로 captures: recording (status chip), and the name dialog after one step.
        if name == "macro-record" || name == "macro-name" {
            self.tab = 8;
            self.macro_dispatch("macro.record.start");
            if name == "macro-name" {
                let _ = self
                    .session
                    .execute(hwpcraft_engine::Command::Insert("매크로 ".into()));
                self.macro_dispatch("macro.record.stop");
            }
            return true;
        }
        // Key tips of one tab (keytips-8 = 도구), or the tab tips (keytips-tabs).
        if let Some(rest) = name.strip_prefix("keytips-")
            && rest != "folded"
        {
            self.ext.shell.keytips = if rest == "tabs" {
                KeyTips::Tabs
            } else {
                let Ok(tab @ 1..=8) = rest.parse::<usize>() else {
                    return false;
                };
                self.tab = tab;
                KeyTips::Items
            };
            return true;
        }
        // The » lists of a 400 px window: the tabs (검토 unfolded) and the 서식 도구 줄.
        if name == "fit-more-tabs" || name == "fit-more-format" {
            self.tab = 1;
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(400.0, 560.0)));
            self.ext.shell.view_popup = Some(if name == "fit-more-tabs" {
                let review = egui::Id::new(("hwp-more-tab", 7usize));
                self.ctx
                    .data_mut(|d| d.insert_temp(review.with("unfold"), true));
                egui::Id::new("hwp-more-tabs")
            } else {
                egui::Id::new("hwp-more-format")
            });
            return true;
        }
        // Ribbon fit captures: fit-<window width>-<tab>, with the key tips showing.
        if let Some((width, tab)) = name.strip_prefix("fit-").and_then(|r| r.split_once('-')) {
            let (Ok(width), Ok(tab @ 1..=8)) = (width.parse::<f32>(), tab.parse::<usize>()) else {
                return false;
            };
            self.tab = tab;
            self.ext.shell.keytips = KeyTips::Items;
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(width, 560.0)));
            return true;
        }
        // A narrow window folds the right-hand 입력 groups; the 상용구 tip opens its group,
        // then the menu inside it.
        if name == "keytips-folded" {
            self.tab = 3;
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(900.0, 640.0)));
            self.ext.shell.view_keytip = Some("menu:상용구");
            return true;
        }
        // The save prompt and the error box on the shared dialog frame.
        if name == "save-prompt" {
            self.pending = Some(Pending::New);
            return true;
        }
        if name == "error-dialog" {
            self.error = Some("저장하지 못했습니다: 이 폴더에 쓸 권한이 없습니다.".into());
            return true;
        }
        if name == "bench" || name == "bench-sync" || name == "bench-edit" {
            // bench-sync: the same run with pages rasterized in the frame, for comparison.
            self.ext.shell.async_raster = name != "bench-sync";
            self.start_bench(name == "bench-edit");
            return true;
        }
        if name == "bench-ime" {
            self.start_ime_bench();
            return true;
        }
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
            "user-info" => "tools.user_info",
            "autocorrect" => "tools.autocorrect",
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
            "split" | "split-side" => {
                self.tab = 2;
                self.set_split(Some(name == "split-side"));
                if let Some(split) = self.split.as_mut() {
                    split.set_other_page(2);
                }
                return true;
            }
            // 변경 추적 문서 저장 확인 창 (the document needs Hangul track changes).
            "tracked-save" => {
                let path = std::env::temp_dir().join("hwpcraft-tracked-save.hwpx");
                return self.confirm_tracked_save(&path);
            }
            "preview-4up" => {
                let ctx = self.ctx.clone();
                self.shell_ext_dispatch("file.preview", &ctx);
                if let Some(Dialog::Preview(preview)) = self.ext.shell.dialog.as_mut() {
                    preview.layout.n_up = 4;
                    preview.watermark = "대외비".into();
                    preview.sheet = 2;
                }
                return true;
            }
            "grid" => {
                self.tab = 2;
                self.ext.shell.show_grid = true;
                return true;
            }
            "hanja-readings" => {
                let ctx = self.ctx.clone();
                self.tab = 2;
                self.shell_ext_dispatch("view.hanja_pronounce", &ctx);
                return true;
            }
            "history" => {
                let ctx = self.ctx.clone();
                self.tab = 7;
                self.shell_ext_dispatch("review.history", &ctx);
                return true;
            }
            "page-outline-off" => {
                self.tab = 2;
                self.ext.shell.page_outline = false;
                return true;
            }
            // Line numbers on in the caret section; with -dialog, 줄 번호 설정 open as well.
            "line-numbers" | "line-numbers-dialog" => {
                let ctx = self.ctx.clone();
                self.tab = 5;
                self.shell_ext_dispatch("page.line_numbers.toggle", &ctx);
                if name == "line-numbers-dialog" {
                    self.shell_ext_dispatch("page.line_numbers.dialog", &ctx);
                }
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
    #[allow(clippy::too_many_arguments)]
    fn print_dialog(
        &mut self,
        ctx: &egui::Context,
        range: &mut usize,
        pages: &mut String,
        copies: &mut u32,
        to_file: &mut bool,
        layout: &mut hwpcraft_engine::PrintLayout,
        watermark: &mut String,
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
                    ui.add_enabled(
                        *copies > 1,
                        egui::Checkbox::new(&mut layout.collate, "한 부씩 찍기"),
                    )
                    .on_disabled_hover_text("매수가 2부 이상일 때 고릅니다.");
                    ui.checkbox(&mut layout.reverse, "역순으로 찍기");
                    group(ui, "인쇄 방식");
                    egui::Grid::new("print-layout")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("쪽 고르기");
                            egui::ComboBox::from_id_salt("print-odd-even")
                                .width(110.0)
                                .selected_text(match layout.odd_even.as_str() {
                                    "odd" => "홀수 쪽만",
                                    "even" => "짝수 쪽만",
                                    _ => "모든 쪽",
                                })
                                .show_ui(ui, |ui| {
                                    for (value, label) in [
                                        ("all", "모든 쪽"),
                                        ("odd", "홀수 쪽만"),
                                        ("even", "짝수 쪽만"),
                                    ] {
                                        if ui
                                            .selectable_label(layout.odd_even == value, label)
                                            .clicked()
                                        {
                                            layout.odd_even = value.into();
                                        }
                                    }
                                });
                            ui.end_row();
                            ui.label("모아 찍기");
                            egui::ComboBox::from_id_salt("print-n-up")
                                .width(110.0)
                                .selected_text(if layout.n_up == 1 {
                                    "한 장에 1쪽".to_owned()
                                } else {
                                    format!("한 장에 {}쪽", layout.n_up)
                                })
                                .show_ui(ui, |ui| {
                                    for n in [1, 2, 4, 8] {
                                        ui.selectable_value(
                                            &mut layout.n_up,
                                            n,
                                            format!("한 장에 {n}쪽"),
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label("워터마크");
                            ui.add(
                                egui::TextEdit::singleline(watermark)
                                    .desired_width(110.0)
                                    .char_limit(40)
                                    .hint_text("예: 대외비"),
                            );
                            ui.end_row();
                        });
                });
            });
            notice(
                ui,
                "워터마크와 모아 찍기는 인쇄물(PDF)에만 들어가고 문서에는 저장되지 않습니다.",
            );
        });
        match button {
            Some(0) => {
                let mut layout = layout.clone();
                layout.watermark =
                    (!watermark.trim().is_empty()).then(|| hwpcraft_engine::Watermark {
                        text: watermark.trim().to_owned(),
                        opacity: None,
                    });
                // A file gets the copies written into it; paper gets them from the printer.
                layout.copies = if *to_file { *copies } else { 1 };
                self.ext.shell.next_print_layout = Some(layout);
                let keep = self.print(*range, pages, *copies, *to_file);
                self.ext.shell.next_print_layout = None;
                keep
            }
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
        let pages = PdfPages::Print(range.to_owned(), self.page);
        self.begin_pdf_prepare(pages, target, (!to_file).then_some(copies))
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
                // The viewer prints one copy unless told otherwise there.
                let copies = if copies > 1 {
                    format!(" 부수({copies}부)는 그 인쇄 창에서 정하세요.")
                } else {
                    String::new()
                };
                self.message = format!(
                    "{count}쪽 PDF를 기본 보기 프로그램으로 열었습니다. 거기서 인쇄하세요.{copies}"
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

/// Hangul dialog frame shared by every area: title bar with ×, content on the left
/// and 80×28 command buttons on the right (`buttons[0]` is the default, drawn with
/// the accent border; the last one cancels or closes). Returns the clicked button.
/// Keyboard: Esc and × count as the last button; Enter counts as the default unless
/// a text field keeps the keyboard (a multi-line editor gets its newline; a
/// single-line field gives up focus on Enter, as in Hangul).
/// Window state is keyed by the title and the calling source location, so two
/// dialogs with the same title (the format area's 스타일 and the object area's
/// 스타일 window) never share position or size; callers pass nothing extra.
#[track_caller]
pub(crate) fn dialog(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    buttons: &[&str],
    body: impl FnOnce(&mut egui::Ui),
) -> Option<usize> {
    let buttons: Vec<(&str, Option<&str>)> = buttons.iter().map(|b| (*b, None)).collect();
    dialog_with(ctx, title, width, &buttons, body)
}

/// [`dialog`] whose buttons can be grey: `(label, Some(reason))` shows the button
/// disabled with `reason` as its tooltip; it neither clicks nor answers Enter.
/// The last button still closes (title ×, Esc).
#[track_caller]
pub(crate) fn dialog_with(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    buttons: &[(&str, Option<&str>)],
    body: impl FnOnce(&mut egui::Ui),
) -> Option<usize> {
    let mut clicked = None;
    let site = std::panic::Location::caller();
    let id = (
        "hwp-shell-dialog",
        title,
        site.file(),
        site.line(),
        site.column(),
    );
    let response = chrome::modal(id).show(ctx, |ui| {
        ui.set_width(width - 24.0);
        if chrome::title(ui, title) {
            clicked = Some(buttons.len() - 1);
        }
        // Buttons keep their label on one line: a long label widens the button column
        // and narrows the body by as much (the usual labels fit in 80 px).
        let label_width = buttons
            .iter()
            .map(|(label, _)| {
                egui::WidgetText::from(*label)
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Button,
                    )
                    .size()
                    .x
                    + 2.0 * ui.spacing().button_padding.x
                    + 4.0
            })
            .fold(80.0_f32, f32::max);
        ui.horizontal_top(|ui| {
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, Color32::from_gray(227)))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.set_width(width - 24.0 - 120.0 - (label_width - 80.0));
                    ui.vertical(body);
                });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                for (i, (label, grey)) in buttons.iter().enumerate() {
                    let (width, color) = if i == 0 && grey.is_none() {
                        (1.5, Tokens::ACCENT)
                    } else {
                        (1.0, Tokens::BORDER)
                    };
                    let b = egui::Button::new(*label)
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(width, color))
                        .min_size(Vec2::new(label_width, 28.0));
                    let response = ui.add_enabled(grey.is_none(), b);
                    let response = match grey {
                        Some(reason) => response.on_disabled_hover_text(*reason),
                        None => response,
                    };
                    if response.clicked() {
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
    // Alt + the letter in "(D)" presses that button, as in Hangul's dialogs.
    if clicked.is_none() {
        clicked = buttons.iter().position(|(label, grey)| {
            grey.is_none()
                && chrome::mnemonic(label)
                    .is_some_and(|key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, key)))
        });
    }
    if clicked.is_none()
        && buttons.first().is_some_and(|(_, grey)| grey.is_none())
        && !ctx.egui_wants_keyboard_input()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
    {
        clicked = Some(0);
    }
    clicked
}

/// Guidance box at the bottom of a dialog for limits and unsupported parts: honest,
/// readable and set apart from the controls, instead of small red text.
pub(crate) fn notice(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    egui::Frame::NONE
        .fill(Color32::from_rgb(243, 246, 251))
        .stroke(Stroke::new(1.0, Color32::from_rgb(214, 224, 240)))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("ⓘ").color(Tokens::ACCENT).size(13.0));
                ui.label(RichText::new(text).size(12.0).color(Color32::from_gray(70)));
            });
        });
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
/// The picture and shape kinds 바탕쪽 넣기 offers: (`page.master.object.insert` kind, label).
const MASTER_OBJECT_KINDS: [(&str, &str); 4] = [
    ("picture", "그림"),
    ("rectangle", "직사각형"),
    ("ellipse", "타원"),
    ("line", "직선"),
];

/// 바탕쪽 창의 그림·도형 넣기 칸: placed on the paper, measured from its top-left corner.
pub(crate) struct MasterObject {
    /// Index into `MASTER_OBJECT_KINDS`.
    kind: usize,
    /// Picture file (그림 only).
    path: String,
    x_mm: f64,
    y_mm: f64,
    width_mm: f64,
    /// Shapes only: a picture keeps its proportions.
    height_mm: f64,
    /// 글 앞 (otherwise 글 뒤, like the master page's text).
    front: bool,
}

impl Default for MasterObject {
    fn default() -> Self {
        Self {
            kind: 1,
            path: String::new(),
            x_mm: 20.0,
            y_mm: 20.0,
            width_mm: 40.0,
            height_mm: 20.0,
            front: false,
        }
    }
}

impl MasterObject {
    /// `page.master.object.insert` parameters for the master page `apply_to`.
    fn params(&self, apply_to: &str) -> serde_json::Value {
        let (kind, _) = MASTER_OBJECT_KINDS[self.kind.min(MASTER_OBJECT_KINDS.len() - 1)];
        let mut params = json!({
            "applyTo": apply_to,
            "kind": kind,
            "x_mm": self.x_mm,
            "y_mm": self.y_mm,
            "width_mm": self.width_mm,
            "wrap": if self.front { "front" } else { "behind" },
        });
        if kind == "picture" {
            params["path"] = json!(self.path.trim());
        } else {
            params["height_mm"] = json!(self.height_mm);
        }
        params
    }
}

impl HwpApp {
    /// 바탕쪽: 양쪽·홀수·짝수 바탕쪽의 글을 만들고 고치거나 지우고, 그림·도형을 넣고
    /// 지운다. Other kinds stay as they are (the reason is shown).
    fn master_page_dialog(
        &mut self,
        ctx: &egui::Context,
        kind: &mut usize,
        list: &mut serde_json::Value,
        text: &mut String,
        objects: &mut serde_json::Value,
        object: &mut MasterObject,
    ) -> bool {
        // The kinds HwpCraft edits, by radio index: (applyTo, model name).
        const KINDS: [(&str, &str); 3] = [("both", "Both"), ("odd", "Odd"), ("even", "Even")];
        let page_of = |kind: usize| {
            let (_, model) = KINDS.get(kind)?;
            list["masterPages"]
                .as_array()?
                .iter()
                .find(|m| m["applyTo"] == *model && m["extension"] == false)
                .cloned()
        };
        let exists = page_of(*kind).is_some();
        let blocked = list["reason"].as_str();
        let why = if *kind >= KINDS.len() {
            Some(
                "마지막 쪽·임의 쪽 바탕쪽은 아직 만들 수 없습니다. 양쪽·홀수 쪽·짝수 쪽 중에서 고르세요.",
            )
        } else {
            blocked
        };
        let delete_why = if !exists {
            Some("이 구역에는 지울 그 종류의 바탕쪽이 없습니다.")
        } else {
            blocked
        };
        let apply_to = KINDS.get(*kind).map_or("both", |(name, _)| *name);
        // 넣기 or an object's 지우기, run when the dialog has been drawn.
        let mut request = None;
        let button = dialog_with(
            ctx,
            "바탕쪽",
            460.0,
            &[
                (
                    if exists {
                        "고치기(D)"
                    } else {
                        "만들기(D)"
                    },
                    why,
                ),
                ("지우기", delete_why),
                ("취소", None),
            ],
            |ui| {
                group(ui, "종류");
                let before = *kind;
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
                if *kind != before {
                    *text = page_of(*kind)
                        .and_then(|m| m["text"].as_str().map(str::to_owned))
                        .unwrap_or_default();
                }
                group(ui, "바탕쪽 글");
                ui.add_enabled(
                    why.is_none(),
                    egui::TextEdit::multiline(text)
                        .desired_rows(4)
                        .desired_width(300.0)
                        .hint_text("예: 대외비"),
                );
                group(ui, "현재 구역의 바탕쪽");
                let pages = list["masterPages"].as_array().cloned().unwrap_or_default();
                if pages.is_empty() {
                    ui.label("없음");
                }
                for m in pages {
                    let kind = match m["applyTo"].as_str().unwrap_or("") {
                        "Both" => "양쪽",
                        "Odd" => "홀수 쪽",
                        "Even" => "짝수 쪽",
                        other => other,
                    };
                    ui.label(format!(
                        "{kind}{} · {}",
                        if m["extension"] == true {
                            " (확장)"
                        } else {
                            ""
                        },
                        m["text"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(40)
                            .collect::<String>()
                    ));
                }
                group(ui, "그림·도형");
                let mm = |v: &serde_json::Value| format!("{:.1}", v.as_f64().unwrap_or(0.0));
                let mine: Vec<&serde_json::Value> = objects["objects"]
                    .as_array()
                    .map(|all| all.iter().filter(|o| o["applyTo"] == apply_to).collect())
                    .unwrap_or_default();
                if mine.is_empty() || why.is_some() {
                    ui.label("없음");
                }
                for o in mine.into_iter().filter(|_| why.is_none()) {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} · 자리 {}×{} mm · 크기 {}×{} mm · {}",
                            if o["kind"] == "picture" {
                                "그림"
                            } else {
                                "도형"
                            },
                            mm(&o["x_mm"]),
                            mm(&o["y_mm"]),
                            mm(&o["width_mm"]),
                            mm(&o["height_mm"]),
                            if o["wrap"] == "front" {
                                "글 앞"
                            } else {
                                "글 뒤"
                            }
                        ));
                        if ui.small_button("지우기").clicked() {
                            request = Some((
                                "page.master.object.delete",
                                json!({"applyTo": apply_to, "index": o["index"]}),
                            ));
                        }
                    });
                }
                ui.add_enabled_ui(why.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("종류");
                        egui::ComboBox::from_id_salt("master-object-kind")
                            .width(91.0)
                            .selected_text(MASTER_OBJECT_KINDS[object.kind].1)
                            .show_ui(ui, |ui| {
                                for (i, (_, label)) in MASTER_OBJECT_KINDS.iter().enumerate() {
                                    ui.selectable_value(&mut object.kind, i, *label);
                                }
                            });
                        ui.radio_value(&mut object.front, false, "글 뒤");
                        ui.radio_value(&mut object.front, true, "글 앞");
                    });
                    if object.kind == 0 {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut object.path)
                                    .desired_width(240.0)
                                    .hint_text("그림 파일"),
                            );
                            if ui.button("찾기…").clicked()
                                && let Some(path) = rfd::FileDialog::new()
                                    .add_filter("그림 파일", &["png", "jpg", "jpeg", "bmp", "gif"])
                                    .pick_file()
                            {
                                object.path = path.display().to_string();
                            }
                        });
                    }
                    fn length(value: &mut f64, min: f64) -> egui::DragValue<'_> {
                        egui::DragValue::new(value)
                            .range(min..=1000.0)
                            .speed(0.5)
                            .suffix(" mm")
                    }
                    ui.horizontal(|ui| {
                        ui.label("가로");
                        ui.add(length(&mut object.x_mm, 0.0));
                        ui.label("세로");
                        ui.add(length(&mut object.y_mm, 0.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("너비");
                        ui.add(length(&mut object.width_mm, 1.0));
                        if object.kind == 0 {
                            ui.label("(높이는 그림 비율대로)");
                        } else {
                            ui.label("높이");
                            ui.add(length(&mut object.height_mm, 1.0));
                        }
                    });
                    let ready = object.kind != 0 || !object.path.trim().is_empty();
                    if ui.add_enabled(ready, egui::Button::new("넣기")).clicked() {
                        request = Some(("page.master.object.insert", object.params(apply_to)));
                    }
                });
                notice(
                    ui,
                    &match blocked {
                        Some(reason) => reason.to_owned(),
                        None => "바탕쪽 글은 쪽의 본문 뒤에 깔립니다. 각 줄이 한 문단이 됩니다. \
                                 홀수 쪽·짝수 쪽 바탕쪽은 그 쪽에서 양쪽 바탕쪽 대신 쓰입니다. \
                                 그림·도형은 종이 왼쪽 위에서 잰 자리에 놓이고, 넣기·지우기는 \
                                 바로 문서에 들어갑니다(실행 취소로 되돌림)."
                            .to_owned(),
                    },
                );
            },
        );
        if let Some((id, params)) = request {
            self.master_object_request(id, params, list, objects);
            return true;
        }
        let (id, params) = match button {
            Some(0) => (
                "page.master.set",
                json!({ "applyTo": apply_to, "text": text.as_str() }),
            ),
            Some(1) => ("page.master.delete", json!({ "applyTo": apply_to })),
            Some(_) => return false,
            None => return true,
        };
        let r = self.session.run(id, params);
        if self.result(r).is_some() {
            self.message = if id == "page.master.delete" {
                "바탕쪽을 지웠습니다.".into()
            } else {
                "바탕쪽을 저장했습니다. 실행 취소로 되돌릴 수 있습니다.".into()
            };
            return false;
        }
        true
    }

    /// Run 바탕쪽 창's 넣기 or 지우기 for a picture or shape, then show the section's
    /// master pages and objects again (넣기 makes the master page when it is missing).
    fn master_object_request(
        &mut self,
        id: &str,
        params: serde_json::Value,
        list: &mut serde_json::Value,
        objects: &mut serde_json::Value,
    ) {
        let r = self.session.run(id, params);
        if self.result(r).is_some() {
            self.message = if id == "page.master.object.delete" {
                "바탕쪽 그림·도형을 지웠습니다. 실행 취소로 되돌릴 수 있습니다.".into()
            } else {
                "바탕쪽에 그림·도형을 넣었습니다. 실행 취소로 되돌릴 수 있습니다.".into()
            };
        }
        *list = self
            .session
            .run("page.master.list", json!({}))
            .unwrap_or_default();
        *objects = self
            .session
            .run("page.master.object.list", json!({}))
            .unwrap_or_default();
    }
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

/// Hangul two-key shortcuts: Ctrl+prefix, then the key, and the command it runs.
/// The command list (`COMMANDS`) shows the same keys; a test keeps the two in step.
const CHORDS: &[(char, egui::Key, &str)] = {
    use egui::Key;
    &[
        ('N', Key::P, "page.number"),
        ('N', Key::H, "page.header"),
        ('N', Key::T, "table.create"),
        ('N', Key::A, "table.split_table"),
        ('N', Key::Z, "table.merge_table"),
        ('N', Key::I, "insert.picture_dialog"),
        ('N', Key::N, "insert.footnote"),
        ('N', Key::E, "insert.endnote"),
        ('N', Key::M, "insert.equation"),
        ('G', Key::C, "view.control_codes"),
        ('G', Key::T, "view.paragraph_marks"),
        ('G', Key::L, "view.page_outline"),
        ('G', Key::Q, "view.zoom_100"),
        ('G', Key::P, "view.zoom_fit_page"),
        ('G', Key::W, "view.zoom_fit_width"),
        ('K', Key::H, "insert.hyperlink"),
        ('K', Key::E, "insert.field"),
        ('K', Key::N, "format.numbering.dialog"),
        ('K', Key::O, "format.outline.dialog"),
        ('Q', Key::I, "file.doc_info"),
        ('Q', Key::D, "file.doc_info"),
        ('Q', Key::R, "view.link_next"),
    ]
};

/// Settings folder shared with the engine (`hwpcraft_engine::config_dir`: Windows
/// `%APPDATA%`, macOS `~/Library/Application Support`, Linux `$XDG_CONFIG_HOME`).
/// `None` means settings are not persisted.
pub(crate) fn config_dir() -> Option<PathBuf> {
    hwpcraft_engine::config_dir()
}
fn recent_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("recent.txt"))
}
/// Screen settings kept between runs (`key=value` lines).
fn view_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("view.txt"))
}
const SHOW_UNSUPPORTED: &str = "show_unsupported_commands";
/// 지원하지 않는 명령 보기 from `view.txt`; anything unreadable means off.
#[cfg(test)]
pub(crate) fn parse_view(text: &str) -> bool {
    settings::Settings::parse(text).show_unsupported
}
/// Recent documents are kept as absolute paths with one separator style, so a
/// document opened from a relative path or with `/` is still found (and not listed
/// twice) after the working folder changes.
pub(crate) fn recent_path(path: &std::path::Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}
/// Move `path` to the front, dropping duplicates and anything past `RECENT_LIMIT`.
pub(crate) fn push_recent(list: &mut Vec<PathBuf>, path: &std::path::Path) {
    let path = recent_path(path);
    list.retain(|p| recent_path(p) != path);
    list.insert(0, path);
    list.truncate(RECENT_LIMIT);
}
pub(crate) fn parse_recent(text: &str) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = recent_path(std::path::Path::new(line));
        if !list.contains(&path) {
            list.push(path);
        }
    }
    list.truncate(RECENT_LIMIT);
    list
}
/// Folder of a recent document for display: the platform separator only, and the
/// middle elided (`C:\…\reports`) when longer than `max_chars`.
pub(crate) fn recent_folder(path: &std::path::Path, max_chars: usize) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    let folder = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = if cfg!(windows) {
        folder.replace('/', "\\")
    } else {
        folder
    };
    if folder.chars().count() <= max_chars {
        return folder;
    }
    let parts: Vec<&str> = folder.split(sep).collect();
    let head = parts.first().copied().unwrap_or("");
    let mut tail: Vec<&str> = Vec::new();
    for part in parts.iter().skip(1).rev() {
        let used = head.chars().count()
            + 2
            + tail.iter().map(|t| t.chars().count() + 1).sum::<usize>()
            + part.chars().count();
        if !tail.is_empty() && used > max_chars {
            break;
        }
        tail.push(part);
    }
    tail.reverse();
    format!("{head}{sep}…{sep}{}", tail.join(&sep.to_string()))
}
/// File name of a recent document, shortened at the end when longer than `max_chars`.
pub(crate) fn recent_name(path: &std::path::Path, max_chars: usize) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    if name.chars().count() <= max_chars {
        return name;
    }
    let keep: String = name.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{keep}…")
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
    (1, 'J', "menu:쪽 여백"),
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
    (2, 'C', "menu:문서 보기 색"),
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
    (2, 'O', "menu:창 배열"),
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
    (3, 'L', "menu:양식 개체"),
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
    (5, 'Z', "menu:쪽 여백"),
    (5, 'C', "page.text_direction"),
    (5, 'F', "page.master"),
    (5, 'H', "page.header"),
    (5, 'T', "page.footer"),
    (5, 'B', "page.border_fill"),
    (5, 'G', "page.number"),
    (5, 'W', "page.new_number"),
    (5, 'I', "page.hide"),
    (5, 'M', "menu:줄 번호"),
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
    (8, 'L', "menu:블록 계산"),
    (8, 'R', "macro.record.start"),
    (8, 'S', "review.spelling"),
    (8, 'Q', "tools.autocorrect"),
    (8, 'A', "tools.dictionary"),
    (8, 'D', "tools.translate"),
    (8, 'P', "tools.presentation"),
    (8, 'K', "tools.keyboard"),
    (8, 'Z', "tools.customize"),
    (8, 'U', "tools.options"),
    (8, 'E', "tools.user_info"),
    // Big buttons, menus and contextual tabs that had no tip. A drop-down menu is
    // "menu:<label>" and opens on its letter; Hangul's letter where it is free, then
    // the first free letter or digit.
    (1, 'H', "insert.shape.rectangle"),
    (1, 'W', "object.unlock_all"),
    (1, 'X', "menu:찾기"),
    (1, 'Z', "menu:문서 영역 편집"),
    (2, 'Y', "menu:안내선"),
    (2, 'Z', "menu:편집 화면 나누기"),
    (2, '1', "menu:새 창"),
    (2, '2', "view.editor"),
    (3, 'A', "menu:표"),
    (3, 'G', "menu:차트"),
    (3, 'I', "menu:번호 넣기"),
    (3, 'J', "menu:각주/미주 모양"),
    (3, 'Q', "note.convert"),
    (3, 'V', "note.delete"),
    (3, 'X', "field.delete"),
    (3, 'Z', "view.link_prev"),
    (3, '1', "view.link_next"),
    (3, '2', "format.char_overlap.dialog"),
    (3, '3', "format.ruby.dialog"),
    (3, '4', "menu:세로 글상자"),
    (3, '5', "menu:상용구"),
    (4, 'A', "format.font.prev"),
    (4, 'G', "format.font.next"),
    (4, 'O', "format.superscript.cycle"),
    (4, 'Q', "menu:문단 정렬"),
    (4, 'S', "format.margin_left.decrease"),
    (4, 'Z', "format.margin_left.increase"),
    (4, '1', "format.drop_cap"),
    (4, '2', "format.indent.positive"),
    (4, '3', "format.indent.negative"),
    (4, '4', "format.indent.at_caret"),
    (4, '7', "format.paragraph.up"),
    (4, '8', "format.paragraph.down"),
    (7, 'T', "memo.list"),
    (8, 'J', "menu:스크립트 매크로"),
    (8, 'M', "menu:MCP 연결"),
    (8, 'N', "object.list"),
    (8, 'O', "help.search"),
    (8, 'V', "help"),
    (9, 'A', "table.select"),
    (9, 'B', "table.properties"),
    (9, 'C', "table.style"),
    (9, 'D', "table.cell_border_fill"),
    (9, 'E', "table.cell_shade.darker"),
    (9, 'F', "table.cell_shade.lighter"),
    (9, 'G', "table.draw"),
    (9, 'H', "table.eraser"),
    (9, 'I', "table.caption_text"),
    (9, 'J', "table.delete"),
    (10, 'A', "table.split"),
    (10, 'B', "table.merge"),
    (10, 'C', "table.equal_height"),
    (10, 'D', "table.equal_width"),
    (10, 'E', "table.text_direction"),
    (10, 'F', "table.align.top"),
    (10, 'G', "table.align.center"),
    (10, 'H', "table.align.bottom"),
    (10, 'I', "table.cell_margin"),
    (10, 'J', "table.margin"),
    (10, 'K', "table.transpose"),
    (10, 'L', "table.split_table"),
    (10, 'M', "table.merge_table"),
    (10, 'N', "table.formula"),
    (10, 'O', "table.block_sum"),
    (10, 'P', "table.block_average"),
    (10, 'Q', "table.block_product"),
    (10, 'R', "table.thousands"),
    (10, 'S', "table.thousands_remove"),
    (10, 'T', "menu:배치"),
    (10, 'U', "table.caption_text"),
    (10, 'V', "table.cell_block"),
    (10, 'W', "menu:셀 이동"),
    (10, 'X', "table.sort"),
    (10, 'Y', "menu:채우기"),
    (10, 'Z', "table.to_text"),
    (11, 'A', "object.properties"),
    (11, 'B', "object.caption"),
    (11, 'C', "menu:배치"),
    (11, 'D', "menu:회전"),
    (11, 'E', "picture.save"),
    (11, 'F', "picture.save_all"),
    (11, 'G', "menu:색 조정"),
    (11, 'H', "menu:맞춤"),
    (11, 'I', "object.protect_size"),
    (11, 'J', "menu:바깥 여백"),
    (11, 'K', "object.format.copy"),
    (11, 'L', "object.format.paste"),
    (11, 'M', "object.next"),
    (11, 'N', "object.prev"),
    (11, 'O', "object.delete"),
    (11, 'P', "object.list"),
    (12, 'A', "object.properties"),
    (12, 'B', "object.caption"),
    (12, 'C', "menu:배치"),
    (12, 'D', "menu:회전"),
    (12, 'E', "object.group"),
    (12, 'F', "object.ungroup"),
    (12, 'G', "object.z_order.front"),
    (12, 'H', "object.z_order.forward"),
    (12, 'I', "object.z_order.back"),
    (12, 'J', "object.z_order.backward"),
    (12, 'K', "menu:채우기 색"),
    (12, 'L', "menu:선 색"),
    (12, 'M', "menu:선 종류"),
    (12, 'N', "menu:선 굵기"),
    (12, 'O', "menu:끝 화살표"),
    (12, 'P', "menu:시작 화살표"),
    (12, 'Q', "menu:그림자"),
    (12, 'R', "menu:연결선 종류"),
    (12, 'S', "menu:글상자 정렬"),
    (12, 'T', "menu:글자 방향"),
    (12, 'U', "menu:글상자 여백"),
    (12, 'V', "menu:맞춤"),
    (12, 'W', "object.protect_size"),
    (12, 'X', "menu:바깥 여백"),
    (12, 'Y', "object.format.copy"),
    (12, 'Z', "object.format.paste"),
    (12, '1', "object.next"),
    (12, '2', "object.prev"),
    (12, '3', "object.delete"),
    (12, '4', "object.list"),
];
/// Letter shown for a command id in the given tab.
pub(crate) fn alt_letter(tab: usize, id: &str) -> Option<char> {
    ALT_KEYS
        .iter()
        .find(|(t, _, i)| *t == tab && *i == id)
        .map(|(_, c, _)| *c)
}

impl HwpApp {
    /// 보기 → 지원하지 않는 명령: flip it and keep the choice for the next run.
    pub(crate) fn toggle_unsupported_commands(&mut self) {
        let show = !self.ext.shell.show_unsupported;
        self.ext.shell.show_unsupported = show;
        self.message = if show {
            "지원하지 않는 명령을 리본에 회색으로 보입니다.".into()
        } else {
            "지원하지 않는 명령을 리본에서 숨겼습니다. 탭 끝의 [지원 안 함]에서 목록을 볼 수 있습니다."
                .into()
        };
        // Keep the other settings (자동 복구, 확대, 단축키) in the same file.
        self.ext.shell.settings.show_unsupported = show;
        self.ext.shell.settings.save();
    }
    /// Per-frame bookkeeping: recent documents and view marks that follow document replacement.
    fn shell_ext_frame(&mut self) {
        if !self.ext.shell.recent_loaded {
            self.ext.shell.recent_loaded = true;
            if let Some(text) = recent_file().and_then(|p| std::fs::read_to_string(p).ok()) {
                self.ext.shell.recent = parse_recent(&text);
            }
            if let Some(text) = view_file().and_then(|p| std::fs::read_to_string(p).ok()) {
                let settings = settings::Settings::parse(&text);
                self.ext.shell.show_unsupported = settings.show_unsupported;
                // Captures and tests keep the default zoom.
                if !cfg!(test) && self.screenshot.is_none() {
                    self.set_zoom(settings.start_zoom as f32 / 100.0);
                }
                self.ext.shell.settings = settings;
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
    #[allow(clippy::too_many_arguments)]
    fn label_dialog(
        &mut self,
        ctx: &egui::Context,
        preset: &mut usize,
        rows: &mut u32,
        columns: &mut u32,
        width: &mut f64,
        height: &mut f64,
        gap: &mut f64,
    ) -> bool {
        let why = self
            .session
            .dirty()
            .then_some("저장하지 않은 변경 내용이 있습니다. 먼저 저장하거나 [새 창]에서 만드세요.");
        let button = dialog_with(
            ctx,
            "라벨 문서 만들기",
            460.0,
            &[("만들기(D)", why), ("취소", None)],
            |ui| {
                group(ui, "라벨 종류 (A4)");
                for (i, (name, r, c, w, h, g)) in LABEL_PRESETS.iter().enumerate() {
                    if ui.radio_value(preset, i, *name).clicked() && i + 1 < LABEL_PRESETS.len() {
                        (*rows, *columns, *width, *height, *gap) = (*r, *c, *w, *h, *g);
                    }
                }
                group(ui, "크기");
                let custom = *preset + 1 == LABEL_PRESETS.len();
                ui.add_enabled_ui(custom, |ui| {
                    egui::Grid::new("label-size")
                        .num_columns(4)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("줄");
                            ui.add(egui::DragValue::new(rows).range(1..=40));
                            ui.label("칸");
                            ui.add(egui::DragValue::new(columns).range(1..=10));
                            ui.end_row();
                            ui.label("너비");
                            ui.add(
                                egui::DragValue::new(width)
                                    .range(5.0..=300.0)
                                    .speed(0.1)
                                    .suffix(" mm"),
                            );
                            ui.label("높이");
                            ui.add(
                                egui::DragValue::new(height)
                                    .range(5.0..=300.0)
                                    .speed(0.1)
                                    .suffix(" mm"),
                            );
                            ui.end_row();
                            ui.label("칸 사이");
                            ui.add(
                                egui::DragValue::new(gap)
                                    .range(0.0..=30.0)
                                    .speed(0.1)
                                    .suffix(" mm"),
                            );
                            ui.end_row();
                        });
                });
                notice(
                    ui,
                    "새 A4 문서에 테두리 없는 표로 라벨 칸을 만듭니다. 각 칸에 글을 넣으세요. \
                     [보기 → 투명 선]을 켜면 칸 경계가 보입니다.",
                );
            },
        );
        match button {
            Some(0) => {
                let r = self.session.run(
                    "page.label",
                    json!({
                        "rows": *rows, "columns": *columns,
                        "labelWidthMm": *width, "labelHeightMm": *height, "gapXMm": *gap,
                    }),
                );
                if let Some(v) = self.result(r) {
                    self.page = 0;
                    self.message = format!("라벨 {}칸짜리 새 문서를 만들었습니다.", v["labels"]);
                    false
                } else {
                    true
                }
            }
            Some(_) => false,
            None => true,
        }
    }

    /// Saving a document that holds Hangul track changes (as HWP, HWPX or DOCX) or picture
    /// effects (as HWP) opens [`Dialog::TrackedSave`] instead; true when it did. One
    /// window asks about both when both apply.
    pub(crate) fn confirm_tracked_save(&mut self, path: &std::path::Path) -> bool {
        let editable = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            ["hwp", "hwpx", "docx"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        });
        let tracked = editable && self.session.has_unkept_track_changes();
        let effects = self.session.picture_effects_blocking_save(path);
        if tracked || effects > 0 {
            self.ext.shell.dialog = Some(Dialog::TrackedSave {
                path: path.to_owned(),
                tracked,
                effects,
            });
            return true;
        }
        false
    }

    fn tracked_save_dialog(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
        tracked: bool,
        effects: usize,
    ) -> bool {
        let (title, accept) = match (tracked, effects > 0) {
            (true, true) => (
                "변경 추적·그림 효과가 있는 문서 저장",
                "결과본·효과 빼고 저장(D)",
            ),
            (false, true) => ("그림 효과가 있는 문서 저장", "효과 빼고 저장(D)"),
            _ => ("변경 추적 문서 저장", "결과본으로 저장(D)"),
        };
        let other = if effects > 0 {
            "HWPX로 저장…"
        } else {
            "다른 이름으로 저장…"
        };
        let button = dialog(ctx, title, 480.0, &[accept, other, "취소"], |ui| {
            if tracked {
                ui.label(
                    "이 문서에는 한글의 변경 추적 기록이 있습니다. HwpCraft는 변경 추적 기록을 \
                     보존하지 못합니다. 저장하면 변경 내용이 지금 보이는 결과본으로 합쳐지고 \
                     변경 기록은 사라집니다. 원본을 남기려면 다른 이름으로 저장하세요.",
                );
            }
            if effects > 0 {
                if tracked {
                    ui.add_space(6.0);
                }
                ui.label(format!(
                    "이 문서의 그림 효과 {effects}개(그림자·네온·부드러운 가장자리·반사)는 아직 \
                     HWP(.hwp)로 저장할 수 없습니다. 효과를 빼고 저장하거나, 효과를 남기려면 \
                     HWPX로 저장하세요."
                ));
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("저장할 파일: {}", path.display()))
                    .color(Color32::from_gray(110)),
            );
        });
        match button {
            Some(0) if effects > 0 => {
                self.ext.shell.dialog = None;
                let r = self.session.save_dropping_picture_effects(path, tracked);
                if let Some(n) = self.result(r) {
                    self.message = format!("그림 효과 {n}개를 빼고 저장 완료 · {}", path.display());
                }
                false
            }
            Some(0)
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("docx")) =>
            {
                self.ext.shell.dialog = None;
                self.export_docx(path, true);
                false
            }
            Some(0) => {
                let r = self.session.save_accepting_track_changes(path);
                if self.result(r).is_some() {
                    self.message = format!("결과본으로 저장 완료 · {}", path.display());
                }
                false
            }
            Some(1) => {
                // A new name keeps the original file; HWPX keeps the picture effects.
                self.ext.shell.dialog = None;
                self.save(true, (effects > 0).then_some("hwpx"));
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    /// Word 문서 (DOCX) export: the document keeps its own file and saved state. What
    /// DOCX cannot hold is named in the message line. `accept` merges Hangul track
    /// changes into the result (after the 변경 추적 문서 저장 question).
    pub(crate) fn export_docx(&mut self, path: &std::path::Path, accept: bool) -> bool {
        let r = self.session.export_docx(path, accept);
        let Some(missing) = self.result(r) else {
            return false;
        };
        self.message = if missing.is_empty() {
            format!("DOCX로 내보냈습니다 · {}", path.display())
        } else {
            format!(
                "DOCX로 내보냈습니다 · {} — 담지 못한 것: {}",
                path.display(),
                missing.join(", ")
            )
        };
        true
    }

    fn template_list(&mut self) -> Vec<serde_json::Value> {
        self.session
            .run("file.template.list", json!({}))
            .ok()
            .and_then(|v| v["templates"].as_array().cloned())
            .unwrap_or_default()
    }

    /// 문서마당: start a new document from a saved template, delete one, or save the
    /// current document as a template.
    fn templates_dialog(
        &mut self,
        ctx: &egui::Context,
        list: &mut Vec<serde_json::Value>,
        selected: &mut usize,
        name: &mut String,
    ) -> bool {
        let chosen = list
            .get(*selected)
            .and_then(|t| t["name"].as_str())
            .map(str::to_owned);
        let open_why = if chosen.is_none() {
            Some("문서마당에 서식이 없습니다. 아래에서 현재 문서를 서식으로 저장하세요.")
        } else if self.session.dirty() {
            Some(
                "저장하지 않은 변경 내용이 있습니다. 먼저 저장하거나 [새 창]에서 문서마당을 여세요.",
            )
        } else {
            None
        };
        let delete_why = chosen.is_none().then_some("지울 서식이 없습니다.");
        let mut save = false;
        let button = dialog_with(
            ctx,
            "문서마당",
            520.0,
            &[
                ("새 문서 만들기(D)", open_why),
                ("지우기", delete_why),
                ("닫기", None),
            ],
            |ui| {
                group(ui, "내 서식");
                if list.is_empty() {
                    ui.label(
                        RichText::new("저장한 서식이 없습니다.").color(Color32::from_gray(120)),
                    );
                }
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (i, t) in list.iter().enumerate() {
                            let label = format!(
                                "{}    {}",
                                t["name"].as_str().unwrap_or("?"),
                                t["format"].as_str().unwrap_or("").to_uppercase()
                            );
                            ui.selectable_value(selected, i, label);
                        }
                    });
                group(ui, "현재 문서를 서식으로 저장");
                ui.horizontal(|ui| {
                    ui.label("서식 이름");
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .desired_width(200.0)
                            .char_limit(60),
                    );
                    save = ui
                        .add_enabled(!name.trim().is_empty(), egui::Button::new("저장"))
                        .clicked();
                });
                notice(
                    ui,
                    "서식은 설정 폴더의 templates 폴더에 저장됩니다. 서식으로 만든 새 문서는 \
                     파일 이름이 없어서, 처음 저장할 때 위치를 고릅니다.",
                );
            },
        );
        if save {
            let r = self
                .session
                .run("file.template.save", json!({ "name": name.trim() }));
            if self.result(r).is_some() {
                self.message = format!("현재 문서를 문서마당 [{}]로 저장했습니다.", name.trim());
                *list = self.template_list();
            }
            return true;
        }
        match (button, chosen) {
            (Some(0), Some(template)) => {
                let r = self
                    .session
                    .run("file.template.new", json!({ "name": template }));
                if self.result(r).is_some() {
                    self.message = format!("문서마당 [{template}]로 새 문서를 만들었습니다.");
                    self.page = 0;
                }
                false
            }
            (Some(1), Some(template)) => {
                let r = self
                    .session
                    .run("file.template.delete", json!({ "name": template }));
                if self.result(r).is_some() {
                    *list = self.template_list();
                    *selected = 0;
                }
                true
            }
            (Some(_), _) => false,
            (None, _) => true,
        }
    }

    fn guide_dialog(&mut self, ctx: &egui::Context, vertical: &mut bool, mm: &mut f32) -> bool {
        let button = dialog(
            ctx,
            "안내선 넣기",
            360.0,
            &["넣기(D)", "취소"],
            |ui| {
                group(ui, "방향");
                ui.horizontal(|ui| {
                    ui.radio_value(vertical, true, "세로선");
                    ui.radio_value(vertical, false, "가로선");
                });
                group(ui, "위치");
                ui.horizontal(|ui| {
                    ui.label(if *vertical {
                        "용지 왼쪽에서"
                    } else {
                        "용지 위에서"
                    });
                    ui.add(
                        egui::DragValue::new(mm)
                            .range(0.0..=1000.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    );
                });
                notice(ui, "안내선은 화면에만 보이고 인쇄·저장되지 않습니다.");
            },
        );
        match button {
            Some(0) => {
                self.ext.shell.guides.push((*vertical, *mm));
                self.ext.shell.show_guides = true;
                self.message = format!(
                    "{}을 {:.1} mm에 넣었습니다.",
                    if *vertical {
                        "세로 안내선"
                    } else {
                        "가로 안내선"
                    },
                    mm
                );
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    /// 새 창: start another HwpCraft (same executable) with `path`, or an empty document.
    pub(crate) fn open_new_window(&mut self, path: Option<PathBuf>) {
        let started = std::env::current_exe().and_then(|exe| {
            let mut command = std::process::Command::new(exe);
            if let Some(path) = &path {
                command.arg(path);
            }
            command.spawn()
        });
        self.message = match (started, path) {
            (Ok(_), Some(path)) => format!(
                "새 창에서 엽니다: {}",
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned()
                )
            ),
            (Ok(_), None) => "새 창을 열었습니다.".into(),
            (Err(e), _) => format!("새 창을 열지 못했습니다: {e}"),
        };
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
        if self.frames >= 4
            && let Some(id) = self.ext.shell.view_keytip.take()
        {
            self.ribbon_keytip(id, ctx);
        }
        if self.frames >= 4
            && let Some(popup) = self.ext.shell.view_popup.take()
        {
            egui::Popup::open_id(ctx, popup);
        }
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
                "Alt 키팁: 메뉴 탭 글자(F E U D J W R H K, 표·그림·도형 탭 Y L P X)를 누르세요. Esc로 닫습니다.".into()
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
                // The contextual tabs (Y L P X) are reached from the tab tips only: Alt+L and
                // Alt+P pressed together stay 글자 모양 and 인쇄.
                if let Some(index) = tab_keys
                    .iter()
                    .find(|(k, _)| k.name() == letter.to_string())
                    .map(|(_, index)| *index)
                    .or_else(|| self.keytip_context_tab(letter))
                {
                    self.keytip_tab(index, ctx);
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
                    let field = ui.add_enabled(
                        !*remove,
                        egui::TextEdit::singleline(password)
                            .password(true)
                            .desired_width(220.0),
                    );
                    if !*remove {
                        chrome::first_focus(&field);
                    }
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
    /// 미리 보기: the sheets as they print — print profile, odd/even pages, 모아 찍기
    /// and watermark — one sheet at a time. 인쇄 carries the settings to the print window.
    fn preview_dialog(&mut self, ctx: &egui::Context, preview: &mut PreviewState) -> bool {
        let mut layout = preview.layout.clone();
        layout.watermark =
            (!preview.watermark.trim().is_empty()).then(|| hwpcraft_engine::Watermark {
                text: preview.watermark.trim().to_owned(),
                opacity: None,
            });
        let key = format!("{layout:?}");
        let generation = self.session.generation;
        if preview.texture.as_ref().is_none_or(|(sheet, g, k, _, _)| {
            *sheet != preview.sheet || *g != generation || *k != key
        }) {
            let sheet = self
                .session
                .print_preview_sheet(preview.sheet, &layout)
                .and_then(|(svg, sheets)| Ok((self.renderer.screen(&svg, 0.75)?, sheets)));
            match sheet {
                Ok((p, sheets)) => {
                    preview.sheets = sheets;
                    preview.sheet = preview.sheet.min(sheets.saturating_sub(1));
                    let image = egui::ColorImage::from_rgba_premultiplied(
                        [p.width as usize, p.height as usize],
                        &p.rgba,
                    );
                    let handle = ctx.load_texture(
                        format!("preview-{}-{generation}", preview.sheet),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    preview.texture = Some((
                        preview.sheet,
                        generation,
                        key,
                        handle,
                        Vec2::new(p.width as f32, p.height as f32),
                    ));
                }
                Err(e) => {
                    self.message = format!("미리 보기를 만들지 못했습니다: {e:#}");
                    preview.texture = None;
                }
            }
        }
        let count = preview.sheets.max(1);
        let mut go: Option<usize> = None;
        let mut print = false;
        let button = dialog(ctx, "미리 보기", 760.0, &["인쇄(D)", "닫기"], |ui| {
            ui.horizontal(|ui| {
                for (label, target) in [
                    ("처음", 0),
                    ("이전", preview.sheet.saturating_sub(1)),
                    ("다음", (preview.sheet + 1).min(count - 1)),
                    ("끝", count - 1),
                ] {
                    if ui.button(label).clicked() {
                        go = Some(target);
                    }
                }
                ui.add_space(12.0);
                ui.label(if preview.layout.n_up > 1 {
                    format!("{} / {} 장", preview.sheet + 1, count)
                } else {
                    format!("{} / {} 쪽", preview.sheet + 1, count)
                });
                ui.add_space(12.0);
                if ui.button("PDF로 저장").clicked() {
                    print = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("쪽 고르기");
                egui::ComboBox::from_id_salt("preview-odd-even")
                    .width(96.0)
                    .selected_text(match preview.layout.odd_even.as_str() {
                        "odd" => "홀수 쪽만",
                        "even" => "짝수 쪽만",
                        _ => "모든 쪽",
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            ("all", "모든 쪽"),
                            ("odd", "홀수 쪽만"),
                            ("even", "짝수 쪽만"),
                        ] {
                            if ui
                                .selectable_label(preview.layout.odd_even == value, label)
                                .clicked()
                            {
                                preview.layout.odd_even = value.into();
                                preview.sheet = 0;
                            }
                        }
                    });
                ui.label("모아 찍기");
                egui::ComboBox::from_id_salt("preview-n-up")
                    .width(96.0)
                    .selected_text(format!("한 장에 {}쪽", preview.layout.n_up))
                    .show_ui(ui, |ui| {
                        for n in [1, 2, 4, 8] {
                            if ui
                                .selectable_label(
                                    preview.layout.n_up == n,
                                    format!("한 장에 {n}쪽"),
                                )
                                .clicked()
                            {
                                preview.layout.n_up = n;
                                preview.sheet = 0;
                            }
                        }
                    });
                ui.checkbox(&mut preview.layout.reverse, "역순");
                ui.label("워터마크");
                ui.add(
                    egui::TextEdit::singleline(&mut preview.watermark)
                        .desired_width(110.0)
                        .char_limit(40)
                        .hint_text("예: 대외비"),
                );
            });
            ui.add_space(6.0);
            let (area, _) = ui.allocate_exact_size(Vec2::new(600.0, 540.0), egui::Sense::hover());
            ui.painter().rect_filled(area, 0.0, Color32::from_gray(235));
            if let Some((_, _, _, handle, size)) = preview.texture.as_ref() {
                let fit = (area.width() / size.x).min(area.height() / size.y).min(1.0);
                let rect = Rect::from_center_size(area.center(), *size * fit);
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
                "인쇄될 모습(인쇄용 조판·모아 찍기·워터마크)으로 보입니다. 설정은 인쇄 창으로 이어집니다.",
            );
        });
        if let Some(sheet) = go {
            preview.sheet = sheet;
        }
        if print || button == Some(0) {
            self.ext.shell.dialog = Some(Dialog::Print {
                range: 0,
                pages: String::new(),
                copies: 1,
                to_file: print,
                layout: preview.layout.clone(),
                watermark: preview.watermark.clone(),
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
        line_numbers: &mut [bool; 2],
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
            ui.checkbox(&mut line_numbers[1], "줄 번호 표시")
                .on_hover_text(
                    "간격·시작 번호 같은 설정은 쪽 → 줄 번호 → 줄 번호 설정에서 바꿉니다.",
                );
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
            Some(0) => {
                let applied = self.run(
                    "page.section",
                    json!({
                        "pageNum": numbers[0], "pictureNum": numbers[1],
                        "tableNum": numbers[2], "equationNum": numbers[3],
                        "defaultTabSpacingMm": *tab_mm,
                        "hideHeader": hide[0], "hideFooter": hide[1], "hideMasterPage": hide[2],
                        "hideBorder": hide[3], "hideFill": hide[4], "hideEmptyLine": hide[5],
                        "wholeDocument": *whole_document,
                    }),
                );
                // 줄 번호 표시 goes only when it changed, so its settings are left alone.
                let numbers_applied = line_numbers[0] == line_numbers[1]
                    || self.run(
                        "page.line_numbers",
                        json!({"show": line_numbers[1], "all_sections": *whole_document}),
                    );
                !(applied && numbers_applied)
            }
            Some(_) => false,
            None => true,
        }
    }

    /// Whether the caret paragraph skips line numbering (줄 번호 건너뜀).
    pub(crate) fn line_number_skipped(&self) -> bool {
        let c = self.session.cursor;
        let doc = self.session.core.document();
        doc.sections
            .get(c.section)
            .and_then(|s| s.paragraphs.get(c.paragraph))
            .and_then(|p| doc.doc_info.para_shapes.get(p.para_shape_id as usize))
            .is_some_and(|shape| shape.suppress_line_numbers)
    }

    /// Whether the caret's section shows line numbers.
    pub(crate) fn line_numbers_shown(&self) -> bool {
        let section = self.session.cursor.section;
        self.session
            .core
            .document()
            .sections
            .get(section)
            .is_some_and(|s| s.section_def.show_line_number)
    }

    // ----- 줄 번호 설정 (한글 배치: 표시 · 시작 번호 · 번호 간격 · 본문과의 간격 · 번호 매기기) -----
    fn line_numbers_dialog(&mut self, ctx: &egui::Context, s: &mut LineNumberSettings) -> bool {
        let button = dialog(
            ctx,
            "줄 번호 설정",
            440.0,
            &["설정(D)", "취소"],
            |ui| {
                ui.checkbox(&mut s.show, "줄 번호 표시");
                group(ui, "번호");
                egui::Grid::new("line-number-grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("시작 번호");
                        ui.add(egui::DragValue::new(&mut s.start).range(1..=65535));
                        ui.end_row();
                        ui.label("번호 간격");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut s.count_by).range(1..=100));
                            ui.label("줄마다");
                        });
                        ui.end_row();
                        ui.label("본문과의 간격");
                        ui.add(
                            egui::DragValue::new(&mut s.distance_mm)
                                .range(0.0..=100.0)
                                .speed(0.1)
                                .max_decimals(1)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                    });
                group(ui, "번호 매기기");
                ui.radio_value(&mut s.restart, 1, "쪽마다 새 번호로 시작");
                ui.radio_value(&mut s.restart, 2, "구역마다 새 번호로 시작");
                ui.radio_value(&mut s.restart, 3, "이전 번호에 이어서");
                ui.horizontal(|ui| {
                    ui.label("적용 범위");
                    egui::ComboBox::from_id_salt("line-number-scope")
                        .width(150.0)
                        .selected_text(if s.all_sections {
                            "문서 전체"
                        } else {
                            "현재 구역"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut s.all_sections, false, "현재 구역");
                            ui.selectable_value(&mut s.all_sections, true, "문서 전체");
                        });
                });
                notice(
                    ui,
                    "본문 줄마다 번호를 붙입니다(표·글상자·각주 안의 줄은 세지 않음). 번호의 위치·글꼴은 한글 화면과 대조하지 못했습니다.",
                );
            },
        );
        match button {
            Some(0) => !self.run(
                "page.line_numbers",
                json!({
                    "show": s.show,
                    "start": s.start,
                    "count_by": s.count_by,
                    "distance_mm": s.distance_mm,
                    "restart": s.restart,
                    "all_sections": s.all_sections,
                }),
            ),
            Some(_) => false,
            None => true,
        }
    }

    // ----- 문서 이력 관리 (읽기만: 한글이 HWP에 기록한 버전) -----
    fn history_dialog(&mut self, ctx: &egui::Context, list: &serde_json::Value) -> bool {
        let entries = list["entries"].as_array().cloned().unwrap_or_default();
        let last = list["lastVersion"].as_bool() == Some(true);
        let buttons: &[&str] = if last {
            &["마지막 버전 저장(S)...", "닫기"]
        } else {
            &["닫기"]
        };
        let button = dialog(ctx, "문서 이력 관리", 560.0, buttons, |ui| {
            if entries.is_empty() {
                ui.label(if list["file"].is_null() {
                    "저장된 파일이 없어 이력이 없습니다."
                } else {
                    "이 문서 파일에는 한글이 기록한 이력이 없습니다."
                });
            } else {
                egui::Grid::new("history-grid")
                    .num_columns(4)
                    .striped(true)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        for head in ["버전", "날짜", "작성자", "설명"] {
                            ui.strong(head);
                        }
                        ui.end_row();
                        for e in &entries {
                            ui.label((e["index"].as_u64().unwrap_or(0) + 1).to_string());
                            ui.label(e["date"].as_str().unwrap_or("—"));
                            ui.label(e["writer"].as_str().unwrap_or(""));
                            ui.label(e["description"].as_str().unwrap_or(""));
                            ui.end_row();
                        }
                    });
            }
            notice(
                ui,
                "한글의 문서 이력 관리로 기록된 버전을 보여 주기만 합니다. 새 버전 기록, 이전 버전 보기·되돌리기는 하지 않습니다. 기록은 HWP로 저장해도 그대로 남습니다.",
            );
        });
        match (button, last) {
            (Some(0), true) => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("HML 문서", &["hml"])
                    .set_file_name("마지막 이력 버전.hml")
                    .save_file()
                    && self.run("review.history.save_last", json!({"path": path}))
                {
                    self.message = "마지막 이력 버전을 HML로 저장했습니다. HwpCraft는 이 HML(HWPML 2.5)을 열지 못합니다.".into();
                }
                true
            }
            (Some(_), _) => false,
            (None, _) => true,
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
    fn recent_paths_are_absolute_deduped_and_shortened_for_display() {
        // A relative path becomes absolute and matches the same file given absolutely.
        let mut list = Vec::new();
        push_recent(&mut list, std::path::Path::new("samples/a.hwpx"));
        assert!(list[0].is_absolute(), "{:?}", list[0]);
        let absolute = list[0].clone();
        push_recent(&mut list, &absolute);
        assert_eq!(list.len(), 1);
        assert!(parse_recent("samples/a.hwpx\n\nsamples/a.hwpx").len() == 1);
        // Long folders keep their start and end; names are cut at the end.
        let sep = std::path::MAIN_SEPARATOR;
        let deep = PathBuf::from(
            [
                "D:",
                "work",
                "2026",
                "planning",
                "quarterly",
                "drafts",
                "review",
                "regional-office-reports",
                "body.hwpx",
            ]
            .join(&sep.to_string()),
        );
        let folder = recent_folder(&deep, 40);
        assert!(folder.chars().count() <= 40, "{folder}");
        assert!(folder.starts_with(&format!("D:{sep}…{sep}")), "{folder}");
        assert!(folder.ends_with("regional-office-reports"), "{folder}");
        let short = PathBuf::from(["C:", "docs", "a.hwpx"].join(&sep.to_string()));
        assert_eq!(recent_folder(&short, 40), format!("C:{sep}docs"));
        assert_eq!(recent_name(&short, 40), "a.hwpx");
        let long = PathBuf::from(
            "02. 2026년도 상반기 사업 추진 실적 및 하반기 세부 운영 계획 보고서.hwpx",
        );
        let name = recent_name(&long, 24);
        assert_eq!(name.chars().count(), 24);
        assert!(name.ends_with('…'));
    }

    /// Press each key in its own frame through the app's shortcut handling.
    fn press(a: &mut HwpApp, keys: &[(egui::Modifiers, egui::Key)]) {
        for &(modifiers, key) in keys {
            let ctx = a.ctx.clone();
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| a.shortcuts(ui.ctx()));
            output.textures_delta.clear();
        }
    }
    const CTRL: egui::Modifiers = egui::Modifiers {
        alt: false,
        ctrl: true,
        shift: false,
        mac_cmd: false,
        command: true,
    };

    /// The command list names some commands differently from the ribbon entry that
    /// runs them (one Hangul dialog, or the screen-only twin of an engine command).
    const SAME_COMMAND: &[(&str, &str)] = &[
        ("page.footer", "page.header"),
        ("picture.insert", "insert.picture_dialog"),
        ("note.insert", "insert.footnote"),
        ("note.insert", "insert.endnote"),
        ("equation.insert", "insert.equation"),
        ("view.marks", "view.paragraph_marks"),
        ("view.marks", "view.control_codes"),
        ("hyperlink.next", "view.link_next"),
    ];

    #[test]
    fn every_two_key_shortcut_in_the_command_list_runs_its_command() -> anyhow::Result<()> {
        let listed: Vec<(&str, &str)> = hwpcraft_engine::COMMANDS
            .iter()
            .chain(hwpcraft_engine::extension_commands())
            .flat_map(|(id, _, keys)| keys.split(" / ").map(move |k| (*id, k.trim())))
            .filter(|(_, k)| k.starts_with("Ctrl+") && k.contains(','))
            .collect();
        assert!(listed.len() >= 15, "{listed:?}");
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        // Resolve only: Ctrl+N,I would open the system file picker.
        a.ext.shell.chords_record_only = true;
        for (id, keys) in &listed {
            let (prefix, second) = keys["Ctrl+".len()..].split_once(',').unwrap();
            let key = |name: &str| egui::Key::from_name(name).unwrap();
            a.ext.shell.last_chord = None;
            press(
                &mut a,
                &[(CTRL, key(prefix)), (egui::Modifiers::NONE, key(second))],
            );
            let ran = a.ext.shell.last_chord;
            assert!(
                ran.is_some_and(|r| r == *id || SAME_COMMAND.contains(&(*id, r))),
                "{keys} is listed for {id} but runs {ran:?}"
            );
        }
        // And no bound chord contradicts what the list shows for its command (Ctrl+Q,D
        // is a second key HwpCraft also takes for 문서 정보).
        for (prefix, key, run) in CHORDS
            .iter()
            .filter(|(p, k, _)| (*p, *k) != ('Q', egui::Key::D))
        {
            let chord = format!("Ctrl+{prefix},{}", key.name());
            let shown: Vec<_> = listed
                .iter()
                .filter(|(id, _)| id == run || SAME_COMMAND.contains(&(*id, *run)))
                .map(|(_, k)| *k)
                .collect();
            assert!(
                shown.is_empty() || shown.contains(&chord.as_str()),
                "{chord}: {shown:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn ctrl_k_chords_still_open_their_dialogs() -> anyhow::Result<()> {
        use egui::Key;
        let chord = |second| -> anyhow::Result<HwpApp> {
            let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
            press(&mut a, &[(CTRL, Key::K)]);
            assert!(a.message.starts_with("Ctrl+K,"), "{}", a.message);
            press(&mut a, &[(egui::Modifiers::NONE, second)]);
            Ok(a)
        };
        let a = chord(Key::H)?;
        assert_eq!(a.ext.shell.last_chord, Some("insert.hyperlink"));
        assert!(matches!(
            a.feature_dialog,
            Some(super::super::feature_dialogs::FeatureDialog::Link { .. })
        ));
        for (second, id) in [
            (Key::N, "format.numbering.dialog"),
            (Key::O, "format.outline.dialog"),
        ] {
            let a = chord(second)?;
            assert_eq!(a.ext.shell.last_chord, Some(id));
            assert!(a.format_ext_modal_open(), "{id} opens its dialog");
        }
        let a = chord(Key::E)?;
        assert_eq!(a.ext.shell.last_chord, Some("insert.field"));
        assert!(a.object_ext_modal_open(), "누름틀 넣기 opens its dialog");
        // Ctrl+K then an unbound key ends the chord without running anything.
        let a = chord(Key::X)?;
        assert_eq!(a.ext.shell.last_chord, None);
        Ok(())
    }

    #[test]
    fn unsupported_commands_setting_reads_only_an_explicit_on() {
        assert!(parse_view("show_unsupported_commands=1\n"));
        assert!(parse_view("other=0\r\n show_unsupported_commands = 1 \r\n"));
        for text in [
            "",
            "show_unsupported_commands=0",
            "show_unsupported_commands",
            "x=1",
        ] {
            assert!(!parse_view(text), "{text:?}");
        }
        // Off by default: the ribbon hides what nothing implements.
        assert!(!State::default().show_unsupported);
    }

    #[test]
    fn recent_list_moves_to_front_dedupes_and_keeps_nine() {
        // Absolute on every OS, so recent_path keeps them as they are.
        let docs = if cfg!(windows) { "C:/docs" } else { "/docs" };
        let mut list = Vec::new();
        for i in 0..12 {
            push_recent(&mut list, std::path::Path::new(&format!("{docs}/{i}.hwpx")));
        }
        assert_eq!(list.len(), RECENT_LIMIT);
        assert_eq!(list[0], PathBuf::from(format!("{docs}/11.hwpx")));
        push_recent(&mut list, std::path::Path::new(&format!("{docs}/5.hwpx")));
        assert_eq!(list[0], PathBuf::from(format!("{docs}/5.hwpx")));
        assert_eq!(list.len(), RECENT_LIMIT);
        assert_eq!(list.iter().filter(|p| p.ends_with("5.hwpx")).count(), 1);
        let text = serialize_recent(&list);
        assert_eq!(parse_recent(&text), list);
        assert_eq!(
            parse_recent(&format!("\n  \n{docs}/a.hwp\n")),
            vec![PathBuf::from(format!("{docs}/a.hwp"))]
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
        for tab in 1..=12usize {
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

    #[test]
    fn key_tips_open_ribbon_menus_and_the_contextual_tabs() -> anyhow::Result<()> {
        use egui::{Key, Modifiers};
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        // Alt, K (도구), M opens the MCP 연결 menu.
        a.ext.shell.keytips = KeyTips::Tabs;
        press(&mut a, &[(Modifiers::NONE, Key::K)]);
        assert_eq!((a.tab, a.ext.shell.keytips), (8, KeyTips::Items));
        press(&mut a, &[(Modifiers::NONE, Key::M)]);
        let menu = egui::Id::new(("hwp-menu", "MCP\n연결", 8usize));
        assert!(egui::Popup::is_id_open(&a.ctx, menu));
        assert_eq!(a.ext.shell.keytips, KeyTips::Off);
        // 그림 (P) is a tab tip only while a picture is selected.
        a.ext.shell.keytips = KeyTips::Tabs;
        press(&mut a, &[(Modifiers::NONE, Key::P)]);
        assert_eq!(a.tab, 8);
        a.ext.shell.object_kind = Some("picture");
        press(&mut a, &[(Modifiers::NONE, Key::P)]);
        assert_eq!((a.tab, a.ext.shell.keytips), (11, KeyTips::Items));
        // Its items have tips too: A is 그림 속성.
        assert_eq!(alt_letter(11, "object.properties"), Some('A'));
        Ok(())
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

/// A page SVG in a screen colour mode: the paper becomes `paper`, and every fill, stroke
/// and gradient colour is mapped with [`view_color`]. Pictures are left alone. Recolouring
/// before rasterizing keeps glyph edges clean (recolouring pixels afterwards turned the
/// anti-aliased edge of coloured text into a light fringe).
pub(crate) fn view_colors_svg(svg: &str, paper: Color32, text: Color32) -> String {
    let mut out = String::with_capacity(svg.len() + 128);
    let mut rest = svg;
    // Paper first, in root coordinates and large enough for any cropped view box.
    if let Some(open) = rest.find("<svg")
        && let Some(end) = rest[open..].find('>')
    {
        let end = open + end + 1;
        out.push_str(&rest[..end]);
        out.push_str(&format!(
            "\n<rect x=\"-100000\" y=\"-100000\" width=\"200000\" height=\"200000\" fill=\"#{:02x}{:02x}{:02x}\"/>",
            paper.r(),
            paper.g(),
            paper.b()
        ));
        rest = &rest[end..];
    }
    while let Some(at) = rest.find("=\"#") {
        let name_start = rest[..at]
            .rfind(|c: char| c.is_whitespace() || c == '<' || c == ';')
            .map_or(0, |i| i + 1);
        let name = &rest[name_start..at];
        let value = &rest[at + 3..];
        let hex = value
            .find('"')
            .map(|end| &value[..end])
            .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()));
        match hex {
            Some(hex) if matches!(name, "fill" | "stroke" | "stop-color" | "flood-color") => {
                let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
                let [r, g, b] = view_color([channel(0), channel(2), channel(4)], paper, text);
                out.push_str(&rest[..at + 3]);
                out.push_str(&format!("{r:02x}{g:02x}{b:02x}"));
                rest = &value[6..];
            }
            _ => {
                out.push_str(&rest[..at + 3]);
                rest = value;
            }
        }
    }
    out.push_str(rest);
    out
}

/// One document colour in a screen colour mode. Greys go from white→`paper` to
/// black→`text` as before; a colour keeps its hue and has its lightness turned over
/// within the paper–text range, so dark blue headings become light blue on dark paper.
pub(crate) fn view_color(rgb: [u8; 3], paper: Color32, text: Color32) -> [u8; 3] {
    let [r, g, b] = rgb.map(|v| f32::from(v) / 255.0);
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    let mix = |a: u8, b: u8, t: f32| f32::from(a) + (f32::from(b) - f32::from(a)) * t;
    let grey = [
        mix(text.r(), paper.r(), lum),
        mix(text.g(), paper.g(), lum),
        mix(text.b(), paper.b(), lum),
    ];
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = max - min;
    if chroma < 0.02 {
        return grey.map(|v| v.round().clamp(0.0, 255.0) as u8);
    }
    let lightness = (max + min) / 2.0;
    let saturation = chroma / (1.0 - (2.0 * lightness - 1.0).abs()).max(1e-3);
    let hue = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    let light = |c: Color32| {
        let [r, g, b] = [c.r(), c.g(), c.b()].map(|v| f32::from(v) / 255.0);
        (r.max(g).max(b) + r.min(g).min(b)) / 2.0
    };
    let (paper_l, text_l) = (light(paper), light(text));
    let l = paper_l + (text_l - paper_l) * (1.0 - lightness);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * saturation.min(1.0);
    let x = c * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match hue as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let colour = [r1 + m, g1 + m, b1 + m].map(|v| v * 255.0);
    // Nearly grey colours stay close to the grey mapping (and its tint).
    let weight = (chroma * 2.0).min(1.0);
    [0, 1, 2].map(|i| {
        (grey[i] + (colour[i] - grey[i]) * weight)
            .round()
            .clamp(0.0, 255.0) as u8
    })
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
        self.shell_page_band_image(index, scale, None)
    }
    /// Page bitmap, or only the pixel rows `band` (start, end) of it: the SVG view box is
    /// cropped to those rows, so the renderer draws just that strip at the same scale.
    fn shell_page_band_image(
        &self,
        index: u32,
        scale: f32,
        band: Option<(usize, usize)>,
    ) -> anyhow::Result<egui::ColorImage> {
        let t0 = std::time::Instant::now();
        let mut svg = self.session.render_page_svg(index)?;
        let t1 = std::time::Instant::now();
        if !self.ext.shell.show_pictures {
            svg = picture_placeholders(&svg);
        }
        if self.ext.shell.hanja_readings {
            svg = hanja_view::hanja_readings_svg(&svg);
        }
        if let Some((paper, text)) = self.page_style().1 {
            svg = view_colors_svg(&svg, paper, text);
        }
        if let Some((y0, y1)) = band {
            svg = crop_rows(&svg, y0 as f32 / scale, (y1 - y0) as f32 / scale)
                .ok_or_else(|| anyhow::anyhow!("쪽 그림의 크기를 읽지 못했습니다."))?;
        }
        // Screen raster: large embedded pictures come from pixel-identical stand-ins.
        let page = self.renderer.screen(&svg, scale)?;
        let [a, b] = self.ext.shell.raster_ms.get();
        self.ext.shell.raster_ms.set([
            a + (t1 - t0).as_secs_f64() * 1000.0,
            b + t1.elapsed().as_secs_f64() * 1000.0,
        ]);
        Ok(egui::ColorImage::from_rgba_premultiplied(
            [page.width as usize, page.height as usize],
            &page.rgba,
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

/// Raster steps: the smallest one at or above the screen scale, so pages are never
/// drawn blurrier than the display, and a small zoom change does not redraw. The
/// renderer accepts 0.25–2.0; above 2.0 the texture is enlarged.
const RASTER_STEPS: [f32; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

/// Raster scale for a zoom (1.0 = 100 %) on a display with `pixels_per_point`.
pub(crate) fn raster_scale(zoom: f32, pixels_per_point: f32) -> f32 {
    let target = zoom * pixels_per_point;
    RASTER_STEPS
        .iter()
        .copied()
        .find(|step| *step + 0.005 >= target)
        .unwrap_or(2.0)
}

/// Lower `scale` so the page's longer side stays within the GPU texture limit
/// (egui refuses larger textures). `page` is the page size in points, if known.
pub(crate) fn fit_texture(scale: f32, page: Option<Vec2>, max_side: usize) -> f32 {
    let Some(page) = page else {
        return scale;
    };
    let longest = page.x.max(page.y);
    if longest <= 0.0 || max_side == 0 {
        return scale;
    }
    // One pixel of slack for the renderer's rounding up to whole pixels.
    let limit = (max_side as f32 - 1.0) / longest;
    scale.min(limit).max(0.25)
}

/// Where the caret paragraph sits on its page and what would move the rest of the
/// page if it changed. A band redraw is safe only when all of this is unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaretExtent {
    generation: u64,
    page: u32,
    section: usize,
    paragraph: usize,
    top: f32,
    bottom: f32,
    lines: usize,
    controls: usize,
    paragraphs: usize,
    pages: u32,
}
impl CaretExtent {
    fn same_layout(&self, other: &Self) -> bool {
        self.page == other.page
            && (self.section, self.paragraph) == (other.section, other.paragraph)
            && (self.top - other.top).abs() < 0.01
            && (self.bottom - other.bottom).abs() < 0.01
            && (self.lines, self.controls, self.paragraphs, self.pages)
                == (other.lines, other.controls, other.paragraphs, other.pages)
    }
}

/// `svg` with its view box narrowed to `height` units starting at `top`, keeping the
/// width, so the renderer draws one horizontal strip of the page.
pub(crate) fn crop_rows(svg: &str, top: f32, height: f32) -> Option<String> {
    let end = svg.find('>')?;
    let root = &svg[..end];
    let attr = |name: &str| -> Option<(usize, usize)> {
        let key = format!(" {name}=\"");
        let start = root.find(&key)? + key.len();
        Some((start, root[start..].find('"')? + start))
    };
    let (vb0, vb1) = attr("viewBox")?;
    let parts: Vec<f32> = root[vb0..vb1]
        .split([' ', ','])
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [x, y, w, _] = parts[..] else {
        return None;
    };
    let (h0, h1) = attr("height")?;
    let mut out = String::with_capacity(svg.len() + 32);
    // Replace the later attribute first so the earlier offsets stay valid.
    let (first, second) = if vb0 < h0 {
        (
            (vb0, vb1, format!("{x} {} {w} {height}", y + top)),
            (h0, h1, format!("{height}")),
        )
    } else {
        (
            (h0, h1, format!("{height}")),
            (vb0, vb1, format!("{x} {} {w} {height}", y + top)),
        )
    };
    out.push_str(&root[..first.0]);
    out.push_str(&first.2);
    out.push_str(&root[first.1..second.0]);
    out.push_str(&second.2);
    out.push_str(&root[second.1..]);
    cull_text_rows(&svg[end..], y + top, y + top + height, &mut out);
    Some(out)
}

/// Copy `body` into `out` without the plain `<text>` elements that cannot reach rows
/// `top..bottom`: a band of a dense page then parses its own few dozen glyph runs rather
/// than the page's thousand or so, and draws the same pixels. Only a `<text x y>` holding
/// bare characters, under nothing but untransformed `<g>`s, is judged; from 2 em above
/// its baseline to 1 em below it must miss the rows. A clipped group is drawn on a layer
/// as large as its contents, so the texts on each group's first and last baseline and at
/// its left and right ends stay too: the layer, and so every pixel, stays the same.
/// Everything else is kept.
fn cull_text_rows(body: &str, top: f32, bottom: f32, out: &mut String) {
    let attr = |tag: &str, name: &str| -> Option<f32> {
        let key = format!(" {name}=\"");
        let start = tag.find(&key)? + key.len();
        tag[start..].split('"').next()?.parse().ok()
    };
    // Judged texts: (start, end, group, x, y, outside the rows).
    let mut texts: Vec<(usize, usize, usize, f32, f32, bool)> = Vec::new();
    // Open elements below the root: (start, true where a text inside may not be judged).
    let mut open: Vec<(usize, bool)> = Vec::new();
    let mut at = 0;
    while let Some(found) = body[at..].find('<') {
        let start = at + found;
        let Some(close) = body[start..].find('>').map(|c| start + c) else {
            break;
        };
        let tag = &body[start..=close];
        at = close + 1;
        if tag.starts_with("<!--") {
            at = body[start..]
                .find("-->")
                .map_or(body.len(), |e| start + e + 3);
            continue;
        }
        if tag.starts_with("<?") || tag.starts_with("<!") {
            continue;
        }
        if tag.starts_with("</") {
            open.pop();
        } else if tag.starts_with("<text ") && !tag.ends_with("/>") {
            let element = body[start..]
                .find("</text>")
                .map(|e| start + e + "</text>".len());
            let judged = !open.iter().any(|(_, blocked)| *blocked) && !tag.contains(" transform=");
            if let (true, Some(element)) = (judged, element) {
                let content = &body[close + 1..element - "</text>".len()];
                let place = (attr(tag, "x"), attr(tag, "y"), attr(tag, "font-size"));
                if let (false, (Some(x), Some(y), Some(size))) = (content.contains('<'), place) {
                    let group = open.last().map_or(usize::MAX, |(s, _)| *s);
                    let outside = y + size < top || y - 2.0 * size > bottom;
                    texts.push((start, element, group, x, y, outside));
                    at = element;
                    continue;
                }
            }
            open.push((start, true));
        } else if !tag.ends_with("/>") {
            let plain_group = tag.starts_with("<g") && !tag.contains(" transform=");
            open.push((start, !plain_group));
        }
    }
    // Each group's extremes.
    let mut ends: std::collections::HashMap<usize, [f32; 4]> = std::collections::HashMap::new();
    for &(_, _, group, x, y, _) in &texts {
        let e = ends.entry(group).or_insert([x, x, y, y]);
        *e = [e[0].min(x), e[1].max(x), e[2].min(y), e[3].max(y)];
    }
    let mut last = 0;
    for &(start, end, group, x, y, outside) in &texts {
        let [x0, x1, y0, y1] = ends[&group];
        if !outside || x == x0 || x == x1 || y == y0 || y == y1 {
            continue;
        }
        out.push_str(&body[last..start]);
        last = end;
        if body[last..].starts_with('\n') {
            last += 1;
        }
    }
    out.push_str(&body[last..]);
}

fn scale_key(scale: f32) -> u16 {
    (scale * 100.0).round() as u16
}

impl HwpApp {
    /// Texture for page `index`, used by the canvas instead of clearing every page on
    /// each edit. A page drawn for an older document generation stays on screen and at
    /// most one such page is redrawn per pass, the caret's page first; the rest follow
    /// on the next passes (a repaint is requested). A page that has no texture yet, for
    /// example one scrolled into view, and a page drawn at another zoom step go to the
    /// raster workers; meanwhile the first shows blank paper and the second its old
    /// bitmap.
    pub(crate) fn shell_page_texture(
        &mut self,
        ctx: &egui::Context,
        index: u32,
    ) -> anyhow::Result<egui::TextureHandle> {
        let generation = self.session.generation;
        let pass = ctx.cumulative_pass_nr();
        if self.ext.shell.requested.0 != pass {
            self.receive_rasters(ctx);
        }
        let requested = &mut self.ext.shell.requested;
        if requested.0 != pass {
            requested.2 = std::mem::take(&mut requested.1);
            requested.0 = pass;
        }
        requested.1.insert(index);
        let scale = fit_texture(
            raster_scale(self.zoom, ctx.pixels_per_point()),
            self.page_sizes.1.get(index as usize).copied(),
            ctx.input(|i| i.max_texture_side),
        );
        let drawn = self.ext.shell.page_drawn.get(&index).copied();
        let cached = self.pages.get(&index).map(|(texture, _)| texture.clone());
        let stale = match (&cached, drawn) {
            (Some(texture), Some(_)) if self.page_current(index, generation, scale) => {
                let texture = texture.clone();
                let c = self.session.cursor;
                if self.ext.shell.caret_extent.is_none_or(|e| {
                    e.generation != generation
                        || (e.section, e.paragraph) != (c.section, c.paragraph)
                }) {
                    self.note_caret_extent(index, generation);
                }
                return Ok(texture);
            }
            (Some(texture), Some(drawn)) if drawn == generation && self.raster_async() => {
                // Only the zoom step changed: keep showing the old bitmap meanwhile.
                let texture = texture.clone();
                self.queue_raster(ctx, index, scale);
                return Ok(texture);
            }
            (Some(texture), Some(_)) => {
                if !self.may_redraw_page(pass, index, generation, scale) {
                    ctx.request_repaint();
                    return Ok(texture.clone());
                }
                true
            }
            _ if self.raster_async() => {
                self.queue_raster(ctx, index, scale);
                return Ok(self.blank_page(ctx));
            }
            _ => false,
        };
        if stale
            && self.ext.shell.page_scale.get(&index) == Some(&scale_key(scale))
            && let Some(texture) = cached.clone()
            && self.band_redraw(index, generation, scale, texture.clone())
        {
            self.ext.shell.redraw_pass = Some(pass);
            return Ok(texture);
        }
        // The shell-page area applies the document view colour and picture
        // placeholder options on top of the renderer's bitmap.
        let image = self.shell_page_image(index, scale)?;
        self.ext.shell.rasters += 1;
        let texture = self.install_page(ctx, index, generation, scale, image);
        if stale {
            self.ext.shell.redraw_pass = Some(pass);
            if self
                .pages
                .keys()
                .any(|page| !self.page_current(*page, generation, scale))
            {
                ctx.request_repaint();
            }
        }
        Ok(texture)
    }

    /// Keep `image` as page `index`'s texture, evicting the page farthest away when
    /// eight are kept already.
    fn install_page(
        &mut self,
        ctx: &egui::Context,
        index: u32,
        generation: u64,
        scale: f32,
        image: egui::ColorImage,
    ) -> egui::TextureHandle {
        if cfg!(any(test, debug_assertions)) {
            self.ext.shell.page_images.insert(index, image.clone());
        }
        let size = Vec2::new(image.size[0] as f32 / scale, image.size[1] as f32 / scale);
        self.ext.shell.uploaded += image.size[0] * image.size[1];
        let texture = ctx.load_texture(
            format!("page-{index}-{generation}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        // Up to 16 page bitmaps (about 6 MB each at 1.25x, so about 100 MB), enough
        // for two panes of a split view. The page farthest away goes first, never one
        // drawn in this pass or the last (the other pane's pages).
        let (_, now, before) = &self.ext.shell.requested;
        if !self.pages.contains_key(&index)
            && self.pages.len() >= 16
            && let Some(far) = self
                .pages
                .keys()
                .copied()
                .filter(|k| !now.contains(k) && !before.contains(k))
                .max_by_key(|k| k.abs_diff(index))
        {
            self.pages.remove(&far);
            self.ext.shell.page_drawn.remove(&far);
            self.ext.shell.page_scale.remove(&far);
            self.ext.shell.page_images.remove(&far);
        }
        self.pages.insert(index, (texture.clone(), size));
        self.ext.shell.page_drawn.insert(index, generation);
        self.ext.shell.page_scale.insert(index, scale_key(scale));
        self.note_caret_extent(index, generation);
        texture
    }

    /// Measure the caret paragraph if it lies on `page` (body text only).
    fn caret_extent(&self, page: u32) -> Option<CaretExtent> {
        let c = self.session.cursor;
        if c.cell.is_some() {
            return None;
        }
        let start = Cursor { offset: 0, ..c };
        let p = self.session.paragraph(start).ok()?;
        let len = p.text.chars().count();
        let (lines, controls) = (p.line_segs.len(), p.controls.len());
        let a = self.session.caret_at(start).ok()?;
        let b = self.session.caret_at(Cursor { offset: len, ..c }).ok()?;
        let page_of =
            |v: &serde_json::Value| v["pageIndex"].as_u64().and_then(|p| u32::try_from(p).ok());
        let (pa, pb) = (page_of(&a)?, page_of(&b)?);
        if pa != page || pb != page {
            return None;
        }
        let num = |v: &serde_json::Value, k: &str| v[k].as_f64().map(|x| x as f32);
        Some(CaretExtent {
            generation: self.session.generation,
            page,
            section: c.section,
            paragraph: c.paragraph,
            top: num(&a, "y")?,
            bottom: num(&b, "y")? + num(&b, "height")?,
            lines,
            controls,
            paragraphs: self
                .session
                .core
                .document()
                .sections
                .get(c.section)?
                .paragraphs
                .len(),
            pages: self.session.page_count().ok()?,
        })
    }
    fn note_caret_extent(&mut self, page: u32, generation: u64) {
        if let Some(extent) = self.caret_extent(page)
            && extent.generation == generation
        {
            self.ext.shell.caret_extent = Some(extent);
        }
    }
    /// Redraw only the caret paragraph's rows of `page` when the edit kept its lines,
    /// its place and everything that could move the rest of the page. True when done.
    fn band_redraw(
        &mut self,
        page: u32,
        generation: u64,
        scale: f32,
        mut texture: egui::TextureHandle,
    ) -> bool {
        let Some(before) = self.ext.shell.caret_extent else {
            return false;
        };
        if Some(&before.generation) != self.ext.shell.page_drawn.get(&page) || before.page != page {
            return false;
        }
        let Some(now) = self.caret_extent(page) else {
            return false;
        };
        if !before.same_layout(&now) {
            return false;
        }
        let [width, height] = texture.size();
        let y0 = ((now.top * scale).floor() as usize).saturating_sub(2);
        let y1 = (((now.bottom * scale).ceil() as usize) + 2).min(height);
        if y1 <= y0 || (y1 - y0) * 2 > height {
            return false;
        }
        let Ok(band) = self.shell_page_band_image(page, scale, Some((y0, y1))) else {
            return false;
        };
        if band.size[0] != width || y0 + band.size[1] > height {
            return false;
        }
        if cfg!(any(test, debug_assertions)) {
            // Patch the stored copy and require it to equal a full render everywhere.
            let Some(mut patched) = self.ext.shell.page_images.get(&page).cloned() else {
                return false;
            };
            for row in 0..band.size[1] {
                let at = (y0 + row) * width;
                patched.pixels[at..at + width]
                    .copy_from_slice(&band.pixels[row * width..(row + 1) * width]);
            }
            let Ok(full) = self.shell_page_image(page, scale) else {
                return false;
            };
            // Byte comparison first (fast even in debug builds); a per-channel
            // tolerance of 2 only when the bytes differ (anti-aliasing at the edges).
            let close = full.size == patched.size
                && (full.as_raw() == patched.as_raw()
                    || full
                        .as_raw()
                        .iter()
                        .zip(patched.as_raw())
                        .all(|(x, y)| x.abs_diff(*y) <= 2));
            if !close {
                eprintln!("page band redraw differed from a full render; drawing the whole page");
                self.ext.shell.band_rejected += 1;
                return false;
            }
            self.ext.shell.page_images.insert(page, patched);
        }
        texture.set_partial([0, y0], band, egui::TextureOptions::LINEAR);
        self.ext.shell.rasters += 1;
        self.ext.shell.page_drawn.insert(page, generation);
        self.ext.shell.caret_extent = Some(now);
        self.ext.shell.band_redraws += 1;
        true
    }

    /// Drawn for this document generation at this raster scale.
    fn page_current(&self, page: u32, generation: u64, scale: f32) -> bool {
        self.ext.shell.page_drawn.get(&page) == Some(&generation)
            && self.ext.shell.page_scale.get(&page) == Some(&scale_key(scale))
    }

    /// One stale page per pass; while the caret's page is visible and stale, it goes first.
    fn may_redraw_page(&mut self, pass: u64, index: u32, generation: u64, scale: f32) -> bool {
        if self.ext.shell.redraw_pass == Some(pass) {
            return false;
        }
        let Some(caret) = self.redraw_caret_page(generation) else {
            return true;
        };
        if caret == index {
            return true;
        }
        let caret_stale =
            self.pages.contains_key(&caret) && !self.page_current(caret, generation, scale);
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
    fn crop_rows_narrows_only_the_view_box_and_height() {
        let svg = r#"<svg xmlns="x" width="100" height="200" viewBox="0 0 100 200"><rect/></svg>"#;
        let band = crop_rows(svg, 50.0, 20.0).unwrap();
        assert!(band.contains(r#"width="100""#), "{band}");
        assert!(band.contains(r#"height="20""#), "{band}");
        assert!(band.contains(r#"viewBox="0 50 100 20""#), "{band}");
        assert!(band.ends_with("<rect/></svg>"));
        assert!(crop_rows("<svg>", 0.0, 1.0).is_none());
    }

    #[test]
    fn a_band_drops_only_text_that_cannot_reach_it() {
        let svg = concat!(
            r#"<svg width="100" height="400" viewBox="0 0 100 400"><g clip-path="url(#c)">"#,
            r#"<text x="1" y="10" font-size="10">첫</text>"#,
            r#"<text x="50" y="20" font-size="10">위</text>"#,
            "\n",
            r#"<text x="50" y="105" font-size="10">안</text>"#,
            r#"<text x="50" y="300" font-size="10">아래</text>"#,
            r#"<text x="90" y="200" font-size="10">오른</text>"#,
            r#"<text x="50" y="125" font-size="10">걸침</text>"#,
            r#"<text x="50" y="390" font-size="10">끝</text>"#,
            r#"</g><g transform="rotate(90)"><text x="1" y="20" font-size="10">돌림</text></g>"#,
            r#"<defs><text x="1" y="20" font-size="10">정의</text></defs>"#,
            r#"<text x="1" y="20" font-size="10"><tspan>속</tspan></text><!-- <text> --></svg>"#,
        );
        let band = crop_rows(svg, 100.0, 10.0).unwrap();
        // Far above and below go; the band's own, one reaching in from below, the group's
        // first and last lines and its left and right ends (they size the clipped layer),
        // and any under a transform, inside defs or with markup stay, as do comments.
        assert!(!band.contains(">위<") && !band.contains(">아래<"), "{band}");
        for kept in [
            ">안<",
            ">걸침<",
            ">첫<",
            ">끝<",
            ">오른<",
            ">돌림<",
            ">정의<",
            "<tspan>속</tspan>",
            "<!-- <text> -->",
        ] {
            assert!(band.contains(kept), "{kept}: {band}");
        }
        assert!(band.ends_with("</svg>"));
    }

    #[test]
    fn a_culled_band_of_a_dense_page_draws_the_same_pixels() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let line = "본 절에서는 사업 추진 현황과 성과를 정리하고 하반기 계획을 제시한다. ";
        for i in 0..40 {
            a.session
                .execute(Command::Insert(format!("{i}. {}\n", line.repeat(3))))?;
        }
        let svg = a.session.render_page_svg(0)?;
        let renderer = hwpcraft_render::Renderer::default();
        for (top, height) in [(140.0, 40.0), (400.0, 24.0), (700.0, 60.0)] {
            let culled = crop_rows(&svg, top, height).unwrap();
            assert!(culled.matches("<text").count() * 4 < svg.matches("<text").count());
            // The same band with every text kept, as before the culling.
            let whole = format!(
                "{}{}",
                &culled[..culled.find('>').unwrap()],
                &svg[svg.find('>').unwrap()..]
            );
            let (a, b) = (
                renderer.screen(&culled, 1.25)?,
                renderer.screen(&whole, 1.25)?,
            );
            assert_eq!((a.width, a.height), (b.width, b.height));
            assert!(a.rgba == b.rgba, "band at {top} differs");
        }
        Ok(())
    }

    fn typing_app() -> anyhow::Result<HwpApp> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        for i in 0..6 {
            a.session.execute(Command::Insert(format!(
                "{i}번째 문단입니다. 한 줄 안에 들어가는 짧은 글입니다."
            )))?;
            a.session.execute(Command::Split)?;
        }
        a.session.select(
            Cursor {
                paragraph: 2,
                offset: 3,
                ..Default::default()
            },
            false,
        )?;
        a.zoom = 1.0;
        pass(&mut a, &[0]);
        pass(&mut a, &[0]);
        Ok(a)
    }

    /// Typing that keeps the paragraph's lines redraws only its rows, and (debug check)
    /// the patched page equals a full render.
    #[test]
    fn typing_within_a_line_redraws_only_the_caret_band() -> anyhow::Result<()> {
        let mut a = typing_app()?;
        for _ in 0..3 {
            a.session.execute(Command::Insert("가".into()))?;
            pass(&mut a, &[0]);
        }
        assert_eq!(a.ext.shell.band_rejected, 0, "band equals a full render");
        assert_eq!(a.ext.shell.band_redraws, 3);
        assert_eq!(a.ext.shell.page_drawn.get(&0), Some(&a.session.generation));
        Ok(())
    }

    /// A change that adds a line or a paragraph draws the whole page.
    #[test]
    fn new_lines_and_paragraphs_redraw_the_whole_page() -> anyhow::Result<()> {
        let mut a = typing_app()?;
        a.session.execute(Command::Insert(
            "아주 긴 글을 넣어 이 문단이 다음 줄로 넘어가게 만듭니다. ".repeat(4),
        ))?;
        pass(&mut a, &[0]);
        assert_eq!(
            a.ext.shell.band_redraws, 0,
            "a new line moves the rest of the page"
        );
        a.session.execute(Command::Split)?;
        pass(&mut a, &[0]);
        assert_eq!(
            a.ext.shell.band_redraws, 0,
            "a new paragraph moves the rest of the page"
        );
        assert_eq!(a.ext.shell.page_drawn.get(&0), Some(&a.session.generation));
        Ok(())
    }

    #[test]
    fn raster_scale_follows_zoom_and_display_density() {
        assert_eq!(raster_scale(0.9, 1.0), 1.0);
        assert_eq!(raster_scale(1.0, 1.0), 1.0);
        assert_eq!(raster_scale(1.25, 1.0), 1.25);
        assert_eq!(raster_scale(1.3, 1.0), 1.5);
        assert_eq!(raster_scale(0.9, 1.5), 1.5);
        assert_eq!(raster_scale(0.2, 1.0), 0.5);
        assert_eq!(raster_scale(5.0, 1.0), 2.0);
    }

    #[test]
    fn page_textures_stay_within_the_gpu_limit() -> anyhow::Result<()> {
        let a4 = Some(Vec2::new(794.0, 1123.0));
        assert_eq!(fit_texture(1.0, a4, 2048), 1.0);
        let capped = fit_texture(2.0, a4, 2048);
        assert!(capped < 2.0 && 1123.0 * capped <= 2047.0, "{capped}");
        assert_eq!(fit_texture(2.0, None, 2048), 2.0);
        // At the largest zoom the drawn page still fits (egui would panic otherwise).
        let mut a = app_with_pages(1)?;
        a.zoom = 5.0;
        a.page_sizes();
        pass(&mut a, &[0]);
        let size = a.pages[&0].0.size();
        assert!(size[0].max(size[1]) <= 2048, "{size:?}");
        Ok(())
    }

    #[test]
    fn a_zoom_step_change_redraws_through_the_queue_at_the_new_scale() -> anyhow::Result<()> {
        let mut a = app_with_pages(3)?;
        a.zoom = 0.9;
        pass(&mut a, &[0, 1, 2]);
        let width = |a: &HwpApp, p: u32| a.pages[&p].0.size()[0];
        let at_one = width(&a, 0);
        // Within the same step: nothing is redrawn.
        a.zoom = 0.95;
        pass(&mut a, &[0, 1, 2]);
        assert_eq!(width(&a, 0), at_one);
        // Next step: the pages become stale and the queue redraws them one per pass.
        a.zoom = 1.4;
        pass(&mut a, &[0, 1, 2]);
        let redrawn = (0..3).filter(|p| width(&a, *p) > at_one).count();
        assert_eq!(redrawn, 1, "one page per pass");
        pass(&mut a, &[0, 1, 2]);
        pass(&mut a, &[0, 1, 2]);
        assert!((0..3).all(|p| width(&a, p) > at_one), "all redrawn at 1.5x");
        assert!((width(&a, 0) as f32 / at_one as f32 - 1.5).abs() < 0.02);
        Ok(())
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

/// Pages of a PDF prepared a slice per frame. Kept as a request rather than a page
/// list, so a restart after an edit asks the edited document again.
pub(crate) enum PdfPages {
    /// The whole document (파일 → PDF로 저장).
    All,
    /// 쪽 범위 저장: 1-based, inclusive.
    Range(u32, u32),
    /// A Hangul 인쇄 범위 and the caret page it was asked from.
    Print(String, u32),
    /// 블록 저장: the selected block as its own session, a copy that edits never change.
    Block(Box<Session>),
}

impl PdfPages {
    fn session<'a>(&'a self, document: &'a Session) -> &'a Session {
        match self {
            PdfPages::Block(block) => block,
            _ => document,
        }
    }
    fn pages(&self, document: &Session) -> anyhow::Result<Vec<u32>> {
        match self {
            PdfPages::All => Ok((0..document.page_count()?).collect()),
            PdfPages::Range(from, to) => {
                let count = document.page_count()?;
                if *from == 0 || to < from || *to > count {
                    anyhow::bail!("쪽 범위는 1–{count} 안에서 작은 번호부터 적으세요: {from}–{to}");
                }
                Ok(((from - 1)..*to).collect())
            }
            PdfPages::Print(range, current) => document.print_pdf_pages(range, *current),
            PdfPages::Block(block) => Ok((0..block.page_count()?).collect()),
        }
    }
}

/// A PDF whose page SVGs are built a slice per frame, then converted in the shared
/// background slot. Building all 381 pages of a long document at once held the window
/// for about 1.8 s. The pages come from one document generation: an edit in between
/// starts the preparation over (two versions never mix in one file), with twice the
/// time per frame each time, so continued typing cannot keep it from finishing.
pub(crate) struct PdfPreparing {
    pages: PdfPages,
    path: PathBuf,
    prep: hwpcraft_engine::PdfPrep,
    budget: std::time::Duration,
    restarts: u32,
    /// Copies to print once the file is written (인쇄 to paper).
    print: Option<u32>,
    /// 인쇄 상세 applied when the pages are finished (인쇄 only).
    layout: Option<hwpcraft_engine::PrintLayout>,
    /// The progress line last shown; refreshed only while nothing replaced it.
    progress: String,
}

/// Preparation time per frame before the first restart.
const PDF_PREPARE_BUDGET: std::time::Duration = std::time::Duration::from_millis(25);
const PDF_BUSY: &str = "이전 PDF 저장이 아직 진행 중입니다. 끝난 뒤 다시 저장해 주세요.";

impl PdfPreparing {
    fn line(&self) -> String {
        let (ready, total) = (self.prep.ready(), self.prep.total());
        if self.restarts == 0 {
            format!("PDF 준비 중… {ready}/{total}쪽 · 준비 중에 편집하면 처음부터 다시 준비합니다")
        } else {
            format!(
                "PDF 준비 중… {ready}/{total}쪽 · 문서가 바뀌어 처음부터 다시 준비하는 중 ({}번째)",
                self.restarts + 1
            )
        }
    }
}

impl HwpApp {
    /// A PDF is being prepared or converted; a new one is refused until it is done.
    pub(crate) fn pdf_busy(&self) -> bool {
        self.pdf.busy() || self.ext.shell.pdf_preparing.is_some()
    }

    /// Start a PDF of `pages`: the pages are prepared a slice per frame (progress in
    /// the status bar), then converted in the background slot shared with the file
    /// menu. `print` sends the written file to the printer. True when started.
    pub(crate) fn begin_pdf_prepare(
        &mut self,
        pages: PdfPages,
        path: PathBuf,
        print: Option<u32>,
    ) -> bool {
        if self.pdf_busy() {
            self.error = Some(PDF_BUSY.into());
            return false;
        }
        let prep = pages
            .pages(&self.session)
            .and_then(|list| pages.session(&self.session).pdf_prepare(&list));
        let Some(prep) = self.result(prep) else {
            return false;
        };
        self.ext.shell.pdf_preparing = Some(PdfPreparing {
            pages,
            path,
            prep,
            budget: self.ext.shell.pdf_budget,
            restarts: 0,
            print,
            layout: self.ext.shell.next_print_layout.take(),
            progress: String::new(),
        });
        // The first slice at once: a short document goes straight to conversion.
        self.step_pdf_prepare()
    }

    /// Every frame while a PDF is being prepared: one slice, and another frame soon.
    pub(crate) fn poll_pdf_prepare(&mut self, ctx: &egui::Context) {
        if self.ext.shell.pdf_preparing.is_some() {
            self.step_pdf_prepare();
            if self.ext.shell.pdf_preparing.is_some() {
                ctx.request_repaint();
            }
        }
    }

    /// Prepare one slice; hand the finished pages to the background slot. False when the
    /// export failed (the error is shown and nothing is written).
    fn step_pdf_prepare(&mut self) -> bool {
        let Some(mut job) = self.ext.shell.pdf_preparing.take() else {
            return true;
        };
        let stale = job.prep.generation() != job.pages.session(&self.session).generation;
        if stale {
            let restarted = job
                .pages
                .pages(&self.session)
                .and_then(|list| job.pages.session(&self.session).pdf_prepare(&list));
            let Some(prep) = self.result(restarted) else {
                return false;
            };
            job.prep = prep;
            job.restarts += 1;
            job.budget = job.budget.saturating_mul(2);
        }
        let step = job
            .pages
            .session(&self.session)
            .pdf_prepare_step(&mut job.prep, job.budget);
        match step {
            Ok(false) => {
                let line = job.line();
                if stale || job.progress.is_empty() || self.message == job.progress {
                    self.message = line.clone();
                }
                job.progress = line;
                self.ext.shell.pdf_preparing = Some(job);
                true
            }
            Ok(true) => {
                let session = job.pages.session(&self.session);
                let finished = match job.layout.as_ref() {
                    Some(layout) if *layout != hwpcraft_engine::PrintLayout::default() => job
                        .pages
                        .pages(&self.session)
                        .and_then(|pages| session.pdf_finish_with(job.prep, &pages, layout)),
                    _ => session.pdf_finish(job.prep),
                };
                let Some(pdf) = self.result(finished) else {
                    return false;
                };
                let count = pdf.page_count();
                if !self.begin_pdf_export(count, job.path.clone(), move || pdf.convert()) {
                    return false;
                }
                if let Some(copies) = job.print {
                    self.ext.shell.print_after = Some((job.path, copies, count));
                }
                true
            }
            Err(e) => {
                self.result::<()>(Err(e.context("PDF로 저장하지 못했습니다")));
                false
            }
        }
    }

    /// 쪽 범위 저장 as PDF (1-based, inclusive), in the background. True when started.
    pub(crate) fn start_pages_pdf(&mut self, path: PathBuf, from: u32, to: u32) -> bool {
        self.begin_pdf_prepare(PdfPages::Range(from, to), path, None)
    }
    /// 블록 저장 as PDF, in the background. True when started.
    pub(crate) fn start_block_pdf(&mut self, path: PathBuf) -> bool {
        if self.pdf_busy() {
            self.error = Some(PDF_BUSY.into());
            return false;
        }
        let block = self.session.block_pdf_session();
        match self.result(block) {
            Some(block) => self.begin_pdf_prepare(PdfPages::Block(Box::new(block)), path, None),
            None => false,
        }
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
        while a.pdf_busy() {
            assert!(
                std::time::Instant::now() < deadline,
                "PDF export did not finish"
            );
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
                a.poll_pdf_prepare(&ctx);
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
    fn pdf_pages_are_prepared_across_frames_and_restart_after_an_edit() -> anyhow::Result<()> {
        let dir = tempfile_dir("bg-pdf-prepare")?;
        let mut a = three_pages()?;
        // One page per slice, so the preparation spans frames even on three pages.
        a.ext.shell.pdf_budget = std::time::Duration::ZERO;
        let path = dir.join("all.pdf");
        // The file menu's PDF goes through the same preparation.
        assert!(a.start_pdf_export(path.clone()));
        assert!(
            a.pdf_busy() && !a.pdf.busy(),
            "preparing, not converting yet"
        );
        assert!(a.message.contains("PDF 준비 중… 1/3쪽"), "{}", a.message);
        assert!(!a.start_pages_pdf(dir.join("second.pdf"), 1, 1));
        assert!(a.error.take().is_some_and(|e| e.contains("진행 중")));
        // An edit before the pages are ready starts over on the edited document.
        a.session.execute(Command::Insert("편집 ".into()))?;
        let ctx = a.ctx.clone();
        a.poll_pdf_prepare(&ctx);
        let Some(job) = a.ext.shell.pdf_preparing.as_ref() else {
            anyhow::bail!("still preparing");
        };
        assert_eq!(job.restarts, 1);
        assert_eq!(job.prep.generation(), a.session.generation);
        assert_eq!(job.prep.ready(), 1, "only pages of the edited document");
        assert!(a.message.contains("다시 준비"), "{}", a.message);
        wait(&mut a);
        assert!(is_pdf(&path));
        assert!(a.message.contains("PDF 저장 완료"), "{}", a.message);
        assert!(!dir.join("second.pdf").exists());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
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
    fn print_details_reach_the_background_pdf() -> anyhow::Result<()> {
        let dir = tempfile_dir("bg-pdf-layout")?;
        let mut a = three_pages()?;
        let print = dir.join("two-up.pdf");
        a.ext.shell.next_print_layout = Some(hwpcraft_engine::PrintLayout {
            n_up: 2,
            watermark: Some(hwpcraft_engine::Watermark {
                text: "대외비".into(),
                opacity: None,
            }),
            ..Default::default()
        });
        assert!(a.start_print_pdf(print.clone(), "all", 1, true));
        assert!(
            a.ext.shell.next_print_layout.is_none(),
            "taken by this PDF only"
        );
        wait(&mut a);
        let bytes = std::fs::read(&print)?;
        let text = String::from_utf8_lossy(&bytes);
        let pages = text.matches("/Type /Page").count() + text.matches("/Type/Page").count()
            - text.matches("/Type /Pages").count()
            - text.matches("/Type/Pages").count();
        assert_eq!(pages, 2, "three pages, two to a sheet");
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

#[cfg(test)]
mod dialog_tests {
    use super::*;

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// One frame of a dialog whose body may hold a text field; returns the button.
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, field: Option<bool>) -> Option<usize> {
        let mut clicked = None;
        let mut text = String::from("줄");
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |_| {
                clicked = dialog(ctx, "시험", 400.0, &["설정(D)", "취소"], |ui| match field {
                    Some(true) => {
                        ui.add(egui::TextEdit::multiline(&mut text).id(egui::Id::new("multi")));
                    }
                    Some(false) => {
                        ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::new("single")));
                    }
                    None => {}
                });
            },
        );
        output.textures_delta.clear();
        clicked
    }

    #[test]
    fn a_modal_dialog_takes_the_keyboard_and_gives_it_back() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        let canvas = egui::Id::new("document-canvas");
        // The page registers the canvas; the dialogs come after the keyboard, as in a frame.
        let run = |a: &mut HwpApp| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 916.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                a.page_view(ui);
                a.keyboard(ui.ctx());
                a.dialogs(ui.ctx());
            });
            output.textures_delta.clear();
            ctx.memory(|m| m.focused())
        };
        ctx.memory_mut(|m| m.request_focus(canvas));
        // While 변경 내용 저장 is up the page lets go (Tab then walks the dialog) ...
        a.pending = Some(crate::Pending::New);
        assert_eq!(run(&mut a), None);
        assert_eq!(run(&mut a), None);
        // ... and typing goes back to the page once it closes, without a click.
        a.pending = None;
        run(&mut a);
        assert_eq!(run(&mut a), Some(canvas));
        // A dialog with a text field gives it the keyboard first.
        a.shell_ext_view("user-info");
        run(&mut a);
        assert!(run(&mut a).is_some_and(|id| id != canvas));
        a.ext.shell.dialog = None;
        run(&mut a);
        assert_eq!(run(&mut a), Some(canvas));
        Ok(())
    }

    #[test]
    fn alt_and_the_access_letter_press_that_button() {
        let alt = |k| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        assert_eq!(chrome::mnemonic("설정(D)"), Some(egui::Key::D));
        assert_eq!(chrome::mnemonic("저장 안 함(N)"), Some(egui::Key::N));
        assert_eq!(chrome::mnemonic("취소"), None);
        assert_eq!(chrome::mnemonic("(가)"), None);
        let ctx = egui::Context::default();
        frame(&ctx, vec![], None);
        assert_eq!(frame(&ctx, vec![alt(egui::Key::D)], None), Some(0));
        // Even while a text field has the keyboard (Enter stays with the field).
        let ctx = egui::Context::default();
        frame(&ctx, vec![], Some(true));
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("multi")));
        frame(&ctx, vec![], Some(true));
        assert_eq!(frame(&ctx, vec![alt(egui::Key::D)], Some(true)), Some(0));
        // chrome::actions: Enter and Alt+D apply when the button is enabled.
        for (events, valid, applied) in [
            (vec![key(egui::Key::Enter)], true, true),
            (vec![alt(egui::Key::D)], true, true),
            (vec![key(egui::Key::Enter)], false, false),
        ] {
            let ctx = egui::Context::default();
            let mut apply = false;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    apply = chrome::actions(ui, (!valid).then_some("고를 것이 없음"), "넣기(D)").0;
                },
            );
            output.textures_delta.clear();
            assert_eq!(apply, applied);
        }
    }

    /// Layers shown by one frame of a fresh context that draws `draw`.
    fn layers(draw: impl Fn(&egui::Context)) -> Vec<egui::LayerId> {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| draw(&ctx));
        output.textures_delta.clear();
        let mut ids: Vec<egui::LayerId> = ctx
            .memory(|m| m.areas().visible_layer_ids())
            .into_iter()
            .collect();
        ids.sort_by_key(|l| (l.order as u8, l.id.value()));
        ids
    }

    fn styles_a(ctx: &egui::Context) {
        dialog(ctx, "스타일", 784.0, &["설정(D)", "취소"], |_| {});
    }

    fn styles_b(ctx: &egui::Context) {
        dialog(ctx, "스타일", 420.0, &["설정(D)", "취소"], |_| {});
    }

    #[test]
    fn same_titled_dialogs_from_different_places_keep_separate_window_state() {
        let a = layers(styles_a);
        assert!(!a.is_empty());
        // The same call site keeps its id from frame to frame and run to run…
        assert_eq!(a, layers(styles_a));
        // …while another call site with the same title gets its own window.
        let b = layers(styles_b);
        assert!(a.iter().any(|layer| !b.contains(layer)), "{a:?} / {b:?}");
    }

    #[test]
    fn enter_is_the_default_button_and_escape_cancels() {
        let ctx = egui::Context::default();
        assert_eq!(frame(&ctx, vec![], None), None);
        assert_eq!(frame(&ctx, vec![key(egui::Key::Enter)], None), Some(0));
        assert_eq!(frame(&ctx, vec![key(egui::Key::Escape)], None), Some(1));
    }

    #[test]
    fn a_grey_default_button_ignores_enter_and_clicks_but_escape_still_cancels() {
        let ctx = egui::Context::default();
        let run = |events: Vec<egui::Event>| {
            let mut clicked = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |_| {
                    clicked = dialog_with(
                        &ctx,
                        "시험",
                        400.0,
                        &[("만들기(D)", Some("먼저 고르세요.")), ("취소", None)],
                        |_| {},
                    );
                },
            );
            output.textures_delta.clear();
            clicked
        };
        assert_eq!(run(vec![]), None);
        assert_eq!(run(vec![key(egui::Key::Enter)]), None);
        assert_eq!(run(vec![key(egui::Key::Escape)]), Some(1));
    }

    #[test]
    fn saving_a_track_changes_document_asks_and_saves_the_result() -> anyhow::Result<()> {
        let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../rhwp/samples/issue5169_viewtext_changetracking.hwp");
        if !sample.is_file() {
            return Ok(());
        }
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session.open(&sample)?;
        if !a.session.has_unkept_track_changes() {
            return Ok(());
        }
        let dir = std::env::temp_dir().join(format!("hwpcraft-tracked-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let target = dir.join("result.hwpx");
        assert!(
            !a.confirm_tracked_save(&dir.join("out.pdf")),
            "PDF is not asked"
        );
        assert!(a.confirm_tracked_save(&target));
        let ctx = a.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![key(egui::Key::Enter)],
                ..Default::default()
            },
            |_| a.shell_ext_dialogs(&ctx),
        );
        output.textures_delta.clear();
        assert!(target.is_file(), "{}", a.message);
        assert!(a.ext.shell.dialog.is_none());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn templates_open_only_without_unsaved_changes() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        a.shell_ext_dispatch("file.template", &ctx.clone());
        let Some(Dialog::Templates { list, .. }) = &a.ext.shell.dialog else {
            panic!("문서마당");
        };
        let _ = list;
        a.session.execute(Command::Insert("고친 글".into()))?;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![key(egui::Key::Enter)],
                ..Default::default()
            },
            |_| a.shell_ext_dialogs(&ctx),
        );
        output.textures_delta.clear();
        assert!(
            a.ext.shell.dialog.is_some() && a.session.dirty(),
            "Enter does not replace a changed document"
        );
        Ok(())
    }

    #[test]
    fn an_hwp_save_with_picture_effects_asks_and_drops_them() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        let dir = std::env::temp_dir().join(format!("hwpcraft-effects-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let png = dir.join("p.png");
        image::RgbImage::from_fn(30, 20, |x, y| image::Rgb([x as u8 * 8, y as u8 * 12, 90]))
            .save_with_format(&png, image::ImageFormat::Png)?;
        let v = a
            .session
            .run("picture.insert", json!({"path": png, "width_mm": 30.0}))?;
        a.session.run(
            "picture.effects",
            json!({"address": v["address"], "glow": {}}),
        )?;
        let out = dir.join("out.hwp");
        assert!(a.confirm_tracked_save(&out));
        assert!(matches!(
            a.ext.shell.dialog,
            Some(Dialog::TrackedSave {
                tracked: false,
                effects: 1,
                ..
            })
        ));
        // HWPX keeps the effects: no question there.
        assert_eq!(
            a.session
                .picture_effects_blocking_save(&dir.join("out.hwpx")),
            0
        );
        for events in [vec![], vec![key(egui::Key::Enter)]] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |_| a.shell_ext_dialogs(&ctx),
            );
            output.textures_delta.clear();
        }
        assert!(a.ext.shell.dialog.is_none(), "{}", a.message);
        assert!(out.exists());
        assert!(
            a.message.starts_with("그림 효과 1개를 빼고 저장 완료"),
            "{}",
            a.message
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_docx_export_leaves_the_document_file_and_saved_state() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let dir = std::env::temp_dir().join(format!("hwpcraft-docx-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        a.session
            .execute(hwpcraft_engine::Command::Insert("내보낼 글".into()))?;
        let own = dir.join("own.hwpx");
        a.session.save(&own)?;
        a.session
            .execute(hwpcraft_engine::Command::Insert(" 더".into()))?;
        assert!(a.session.dirty());
        let docx = dir.join("out.docx");
        assert!(a.export_docx(&docx, false));
        assert!(docx.exists());
        assert_eq!(a.session.path.as_deref(), Some(own.as_path()));
        assert!(a.session.dirty(), "an export does not save the document");
        assert!(
            a.message.starts_with("DOCX로 내보냈습니다"),
            "{}",
            a.message
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn master_page_dialog_saves_its_text_and_deletes_it() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        let frame = |a: &mut HwpApp, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |_| a.shell_ext_dialogs(&ctx),
            );
            output.textures_delta.clear();
        };
        a.shell_ext_dispatch("page.master", &ctx.clone());
        if let Some(Dialog::MasterPage { text, .. }) = a.ext.shell.dialog.as_mut() {
            *text = "대외비".into();
        }
        frame(&mut a, vec![]);
        frame(&mut a, vec![key(egui::Key::Enter)]);
        assert!(a.ext.shell.dialog.is_none(), "saved and closed");
        assert_eq!(
            a.session.run("page.master.get", json!({}))?["text"],
            "대외비"
        );
        // Reopened, it shows the text and offers 지우기.
        a.shell_ext_dispatch("page.master", &ctx.clone());
        let Some(Dialog::MasterPage { text, .. }) = &a.ext.shell.dialog else {
            panic!("dialog");
        };
        assert_eq!(text, "대외비");
        assert!(a.ext.shell.dialog.is_some());
        let _ = a.session.run("page.master.delete", json!({}))?;
        assert_eq!(
            a.session.run("page.master.get", json!({}))?["exists"],
            false
        );
        // 홀수 쪽 picked in the dialog saves an odd-page master page only.
        a.shell_ext_dispatch("page.master", &ctx.clone());
        if let Some(Dialog::MasterPage { kind, text, .. }) = a.ext.shell.dialog.as_mut() {
            *kind = 1;
            *text = "홀".into();
        }
        frame(&mut a, vec![]);
        frame(&mut a, vec![key(egui::Key::Enter)]);
        assert!(a.ext.shell.dialog.is_none(), "saved and closed");
        let odd = a
            .session
            .run("page.master.get", json!({"applyTo": "odd"}))?;
        assert_eq!(odd["text"], "홀");
        assert_eq!(
            a.session.run("page.master.get", json!({}))?["exists"],
            false
        );
        Ok(())
    }

    #[test]
    fn master_page_dialog_puts_a_shape_on_the_paper_and_removes_it() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        a.shell_ext_dispatch("page.master", &ctx.clone());
        let Some(Dialog::MasterPage {
            list,
            objects,
            object,
            ..
        }) = a.ext.shell.dialog.take()
        else {
            panic!("dialog");
        };
        let (mut list, mut objects) = (list, objects);
        // 넣기 with the defaults: a rectangle behind the text, on a new 양쪽 바탕쪽.
        let params = object.params("both");
        assert_eq!(params["kind"], "rectangle");
        assert_eq!(params["height_mm"], 20.0);
        a.master_object_request("page.master.object.insert", params, &mut list, &mut objects);
        assert!(a.error.is_none(), "{:?}", a.error);
        let shown = objects["objects"].as_array().cloned().unwrap_or_default();
        assert_eq!(shown.len(), 1, "{objects}");
        assert_eq!(shown[0]["applyTo"], "both");
        assert_eq!(shown[0]["wrap"], "behind");
        assert_eq!(shown[0]["x_mm"], 20.0);
        assert!(
            list["masterPages"]
                .as_array()
                .is_some_and(|m| m.iter().any(|m| m["applyTo"] == "Both")),
            "the master page was made: {list}"
        );
        // A picture needs its file; the dialog draws with the object listed.
        let picture = MasterObject {
            kind: 0,
            ..MasterObject::default()
        };
        assert_eq!(picture.params("odd")["path"], "");
        assert!(picture.params("odd").get("height_mm").is_none());
        a.ext.shell.dialog = Some(Dialog::MasterPage {
            kind: 0,
            list: list.clone(),
            text: String::new(),
            objects: objects.clone(),
            object: picture,
        });
        let mut output = ctx.run_ui(Default::default(), |_| a.shell_ext_dialogs(&ctx));
        output.textures_delta.clear();
        assert!(a.ext.shell.dialog.is_some(), "still open");
        // 지우기 removes it again; one undo brings it back.
        a.master_object_request(
            "page.master.object.delete",
            json!({"applyTo": "both", "index": 0}),
            &mut list,
            &mut objects,
        );
        assert_eq!(objects["objects"].as_array().map(Vec::len), Some(0));
        a.session.execute(Command::Undo)?;
        let back = a.session.run("page.master.object.list", json!({}))?;
        assert_eq!(back["objects"].as_array().map(Vec::len), Some(1));
        Ok(())
    }

    #[test]
    fn enter_in_a_multi_line_field_stays_a_newline() {
        let ctx = egui::Context::default();
        frame(&ctx, vec![], Some(true));
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("multi")));
        frame(&ctx, vec![], Some(true));
        assert_eq!(frame(&ctx, vec![key(egui::Key::Enter)], Some(true)), None);
        // A single-line field gives up focus on Enter, so Enter confirms.
        let ctx = egui::Context::default();
        frame(&ctx, vec![], Some(false));
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("single")));
        frame(&ctx, vec![], Some(false));
        assert_eq!(
            frame(&ctx, vec![key(egui::Key::Enter)], Some(false)),
            Some(0)
        );
    }
}

#[cfg(test)]
mod page_outline_tests {
    use super::*;

    /// One full frame of the page view in a 1500 x 916 window.
    fn frame(a: &mut HwpApp) {
        let ctx = a.ctx.clone();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500.0, 916.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| a.page_view(ui));
        output.textures_delta.clear();
    }

    #[test]
    fn page_outline_off_stacks_only_the_body_of_each_page() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session.execute(Command::Insert("첫 쪽".into()))?;
        a.session.execute(Command::PageBreak)?;
        a.session.execute(Command::Insert("둘째 쪽".into()))?;
        a.zoom = 0.5;
        frame(&mut a);
        let pages = |a: &HwpApp| a.wheel_zoom.pages.clone();
        let on = pages(&a);
        assert_eq!(on.len(), 2, "{on:?}");
        let height = on[0].1.height();
        // 쪽 윤곽 on: whole pages with a gap.
        assert!(on[1].1.min.y >= on[0].1.max.y + 1.0);
        a.shell_ext_dispatch("view.page_outline", &a.ctx.clone());
        assert!(!a.ext.shell.page_outline && !a.shell_ext_checked("view.page_outline"));
        frame(&mut a);
        let off = pages(&a);
        let (top, bottom) = a.ext.shell.page_bodies[0];
        assert!(top > 0.0 && bottom < height / a.zoom, "{top} {bottom}");
        // Each page rectangle keeps its full size, but the second one starts where the
        // first one's body ends: margins, header and footer are scrolled out of view.
        assert_eq!(off[0].1.height(), height);
        let body_end = off[0].1.min.y + bottom * a.zoom;
        let (top1, _) = a.ext.shell.page_bodies[1];
        assert!(
            (off[1].1.min.y + top1 * a.zoom - body_end).abs() < 0.5,
            "{off:?}"
        );
        a.shell_ext_dispatch("view.page_outline", &a.ctx.clone());
        assert!(a.ext.shell.page_outline);
        Ok(())
    }
}

#[cfg(test)]
mod line_number_tests {
    use super::*;

    #[test]
    fn the_line_number_menu_turns_numbering_on_and_opens_its_settings() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        a.session
            .execute(Command::Insert("첫 줄\n둘째 줄\n셋째 줄".into()))?;
        let before = a.session.render_page_svg(0)?;
        assert!(!a.line_numbers_shown() && !a.shell_ext_checked("page.line_numbers.toggle"));
        assert_eq!(a.shell_ext_enabled("page.line_numbers.toggle"), Some(true));
        // 줄 번호 표시: on, drawn (RHWP 0037), and off again.
        a.shell_ext_dispatch("page.line_numbers.toggle", &ctx);
        assert!(a.line_numbers_shown() && a.shell_ext_checked("page.line_numbers.toggle"));
        assert_ne!(a.session.render_page_svg(0)?, before, "numbers are drawn");
        // 줄 번호 설정 opens with the stored values (restart shown as 이어서 when unset).
        a.shell_ext_dispatch("page.line_numbers.dialog", &ctx);
        let Some(Dialog::LineNumbers(s)) = &a.ext.shell.dialog else {
            anyhow::bail!("dialog");
        };
        assert_eq!(
            (s.show, s.start, s.count_by, s.restart, s.all_sections),
            (true, 1, 1, 3, false)
        );
        a.ext.shell.dialog = None;
        // 현재 문단 건너뜀: checked while the caret paragraph skips numbering.
        assert_eq!(
            a.shell_ext_enabled("page.line_numbers.skip_paragraph"),
            Some(true)
        );
        assert!(!a.shell_ext_checked("page.line_numbers.skip_paragraph"));
        let numbered = a.session.render_page_svg(0)?;
        a.shell_ext_dispatch("page.line_numbers.skip_paragraph", &ctx);
        assert!(a.shell_ext_checked("page.line_numbers.skip_paragraph"));
        assert_ne!(a.session.render_page_svg(0)?, numbered);
        a.shell_ext_dispatch("page.line_numbers.skip_paragraph", &ctx);
        assert!(!a.shell_ext_checked("page.line_numbers.skip_paragraph"));
        a.shell_ext_dispatch("page.line_numbers.toggle", &ctx);
        assert!(!a.line_numbers_shown());
        assert_eq!(
            a.session.render_page_svg(0)?,
            before,
            "off draws the page as before"
        );
        Ok(())
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn document_history_lists_the_versions_hangul_recorded() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        assert_eq!(a.shell_ext_enabled("review.history"), Some(true));
        a.shell_ext_dispatch("review.history", &ctx);
        let Some(Dialog::History(list)) = &a.ext.shell.dialog else {
            anyhow::bail!("dialog");
        };
        assert_eq!(list["entries"], json!([]), "a new document has no history");
        a.ext.shell.dialog = None;
        a.session.open(std::path::Path::new(
            "../../../rhwp/samples/basic/treatise sample.hwp",
        ))?;
        a.shell_ext_dispatch("review.history", &ctx);
        let Some(Dialog::History(list)) = &a.ext.shell.dialog else {
            anyhow::bail!("dialog");
        };
        assert_eq!(list["entries"].as_array().map(Vec::len), Some(4));
        assert_eq!(list["lastVersion"], true);
        Ok(())
    }
}
