//! Menu tabs, ribbon and format tool bar in a Hangul-style arrangement.
//! Owner: shell/page area.
use super::icons::{self, Icon};
use super::*;

#[derive(Clone, Copy)]
enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Pdf,
    Text,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Character,
    Paragraph,
    Page,
    Portrait,
    Landscape,
    Table,
    Break,
    Find,
    Replace,
    Navigation,
    Editor,
    Rulers,
    ZoomIn,
    ZoomOut,
    Fit,
    Help,
    TableEdit(hwpcraft_engine::TableEdit),
    Link,
    Bookmarks,
    Columns,
    ColumnBreak,
    Picture,
    CopyFormat,
    PasteFormat,
    Search,
    HeaderFooter(bool),
    PageNumber,
    Equation,
    Note(bool),
    Shape(hwpcraft_engine::ShapeKind),
    Objects,
    Styles,
    Symbols,
    /// Toggle a character property (`bold`, `italic`, `underline`, `strikethrough`).
    Char(&'static str),
    /// Paragraph alignment (`justify`, `left`, …).
    Align(&'static str),
    /// Command implemented in an `ext_*` module, addressed by command id.
    Ext(&'static str),
    /// The module command when its owner provides it, otherwise the built-in one.
    Or(&'static str, &'static Action),
    /// 보기 → 지원하지 않는 명령: show the commands in `UNSUPPORTED` (grey) in the ribbon.
    ShowUnsupported,
}

/// Small 24 px tool inside a stacked column.
#[derive(Clone, Copy)]
enum Mini {
    /// Icon only; tooltip, action, drop-down mark.
    Icon(Icon, &'static str, Action, bool),
    /// Icon and label.
    Text(Icon, &'static str, Action, bool),
    /// A combo box or split button (`format_widget`), with its width.
    Widget(&'static str, f32),
}

#[derive(Clone, Copy)]
enum Item {
    /// 76 px button: icon, label (`\n` = second line), action, drop-down mark.
    Big(Icon, &'static str, Action, bool),
    /// Big button that only opens a list of commands.
    Menu(Icon, &'static str, &'static [(&'static str, Action)]),
    /// One column of rows of small tools.
    Stack(&'static [&'static [Mini]]),
    /// One column of check boxes.
    Checks(&'static [(&'static str, Action)]),
    /// 입력 탭 drawing gallery: 5 × 3 cells of 24 px.
    Shapes,
    /// 서식 탭 style gallery (2 × 2 styles).
    StyleGallery,
    /// A big widget from the format module (id, width); the item is drawn when it has none.
    Custom(&'static str, f32, &'static Item),
    Sep,
}
use Action as A;
use Item::{Big, Checks, Sep, Stack};
use Mini::{Icon as I, Text as T, Widget as W};

const SHAPES: &[(Icon, &str, Action)] = &[
    (
        Icon::Textbox,
        "가로 글상자",
        A::Or(
            "insert.shape.textbox",
            &A::Shape(hwpcraft_engine::ShapeKind::Textbox),
        ),
    ),
    (
        Icon::VTextbox,
        "세로 글상자",
        A::Ext("insert.shape.vtextbox"),
    ),
    (
        Icon::Rectangle,
        "직사각형",
        A::Or(
            "insert.shape.rectangle",
            &A::Shape(hwpcraft_engine::ShapeKind::Rectangle),
        ),
    ),
    (
        Icon::Ellipse,
        "타원",
        A::Or(
            "insert.shape.ellipse",
            &A::Shape(hwpcraft_engine::ShapeKind::Ellipse),
        ),
    ),
    (
        Icon::Line,
        "직선",
        A::Or(
            "insert.shape.line",
            &A::Shape(hwpcraft_engine::ShapeKind::Line),
        ),
    ),
    (Icon::Arc, "호", A::Ext("insert.shape.arc")),
    (Icon::Polygon, "다각형", A::Ext("insert.shape.polygon")),
    (Icon::Curve, "곡선", A::Ext("insert.shape.curve")),
    (Icon::Freeline, "자유선", A::Ext("insert.shape.freeline")),
    (
        Icon::Connector,
        "직선 연결선",
        A::Ext("insert.shape.connector.straight"),
    ),
    (
        Icon::Connector,
        "직선 화살표 연결선",
        A::Ext("insert.shape.connector.straight_arrow"),
    ),
    (
        Icon::Connector,
        "직선 양쪽 화살표 연결선",
        A::Ext("insert.shape.connector.straight_both"),
    ),
    (
        Icon::Connector,
        "꺾인 연결선",
        A::Ext("insert.shape.connector.stroke"),
    ),
    (
        Icon::Connector,
        "꺾인 화살표 연결선",
        A::Ext("insert.shape.connector.stroke_arrow"),
    ),
    (
        Icon::Connector,
        "꺾인 양쪽 화살표 연결선",
        A::Ext("insert.shape.connector.stroke_both"),
    ),
];

const EDIT: &[Item] = &[
    Big(Icon::Cut, "오려\n두기", A::Cut, false),
    Big(Icon::Copy, "복사하기", A::Copy, false),
    Big(Icon::Paste, "붙이기", A::Paste, true),
    Big(
        Icon::FormatCopy,
        "모양\n복사",
        A::Or("format.copy_shape", &A::CopyFormat),
        false,
    ),
    Big(
        Icon::CtrlMarkDelete,
        "조판 부호\n지우기",
        A::Ext("edit.delete_ctrl_marks"),
        false,
    ),
    Sep,
    Big(
        Icon::Character,
        "글자\n모양",
        A::Or("format.character_dialog", &A::Character),
        false,
    ),
    Big(
        Icon::Paragraph,
        "문단\n모양",
        A::Or("format.paragraph_dialog", &A::Paragraph),
        false,
    ),
    Big(
        Icon::Style,
        "스타일",
        A::Or("format.style.dialog", &A::Styles),
        true,
    ),
    Sep,
    Big(Icon::Portrait, "세로", A::Portrait, false),
    Big(Icon::Landscape, "가로", A::Landscape, false),
    Item::Menu(Icon::PageMargins, "쪽\n여백", MARGIN_MENU),
    Big(Icon::MasterPage, "바탕쪽", A::Ext("page.master"), false),
    Big(Icon::Columns, "단", A::Columns, true),
    Sep,
    Big(
        Icon::Shape,
        "도형",
        A::Or(
            "insert.shape.rectangle",
            &A::Shape(hwpcraft_engine::ShapeKind::Rectangle),
        ),
        true,
    ),
    Big(
        Icon::Image,
        "그림",
        A::Or("insert.picture_dialog", &A::Picture),
        true,
    ),
    Big(Icon::Table, "표", A::Or("table.create", &A::Table), true),
    Big(Icon::Chart, "차트", A::Ext("insert.chart"), true),
    Sep,
    Big(
        Icon::ObjectSelect,
        "개체\n선택",
        A::Ext("object.select"),
        true,
    ),
    Big(
        Icon::ObjectProtect,
        "개체\n보호",
        A::Ext("object.lock"),
        true,
    ),
    Big(
        Icon::ObjectProtect,
        "보호 모두\n해제",
        A::Ext("object.unlock_all"),
        false,
    ),
    Sep,
    Item::Menu(
        Icon::Find,
        "찾기",
        &[
            ("찾기", A::Find),
            ("찾아 바꾸기", A::Replace),
            ("이동", A::Ext("edit.goto")),
        ],
    ),
    Big(Icon::Replace, "글자\n바꾸기", A::Replace, true),
    Big(Icon::Sort, "정렬", A::Ext("edit.sort"), false),
    // One entry point for the region editors; the nested-table editor is the narrower one.
    Item::Menu(
        Icon::Header,
        "문서 영역\n편집",
        &[
            (
                "문서 영역 편집 (본문·표·머리말·꼬리말·글상자·주석)",
                A::Ext("edit.regions"),
            ),
            ("중첩 표 편집", A::Ext("edit.contexts")),
        ],
    ),
];
/// Tooltips for ribbon commands that no module implements yet, shown while they are grey.
/// `unsupported_reasons_name_only_unimplemented_commands` keeps this list honest.
const WHY_TRACK: &str = "변경 내용 추적은 지원하지 않습니다. HWP 파일의 변경 추적 기록은 보존하고, HWPX의 변경 추적 기록은 아직 읽거나 보존하지 못합니다. 본문에 변경 표시는 그리지 않습니다. 두 파일의 차이는 [문서 비교]로 볼 수 있습니다.";
const WHY_SPELLING: &str = "맞춤법 검사는 지원하지 않습니다. 내장 맞춤법 사전이 없습니다.";
const WHY_DICTIONARY: &str = "사전 찾기는 지원하지 않습니다. 내장 국어·한자 사전이 없습니다. 한자의 음과 뜻 후보는 [한자로 바꾸기](F9)에서 볼 수 있습니다.";
const WHY_PROOF: &str = "교정 부호 넣기와 보기는 지원하지 않습니다.";
const WHY_LINE_NUMBERS: &str = "줄 번호는 지원하지 않습니다. 조판 엔진(RHWP)에 줄 번호가 없습니다.";
const WHY_GRID: &str = "격자 보기는 지원하지 않습니다. 화면 렌더에 격자 층이 없습니다.";
const WHY_OUTLINE: &str =
    "쪽 윤곽 끄기(본문만 이어 보기)는 지원하지 않습니다. 문서는 항상 쪽 모양으로 보입니다.";
const WHY_DISTRIBUTION: &str = "배포용 문서 저장·편집은 지원하지 않습니다.";
const WHY_WINDOWS: &str = "한 창에 문서 하나만 엽니다. 창 나누기·배열·전환은 지원하지 않습니다.";
const WHY_NOT_YET: &str = "아직 지원하지 않는 기능입니다.";
const UNSUPPORTED: &[(&str, &str)] = &[
    ("review.history", WHY_TRACK),
    ("review.track_changes", WHY_TRACK),
    ("review.balloons", WHY_TRACK),
    ("review.show_changes", WHY_TRACK),
    ("review.view_original", WHY_TRACK),
    ("review.view_original_marked", WHY_TRACK),
    ("review.view_final_marked", WHY_TRACK),
    ("review.view_final", WHY_TRACK),
    ("review.accept", WHY_TRACK),
    ("review.reject", WHY_TRACK),
    ("review.prev", WHY_TRACK),
    ("review.next", WHY_TRACK),
    ("review.spelling", WHY_SPELLING),
    ("tools.dictionary", WHY_DICTIONARY),
    ("view.proof_marks", WHY_PROOF),
    ("review.proof_marks", WHY_PROOF),
    ("page.line_numbers", WHY_LINE_NUMBERS),
    ("view.grid", WHY_GRID),
    ("view.page_outline", WHY_OUTLINE),
    ("security.distribution", WHY_DISTRIBUTION),
    ("security.distribution_edit", WHY_DISTRIBUTION),
    ("security.distribution_change", WHY_DISTRIBUTION),
    ("view.split", WHY_WINDOWS),
    ("view.arrange", WHY_WINDOWS),
    ("view.switch_window", WHY_WINDOWS),
    ("view.guides", WHY_NOT_YET),
    ("format.style.gallery", WHY_NOT_YET),
    ("format.drop_cap", WHY_NOT_YET),
    ("format.picture_bullet", WHY_NOT_YET),
    ("view.hanja_pronounce", WHY_NOT_YET),
    ("insert.shape.freeline", WHY_NOT_YET),
    ("insert.textart", WHY_NOT_YET),
    ("insert.ole", WHY_NOT_YET),
    ("insert.cross_reference", WHY_NOT_YET),
    ("insert.helper", WHY_NOT_YET),
    ("page.text_direction", WHY_NOT_YET),
    ("page.column_def_break", WHY_NOT_YET),
    ("page.section_break", WHY_NOT_YET),
    ("page.label", WHY_NOT_YET),
    ("page.manuscript", WHY_NOT_YET),
    ("security.protect_text", WHY_NOT_YET),
    ("security.privacy_clear", WHY_NOT_YET),
    ("security.privacy_password", WHY_NOT_YET),
    ("security.settings", WHY_NOT_YET),
    ("tools.index", WHY_NOT_YET),
    ("tools.toc", WHY_NOT_YET),
    ("tools.citation", WHY_NOT_YET),
    ("tools.bibliography", WHY_NOT_YET),
    ("tools.bibliography_style", WHY_NOT_YET),
    ("tools.bibliography_refresh", WHY_NOT_YET),
    ("tools.macro", WHY_NOT_YET),
    ("tools.autocorrect", WHY_NOT_YET),
    ("tools.translate", WHY_NOT_YET),
    ("tools.presentation", WHY_NOT_YET),
    ("tools.keyboard", WHY_NOT_YET),
    ("tools.customize", WHY_NOT_YET),
    ("tools.options", WHY_NOT_YET),
    ("tools.user_info", WHY_NOT_YET),
    ("file.template", WHY_NOT_YET),
];

fn unsupported_reason(action: Action) -> Option<&'static str> {
    let (A::Ext(id) | A::Or(id, _)) = action else {
        return None;
    };
    UNSUPPORTED
        .iter()
        .find(|(cmd, _)| *cmd == id)
        .map(|(_, why)| *why)
}

/// Why a supported command is grey: what to do first.
const IN_TABLE: &str = "표 안에 커서를 두면 쓸 수 있습니다.";
const OUT_OF_TABLE: &str = "표 안에서는 쓸 수 없습니다. 표 밖 본문에 커서를 두세요.";
const SELECT_TEXT: &str = "글자를 먼저 선택하세요.";
const SELECT_OBJECT: &str = "개체(그림·도형·글상자)를 먼저 선택하세요.";
const NOT_FOR_OBJECT: &str =
    "선택한 개체에는 쓸 수 없습니다. 보호된 개체라면 [개체 보호]를 먼저 푸세요.";
const NO_SELECTION: &str = "선택을 풀고 넣을 자리에 커서를 두세요.";
const CELL_BLOCK: &str = "셀 블록을 먼저 잡으세요(F5).";
const NOT_NOW: &str = "지금 상태에서는 쓸 수 없습니다.";

/// Shortcut of a command as the command list (help, MCP catalog) gives it.
fn shortcut(action: Action) -> Option<&'static str> {
    static KEYS: std::sync::OnceLock<std::collections::HashMap<&str, &str>> =
        std::sync::OnceLock::new();
    let id = HwpApp::action_id(action)?;
    let keys = KEYS.get_or_init(|| {
        let mut keys = std::collections::HashMap::new();
        for (id, _, key) in hwpcraft_engine::COMMANDS
            .iter()
            .chain(hwpcraft_engine::extension_commands())
        {
            if !key.is_empty() {
                keys.entry(*id).or_insert(*key);
            }
        }
        keys
    });
    keys.get(id).copied()
}

/// A stack whose rows were thinned to one tool by hiding unsupported commands; that
/// tool is drawn as a big button so no empty row is left beside it.
fn lone_mini(rows: &[&[Mini]], hide: bool) -> Option<Mini> {
    let all: Vec<Mini> = rows.iter().flat_map(|r| r.iter()).copied().collect();
    let shown: Vec<Mini> = all
        .iter()
        .copied()
        .filter(|m| match m {
            I(_, _, a, _) | T(_, _, a, _) => !(hide && unsupported_reason(*a).is_some()),
            W(..) => true,
        })
        .collect();
    match shown[..] {
        [one @ (I(..) | T(..))] if all.len() > 1 => Some(one),
        _ => None,
    }
}

/// An item with nothing left to draw when unsupported commands are hidden.
fn item_hidden(item: &Item) -> bool {
    let gone = |a: &Action| unsupported_reason(*a).is_some();
    match item {
        Big(_, _, action, _) => gone(action),
        Item::Menu(_, _, list) | Checks(list) => list.iter().all(|(_, a)| gone(a)),
        Stack(rows) => rows.iter().flat_map(|r| r.iter()).all(|m| match m {
            I(_, _, a, _) | T(_, _, a, _) => gone(a),
            W(..) => false,
        }),
        Item::Custom(_, _, inner) => item_hidden(inner),
        Item::Shapes | Item::StyleGallery | Sep => false,
    }
}

/// A tab's ribbon groups (the runs between separators). With `hide`, items that only
/// hold unsupported commands are left out and a group left empty is dropped.
fn ribbon_groups(items: &[Item], hide: bool) -> Vec<Vec<Item>> {
    items
        .split(|i| matches!(i, Sep))
        .map(|g| {
            g.iter()
                .filter(|i| !(hide && item_hidden(i)))
                .copied()
                .collect::<Vec<_>>()
        })
        .filter(|g| !g.is_empty())
        .collect()
}

/// Unsupported commands of a tab with their reasons, one entry per command id.
fn unsupported_in(items: &[Item]) -> Vec<(String, &'static str)> {
    let mut seen = std::collections::BTreeSet::new();
    HwpApp::tools(items)
        .into_iter()
        .filter_map(|(_, label, action)| {
            let why = unsupported_reason(action)?;
            seen.insert(HwpApp::action_id(action)?)
                .then_some((label, why))
        })
        .collect()
}

/// Hangul 쪽 여백 gallery (measured values live in the engine's `page.margins.preset`).
const MARGIN_MENU: &[(&str, Action)] = &[
    ("기본", A::Ext("page.margins.default")),
    (
        "좁게1 (머리말/꼬리말 여백 포함)",
        A::Ext("page.margins.narrow1_hf"),
    ),
    (
        "좁게2 (머리말/꼬리말 여백 포함)",
        A::Ext("page.margins.narrow2_hf"),
    ),
    (
        "넓게 (머리말/꼬리말 여백 포함)",
        A::Ext("page.margins.wide_hf"),
    ),
    ("좁게", A::Ext("page.margins.narrow")),
    ("넓게", A::Ext("page.margins.wide")),
    ("쪽 여백 설정(A)...", A::Page),
];
/// Hangul 문서 보기 색 (screen only).
const VIEW_COLOR_MENU: &[(&str, Action)] = &[
    ("기본", A::Ext("view.color.light")),
    ("어둡게", A::Ext("view.color.dark")),
    ("고대비", A::Ext("view.color.highcontrast")),
    ("옛 한글", A::Ext("view.color.oldhwp")),
    ("사용자 설정...", A::Ext("view.color.custom")),
];
const VIEW: &[Item] = &[
    Big(
        Icon::PageOutline,
        "쪽\n윤곽",
        A::Ext("view.page_outline"),
        false,
    ),
    Item::Menu(Icon::ViewColor, "문서\n보기 색", VIEW_COLOR_MENU),
    Sep,
    Checks(&[
        ("문단 부호", A::Ext("view.paragraph_marks")),
        ("조판 부호", A::Ext("view.control_codes")),
        ("교정 부호", A::Ext("view.proof_marks")),
    ]),
    Checks(&[
        ("그림", A::Ext("view.pictures")),
        ("투명 선", A::Ext("view.transparent_lines")),
        ("메모", A::Ext("view.memo")),
    ]),
    Sep,
    Checks(&[
        ("상황 선", A::Ext("view.status_bar")),
        ("문서 탭", A::Ext("view.doc_tabs")),
        ("눈금자", A::Rulers),
    ]),
    Checks(&[("지원하지 않는 명령", A::ShowUnsupported)]),
    Big(Icon::Grid, "격자", A::Ext("view.grid"), true),
    Big(Icon::Eye, "안내선", A::Ext("view.guides"), true),
    Sep,
    Big(
        Icon::Pronounce,
        "한자 발음\n표시",
        A::Ext("view.hanja_pronounce"),
        true,
    ),
    Sep,
    Big(Icon::Windows, "작업\n창", A::Navigation, true),
    Sep,
    Big(
        Icon::SplitView,
        "편집 화면\n나누기",
        A::Ext("view.split"),
        true,
    ),
    Big(Icon::Windows, "창\n배열", A::Ext("view.arrange"), true),
    Big(
        Icon::Windows,
        "창\n전환",
        A::Ext("view.switch_window"),
        true,
    ),
    Sep,
    Big(Icon::ZoomOut, "축소", A::ZoomOut, false),
    Big(Icon::ZoomIn, "확대", A::ZoomIn, false),
    Sep,
    Big(Icon::Zoom100, "100%", A::Ext("view.zoom_100"), false),
    Stack(&[
        &[T(Icon::FitPage, "쪽 맞춤", A::Fit, false)],
        &[T(
            Icon::FitWidth,
            "폭 맞춤",
            A::Ext("view.zoom_fit_width"),
            false,
        )],
    ]),
    Sep,
    Big(Icon::Paragraph, "문단\n편집 패널", A::Editor, false),
];
const INSERT: &[Item] = &[
    Item::Shapes,
    Big(
        Icon::Image,
        "그림",
        A::Or("insert.picture_dialog", &A::Picture),
        true,
    ),
    Item::Menu(
        Icon::Table,
        "표",
        &[
            ("표 만들기", A::Or("table.create", &A::Table)),
            ("표 그리기", A::Ext("table.draw")),
            ("표 지우개", A::Ext("table.eraser")),
            ("문자열을 표로", A::Ext("table.from_text")),
        ],
    ),
    Item::Menu(
        Icon::Chart,
        "차트",
        &[
            ("차트 만들기", A::Ext("insert.chart")),
            ("차트 데이터 편집", A::Ext("chart.edit")),
        ],
    ),
    Big(Icon::TextArt, "글맵시", A::Ext("insert.textart"), true),
    Big(
        Icon::Equation,
        "수식",
        A::Or("insert.equation", &A::Equation),
        true,
    ),
    Big(Icon::Ole, "OLE\n개체", A::Ext("insert.ole"), false),
    Sep,
    Big(
        Icon::FormObject,
        "양식\n개체",
        A::Ext("insert.form_object"),
        true,
    ),
    Big(Icon::Field, "누름틀", A::Ext("insert.field"), false),
    Sep,
    Item::Menu(Icon::NewNumber, "번호\n넣기", AUTO_NUMBER),
    Big(
        Icon::Footnote,
        "각주",
        A::Or("insert.footnote", &A::Note(false)),
        false,
    ),
    Item::Menu(Icon::Footnote, "각주/미주\n모양", NOTE_SHAPE),
    Stack(&[
        &[T(
            Icon::Endnote,
            "각주↔미주 변환",
            A::Ext("note.convert"),
            false,
        )],
        &[T(
            Icon::CtrlMarkDelete,
            "주석 지우기",
            A::Ext("note.delete"),
            false,
        )],
        &[T(
            Icon::Field,
            "누름틀 지우기",
            A::Ext("field.delete"),
            false,
        )],
    ]),
    Big(
        Icon::Endnote,
        "미주",
        A::Or("insert.endnote", &A::Note(true)),
        false,
    ),
    Big(Icon::Memo, "메모", A::Ext("insert.memo"), false),
    Big(
        Icon::ProofMark,
        "교정\n부호",
        A::Ext("review.proof_marks"),
        true,
    ),
    Sep,
    Big(Icon::Bookmark, "책갈피", A::Bookmarks, false),
    Big(
        Icon::CrossRef,
        "상호\n참조",
        A::Ext("insert.cross_reference"),
        false,
    ),
    Sep,
    Big(Icon::Link, "하이퍼링크", A::Link, false),
    Stack(&[
        &[I(
            Icon::Prev,
            "하이퍼링크 이전",
            A::Ext("view.link_prev"),
            false,
        )],
        &[I(
            Icon::Next,
            "하이퍼링크 다음",
            A::Ext("view.link_next"),
            false,
        )],
    ]),
    Sep,
    Big(
        Icon::Symbol,
        "문자표",
        A::Or("insert.symbol", &A::Symbols),
        true,
    ),
    Stack(&[
        &[T(
            Icon::Translate,
            "입력 도우미",
            A::Ext("insert.helper"),
            true,
        )],
        &[T(Icon::Hanja, "한자 입력", A::Ext("insert.hanja"), true)],
    ]),
    Big(
        Icon::Character,
        "글자\n겹치기",
        A::Ext("format.char_overlap.dialog"),
        false,
    ),
    Item::Menu(
        Icon::VTextbox,
        "세로\n글상자",
        &[
            ("세로 글상자 (영문 눕힘)", A::Ext("insert.shape.vtextbox")),
            (
                "세로 글상자 (영문 세움)",
                A::Ext("insert.shape.vtextbox_all"),
            ),
        ],
    ),
    Item::Menu(
        Icon::Bookmark,
        "상용구",
        &[
            ("상용구 내용 (Ctrl+F3)", A::Ext("tools.autotext")),
            ("상용구 등록", A::Ext("tools.autotext.register")),
            ("상용구 바로 넣기 (Alt+I)", A::Ext("tools.autotext.expand")),
        ],
    ),
];
const ALIGN_MENU: Item = Item::Menu(
    Icon::Justify,
    "문단\n정렬",
    &[
        ("양쪽 정렬", A::Align("justify")),
        ("왼쪽 정렬", A::Align("left")),
        ("가운데 정렬", A::Align("center")),
        ("오른쪽 정렬", A::Align("right")),
        (
            "배분 정렬",
            A::Or("format.align.distribute", &A::Align("distribute")),
        ),
        ("나눔 정렬", A::Or("format.align.split", &A::Align("split"))),
    ],
);
const FORMAT: &[Item] = &[
    Item::StyleGallery,
    Stack(&[
        &[I(
            Icon::Style,
            "스타일 추가하기",
            A::Ext("format.style.create"),
            false,
        )],
        &[I(
            Icon::StyleGallery,
            "스타일마당",
            A::Ext("format.style.gallery"),
            false,
        )],
    ]),
    Sep,
    Big(
        Icon::Character,
        "글자\n모양",
        A::Or("format.character_dialog", &A::Character),
        false,
    ),
    Stack(&[
        &[
            W("format.font_lang", 91.0),
            W("format.font_face", 150.0),
            W("format.font_size", 106.0),
            I(Icon::Prev, "이전 글꼴", A::Ext("format.font.prev"), false),
            I(Icon::Next, "다음 글꼴", A::Ext("format.font.next"), false),
        ],
        &[
            W("format.bold", 23.0),
            W("format.italic", 23.0),
            W("format.underline", 39.0),
            W("format.strikethrough", 39.0),
            I(
                Icon::Subscript,
                "아래 첨자",
                A::Ext("format.subscript"),
                false,
            ),
            I(
                Icon::Superscript,
                "위 첨자",
                A::Ext("format.superscript"),
                false,
            ),
            I(
                Icon::Superscript,
                "위 첨자/아래 첨자 순환",
                A::Ext("format.superscript.cycle"),
                false,
            ),
            W("format.char_border", 39.0),
            W("format.highlight", 39.0),
            W("format.text_color", 39.0),
            W("format.shade_color", 39.0),
            I(
                Icon::SpacingNarrow,
                "글자 자간 좁게",
                A::Ext("format.spacing.decrease"),
                false,
            ),
            I(
                Icon::SpacingWide,
                "글자 자간 넓게",
                A::Ext("format.spacing.increase"),
                false,
            ),
        ],
    ]),
    Sep,
    Big(
        Icon::Paragraph,
        "문단\n모양",
        A::Or("format.paragraph_dialog", &A::Paragraph),
        false,
    ),
    Item::Custom("format.align", 54.0, &ALIGN_MENU),
    Stack(&[
        &[
            I(
                Icon::MarginDecrease,
                "왼쪽 여백 줄이기",
                A::Ext("format.margin_left.decrease"),
                false,
            ),
            I(
                Icon::MarginIncrease,
                "왼쪽 여백 늘리기",
                A::Ext("format.margin_left.increase"),
                false,
            ),
            I(
                Icon::DropCap,
                "문단 첫 글자 장식",
                A::Ext("format.drop_cap"),
                false,
            ),
        ],
        &[
            I(
                Icon::IndentFirst,
                "첫 줄 들여쓰기",
                A::Ext("format.indent.positive"),
                false,
            ),
            I(
                Icon::OutdentFirst,
                "첫 줄 내어쓰기",
                A::Ext("format.indent.negative"),
                false,
            ),
            I(
                Icon::IndentFirst,
                "현재 위치로 들여쓰기",
                A::Ext("format.indent.at_caret"),
                false,
            ),
        ],
    ]),
    Stack(&[
        &[I(
            Icon::Prev,
            "문단 위로",
            A::Ext("format.paragraph.up"),
            false,
        )],
        &[I(
            Icon::Next,
            "문단 아래로",
            A::Ext("format.paragraph.down"),
            false,
        )],
    ]),
    Sep,
    Item::Custom(
        "format.outline.toggle",
        38.0,
        &Big(Icon::Outline, "개요", A::Ext("format.outline.toggle"), true),
    ),
    Stack(&[
        &[
            W("format.bullet.toggle", 39.0),
            I(
                Icon::PictureBullet,
                "그림 글머리표",
                A::Ext("format.picture_bullet"),
                true,
            ),
        ],
        &[
            W("format.numbering.toggle", 39.0),
            I(
                Icon::NumberRestart,
                "문단 번호 새 번호로 시작",
                A::Ext("format.numbering.restart"),
                false,
            ),
        ],
    ]),
    Stack(&[
        &[I(
            Icon::LevelUp,
            "한 수준 증가",
            A::Ext("format.level.increase"),
            false,
        )],
        &[I(
            Icon::LevelDown,
            "한 수준 감소",
            A::Ext("format.level.decrease"),
            false,
        )],
    ]),
];
const PAGE: &[Item] = &[
    Big(Icon::PageSetup, "편집\n용지", A::Page, false),
    Big(Icon::Portrait, "세로", A::Portrait, false),
    Big(Icon::Landscape, "가로", A::Landscape, false),
    Item::Menu(Icon::PageMargins, "쪽\n여백", MARGIN_MENU),
    Sep,
    Big(
        Icon::TextDirection,
        "글자\n방향",
        A::Ext("page.text_direction"),
        true,
    ),
    Sep,
    Big(Icon::MasterPage, "바탕쪽", A::Ext("page.master"), false),
    Big(
        Icon::Header,
        "머리말",
        A::Or("page.header", &A::HeaderFooter(true)),
        true,
    ),
    Big(
        Icon::Footer,
        "꼬리말",
        A::Or("page.footer", &A::HeaderFooter(false)),
        true,
    ),
    Big(
        Icon::PageBorder,
        "쪽 테두리/\n배경",
        A::Ext("page.border_fill"),
        false,
    ),
    Big(
        Icon::PageNumber,
        "쪽 번호\n매기기",
        A::Or("page.number", &A::PageNumber),
        false,
    ),
    Big(
        Icon::NewNumber,
        "새 번호로\n시작",
        A::Ext("page.new_number"),
        false,
    ),
    Big(
        Icon::HidePage,
        "현재 쪽만\n감추기",
        A::Ext("page.hide"),
        false,
    ),
    Big(
        Icon::LineNumber,
        "줄\n번호",
        A::Ext("page.line_numbers"),
        true,
    ),
    Sep,
    Big(Icon::PageBreak, "쪽\n나누기", A::Break, false),
    Big(Icon::ColumnBreak, "단\n나누기", A::ColumnBreak, false),
    Big(Icon::PageDelete, "쪽\n지우기", A::Ext("page.delete"), false),
    Sep,
    Big(Icon::Columns, "단", A::Columns, true),
    Big(
        Icon::ColumnDefBreak,
        "단 설정\n나누기",
        A::Ext("page.column_def_break"),
        false,
    ),
    Sep,
    Big(
        Icon::Section,
        "구역\n설정",
        A::Ext("page.section_dialog"),
        false,
    ),
    Big(
        Icon::PageBreak,
        "구역\n나누기",
        A::Ext("page.section_break"),
        false,
    ),
    Sep,
    Big(Icon::Label, "라벨", A::Ext("page.label"), true),
    Big(Icon::Grid, "원고지", A::Ext("page.manuscript"), false),
];
const SECURITY: &[Item] = &[
    Big(
        Icon::Lock,
        "문서 암호\n설정",
        A::Ext("security.password"),
        false,
    ),
    Big(
        Icon::Lock,
        "문서 암호\n변경/해제",
        A::Ext("security.password_change"),
        false,
    ),
    Sep,
    Big(
        Icon::Lock,
        "배포용 문서로\n저장",
        A::Ext("security.distribution"),
        false,
    ),
    Big(
        Icon::Lock,
        "배포용\n문서 편집",
        A::Ext("security.distribution_edit"),
        false,
    ),
    Big(
        Icon::Lock,
        "배포용 문서 암호\n변경/해제",
        A::Ext("security.distribution_change"),
        false,
    ),
    Sep,
    Big(
        Icon::Privacy,
        "선택 글자\n보호",
        A::Ext("security.protect_text"),
        false,
    ),
    Big(
        Icon::Privacy,
        "개인 정보\n찾아서 보호",
        A::Ext("security.privacy"),
        false,
    ),
    Big(
        Icon::Privacy,
        "개인 정보\n보호 해제",
        A::Ext("security.privacy_clear"),
        false,
    ),
    Big(
        Icon::Privacy,
        "개인 정보 보호\n암호 변경",
        A::Ext("security.privacy_password"),
        false,
    ),
    Sep,
    Big(
        Icon::Lock,
        "문서 보안\n설정",
        A::Ext("security.settings"),
        false,
    ),
];
const REVIEW: &[Item] = &[
    Big(
        Icon::ProofMark,
        "교정\n부호",
        A::Ext("review.proof_marks"),
        true,
    ),
    Sep,
    Big(
        Icon::History,
        "문서 이력\n관리",
        A::Ext("review.history"),
        false,
    ),
    Big(Icon::Compare, "문서\n비교", A::Ext("review.compare"), false),
    Sep,
    Big(
        Icon::TrackChanges,
        "변경 내용\n추적",
        A::Ext("review.track_changes"),
        true,
    ),
    Big(
        Icon::Balloons,
        "안내문에\n표시",
        A::Ext("review.balloons"),
        false,
    ),
    Big(
        Icon::Eye,
        "변경 내용\n보기",
        A::Ext("review.show_changes"),
        true,
    ),
    Sep,
    Big(
        Icon::PageSetup,
        "원본",
        A::Ext("review.view_original"),
        false,
    ),
    Big(
        Icon::TrackChanges,
        "원본 및\n변경 내용",
        A::Ext("review.view_original_marked"),
        false,
    ),
    Big(
        Icon::TrackChanges,
        "최종본 및\n변경 내용",
        A::Ext("review.view_final_marked"),
        false,
    ),
    Big(
        Icon::PageSetup,
        "최종본",
        A::Ext("review.view_final"),
        false,
    ),
    Sep,
    Big(Icon::Accept, "적용", A::Ext("review.accept"), true),
    Big(Icon::Reject, "적용\n안 함", A::Ext("review.reject"), true),
    Stack(&[
        &[I(Icon::Prev, "이전", A::Ext("review.prev"), false)],
        &[I(Icon::Next, "다음", A::Ext("review.next"), false)],
    ]),
    Sep,
    Big(Icon::Memo, "새 메모", A::Ext("insert.memo"), false),
    Big(Icon::MemoList, "메모", A::Ext("memo.list"), true),
];
/// 블록 계산: the cell-block commands of the 표 레이아웃 tab (usable inside a table).
const BLOCK_CALC: &[(&str, Action)] = &[
    ("블록 합계", A::Ext("table.block_sum")),
    ("블록 평균", A::Ext("table.block_average")),
    ("블록 곱", A::Ext("table.block_product")),
];
const TOOLS: &[Item] = &[
    Big(Icon::Index, "색인", A::Ext("tools.index"), true),
    Big(Icon::Toc, "제목\n차례", A::Ext("tools.toc"), true),
    Sep,
    Big(Icon::Citation, "인용", A::Ext("tools.citation"), true),
    Stack(&[
        &[I(
            Icon::Bibliography,
            "참고 문헌 삽입",
            A::Ext("tools.bibliography"),
            false,
        )],
        &[I(
            Icon::Bibliography,
            "참고 문헌 스타일",
            A::Ext("tools.bibliography_style"),
            true,
        )],
        &[I(
            Icon::Bibliography,
            "참고 문헌 새로 고침",
            A::Ext("tools.bibliography_refresh"),
            false,
        )],
    ]),
    Sep,
    Item::Menu(Icon::BlockCalc, "블록\n계산", BLOCK_CALC),
    Sep,
    Big(Icon::Macro, "스크립트\n매크로", A::Ext("tools.macro"), true),
    Sep,
    Big(Icon::ProofMark, "맞춤법", A::Ext("review.spelling"), true),
    Big(
        Icon::AutoCorrect,
        "빠른\n교정",
        A::Ext("tools.autocorrect"),
        true,
    ),
    Big(Icon::Dictionary, "사전", A::Ext("tools.dictionary"), false),
    Sep,
    Big(Icon::Translate, "번역", A::Ext("tools.translate"), true),
    Sep,
    Big(
        Icon::Macro,
        "프레젠테이션",
        A::Ext("tools.presentation"),
        true,
    ),
    Sep,
    Big(Icon::Customize, "글자판", A::Ext("tools.keyboard"), true),
    Big(
        Icon::Customize,
        "사용자\n설정",
        A::Ext("tools.customize"),
        false,
    ),
    Big(
        Icon::Customize,
        "환경\n설정",
        A::Ext("tools.options"),
        false,
    ),
    Sep,
    Big(
        Icon::UserInfo,
        "개인 정보\n바꾸기",
        A::Ext("tools.user_info"),
        true,
    ),
    Sep,
    Big(
        Icon::ObjectSelect,
        "개체\n목록",
        A::Or("object.list", &A::Objects),
        false,
    ),
    Big(Icon::Find, "명령\n찾기", A::Search, false),
    Big(Icon::Help, "사용\n안내", A::Help, false),
];
const TABLE_DESIGN: &[Item] = &[
    // Hangul's order: selection, then the 모양 group (표마당·테두리·음영), drawing, caption.
    Big(
        Icon::ObjectSelect,
        "표\n선택",
        A::Ext("table.select"),
        false,
    ),
    Big(
        Icon::TableProperties,
        "표/셀\n속성",
        A::Ext("table.properties"),
        false,
    ),
    Sep,
    Big(Icon::Table, "표마당", A::Ext("table.style"), true),
    Big(
        Icon::CellBorder,
        "테두리",
        A::Ext("table.cell_border_fill"),
        true,
    ),
    Stack(&[
        &[T(
            Icon::Highlight,
            "음영 어둡게",
            A::Ext("table.cell_shade.darker"),
            false,
        )],
        &[T(
            Icon::Highlight,
            "음영 밝게",
            A::Ext("table.cell_shade.lighter"),
            false,
        )],
    ]),
    Sep,
    Big(Icon::Table, "표\n그리기", A::Ext("table.draw"), false),
    Big(
        Icon::DeleteTable,
        "표\n지우개",
        A::Ext("table.eraser"),
        false,
    ),
    Sep,
    Big(
        Icon::Caption,
        "캡션",
        A::Or("table.caption_text", &A::Ext("table.caption")),
        true,
    ),
    Big(
        Icon::DeleteTable,
        "표\n지우기",
        A::Ext("table.delete"),
        false,
    ),
];
const TABLE_LAYOUT: &[Item] = &[
    Big(
        Icon::RowAbove,
        "위쪽에\n줄 추가",
        A::TableEdit(hwpcraft_engine::TableEdit::RowAbove),
        false,
    ),
    Big(
        Icon::RowBelow,
        "아래쪽에\n줄 추가",
        A::TableEdit(hwpcraft_engine::TableEdit::RowBelow),
        false,
    ),
    Big(
        Icon::ColumnLeft,
        "왼쪽에\n칸 추가",
        A::TableEdit(hwpcraft_engine::TableEdit::ColumnLeft),
        false,
    ),
    Big(
        Icon::ColumnRight,
        "오른쪽에\n칸 추가",
        A::TableEdit(hwpcraft_engine::TableEdit::ColumnRight),
        false,
    ),
    Big(
        Icon::DeleteRow,
        "줄\n지우기",
        A::TableEdit(hwpcraft_engine::TableEdit::DeleteRow),
        false,
    ),
    Big(
        Icon::DeleteColumn,
        "칸\n지우기",
        A::TableEdit(hwpcraft_engine::TableEdit::DeleteColumn),
        false,
    ),
    Sep,
    Big(
        Icon::SplitCell,
        "셀\n나누기",
        A::Or(
            "table.split",
            &A::TableEdit(hwpcraft_engine::TableEdit::SplitMerged),
        ),
        false,
    ),
    Big(
        Icon::Merge,
        "셀\n합치기",
        A::Or(
            "table.merge",
            &A::TableEdit(hwpcraft_engine::TableEdit::MergeRight),
        ),
        false,
    ),
    Big(
        Icon::EqualHeight,
        "셀 높이를\n같게",
        A::Ext("table.equal_height"),
        false,
    ),
    Big(
        Icon::EqualWidth,
        "셀 너비를\n같게",
        A::Ext("table.equal_width"),
        false,
    ),
    Sep,
    Big(
        Icon::TextDirection,
        "글자\n방향",
        A::Ext("table.text_direction"),
        false,
    ),
    Stack(&[
        &[T(Icon::AlignLeft, "위쪽", A::Ext("table.align.top"), false)],
        &[T(
            Icon::AlignCenter,
            "가운데",
            A::Ext("table.align.center"),
            false,
        )],
        &[T(
            Icon::AlignRight,
            "아래쪽",
            A::Ext("table.align.bottom"),
            false,
        )],
    ]),
    Big(
        Icon::PageMargins,
        "셀 안\n여백",
        A::Ext("table.cell_margin"),
        false,
    ),
    Big(Icon::PageMargins, "표\n여백", A::Ext("table.margin"), false),
    Sep,
    Big(
        Icon::FitTable,
        "표\n뒤집기",
        A::Ext("table.transpose"),
        false,
    ),
    Big(
        Icon::SplitCell,
        "표\n나누기",
        A::Ext("table.split_table"),
        false,
    ),
    Big(
        Icon::Merge,
        "표\n붙이기",
        A::Ext("table.merge_table"),
        false,
    ),
    Sep,
    Big(Icon::Formula, "계산식", A::Ext("table.formula"), false),
    Stack(&[
        &[T(
            Icon::BlockCalc,
            "블록 합계",
            A::Ext("table.block_sum"),
            false,
        )],
        &[T(
            Icon::BlockCalc,
            "블록 평균",
            A::Ext("table.block_average"),
            false,
        )],
        &[T(
            Icon::BlockCalc,
            "블록 곱",
            A::Ext("table.block_product"),
            false,
        )],
    ]),
    Stack(&[
        &[T(
            Icon::Numbering,
            "1,000 단위 구분 쉼표",
            A::Ext("table.thousands"),
            false,
        )],
        &[T(
            Icon::Reject,
            "쉼표 빼기",
            A::Ext("table.thousands_remove"),
            false,
        )],
    ]),
    Sep,
    Item::Menu(
        Icon::Shape,
        "배치",
        &[
            ("글자처럼 취급", A::Ext("table.placement.inline")),
            ("어울림", A::Ext("table.placement.square")),
            ("자리 차지", A::Ext("table.placement.top_bottom")),
            ("글 앞으로", A::Ext("table.placement.front")),
            ("글 뒤로", A::Ext("table.placement.behind")),
        ],
    ),
    Big(
        Icon::Caption,
        "캡션",
        A::Or("table.caption_text", &A::Ext("table.caption")),
        true,
    ),
    Sep,
    Big(
        Icon::CellBlock,
        "셀\n블록",
        A::Ext("table.cell_block"),
        false,
    ),
    Item::Menu(Icon::CellBlock, "셀\n이동", CELL_MOVE),
    Big(
        Icon::FitTable,
        "본문 폭에\n축소",
        A::TableEdit(hwpcraft_engine::TableEdit::FitPage),
        false,
    ),
    Sep,
    Big(Icon::Sort, "정렬", A::Ext("table.sort"), false),
    Item::Menu(
        Icon::Numbering,
        "채우기",
        &[
            ("자동 채우기", A::Ext("table.auto_fill")),
            ("사용자 정의 자동 채우기", A::Ext("table.auto_fill_user")),
        ],
    ),
    Big(
        Icon::Paragraph,
        "표를\n문자열로",
        A::Ext("table.to_text"),
        false,
    ),
];
/// 그림 / 도형 상황 탭 (IDC_PictureToolBox, IDC_ShapeToolBox) — shared commands.
const PLACEMENT: &[(&str, Action)] = &[
    ("글자처럼 취급", A::Ext("object.placement.inline")),
    ("어울림", A::Ext("object.placement.square")),
    ("자리 차지", A::Ext("object.placement.top_bottom")),
    ("글 앞으로", A::Ext("object.placement.front")),
    ("글 뒤로", A::Ext("object.placement.behind")),
];
const ROTATE: &[(&str, Action)] = &[
    ("왼쪽으로 90° 회전", A::Ext("object.rotate.left")),
    ("오른쪽으로 90° 회전", A::Ext("object.rotate.right")),
    ("좌우 대칭", A::Ext("object.flip.horizontal")),
    ("상하 대칭", A::Ext("object.flip.vertical")),
];
/// 표 레이아웃 셀 이동 (table-object area).
const CELL_MOVE: &[(&str, Action)] = &[
    ("왼쪽 셀", A::Ext("table.cell_move.left")),
    ("오른쪽 셀", A::Ext("table.cell_move.right")),
    ("위 셀", A::Ext("table.cell_move.up")),
    ("아래 셀", A::Ext("table.cell_move.down")),
    ("줄의 첫 셀", A::Ext("table.cell_move.row_begin")),
    ("줄의 끝 셀", A::Ext("table.cell_move.row_end")),
    ("칸의 첫 셀", A::Ext("table.cell_move.col_top")),
    ("칸의 끝 셀", A::Ext("table.cell_move.col_bottom")),
];
const OBJECT_ALIGN: &[(&str, Action)] = &[
    ("왼쪽 맞춤", A::Ext("object.align.left")),
    ("가운데 맞춤", A::Ext("object.align.center")),
    ("오른쪽 맞춤", A::Ext("object.align.right")),
    ("위쪽 맞춤", A::Ext("object.align.top")),
    ("중간 맞춤", A::Ext("object.align.middle")),
    ("아래쪽 맞춤", A::Ext("object.align.bottom")),
    ("같은 너비", A::Ext("object.align.same_width")),
    ("같은 높이", A::Ext("object.align.same_height")),
    ("같은 크기", A::Ext("object.align.same_size")),
    ("가로 간격을 동일하게", A::Ext("object.align.space_h")),
    ("세로 간격을 동일하게", A::Ext("object.align.space_v")),
];
const OUTER_MARGIN: &[(&str, Action)] = &[
    ("없음", A::Ext("object.outer_margin.none")),
    ("좁게", A::Ext("object.outer_margin.narrow")),
    ("보통", A::Ext("object.outer_margin.normal")),
    ("넓게", A::Ext("object.outer_margin.wide")),
];
const FILL_COLORS: &[(&str, Action)] = &[
    ("검정", A::Ext("object.style.fill.black")),
    ("흰색", A::Ext("object.style.fill.white")),
    ("빨강", A::Ext("object.style.fill.red")),
    ("주황", A::Ext("object.style.fill.orange")),
    ("노랑", A::Ext("object.style.fill.yellow")),
    ("초록", A::Ext("object.style.fill.green")),
    ("파랑", A::Ext("object.style.fill.blue")),
    ("보라", A::Ext("object.style.fill.purple")),
    ("회색", A::Ext("object.style.fill.gray")),
    ("채우기 없음", A::Ext("object.style.fill.none")),
];
const OUTLINE_COLORS: &[(&str, Action)] = &[
    ("검정", A::Ext("object.style.outline.black")),
    ("흰색", A::Ext("object.style.outline.white")),
    ("빨강", A::Ext("object.style.outline.red")),
    ("주황", A::Ext("object.style.outline.orange")),
    ("노랑", A::Ext("object.style.outline.yellow")),
    ("초록", A::Ext("object.style.outline.green")),
    ("파랑", A::Ext("object.style.outline.blue")),
    ("보라", A::Ext("object.style.outline.purple")),
    ("회색", A::Ext("object.style.outline.gray")),
    ("선 없음", A::Ext("object.style.outline.none")),
];
const DASHES: &[(&str, Action)] = &[
    ("없음", A::Ext("object.style.dash.0")),
    ("실선", A::Ext("object.style.dash.1")),
    ("긴 점선", A::Ext("object.style.dash.2")),
    ("점선", A::Ext("object.style.dash.3")),
    ("일점 쇄선", A::Ext("object.style.dash.4")),
    ("이점 쇄선", A::Ext("object.style.dash.5")),
    ("긴 파선", A::Ext("object.style.dash.6")),
    ("원형 점선", A::Ext("object.style.dash.7")),
    ("이중 실선", A::Ext("object.style.dash.8")),
    ("얇고 굵은 이중선", A::Ext("object.style.dash.9")),
    ("굵고 얇은 이중선", A::Ext("object.style.dash.10")),
    ("삼중선", A::Ext("object.style.dash.11")),
    ("물결", A::Ext("object.style.dash.12")),
    ("이중 물결", A::Ext("object.style.dash.13")),
    ("두꺼운 3D", A::Ext("object.style.dash.14")),
    ("두꺼운 3D(역)", A::Ext("object.style.dash.15")),
    ("3D 단선", A::Ext("object.style.dash.16")),
    ("3D 단선(역)", A::Ext("object.style.dash.17")),
];
const PEN_WIDTHS: &[(&str, Action)] = &[
    ("0.1 mm", A::Ext("object.style.pen.0.1")),
    ("0.25 mm", A::Ext("object.style.pen.0.25")),
    ("0.5 mm", A::Ext("object.style.pen.0.5")),
    ("1 mm", A::Ext("object.style.pen.1")),
    ("2 mm", A::Ext("object.style.pen.2")),
    ("3 mm", A::Ext("object.style.pen.3")),
];
const ARROW_END: &[(&str, Action)] = &[
    ("모양 없음", A::Ext("object.style.arrow_end.0")),
    ("화살", A::Ext("object.style.arrow_end.1")),
    ("오목 화살", A::Ext("object.style.arrow_end.2")),
    ("속 빈 다이아몬드", A::Ext("object.style.arrow_end.3")),
    ("속 빈 원", A::Ext("object.style.arrow_end.4")),
    ("속 빈 사각", A::Ext("object.style.arrow_end.5")),
    ("다이아몬드", A::Ext("object.style.arrow_end.6")),
    ("원", A::Ext("object.style.arrow_end.7")),
    ("사각", A::Ext("object.style.arrow_end.8")),
];
const ARROW_START: &[(&str, Action)] = &[
    ("모양 없음", A::Ext("object.style.arrow_start.0")),
    ("화살", A::Ext("object.style.arrow_start.1")),
    ("오목 화살", A::Ext("object.style.arrow_start.2")),
    ("속 빈 다이아몬드", A::Ext("object.style.arrow_start.3")),
    ("속 빈 원", A::Ext("object.style.arrow_start.4")),
    ("속 빈 사각", A::Ext("object.style.arrow_start.5")),
    ("다이아몬드", A::Ext("object.style.arrow_start.6")),
    ("원", A::Ext("object.style.arrow_start.7")),
    ("사각", A::Ext("object.style.arrow_start.8")),
];
const SHADOW: &[(&str, Action)] = &[
    ("그림자 넣기", A::Ext("object.shadow.on")),
    ("없음", A::Ext("object.shadow.none")),
    ("왼쪽으로 이동", A::Ext("object.shadow.left")),
    ("오른쪽으로 이동", A::Ext("object.shadow.right")),
    ("위로 이동", A::Ext("object.shadow.up")),
    ("아래로 이동", A::Ext("object.shadow.down")),
    ("원래대로", A::Ext("object.shadow.reset")),
];
const CONNECTOR_TYPE: &[(&str, Action)] = &[
    ("직선", A::Ext("object.connector_type.straight")),
    ("꺾인 선", A::Ext("object.connector_type.stroke")),
    ("곡선", A::Ext("object.connector_type.arc")),
];
const TEXTBOX_ALIGN: &[(&str, Action)] = &[
    ("왼쪽 위", A::Ext("textbox.align.left_top")),
    ("가운데 위", A::Ext("textbox.align.center_top")),
    ("오른쪽 위", A::Ext("textbox.align.right_top")),
    ("왼쪽 중간", A::Ext("textbox.align.left_center")),
    ("가운데 중간", A::Ext("textbox.align.center_center")),
    ("오른쪽 중간", A::Ext("textbox.align.right_center")),
    ("왼쪽 아래", A::Ext("textbox.align.left_bottom")),
    ("가운데 아래", A::Ext("textbox.align.center_bottom")),
    ("오른쪽 아래", A::Ext("textbox.align.right_bottom")),
];
const TEXTBOX_DIRECTION: &[(&str, Action)] = &[
    ("가로쓰기", A::Ext("textbox.direction.horizontal")),
    ("세로쓰기 (영문 눕힘)", A::Ext("textbox.direction.vertical")),
    (
        "세로쓰기 (영문 세움)",
        A::Ext("textbox.direction.vertical_all"),
    ),
];
const TEXTBOX_MARGIN: &[(&str, Action)] = &[
    ("없음", A::Ext("textbox.margin.none")),
    ("좁게", A::Ext("textbox.margin.narrow")),
    ("보통", A::Ext("textbox.margin.normal")),
    ("넓게", A::Ext("textbox.margin.wide")),
];
const RECOLOR: &[(&str, Action)] = &[
    ("원래 색", A::Ext("picture.recolor.original")),
    ("회색조", A::Ext("picture.recolor.grayscale")),
    ("흑백", A::Ext("picture.recolor.black_white")),
    ("워터마크", A::Ext("picture.recolor.watermark")),
];
const AUTO_NUMBER: &[(&str, Action)] = &[
    ("쪽 번호", A::Ext("insert.auto_number.page")),
    ("전체 쪽수", A::Ext("insert.auto_number.total")),
    ("그림 번호", A::Ext("insert.auto_number.picture")),
    ("표 번호", A::Ext("insert.auto_number.table")),
    ("수식 번호", A::Ext("insert.auto_number.equation")),
];
const NOTE_SHAPE: &[(&str, Action)] = &[
    ("각주/미주 모양", A::Ext("note.shape")),
    ("각주 모양", A::Ext("note.shape.footnote")),
    ("미주 모양", A::Ext("note.shape.endnote")),
];
const PICTURE: &[Item] = &[
    Big(
        Icon::Image,
        "그림
속성",
        A::Ext("object.properties"),
        false,
    ),
    Sep,
    Big(
        Icon::Caption,
        "캡션
넣기",
        A::Ext("object.caption"),
        true,
    ),
    Item::Menu(Icon::Shape, "배치", PLACEMENT),
    Item::Menu(Icon::Windows, "회전", ROTATE),
    Sep,
    Stack(&[
        &[T(
            Icon::Image,
            "그림 파일로 저장",
            A::Ext("picture.save"),
            false,
        )],
        &[T(
            Icon::Image,
            "모든 그림 저장",
            A::Ext("picture.save_all"),
            false,
        )],
    ]),
    Item::Menu(Icon::ViewColor, "색\n조정", RECOLOR),
    Sep,
    Item::Menu(Icon::AlignLeft, "맞춤", OBJECT_ALIGN),
    Big(
        Icon::ObjectProtect,
        "크기\n고정",
        A::Ext("object.protect_size"),
        false,
    ),
    Item::Menu(Icon::PageMargins, "바깥\n여백", OUTER_MARGIN),
    Stack(&[
        &[T(
            Icon::FormatCopy,
            "모양 복사",
            A::Ext("object.format.copy"),
            false,
        )],
        &[T(
            Icon::Paste,
            "모양 붙이기",
            A::Ext("object.format.paste"),
            false,
        )],
    ]),
    Stack(&[
        &[T(Icon::Next, "다음 개체", A::Ext("object.next"), false)],
        &[T(Icon::Prev, "이전 개체", A::Ext("object.prev"), false)],
    ]),
    Sep,
    Big(Icon::Reject, "지우기", A::Ext("object.delete"), false),
    Big(
        Icon::ObjectSelect,
        "개체\n목록",
        A::Or("object.list", &A::Objects),
        false,
    ),
];
const SHAPE: &[Item] = &[
    Big(
        Icon::Shape,
        "도형
속성",
        A::Ext("object.properties"),
        false,
    ),
    Sep,
    Big(
        Icon::Caption,
        "캡션
넣기",
        A::Ext("object.caption"),
        true,
    ),
    Item::Menu(Icon::Shape, "배치", PLACEMENT),
    Item::Menu(Icon::Windows, "회전", ROTATE),
    Sep,
    Big(Icon::ObjectSelect, "묶기", A::Ext("object.group"), false),
    Big(Icon::ObjectSelect, "풀기", A::Ext("object.ungroup"), false),
    Stack(&[
        &[
            T(
                Icon::LevelUp,
                "맨 앞으로",
                A::Ext("object.z_order.front"),
                false,
            ),
            T(
                Icon::LevelUp,
                "앞으로",
                A::Ext("object.z_order.forward"),
                false,
            ),
        ],
        &[
            T(
                Icon::LevelDown,
                "맨 뒤로",
                A::Ext("object.z_order.back"),
                false,
            ),
            T(
                Icon::LevelDown,
                "뒤로",
                A::Ext("object.z_order.backward"),
                false,
            ),
        ],
    ]),
    Sep,
    Item::Menu(Icon::Highlight, "채우기\n색", FILL_COLORS),
    Item::Menu(Icon::TextColor, "선 색", OUTLINE_COLORS),
    Item::Menu(Icon::Line, "선\n종류", DASHES),
    Item::Menu(Icon::Line, "선\n굵기", PEN_WIDTHS),
    Item::Menu(Icon::Arc, "끝\n화살표", ARROW_END),
    Item::Menu(Icon::Arc, "시작\n화살표", ARROW_START),
    Item::Menu(Icon::Shape, "그림자", SHADOW),
    Item::Menu(Icon::Connector, "연결선\n종류", CONNECTOR_TYPE),
    Sep,
    Item::Menu(Icon::AlignCenter, "글상자\n정렬", TEXTBOX_ALIGN),
    Item::Menu(Icon::TextDirection, "글자\n방향", TEXTBOX_DIRECTION),
    Item::Menu(Icon::PageMargins, "글상자\n여백", TEXTBOX_MARGIN),
    Sep,
    Item::Menu(Icon::AlignLeft, "맞춤", OBJECT_ALIGN),
    Big(
        Icon::ObjectProtect,
        "크기\n고정",
        A::Ext("object.protect_size"),
        false,
    ),
    Item::Menu(Icon::PageMargins, "바깥\n여백", OUTER_MARGIN),
    Stack(&[
        &[T(
            Icon::FormatCopy,
            "모양 복사",
            A::Ext("object.format.copy"),
            false,
        )],
        &[T(
            Icon::Paste,
            "모양 붙이기",
            A::Ext("object.format.paste"),
            false,
        )],
    ]),
    Stack(&[
        &[T(Icon::Next, "다음 개체", A::Ext("object.next"), false)],
        &[T(Icon::Prev, "이전 개체", A::Ext("object.prev"), false)],
    ]),
    Sep,
    Big(Icon::Reject, "지우기", A::Ext("object.delete"), false),
    Big(
        Icon::ObjectSelect,
        "개체
목록",
        A::Or("object.list", &A::Objects),
        false,
    ),
];
/// Hangul-style file menu, in order. `None` is a separator.
const FILE_MENU: &[Option<(&str, &str, Action)>] = &[
    Some(("새 문서", "Alt+N", A::New)),
    Some(("문서마당", "Ctrl+Alt+N", A::Ext("file.template"))),
    Some(("불러오기", "Alt+O", A::Open)),
    Some(("끼워 넣기", "", A::Ext("file.insert"))),
    None,
    Some(("저장하기", "Alt+S", A::Save)),
    Some(("다른 이름으로 저장하기", "Alt+V", A::SaveAs)),
    Some(("블록 저장하기", "", A::Ext("file.save_block"))),
    Some(("PDF로 저장하기", "", A::Pdf)),
    Some(("그림 파일로 저장하기", "", A::Ext("file.save_picture"))),
    Some(("HML로 저장하기", "", A::Ext("file.save_hml"))),
    Some(("현재 쪽만 저장하기", "", A::Ext("file.save_pages"))),
    Some(("텍스트로 내보내기", "", A::Text)),
    None,
    Some(("문서 정보", "Ctrl+Q,I", A::Ext("file.doc_info"))),
    None,
    Some(("편집 용지", "F7", A::Page)),
    Some(("미리 보기", "", A::Ext("file.preview"))),
    Some(("인쇄", "Alt+P", A::Ext("file.print"))),
    None,
    Some(("최근 문서", "", A::Ext("file.recent"))),
    None,
    Some(("문서 닫기", "Ctrl+F4", A::New)),
    Some(("끝", "Alt+X", A::Ext("file.exit"))),
];
/// 파일 tab command list: the 파일 menu without 최근 문서 (listed in full beside it),
/// with no doubled or trailing separators left behind.
fn backstage_entries() -> Vec<Option<(&'static str, &'static str, Action)>> {
    let mut out: Vec<Option<(&'static str, &'static str, Action)>> = Vec::new();
    for entry in FILE_MENU {
        match entry {
            Some((_, _, A::Ext("file.recent"))) => {}
            None if out.last().is_none_or(Option::is_none) => {}
            entry => out.push(*entry),
        }
    }
    if out.last().is_some_and(Option::is_none) {
        out.pop();
    }
    out
}
/// (tab label, mnemonic, items). Index 0 is the 파일 menu; 9–10 appear inside tables.
const TABS: &[(&str, char, &[Item])] = &[
    ("파일", 'F', &[]),
    ("편집", 'E', EDIT),
    ("보기", 'U', VIEW),
    ("입력", 'D', INSERT),
    ("서식", 'J', FORMAT),
    ("쪽", 'W', PAGE),
    ("보안", 'R', SECURITY),
    ("검토", 'H', REVIEW),
    ("도구", 'K', TOOLS),
    ("표 디자인", 'Y', TABLE_DESIGN),
    ("표 레이아웃", 'L', TABLE_LAYOUT),
    ("그림", 'P', PICTURE),
    ("도형", 'X', SHAPE),
];
/// Space after a column of check boxes (보기 tab), so neighbouring columns do not touch.
const CHECK_GAP: f32 = 10.0;
/// Context tabs shown only while a table cell or an object is current.
const CONTEXT_TABS: std::ops::RangeInclusive<usize> = 9..=12;
const ACCENT: Color32 = Color32::from_rgb(0, 150, 250);
const CONTEXT: Color32 = Color32::from_rgb(32, 139, 121);
const LINE: Color32 = Color32::from_rgb(229, 229, 229);

impl HwpApp {
    pub(super) fn canvas_context_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            for (label, key) in [("가", "bold"), ("기울임", "italic"), ("밑줄", "underline")]
            {
                if ui.button(label).clicked() {
                    self.dispatch(A::Char(key), &ctx);
                    ui.close();
                }
            }
        });
        ui.separator();
        let mut items: Vec<(&str, Action)> = vec![
            ("오려 두기", A::Cut),
            ("복사하기", A::Copy),
            ("붙이기", A::Paste),
            ("글자 모양", A::Or("format.character_dialog", &A::Character)),
            ("문단 모양", A::Or("format.paragraph_dialog", &A::Paragraph)),
            ("스타일", A::Or("format.style.dialog", &A::Styles)),
            ("하이퍼링크", A::Link),
            ("개체 목록", A::Or("object.list", &A::Objects)),
            ("쪽 복사하기", A::Ext("page.copy")),
            ("쪽 붙이기", A::Ext("page.paste")),
            ("쪽 지우기", A::Ext("page.delete")),
        ];
        if self.session.cursor.cell.is_some() {
            items.extend([
                ("표 선택 / 크기 조절", A::Ext("table.select")),
                ("표/셀 속성", A::Ext("table.properties")),
                ("셀 안 여백", A::Ext("table.cell_margin")),
                ("셀 테두리/배경", A::Ext("table.cell_border_fill")),
                ("셀 블록", A::Ext("table.cell_block")),
                (
                    "셀 합치기",
                    A::Or(
                        "table.merge",
                        &A::TableEdit(hwpcraft_engine::TableEdit::MergeRight),
                    ),
                ),
                (
                    "셀 나누기",
                    A::Or(
                        "table.split",
                        &A::TableEdit(hwpcraft_engine::TableEdit::SplitMerged),
                    ),
                ),
                (
                    "위쪽에 줄 추가하기",
                    A::TableEdit(hwpcraft_engine::TableEdit::RowAbove),
                ),
                (
                    "아래쪽에 줄 추가하기",
                    A::TableEdit(hwpcraft_engine::TableEdit::RowBelow),
                ),
                (
                    "왼쪽에 칸 추가하기",
                    A::TableEdit(hwpcraft_engine::TableEdit::ColumnLeft),
                ),
                (
                    "오른쪽에 칸 추가하기",
                    A::TableEdit(hwpcraft_engine::TableEdit::ColumnRight),
                ),
                (
                    "줄 지우기",
                    A::TableEdit(hwpcraft_engine::TableEdit::DeleteRow),
                ),
                (
                    "칸 지우기",
                    A::TableEdit(hwpcraft_engine::TableEdit::DeleteColumn),
                ),
            ]);
        }
        for (i, (label, action)) in items.into_iter().enumerate() {
            if i == 3 || i == 8 {
                ui.separator();
            }
            if ui
                .add_enabled(self.enabled(action), egui::Button::new(label))
                .clicked()
            {
                self.dispatch(action, &ctx);
                ui.close();
            }
        }
    }
    fn enabled(&self, action: Action) -> bool {
        let free_text = self.session.cursor.cell.is_none();
        match action {
            A::Equation | A::Note(_) | A::Shape(_) => free_text && !self.session.has_selection(),
            A::Ext(id) => self.ext_enabled(id),
            A::Or(id, base) => self.ext_enabled(id) || self.enabled(*base),
            A::Undo => self.session.can_undo(),
            A::Redo => self.session.can_redo(),
            A::Cut | A::Copy => self.session.has_selection(),
            A::Table | A::Break | A::Columns | A::ColumnBreak | A::Picture => free_text,
            A::TableEdit(_) => !free_text,
            A::PasteFormat => self.format_clipboard.is_some(),
            _ => true,
        }
    }
    /// A ribbon drop-down is usable when at least one of its entries is.
    fn menu_enabled(&self, list: &[(&str, Action)]) -> bool {
        list.iter().any(|(_, action)| self.enabled(*action))
    }
    fn checked(&self, action: Action) -> bool {
        match action {
            A::Rulers => self.show_rulers,
            A::Navigation => self.show_navigation,
            A::Editor => self.show_editor,
            A::Char(key) => match key {
                "bold" => self.bold,
                "italic" => self.italic,
                "underline" => self.underline,
                _ => false,
            },
            A::Align(align) => self.alignment == align,
            A::Portrait | A::Landscape => self
                .session
                .page_setup()
                .map(|p| p.landscape == matches!(action, A::Landscape))
                .unwrap_or(false),
            A::Ext(id) => self.pen_checked(id) || self.shell_ext_checked(id),
            A::Or(id, base) => self.shell_ext_checked(id) || self.checked(*base),
            A::ShowUnsupported => self.ext.shell.show_unsupported,
            _ => false,
        }
    }
    fn dispatch(&mut self, action: Action, ctx: &egui::Context) {
        use super::feature_dialogs::FeatureDialog;
        use super::object_dialogs::ObjectDialog;
        match action {
            A::Or(id, base) => {
                if self.ext_enabled(id) {
                    self.ext_dispatch(id, ctx);
                } else {
                    self.dispatch(*base, ctx);
                }
            }
            A::Ext(id) => self.ext_dispatch(id, ctx),
            A::Char(key) => {
                let value = !self.checked(action);
                self.command(Command::Character(json!({ key: value })));
            }
            A::Align(align) => self.command(Command::Paragraph(json!({"alignment":align}))),
            A::Equation => {
                self.feature_dialog = Some(FeatureDialog::Object(ObjectDialog::equation()))
            }
            A::Note(endnote) => {
                self.feature_dialog = Some(FeatureDialog::Object(ObjectDialog::Note {
                    address: None,
                    endnote,
                    text: String::new(),
                }))
            }
            A::Shape(kind) => {
                self.feature_dialog = Some(FeatureDialog::Object(ObjectDialog::shape(kind)))
            }
            A::Objects => self.feature_dialog = Some(FeatureDialog::Object(ObjectDialog::Objects)),
            A::Styles => self.feature_dialog = Some(FeatureDialog::Object(ObjectDialog::Styles)),
            A::Symbols => self.open_symbol_dialog(),
            A::HeaderFooter(is_header) => self.open_header_footer_editor(is_header, 0),
            A::PageNumber => {
                self.run("insert.page_number", json!({}));
            }
            A::New => self.request(Pending::New, ctx),
            A::Open => self.open(ctx),
            A::Save => {
                self.save(false, None);
            }
            A::SaveAs => {
                self.save(true, None);
            }
            A::Pdf => {
                self.save(true, Some("pdf"));
            }
            A::Text => {
                self.save(true, Some("txt"));
            }
            A::Undo => self.command(Command::Undo),
            A::Redo => self.command(Command::Redo),
            A::Copy | A::Cut => {
                let r = if matches!(action, A::Cut) {
                    self.session.cut()
                } else {
                    self.session.copy()
                };
                if let Some(text) = self.result(r)
                    && !text.is_empty()
                {
                    ctx.copy_text(text);
                }
                self.sync_formatting();
                self.follow_cursor();
            }
            A::Paste => {
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            A::Character => {
                self.format_draft = None;
                self.show_character = true;
            }
            A::Paragraph => {
                self.format_draft = None;
                self.show_paragraph = true;
            }
            A::Page => self.open_page_setup(),
            A::Portrait | A::Landscape => {
                if let Ok(mut p) = self.session.page_setup() {
                    p.landscape = matches!(action, A::Landscape);
                    self.command(Command::PageSetup(p));
                }
            }
            A::Table => self.show_table = true,
            A::Break => self.command(Command::PageBreak),
            A::Find => self.open_find(false),
            A::Replace => self.open_find(true),
            A::Navigation => self.show_navigation = !self.show_navigation,
            A::Editor => self.show_editor = !self.show_editor,
            A::Rulers => self.show_rulers = !self.show_rulers,
            A::ZoomIn => self.set_zoom(self.zoom + 0.1),
            A::ZoomOut => self.set_zoom(self.zoom - 0.1),
            A::Fit => self.zoom_to_fit(false),
            A::Help => self.show_help = true,
            A::TableEdit(action) => {
                self.run("table.edit", json!({"action":action}));
            }
            A::Link => {
                self.feature_dialog = Some(FeatureDialog::Link {
                    uri: String::new(),
                    text: self.session.selected_text(),
                })
            }
            A::Bookmarks => {
                self.feature_dialog = Some(FeatureDialog::Bookmarks {
                    name: String::new(),
                })
            }
            A::Columns => {
                self.feature_dialog = Some(FeatureDialog::Columns {
                    count: 2,
                    spacing: 8.0,
                })
            }
            A::ColumnBreak => {
                self.run("insert.column_break", json!({}));
            }
            A::Picture => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG 그림", &["png"])
                    .pick_file()
                {
                    self.run("insert.picture", json!({"path":path}));
                }
            }
            A::CopyFormat => self.copy_format(),
            A::PasteFormat => {
                if let Some(p) = self.format_clipboard.clone() {
                    self.command(Command::Character(p));
                }
            }
            A::Search => {
                self.feature_dialog = Some(FeatureDialog::Search {
                    query: String::new(),
                })
            }
            A::ShowUnsupported => self.toggle_unsupported_commands(),
        }
    }
    /// Run a command by id the way its ribbon or 파일 entry does, including the enabled
    /// check (a grey one only says why). False when no entry has this id.
    pub(crate) fn run_ribbon_command(&mut self, id: &str, ctx: &egui::Context) -> bool {
        let found = TABS
            .iter()
            .flat_map(|(_, _, items)| Self::tools(items))
            .chain(SHAPES.iter().map(|(i, l, a)| (*i, (*l).to_owned(), *a)))
            .chain(
                FILE_MENU
                    .iter()
                    .flatten()
                    .map(|(l, _, a)| (Icon::Eye, (*l).to_owned(), *a)),
            )
            .find(|(_, _, action)| Self::action_id(*action) == Some(id));
        let Some((_, label, action)) = found else {
            return false;
        };
        if self.enabled(action) {
            self.dispatch(action, ctx);
        } else {
            self.message = format!("{label}: {}", self.disabled_reason(action));
        }
        true
    }
    /// First ribbon tab that holds the command `id`.
    pub(crate) fn ribbon_tab_of(&self, id: &str) -> Option<usize> {
        TABS.iter().position(|(_, _, items)| {
            Self::tools(items)
                .iter()
                .any(|(_, _, a)| Self::action_id(*a) == Some(id))
        })
    }
    /// Keep the tooltip of the command named by `--view tooltip-<id>` open.
    fn pin_tip(&self, response: &egui::Response, action: Action, tip: &str) {
        if self.ext.shell.pinned_tip.as_deref().is_some()
            && self.ext.shell.pinned_tip.as_deref() == Self::action_id(action)
        {
            response.show_tooltip_text(tip.to_owned());
        }
    }
    /// A command the ribbon leaves out: unsupported, and 지원하지 않는 명령 is off.
    fn hides(&self, action: Action) -> bool {
        !self.ext.shell.show_unsupported && unsupported_reason(action).is_some()
    }
    /// Why a command is grey: not implemented, or what the user has to do first.
    fn disabled_reason(&self, action: Action) -> &'static str {
        if let Some(why) = unsupported_reason(action) {
            return why;
        }
        let in_table = self.session.cursor.cell.is_some();
        match action {
            A::Cut | A::Copy => return SELECT_TEXT,
            A::Undo => return "되돌릴 작업이 없습니다.",
            A::Redo => return "다시 실행할 작업이 없습니다.",
            A::PasteFormat => return "[모양 복사]로 글자 모양을 먼저 복사하세요.",
            A::TableEdit(_) => return IN_TABLE,
            _ => {}
        }
        let id = Self::action_id(action).unwrap_or("");
        match id {
            "edit.sort" if !in_table => "정렬할 문단들을 먼저 블록으로 선택하세요.",
            "note.convert" | "note.delete" => "각주나 미주 안에 커서를 두세요.",
            "page.delete" => "쪽이 하나뿐이라 지울 쪽이 없습니다.",
            "security.password_change" => {
                "암호가 걸린 문서가 아닙니다. [문서 암호 설정]으로 먼저 거세요."
            }
            "object.select" | "object.next" | "object.prev" | "picture.save_all" => {
                "문서에 개체가 없습니다."
            }
            "object.format.paste" => "개체의 [모양 복사]를 먼저 하세요.",
            "insert.memo" | "memo.create" if !in_table => {
                "메모는 본문의 한 문단 안에서만 만들 수 있습니다. 한 문단 안에서만 선택하세요."
            }
            "table.create" | "table.from_text" if in_table => OUT_OF_TABLE,
            "table.merge" | "table.equal_height" | "table.equal_width" if in_table => CELL_BLOCK,
            _ if id.starts_with("table.") && !in_table => IN_TABLE,
            _ if in_table && !id.starts_with("table.") => OUT_OF_TABLE,
            _ if ["object.", "picture.", "textbox."]
                .iter()
                .any(|p| id.starts_with(p)) =>
            {
                if self.object_ext_selected_kind().is_none() {
                    SELECT_OBJECT
                } else {
                    NOT_FOR_OBJECT
                }
            }
            _ if self.session.has_selection() => NO_SELECTION,
            _ => NOT_NOW,
        }
    }
    /// Tooltip of a ribbon command: its name and shortcut, and when grey, why.
    fn tip(&self, label: &str, action: Action, enabled: bool) -> String {
        let label = label.replace('\n', " ");
        // A label that already names one of the keys (진하게 (Ctrl+B)) keeps its text.
        let mut tip = match shortcut(action) {
            Some(keys) if !keys.split(" / ").any(|k| label.contains(k.trim())) => {
                format!("{label} ({keys})")
            }
            _ => label,
        };
        if !enabled {
            tip.push('\n');
            tip.push_str(self.disabled_reason(action));
        }
        tip
    }
    /// The reason on a grey list entry, check box or menu row (`add_enabled(false)`).
    fn explain(&self, response: egui::Response, action: Action, enabled: bool) -> egui::Response {
        if enabled {
            response
        } else {
            response.on_disabled_hover_text(self.disabled_reason(action))
        }
    }
    fn copy_format(&mut self) {
        let Ok(p) = self.session.character_properties() else {
            return;
        };
        let mut copy = serde_json::Map::new();
        for key in [
            "fontFamilies",
            "fontSize",
            "bold",
            "italic",
            "underline",
            "textColor",
            "strikethrough",
            "shadeColor",
            "ratios",
            "spacings",
            "relativeSizes",
            "charOffsets",
            "underlineType",
            "underlineColor",
            "strikeColor",
            "subscript",
            "superscript",
            "emboss",
            "engrave",
            "kerning",
            "outlineType",
            "shadowType",
            "shadowColor",
            "shadowOffsetX",
            "shadowOffsetY",
            "emphasisDot",
            "underlineShape",
            "strikeShape",
        ] {
            if let Some(v) = p.get(key) {
                copy.insert(key.into(), v.clone());
            }
        }
        self.format_clipboard = Some(copy.into());
        self.message = "글자 모양을 복사했습니다. 붙일 곳을 선택하고 오른쪽 클릭 메뉴나 명령 찾기에서 '글자 모양 붙이기'를 실행하세요.".into();
    }
    /// Every command reachable from the ribbon, flattened, for command search and tab menus.
    fn tools(items: &[Item]) -> Vec<(Icon, String, Action)> {
        let mut out = Vec::new();
        for item in items {
            match *item {
                Big(icon, label, action, _) => out.push((icon, label.replace('\n', " "), action)),
                Item::Menu(icon, _, list) => {
                    out.extend(list.iter().map(|(l, a)| (icon, (*l).to_owned(), *a)))
                }
                Stack(rows) => {
                    for mini in rows.iter().flat_map(|r| r.iter()) {
                        match *mini {
                            I(icon, label, action, _) | T(icon, label, action, _) => {
                                out.push((icon, label.to_owned(), action))
                            }
                            W(id, _) => {
                                if let Some((icon, label, action)) = widget_fallback(id) {
                                    out.push((icon, label.to_owned(), action));
                                }
                            }
                        }
                    }
                }
                Checks(list) => {
                    out.extend(list.iter().map(|(l, a)| (Icon::Eye, (*l).to_owned(), *a)))
                }
                Item::Shapes => {
                    out.extend(SHAPES.iter().map(|(i, l, a)| (*i, (*l).to_owned(), *a)))
                }
                Item::Custom(_, _, inner) => out.extend(Self::tools(std::slice::from_ref(inner))),
                Item::StyleGallery | Sep => {}
            }
        }
        out
    }
    pub(super) fn command_search_results(&mut self, ui: &mut egui::Ui, query: &str) -> bool {
        let ctx = ui.ctx().clone();
        let mut close = false;
        let q = query.to_lowercase();
        let mut seen = std::collections::BTreeSet::new();
        egui::ScrollArea::vertical()
            .max_height(330.0)
            .show(ui, |ui| {
                for (tab, _, items) in TABS {
                    for (_, label, action) in Self::tools(items) {
                        if matches!(action, A::Search)
                            || !label.to_lowercase().contains(&q)
                            || !seen.insert(label.clone())
                        {
                            continue;
                        }
                        let enabled = self.enabled(action);
                        let entry = ui.add_enabled(
                            enabled,
                            egui::Button::new(format!("{tab}  /  {label}"))
                                .shortcut_text(shortcut(action).unwrap_or(""))
                                .frame(false),
                        );
                        if self.explain(entry, action, enabled).clicked() {
                            self.dispatch(action, &ctx);
                            close = true;
                        }
                    }
                }
                for (label, _, action) in FILE_MENU.iter().flatten() {
                    if label.to_lowercase().contains(&q)
                        && seen.insert((*label).to_owned())
                        && ui
                            .add_enabled(
                                self.enabled(*action),
                                egui::Button::new(format!("파일  /  {label}")).frame(false),
                            )
                            .clicked()
                    {
                        self.dispatch(*action, &ctx);
                        close = true;
                    }
                }
                if "글자 모양 붙이기".contains(&q)
                    && ui
                        .add_enabled(
                            self.enabled(A::PasteFormat),
                            egui::Button::new("편집  /  글자 모양 붙이기").frame(false),
                        )
                        .clicked()
                {
                    self.dispatch(A::PasteFormat, &ctx);
                    close = true;
                }
            });
        close
    }
    /// 파일 tab: the file commands on the left (same entries and order as the 파일 menu),
    /// the document summary and recent documents on the right.
    pub(super) fn file_backstage(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::left("hwp-file-commands")
            .exact_size(280.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_gray(245))
                    .inner_margin(egui::Margin::symmetric(8, 16)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in backstage_entries() {
                        let Some((label, key, action)) = entry else {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);
                            continue;
                        };
                        let enabled = self.enabled(action);
                        let response = ui.add_enabled(
                            enabled,
                            egui::Button::new(label)
                                .shortcut_text(key)
                                .frame(false)
                                .min_size(Vec2::new(262.0, 28.0)),
                        );
                        if self.explain(response, action, enabled).clicked() {
                            self.dispatch(action, &ctx);
                        }
                    }
                });
            });
        let mut open_recent = None;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(22.0);
            ui.heading("문서 정보");
            ui.add_space(12.0);
            ui.label(
                self.session
                    .path
                    .as_ref()
                    .map(|p| super::ext_shell::recent_path(p).display().to_string())
                    .unwrap_or_else(|| "아직 저장하지 않은 문서".into()),
            );
            ui.label(format!(
                "{}쪽 · {}개 구역",
                self.layout_page_count(),
                self.session.core.document().sections.len()
            ));
            ui.label(if self.session.dirty() {
                "저장하지 않은 변경 내용이 있습니다."
            } else {
                "저장된 상태입니다."
            });
            ui.add_space(12.0);
            if ui.button("문서로 돌아가기").clicked() {
                self.tab = 1;
            }
            ui.add_space(26.0);
            ui.heading("최근 문서");
            ui.add_space(8.0);
            if self.ext.shell.recent.is_empty() {
                ui.label(
                    RichText::new("최근에 연 문서가 없습니다.").color(Color32::from_gray(110)),
                );
            }
            egui::Grid::new("hwp-file-recent")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for (i, path) in self.ext.shell.recent.iter().enumerate() {
                        let exists = path.is_file();
                        let name = super::ext_shell::recent_name(path, 32);
                        let folder = super::ext_shell::recent_folder(path, 64);
                        let (label, color) = if exists {
                            (format!("{}  {name}", i + 1), ui.visuals().text_color())
                        } else {
                            (
                                format!("{}  {name}  (찾을 수 없음)", i + 1),
                                Color32::from_gray(150),
                            )
                        };
                        let response = ui
                            .add(egui::Button::new(RichText::new(label).color(color)).frame(false));
                        ui.label(RichText::new(folder).color(Color32::from_gray(120)));
                        ui.end_row();
                        let tip = if exists {
                            path.display().to_string()
                        } else {
                            format!(
                                "파일을 찾을 수 없습니다. 누르면 목록에서 지웁니다.
{}",
                                path.display()
                            )
                        };
                        if response.on_hover_text(tip).clicked() {
                            open_recent = Some(i);
                        }
                    }
                });
        });
        if let Some(i) = open_recent {
            self.tab = 1;
            self.open_recent(i, &ctx);
        }
    }
    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.set_min_width(300.0);
        for entry in FILE_MENU {
            let Some((label, key, action)) = entry else {
                ui.separator();
                continue;
            };
            if matches!(action, A::Ext("file.recent")) {
                self.recent_menu(ui, &ctx);
                continue;
            }
            let enabled = self.enabled(*action);
            let response = ui.add_enabled(
                enabled,
                egui::Button::new(*label)
                    .shortcut_text(*key)
                    .frame(false)
                    .min_size(Vec2::new(290.0, 26.0)),
            );
            if response.clicked() {
                ui.close();
                self.dispatch(*action, &ctx);
            }
        }
    }
    /// 최근 문서 (한글 파일 메뉴의 최근 문서 목록, 최대 9개).
    fn recent_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let recent = self.ext.shell.recent.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button("최근 문서", |ui| {
                ui.set_min_width(320.0);
                let mut open = None;
                for (i, path) in recent.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let label = format!("{} {name}", i + 1);
                    if ui
                        .add(
                            egui::Button::new(label)
                                .frame(false)
                                .min_size(Vec2::new(300.0, 26.0)),
                        )
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        open = Some(i);
                    }
                }
                if let Some(i) = open {
                    ui.close();
                    self.open_recent(i, ctx);
                }
            });
        });
    }
    /// Run the ribbon item of the current tab that owns `id` (Alt key tips).
    pub(crate) fn ribbon_keytip(&mut self, id: &str, ctx: &egui::Context) {
        let Some((_, _, items)) = TABS.get(self.tab) else {
            return;
        };
        let found = Self::tools(items)
            .into_iter()
            .map(|(_, _, action)| action)
            .chain(SHAPES.iter().map(|(_, _, a)| *a))
            .find(|action| Self::action_id(*action) == Some(id));
        match found {
            Some(action) if self.enabled(action) => self.dispatch(action, ctx),
            Some(_) => self.message = format!("{id}: 지금은 쓸 수 없는 명령입니다."),
            None => self.message = format!("{id}: 이 탭에 없는 명령입니다."),
        }
    }
    /// Command id of a ribbon action, for Alt key tips and coverage mapping.
    fn action_id(action: Action) -> Option<&'static str> {
        Some(match action {
            A::Ext(id) | A::Or(id, _) => id,
            A::New => "file.new",
            A::Open => "file.open",
            A::Save => "file.save",
            A::SaveAs => "file.save_as",
            A::Pdf => "file.save_pdf",
            A::Text => "file.save_text",
            A::Undo => "edit.undo",
            A::Redo => "edit.redo",
            A::Cut => "edit.cut",
            A::Copy => "edit.copy",
            A::Paste => "edit.paste",
            A::Character => "format.character_dialog",
            A::Paragraph => "format.paragraph_dialog",
            A::Page => "page.setup_dialog",
            A::Portrait => "page.portrait",
            A::Landscape => "page.landscape",
            A::Table => "table.create",
            A::Break => "insert.page_break",
            A::Find => "edit.find",
            A::Replace => "edit.replace",
            A::Navigation => "view.task_pane",
            A::Editor => "view.editor",
            A::Rulers => "view.rulers",
            A::ZoomIn => "view.zoom_in",
            A::ZoomOut => "view.zoom_out",
            A::Fit => "view.zoom_fit_page",
            A::Help => "help",
            A::Link => "insert.hyperlink",
            A::Bookmarks => "insert.bookmark",
            A::Columns => "page.columns",
            A::ColumnBreak => "insert.column_break",
            A::Picture => "insert.picture_dialog",
            A::CopyFormat => "format.copy_shape",
            A::PasteFormat => "format.paste_shape",
            A::Search => "help.search",
            A::HeaderFooter(true) => "page.header",
            A::HeaderFooter(false) => "page.footer",
            A::PageNumber => "page.number",
            A::Equation => "insert.equation",
            A::Note(false) => "insert.footnote",
            A::Note(true) => "insert.endnote",
            A::Objects => "object.list",
            A::Styles => "format.style.dialog",
            A::Symbols => "insert.symbol",
            A::Char("bold") => "format.bold",
            A::Char("italic") => "format.italic",
            A::Char("underline") => "format.underline",
            A::Char("strikethrough") => "format.strikethrough",
            A::Shape(_) | A::TableEdit(_) | A::Char(_) | A::Align(_) | A::ShowUnsupported => {
                return None;
            }
        })
    }
    /// Draw the Alt key tip for an action over its widget when tips are showing.
    fn keytip_badge(&self, ui: &egui::Ui, rect: Rect, action: Action) {
        if self.ext.shell.keytips != super::ext_shell::KeyTips::Items {
            return;
        }
        let Some(letter) =
            Self::action_id(action).and_then(|id| super::ext_shell::alt_letter(self.tab, id))
        else {
            return;
        };
        keytip(
            ui,
            Pos2::new(rect.left() + 2.0, rect.bottom() - 16.0),
            letter,
        );
    }
    fn tab_menu(&mut self, ui: &mut egui::Ui, index: usize) {
        let ctx = ui.ctx().clone();
        let Some((_, _, items)) = TABS.get(index) else {
            return;
        };
        ui.set_min_width(220.0);
        for (icon, label, action) in Self::tools(items) {
            if self.hides(action) {
                continue;
            }
            let enabled = self.enabled(action);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                self.icons.paint(ui, rect, icon, enabled);
                let entry = ui.add_enabled(
                    enabled,
                    egui::Button::new(label)
                        .shortcut_text(shortcut(action).unwrap_or(""))
                        .frame(false),
                );
                if self.explain(entry, action, enabled).clicked() {
                    ui.close();
                    self.dispatch(action, &ctx);
                }
            });
        }
    }
    fn tab_visible(&self, index: usize, in_table: bool) -> bool {
        match index {
            9 | 10 => in_table,
            11 => self.ext.shell.object_kind == Some("picture"),
            12 => self.ext.shell.object_kind.is_some_and(|k| k != "picture"),
            _ => true,
        }
    }
    fn menu_tabs(&mut self, ui: &mut egui::Ui) {
        let in_table = self.session.cursor.cell.is_some();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (index, (name, key, _)) in TABS.iter().enumerate() {
                if !self.tab_visible(index, in_table) {
                    continue;
                }
                let selected = self.tab == index;
                let color = if selected {
                    ACCENT
                } else if CONTEXT_TABS.contains(&index) {
                    CONTEXT
                } else {
                    Color32::from_gray(34)
                };
                let font = egui::FontId::proportional(13.0);
                let text = ui
                    .painter()
                    .layout_no_wrap((*name).to_owned(), font.clone(), color);
                let menu_part = if index == 0 { 0.0 } else { 18.0 };
                let width = if index == 0 {
                    65.0
                } else {
                    (text.size().x + 22.0 + menu_part).max(50.0)
                };
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(width, 29.0), egui::Sense::click());
                let label_rect = Rect::from_min_max(
                    rect.min,
                    Pos2::new(rect.right() - menu_part, rect.bottom()),
                );
                if response.hovered() {
                    ui.painter().rect_filled(rect, 2.0, Color32::from_gray(242));
                }
                ui.painter().galley(
                    Pos2::new(
                        label_rect.center().x - text.size().x / 2.0,
                        rect.center().y - text.size().y / 2.0,
                    ),
                    text,
                    color,
                );
                if index > 0 {
                    icons::chevron(
                        ui,
                        Pos2::new(rect.right() - 12.0, rect.center().y + 1.0),
                        Color32::from_gray(90),
                    );
                }
                if selected {
                    ui.painter().line_segment(
                        [
                            rect.left_bottom() + Vec2::new(2.0, -1.0),
                            rect.right_bottom() + Vec2::new(-2.0, -1.0),
                        ],
                        Stroke::new(2.0, ACCENT),
                    );
                }
                let response = response.on_hover_text(format!("{name} (Alt+{key})"));
                if self.ext.shell.keytips != super::ext_shell::KeyTips::Off && index <= 8 {
                    keytip(
                        ui,
                        Pos2::new(label_rect.center().x - 7.0, rect.bottom() - 15.0),
                        *key,
                    );
                }
                let pointer = response.interact_pointer_pos();
                let on_menu_part = pointer.is_some_and(|p| p.x > label_rect.right());
                let popup = egui::Id::new(("hwp-tab-menu", index));
                if response.clicked() {
                    if index == 0 || on_menu_part {
                        egui::Popup::toggle_id(ui.ctx(), popup);
                    } else {
                        self.tab = index;
                        self.show_ribbon = true;
                    }
                }
                egui::Popup::from_response(&response)
                    .id(popup)
                    .open_memory(None)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        if index == 0 {
                            self.file_menu(ui);
                        } else {
                            self.tab_menu(ui, index);
                        }
                    });
                if index == 0 {
                    ui.add_space(4.0);
                    let x = ui.cursor().min.x;
                    ui.painter().line_segment(
                        [
                            Pos2::new(x, rect.top() + 7.0),
                            Pos2::new(x, rect.bottom() - 7.0),
                        ],
                        Stroke::new(1.0, Color32::from_gray(200)),
                    );
                    ui.add_space(4.0);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add_space(8.0);
                let collapse = if self.show_ribbon { "︿" } else { "﹀" };
                if ui
                    .add(egui::Button::new(collapse).frame(false))
                    .on_hover_text("기본 도구 상자 접기/펴기 (Ctrl+F1)")
                    .clicked()
                {
                    self.show_ribbon = !self.show_ribbon;
                }
                // Hangul's quick search box: white, grey outline, ∨ at the right.
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(160.0, 24.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, Color32::from_gray(200)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.left_center() + Vec2::new(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "찾을 내용",
                    egui::FontId::proportional(12.0),
                    Color32::from_gray(170),
                );
                icons::chevron(
                    ui,
                    rect.right_center() - Vec2::new(10.0, 0.0),
                    Color32::from_gray(110),
                );
                if response.on_hover_text("명령 찾기").clicked() {
                    self.dispatch(A::Search, &ui.ctx().clone());
                }
            });
        });
    }
    /// Layout width of an item at a ribbon level (full, compact, collapsed).
    fn item_width(ui: &egui::Ui, item: &Item, hide: bool) -> [f32; 3] {
        let shown = |a: &Action| !(hide && unsupported_reason(*a).is_some());
        let font = egui::FontId::proportional(12.0);
        let text = |s: &str| {
            ui.painter()
                .layout_no_wrap(s.to_owned(), font.clone(), Color32::BLACK)
                .size()
                .x
        };
        let big = |label: &str, menu: bool| {
            let lines: Vec<&str> = label.split('\n').collect();
            let w = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    text(l)
                        + if menu && i + 1 == lines.len() && lines.len() > 1 {
                            10.0
                        } else {
                            0.0
                        }
                })
                .fold(0.0_f32, f32::max);
            (w + 12.0).max(38.0).round()
        };
        let mini = |m: &Mini| match *m {
            I(_, _, a, _) | T(_, _, a, _) if !shown(&a) => 0.0,
            I(_, _, _, menu) => 23.0 + if menu { 16.0 } else { 0.0 },
            T(_, label, _, menu) => 29.0 + text(label) + if menu { 16.0 } else { 0.0 },
            W(_, width) => width + 4.0,
        };
        match item {
            Big(_, label, _, menu) => [big(label, *menu), 34.0, 34.0],
            Item::Menu(_, label, _) => [big(label, true), 34.0, 34.0],
            Stack(rows) => {
                if let Some(I(_, label, _, menu) | T(_, label, _, menu)) = lone_mini(rows, hide) {
                    return [big(label, menu), 34.0, 34.0];
                }
                let w = rows
                    .iter()
                    .map(|r| r.iter().map(mini).sum::<f32>())
                    .fold(0.0_f32, f32::max);
                [w, w, w]
            }
            Checks(list) => {
                let w = list
                    .iter()
                    .filter(|(_, a)| shown(a))
                    .map(|(l, _)| text(l) + 24.0)
                    .fold(0.0_f32, f32::max)
                    + CHECK_GAP;
                [w, w, w]
            }
            Item::Shapes => [139.0, 139.0, 139.0],
            Item::StyleGallery => [407.0, 200.0, 200.0],
            Item::Custom(_, width, _) => [*width, 34.0, 34.0],
            Sep => [13.0, 13.0, 13.0],
        }
    }
    fn ribbon(&mut self, ui: &mut egui::Ui) {
        let Some((_, _, items)) = TABS.get(self.tab) else {
            return;
        };
        // Groups are the runs between separators; WordCraft's planner shrinks from the right.
        let hide = !self.ext.shell.show_unsupported;
        let groups = ribbon_groups(items, hide);
        let widths: Vec<[f32; 3]> = groups
            .iter()
            .map(|g| {
                let mut w = [13.0_f32; 3];
                for item in g.iter() {
                    let iw = Self::item_width(ui, item, hide);
                    for level in 0..3 {
                        w[level] += iw[level];
                    }
                }
                w[2] = 54.0;
                w
            })
            .collect();
        let levels = super::ribbon_layout::plan(&widths, ui.available_width() - 16.0);
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            // Hangul packs ribbon buttons edge to edge; groups are 13 px apart.
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            ui.add_space(14.0);
            for (gi, (group, level)) in groups.iter().zip(levels).enumerate() {
                if gi > 0 {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(13.0, icons::LARGE_HEIGHT),
                        egui::Sense::hover(),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(rect.center().x, rect.top() + 2.0),
                            Pos2::new(rect.center().x, rect.bottom() - 2.0),
                        ],
                        Stroke::new(1.0, LINE),
                    );
                }
                ui.push_id((self.tab, gi), |ui| {
                    if level == super::ribbon_layout::COLLAPSED {
                        self.collapsed_group(ui, group);
                    } else {
                        for item in group.iter() {
                            self.ribbon_item(ui, item, level == super::ribbon_layout::FULL);
                        }
                    }
                });
            }
            if hide {
                self.unsupported_chip(ui, items);
            }
        });
    }
    /// 지원 안 함 n: what this tab leaves out, with the reasons and the switch to show them.
    fn unsupported_chip(&mut self, ui: &mut egui::Ui, items: &[Item]) {
        let list = unsupported_in(items);
        if list.is_empty() {
            return;
        }
        ui.add_space(13.0);
        let response = ui
            .vertical(|ui| {
                ui.add_space(4.0);
                ui.add(
                    egui::Button::new(
                        RichText::new(format!("지원 안 함 {}", list.len()))
                            .size(11.0)
                            .color(Color32::from_gray(90)),
                    )
                    .fill(Color32::from_gray(240))
                    .corner_radius(8.0),
                )
            })
            .inner
            .on_hover_text("이 탭에서 숨긴, 아직 지원하지 않는 명령");
        let popup = egui::Id::new(("hwp-unsupported", self.tab));
        if response.clicked() {
            egui::Popup::toggle_id(ui.ctx(), popup);
        }
        egui::Popup::from_response(&response)
            .id(popup)
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_max_width(420.0);
                ui.label(RichText::new("이 탭에서 숨긴 명령").strong());
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        egui::Grid::new("hwp-unsupported-list")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                for (label, why) in &list {
                                    ui.label(label);
                                    ui.add(egui::Label::new(
                                        RichText::new(*why).color(Color32::from_gray(100)),
                                    ));
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                let mut show = self.ext.shell.show_unsupported;
                if ui
                    .checkbox(&mut show, "지원하지 않는 명령도 리본에 보이기")
                    .clicked()
                {
                    ui.close();
                    self.toggle_unsupported_commands();
                }
            });
    }
    fn collapsed_group(&mut self, ui: &mut egui::Ui, group: &[Item]) {
        let ctx = ui.ctx().clone();
        let Some((icon, label)) = group.iter().find_map(|i| match *i {
            Big(icon, label, _, _) | Item::Menu(icon, label, _) => Some((icon, label)),
            _ => None,
        }) else {
            return;
        };
        let label = label.split('\n').next().unwrap_or(label);
        let response = self.icons.large(ui, icon, label, true, true, false);
        let response = response.on_hover_text(label);
        let popup = egui::Id::new(("hwp-group", label, self.tab));
        if response.clicked() {
            egui::Popup::toggle_id(&ctx, popup);
        }
        egui::Popup::from_response(&response)
            .id(popup)
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                for (icon, label, action) in Self::tools(group) {
                    if self.hides(action) {
                        continue;
                    }
                    let enabled = self.enabled(action);
                    let tip = self.tip(&label, action, enabled);
                    if self
                        .icons
                        .small(
                            ui,
                            icon,
                            Some(&label),
                            &tip,
                            enabled,
                            false,
                            self.checked(action),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.dispatch(action, &ctx);
                    }
                }
            });
    }
    fn ribbon_item(&mut self, ui: &mut egui::Ui, item: &Item, full: bool) {
        let ctx = ui.ctx().clone();
        match *item {
            Big(icon, label, action, menu) => {
                let enabled = self.enabled(action);
                let checked = self.checked(action);
                let tip = self.tip(label, action, enabled);
                let response = if full {
                    let response = self.icons.large(ui, icon, label, enabled, menu, checked);
                    self.pin_tip(&response, action, &tip);
                    response.on_hover_text(tip)
                } else {
                    ui.vertical(|ui| {
                        ui.add_space(26.0);
                        self.icons
                            .small(ui, icon, None, &tip, enabled, false, checked)
                    })
                    .inner
                };
                self.keytip_badge(ui, response.rect, action);
                if response.clicked() {
                    self.dispatch(action, &ctx);
                }
            }
            Item::Menu(icon, label, list) => {
                // Grey out (and ignore clicks on) a menu whose every entry is unavailable.
                let any = self.menu_enabled(list);
                let mut tip = label.replace('\n', " ");
                if !any && let Some((_, first)) = list.iter().find(|(_, a)| !self.hides(*a)) {
                    tip = format!("{tip}\n{}", self.disabled_reason(*first));
                }
                let response = self
                    .icons
                    .large(ui, icon, label, any, true, false)
                    .on_hover_text(tip);
                let popup = egui::Id::new(("hwp-menu", label, self.tab));
                if response.clicked() {
                    egui::Popup::toggle_id(&ctx, popup);
                }
                egui::Popup::from_response(&response)
                    .id(popup)
                    .open_memory(None)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        for (label, action) in list {
                            if self.hides(*action) {
                                continue;
                            }
                            let enabled = self.enabled(*action);
                            let entry = ui.add_enabled(
                                enabled,
                                egui::Button::selectable(self.checked(*action), *label)
                                    .shortcut_text(shortcut(*action).unwrap_or("")),
                            );
                            if self.explain(entry, *action, enabled).clicked() {
                                ui.close();
                                self.dispatch(*action, &ctx);
                            }
                        }
                    });
            }
            Stack(rows) => {
                if let Some(I(icon, label, action, menu) | T(icon, label, action, menu)) =
                    lone_mini(rows, !self.ext.shell.show_unsupported)
                {
                    return self.ribbon_item(ui, &Big(icon, label, action, menu), full);
                }
                let pitch = if rows.len() >= 3 { 26.0 } else { 40.0 };
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = pitch - icons::SMALL;
                    if rows.len() < 3 {
                        ui.add_space(6.0);
                    }
                    for row in rows {
                        if row.iter().all(|m| match *m {
                            I(_, _, a, _) | T(_, _, a, _) => self.hides(a),
                            W(..) => false,
                        }) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for mini in row.iter() {
                                self.mini(ui, mini);
                            }
                        });
                    }
                });
            }
            Checks(list) => {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.add_space(3.0);
                    for (label, action) in list {
                        if self.hides(*action) {
                            continue;
                        }
                        let enabled = self.enabled(*action);
                        let mut on = self.checked(*action);
                        let response =
                            ui.add_enabled(enabled, egui::Checkbox::new(&mut on, *label));
                        let response = if enabled {
                            match shortcut(*action) {
                                Some(key) => response.on_hover_text(format!("{label} ({key})")),
                                None => response,
                            }
                        } else {
                            response.on_disabled_hover_text(self.tip(label, *action, false))
                        };
                        self.keytip_badge(
                            ui,
                            response.rect.translate(Vec2::new(-4.0, 6.0)),
                            *action,
                        );
                        if response.clicked() {
                            self.dispatch(*action, &ctx);
                        }
                    }
                });
                // Keep the next check column's boxes clear of this column's labels.
                ui.add_space(CHECK_GAP);
            }
            Item::Shapes => self.shape_gallery(ui),
            Item::StyleGallery => self.style_gallery(ui, if full { 407.0 } else { 200.0 }),
            Item::Custom(id, width, inner) => {
                let size = if full {
                    Vec2::new(width, icons::LARGE_HEIGHT)
                } else {
                    Vec2::new(width, icons::SMALL)
                };
                if self.format_ext_widget(ui, id, size).is_none() {
                    self.ribbon_item(ui, inner, full);
                }
            }
            Sep => {}
        }
    }
    fn mini(&mut self, ui: &mut egui::Ui, mini: &Mini) {
        let ctx = ui.ctx().clone();
        let (icon, label, tooltip, action, menu) = match *mini {
            I(icon, tip, action, menu) => (icon, None, tip, action, menu),
            T(icon, label, action, menu) => (icon, Some(label), label, action, menu),
            W(id, width) => {
                self.format_widget(ui, id, Vec2::new(width, icons::SMALL));
                ui.add_space(4.0);
                return;
            }
        };
        if self.hides(action) {
            return;
        }
        let enabled = self.enabled(action);
        let tip = self.tip(tooltip, action, enabled);
        let response = self
            .icons
            .small(ui, icon, label, &tip, enabled, menu, self.checked(action));
        self.pin_tip(&response, action, &tip);
        self.keytip_badge(ui, response.rect, action);
        if response.clicked() {
            self.dispatch(action, &ctx);
        }
    }
    fn shape_gallery(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(139.0, icons::LARGE_HEIGHT), egui::Sense::hover());
        ui.painter().rect_stroke(
            Rect::from_min_size(rect.min + Vec2::new(0.0, 2.0), Vec2::new(122.0, 72.0)),
            0.0,
            Stroke::new(1.0, LINE),
            egui::StrokeKind::Inside,
        );
        let shapes: Vec<_> = SHAPES.iter().filter(|(_, _, a)| !self.hides(*a)).collect();
        for (i, (icon, label, action)) in shapes.into_iter().enumerate() {
            let cell = Rect::from_min_size(
                rect.min + Vec2::new(1.0 + (i % 5) as f32 * 24.0, 2.0 + (i / 5) as f32 * 24.0),
                Vec2::splat(24.0),
            );
            let enabled = self.enabled(*action);
            let tip = self.tip(label, *action, enabled);
            let response = ui
                .interact(
                    cell,
                    egui::Id::new(("hwp-shape", i)),
                    if enabled {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    },
                )
                .on_hover_text(tip);
            if response.hovered() && enabled {
                ui.painter()
                    .rect_filled(cell, 0.0, Color32::from_rgb(229, 241, 251));
            }
            self.icons.paint(ui, cell.shrink(3.0), *icon, enabled);
            if response.clicked() {
                self.dispatch(*action, &ctx);
            }
        }
        // Hangul's scroll arrows beside the gallery; this gallery has one page.
        for (row, up) in [(0.0, true), (25.0, false)] {
            let r = Rect::from_min_size(rect.min + Vec2::new(123.0, row), Vec2::new(16.0, 25.0));
            let c = r.center();
            let s = if up { -2.0 } else { 2.0 };
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(-4.0, -s),
                    c + Vec2::new(4.0, -s),
                    c + Vec2::new(0.0, s),
                ],
                Color32::from_gray(150),
                Stroke::NONE,
            ));
        }
    }
    fn style_gallery(&mut self, ui: &mut egui::Ui, width: f32) {
        let current = self
            .session
            .paragraph(self.session.cursor)
            .map(|p| p.style_id as u64)
            .unwrap_or(0);
        let styles = self.session.styles();
        let list: Vec<(u64, String)> = styles
            .as_array()
            .into_iter()
            .flatten()
            .take(4)
            .filter_map(|s| Some((s["id"].as_u64()?, s["name"].as_str()?.to_owned())))
            .collect();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(width, icons::LARGE_HEIGHT), egui::Sense::hover());
        let gallery = Rect::from_min_size(rect.min, Vec2::new(width - 17.0, icons::LARGE_HEIGHT));
        ui.painter().rect_stroke(
            gallery,
            0.0,
            Stroke::new(1.0, LINE),
            egui::StrokeKind::Inside,
        );
        let cell = Vec2::new((gallery.width() - 2.0) / 2.0, 37.0);
        for (i, (id, name)) in list.into_iter().enumerate() {
            let r = Rect::from_min_size(
                gallery.min
                    + Vec2::new(1.0 + (i % 2) as f32 * cell.x, 1.0 + (i / 2) as f32 * cell.y),
                cell,
            );
            let response = ui.interact(r, egui::Id::new(("hwp-style", i)), egui::Sense::click());
            if id == current {
                ui.painter().rect_filled(r, 0.0, Color32::from_gray(222));
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(r, 0.0, Color32::from_rgb(229, 241, 251));
            }
            ui.painter().text(
                r.left_center() + Vec2::new(34.0, 0.0),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(13.0),
                Color32::from_gray(20),
            );
            self.icons.paint(
                ui,
                Rect::from_center_size(r.left_center() + Vec2::new(16.0, 0.0), Vec2::splat(16.0)),
                Icon::Paragraph,
                true,
            );
            if response.clicked() {
                self.run("style.apply", json!({"id":id}));
            }
        }
        let ctx = ui.ctx().clone();
        let more = Rect::from_min_max(Pos2::new(gallery.right(), rect.top()), rect.max);
        let response = ui
            .interact(more, egui::Id::new("hwp-style-more"), egui::Sense::click())
            .on_hover_text("스타일 목록");
        icons::chevron(ui, more.center(), Color32::from_gray(90));
        if response.clicked() {
            self.dispatch(A::Or("format.style.dialog", &A::Styles), &ctx);
        }
    }
    /// Combo boxes and split buttons. Drawn locally until the format module provides them.
    fn format_widget(&mut self, ui: &mut egui::Ui, id: &str, size: Vec2) {
        if self.format_ext_widget(ui, id, size).is_some() {
            return;
        }
        let ctx = ui.ctx().clone();
        match id {
            "format.style" => {
                let style_id = self
                    .session
                    .paragraph(self.session.cursor)
                    .map(|p| p.style_id as usize)
                    .unwrap_or(0);
                let styles = self.session.styles();
                let name = styles[style_id]["name"]
                    .as_str()
                    .unwrap_or("스타일")
                    .to_owned();
                egui::ComboBox::from_id_salt("document-style")
                    .width(size.x - 8.0)
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for s in styles.as_array().into_iter().flatten() {
                            if ui
                                .selectable_label(
                                    s["id"].as_u64() == Some(style_id as u64),
                                    s["name"].as_str().unwrap_or("이름 없음"),
                                )
                                .clicked()
                            {
                                self.run("style.apply", json!({"id":s["id"]}));
                            }
                        }
                    });
            }
            "format.font_lang" => {
                egui::ComboBox::from_id_salt("font-lang")
                    .width(size.x - 8.0)
                    .selected_text("대표")
                    .show_ui(ui, |ui| {
                        for lang in [
                            "대표",
                            "한글",
                            "영문",
                            "한자",
                            "일어",
                            "외국어",
                            "기호",
                            "사용자",
                        ] {
                            ui.add_enabled(
                                lang == "대표",
                                egui::Button::selectable(lang == "대표", lang),
                            );
                        }
                    })
                    .response
                    .on_hover_text("언어별 글꼴은 글자 모양 창에서 바꿉니다.");
            }
            "format.font_face" => {
                let mut family = self.font_family.clone();
                egui::ComboBox::from_id_salt("font-family")
                    .width(size.x - 8.0)
                    .selected_text(&self.font_family)
                    .show_ui(ui, |ui| {
                        for font in [
                            "함초롬바탕",
                            "함초롬돋움",
                            "맑은 고딕",
                            "바탕",
                            "돋움",
                            "굴림",
                            "궁서",
                            "Arial",
                            "Times New Roman",
                        ] {
                            ui.selectable_value(&mut family, font.into(), font);
                        }
                    });
                if family != self.font_family {
                    self.command(Command::Character(json!({"fontFamily":family})));
                }
            }
            "format.font_size" => {
                let response = ui.add_sized(
                    size,
                    egui::DragValue::new(&mut self.font_size)
                        .range(1.0..=4096.0)
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" pt"),
                );
                if response.changed() {
                    self.command(Command::Character(
                        json!({"fontSize":(self.font_size*100.0).round() as u32}),
                    ));
                }
            }
            "format.line_spacing" => {
                let percent = self.line_spacing_type == "Percent";
                let response = ui
                    .add_sized(
                        size,
                        egui::DragValue::new(&mut self.line_spacing)
                            .range(if percent { 50.0..=500.0 } else { 1.0..=200.0 })
                            .speed(1.0)
                            .suffix(if percent { " %" } else { " pt" }),
                    )
                    .on_hover_text("줄 간격");
                if response.changed() {
                    self.command(Command::Paragraph(json!({"lineSpacing":
                        (self.line_spacing * if percent { 1.0 } else { 100.0 }).round() as i32})));
                }
            }
            _ => {
                let Some((icon, label, action)) = widget_fallback(id) else {
                    return;
                };
                let menu = size.x > 30.0;
                let enabled = self.enabled(action);
                if self
                    .icons
                    .small(ui, icon, None, label, enabled, menu, self.checked(action))
                    .clicked()
                {
                    self.dispatch(action, &ctx);
                }
            }
        }
    }
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let white = egui::Frame::NONE.fill(Color32::WHITE);
        let in_table = self.session.cursor.cell.is_some();
        let kind = self.object_ext_selected_kind();
        if kind != self.ext.shell.object_kind {
            self.ext.shell.object_kind = kind;
            // Hangul opens the object's context tab when it is selected.
            match kind {
                Some("picture") => self.tab = 11,
                Some(_) => self.tab = 12,
                None => {}
            }
        }
        if !self.tab_visible(self.tab, in_table) {
            self.tab = 1;
        }
        egui::Panel::top("hwp-menus")
            .exact_size(33.0)
            .frame(white.inner_margin(egui::Margin {
                left: 1,
                right: 0,
                top: 4,
                bottom: 0,
            }))
            .show(ui, |ui| self.menu_tabs(ui));
        if self.show_ribbon && self.tab != 0 {
            egui::Panel::top("hwp-ribbon")
                .exact_size(88.0)
                .frame(white)
                .show(ui, |ui| self.ribbon(ui));
        }
        egui::Panel::top("hwp-format-bar")
            .exact_size(32.0)
            .frame(
                white
                    .inner_margin(egui::Margin {
                        left: 18,
                        right: 0,
                        top: 4,
                        bottom: 0,
                    })
                    .stroke(Stroke::new(1.0, Color32::from_rgb(210, 210, 210))),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (icon, label, action, menu) in [
                        (Icon::New, "새 문서 (Alt+N)", A::New, true),
                        (Icon::Open, "불러오기 (Alt+O)", A::Open, true),
                        (Icon::Save, "저장하기 (Alt+S)", A::Save, true),
                        (Icon::Print, "인쇄 (Alt+P)", A::Ext("file.print"), false),
                        (Icon::Preview, "미리 보기", A::Ext("file.preview"), false),
                        (Icon::Undo, "되돌리기 (Ctrl+Z)", A::Undo, true),
                        (Icon::Redo, "다시 실행 (Ctrl+Shift+Z)", A::Redo, true),
                    ] {
                        let enabled = self.enabled(action);
                        if self
                            .icons
                            .small(ui, icon, None, label, enabled, menu, false)
                            .clicked()
                        {
                            self.dispatch(action, &ctx);
                        }
                    }
                    for (id, width) in [
                        ("format.style", 106.0),
                        ("format.font_lang", 91.0),
                        ("format.font_face", 150.0),
                        ("format.font_size", 106.0),
                        ("format.bold", 24.0),
                        ("format.italic", 24.0),
                        ("format.underline", 40.0),
                        ("format.strikethrough", 40.0),
                        ("format.text_color", 40.0),
                        ("format.shade_color", 40.0),
                        ("format.align.justify", 24.0),
                        ("format.align.left", 24.0),
                        ("format.align.center", 24.0),
                        ("format.align.right", 24.0),
                        ("format.align.distribute", 24.0),
                        ("format.align.split", 24.0),
                        ("format.line_spacing", 123.0),
                    ] {
                        self.format_widget(ui, id, Vec2::new(width, icons::SMALL));
                    }
                });
            });
    }

    pub(super) fn editing_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_table {
            let mut close = false;
            let mut create = false;
            let r = chrome::modal("table-create").show(ctx, |ui| {
                ui.set_width(580.0);
                close = chrome::title(ui, "표 만들기");
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(468.0);
                        chrome::section(ui, "줄 / 칸");
                        ui.horizontal(|ui| {
                            ui.label("줄 수");
                            ui.add(egui::DragValue::new(&mut self.rows).range(1..=30));
                            ui.add_space(20.0);
                            ui.label("칸 수");
                            ui.add(egui::DragValue::new(&mut self.columns).range(1..=12));
                        });
                        chrome::section(ui, "미리 보기");
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(450.0, 170.0), egui::Sense::hover());
                        let rect = rect.shrink(10.0);
                        for row in 0..=self.rows.min(10) {
                            let y =
                                rect.top() + row as f32 * rect.height() / self.rows.min(10) as f32;
                            ui.painter().line_segment(
                                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                                Stroke::new(1.0, Color32::GRAY),
                            );
                        }
                        for col in 0..=self.columns {
                            let x = rect.left() + col as f32 * rect.width() / self.columns as f32;
                            ui.painter().line_segment(
                                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                                Stroke::new(1.0, Color32::GRAY),
                            );
                        }
                        ui.label("너비: 본문 폭에 맞춤 · 셀 크기: 동일");
                    });
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        let (a, c) = chrome::actions(ui, true, "만들기");
                        create = a;
                        close |= c;
                    });
                });
            });
            if create {
                close = self.run(
                    "insert.table",
                    json!({"rows":self.rows,"columns":self.columns}),
                );
            }
            if close || r.should_close() {
                self.show_table = false;
            }
        }
        self.format_dialogs(ctx);
    }
}

/// Plain button standing in for a format widget: (icon, tooltip, action).
fn widget_fallback(id: &str) -> Option<(Icon, &'static str, Action)> {
    Some(match id {
        "format.bold" => (Icon::Bold, "진하게 (Ctrl+B)", A::Char("bold")),
        "format.italic" => (Icon::Italic, "기울임 (Ctrl+I)", A::Char("italic")),
        "format.underline" => (Icon::Underline, "밑줄 (Ctrl+U)", A::Char("underline")),
        "format.strikethrough" => (
            Icon::Strike,
            "취소선",
            A::Or("format.strikethrough", &A::Char("strikethrough")),
        ),
        "format.text_color" => (Icon::TextColor, "글자 색", A::Ext("format.text_color")),
        "format.highlight" => (Icon::Highlight, "형광펜", A::Ext("format.highlight")),
        "format.char_border" => (
            Icon::CharBorder,
            "글자 테두리",
            A::Ext("format.char_border"),
        ),
        "format.bullet.toggle" => (Icon::Bullet, "글머리표", A::Ext("format.bullet.toggle")),
        "format.numbering.toggle" => (
            Icon::Numbering,
            "문단 번호",
            A::Ext("format.numbering.toggle"),
        ),
        "format.align.justify" => (Icon::Justify, "양쪽 정렬", A::Align("justify")),
        "format.align.left" => (Icon::AlignLeft, "왼쪽 정렬", A::Align("left")),
        "format.align.center" => (Icon::AlignCenter, "가운데 정렬", A::Align("center")),
        "format.align.right" => (Icon::AlignRight, "오른쪽 정렬", A::Align("right")),
        "format.align.distribute" => (
            Icon::Distribute,
            "배분 정렬",
            A::Or("format.align.distribute", &A::Align("distribute")),
        ),
        "format.align.split" => (
            Icon::SplitAlign,
            "나눔 정렬",
            A::Or("format.align.split", &A::Align("split")),
        ),
        _ => return None,
    })
}

/// Alt key tip badge: 14 px dark square with one letter, like Hangul's key tips.
fn keytip(ui: &egui::Ui, at: Pos2, letter: char) {
    let rect = Rect::from_min_size(at, Vec2::splat(14.0));
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, Color32::from_rgb(60, 60, 60));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::proportional(10.0),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same chain as `ext_enabled`, but distinguishing "no module owns this id".
    fn owned(app: &HwpApp, id: &str) -> bool {
        matches!(id, "edit.regions" | "tools.autotext.body")
            || app.pen_enabled(id).is_some()
            || app.core_ext_enabled(id).is_some()
            || app.format_ext_enabled(id).is_some()
            || app.table_ext_enabled(id).is_some()
            || app.comments_ext_enabled(id).is_some()
            || app.contexts_ext_enabled(id).is_some()
            || app.object_ext_enabled(id).is_some()
            || app.shell_ext_enabled(id).is_some()
            || app.tools_ext_enabled(id).is_some()
    }

    #[test]
    fn file_tab_lists_file_commands_without_recent_or_double_separators() {
        let entries = backstage_entries();
        assert!(entries.first().is_some_and(Option::is_some));
        assert!(entries.last().is_some_and(Option::is_some));
        assert!(entries.windows(2).all(|w| w[0].is_some() || w[1].is_some()));
        let ids: Vec<_> = entries
            .iter()
            .flatten()
            .filter_map(|(_, _, a)| HwpApp::action_id(*a))
            .collect();
        assert!(!ids.contains(&"file.recent"));
        for id in [
            "file.new",
            "file.open",
            "file.save",
            "file.save_as",
            "file.print",
        ] {
            assert!(ids.contains(&id), "{id}");
        }
    }

    #[test]
    fn region_editors_share_one_edit_tab_menu() {
        let menus: Vec<&[(&str, Action)]> = EDIT
            .iter()
            .filter_map(|item| match item {
                Item::Menu(_, _, list) => Some(*list),
                _ => None,
            })
            .filter(|list| {
                list.iter()
                    .any(|(_, a)| matches!(a, A::Ext("edit.regions" | "edit.contexts")))
            })
            .collect();
        assert_eq!(menus.len(), 1);
        let ids: Vec<_> = HwpApp::tools(EDIT)
            .into_iter()
            .filter_map(|(_, _, a)| HwpApp::action_id(a))
            .filter(|id| matches!(*id, "edit.regions" | "edit.contexts"))
            .collect();
        assert_eq!(ids, ["edit.regions", "edit.contexts"]);
    }

    #[test]
    fn unsupported_reasons_name_only_unimplemented_commands() {
        let app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let mut table = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        enter_table(&mut table);
        // A command that gained a handler must lose its "not supported" tooltip. An
        // owner that keeps an id always grey (the format area's own list) has not.
        for (id, _) in UNSUPPORTED {
            assert!(
                !(owned(&app, id) && (app.ext_enabled(id) || table.ext_enabled(id))),
                "{id} is implemented now; remove it from UNSUPPORTED"
            );
        }
        // Every ribbon command nobody implements explains why it is grey.
        for (tab, _, items) in TABS {
            for (_, label, action) in HwpApp::tools(items) {
                if let A::Ext(id) = action
                    && !owned(&app, id)
                {
                    assert!(
                        unsupported_reason(action).is_some(),
                        "{tab} {label} ({id}) is always grey without a reason"
                    );
                }
            }
        }
        for (_, _, action) in FILE_MENU.iter().flatten() {
            if let A::Ext(id) = action
                && !owned(&app, id)
                && *id != "file.recent"
            {
                assert!(unsupported_reason(*action).is_some(), "file menu {id}");
            }
        }
    }

    #[test]
    fn hidden_unsupported_commands_leave_no_empty_groups() {
        let shown = |items: &[Item]| -> Vec<&'static str> {
            ribbon_groups(items, true)
                .iter()
                .flat_map(|g| HwpApp::tools(g))
                .filter_map(|(_, _, a)| HwpApp::action_id(a))
                .collect()
        };
        for (tab, _, items) in TABS {
            let groups = ribbon_groups(items, true);
            assert!(groups.iter().all(|g| !g.is_empty()), "{tab}");
            // Every whole item left holds at least one supported command.
            assert!(groups.iter().flatten().all(|i| !item_hidden(i)), "{tab}");
            let all = ribbon_groups(items, false);
            let runs = items.split(|i| matches!(i, Sep)).filter(|g| !g.is_empty());
            assert_eq!(all.len(), runs.count(), "{tab}");
        }
        // 도구: the reference tools go entirely; 블록 계산 (supported now) and help stay.
        let tools = shown(TOOLS);
        assert!(!tools.contains(&"tools.citation") && !tools.contains(&"tools.macro"));
        assert!(tools.contains(&"table.block_sum") && tools.contains(&"help"));
        assert_eq!(ribbon_groups(TOOLS, true).len(), 2);
        // The 지원 안 함 list names each hidden command once, with its reason.
        let list = unsupported_in(REVIEW);
        assert!(list.iter().all(|(_, why)| !why.is_empty()));
        assert_eq!(
            list.len(),
            HwpApp::tools(REVIEW)
                .iter()
                .filter_map(|(_, _, a)| unsupported_reason(*a).and(HwpApp::action_id(*a)))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
        let format: Vec<_> = unsupported_in(FORMAT).into_iter().map(|(l, _)| l).collect();
        assert_eq!(format, ["스타일마당", "문단 첫 글자 장식", "그림 글머리표"]);
        assert!(unsupported_in(TABLE_LAYOUT).is_empty());
    }

    /// Put the caret in the first cell of a new 2×2 table.
    fn enter_table(app: &mut HwpApp) {
        app.session
            .execute(Command::Table {
                rows: 2,
                columns: 2,
            })
            .unwrap();
        let cell = (0..4)
            .flat_map(|paragraph| (0..3).map(move |control| (paragraph, control)))
            .map(|(paragraph, control)| hwpcraft_engine::Cursor {
                section: 0,
                paragraph,
                offset: 0,
                cell: Some(hwpcraft_engine::Cell {
                    control,
                    index: 0,
                    paragraph: 0,
                }),
            })
            .find(|at| app.session.select(*at, false).is_ok());
        assert!(cell.is_some() && app.session.cursor.cell.is_some());
    }

    #[test]
    fn grey_commands_say_why_and_tooltips_show_shortcuts() {
        let blank = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let mut table = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        enter_table(&mut table);
        let mut grey = 0;
        for app in [&blank, &table] {
            for (tab, _, items) in TABS {
                for (_, label, action) in HwpApp::tools(items) {
                    if app.enabled(action) || unsupported_reason(action).is_some() {
                        continue;
                    }
                    grey += 1;
                    // A supported command says what to do, never "not supported yet".
                    let why = app.disabled_reason(action);
                    assert!(
                        why != NOT_NOW && !why.contains("지원하지"),
                        "{tab} {label}: {why}"
                    );
                    assert!(app.tip(&label, action, false).ends_with(why));
                }
            }
        }
        assert!(grey > 40, "{grey}");
        assert_eq!(blank.disabled_reason(A::Copy), SELECT_TEXT);
        assert_eq!(table.disabled_reason(A::Table), OUT_OF_TABLE);
        assert_eq!(blank.disabled_reason(A::Ext("table.merge")), IN_TABLE);
        assert_eq!(table.disabled_reason(A::Ext("table.merge")), CELL_BLOCK);
        assert_eq!(blank.disabled_reason(A::Ext("object.lock")), SELECT_OBJECT);
        // Shortcuts from the command list, once, after the name.
        assert_eq!(blank.tip("오려\n두기", A::Cut, true), "오려 두기 (Ctrl+X)");
        assert_eq!(
            blank.tip("하이퍼링크", A::Link, true),
            "하이퍼링크 (Ctrl+K,H)"
        );
        assert_eq!(
            blank.tip("진하게 (Ctrl+B)", A::Char("bold"), true),
            "진하게 (Ctrl+B)"
        );
        assert_eq!(blank.tip("바탕쪽", A::Ext("page.master"), true), "바탕쪽");
    }

    #[test]
    fn a_stack_thinned_to_one_tool_becomes_a_big_button() {
        let stack = INSERT
            .iter()
            .find_map(|item| match item {
                Stack(rows)
                    if HwpApp::tools(std::slice::from_ref(item)).len() == 2
                        && rows
                            .iter()
                            .flat_map(|r| r.iter())
                            .any(|m| matches!(m, T(_, "한자 입력", ..))) =>
                {
                    Some(*rows)
                }
                _ => None,
            })
            .expect("입력 도우미 / 한자 입력 stack");
        assert!(matches!(
            lone_mini(stack, true),
            Some(T(_, "한자 입력", ..))
        ));
        assert!(lone_mini(stack, false).is_none());
        // A full stack keeps its rows.
        let notes = INSERT.iter().find_map(|item| match item {
            Stack(rows) if rows.len() == 3 => Some(*rows),
            _ => None,
        });
        assert!(notes.is_some_and(|rows| lone_mini(rows, true).is_none()));
    }

    #[test]
    fn block_calc_and_new_chart_are_ribbon_commands_now() {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        let ids: Vec<_> = HwpApp::tools(TOOLS)
            .into_iter()
            .filter_map(|(_, _, a)| HwpApp::action_id(a))
            .collect();
        assert!(ids.contains(&"table.block_average") && !ids.contains(&"table.block_calc"));
        // Body caret: the new-chart window opens; block calculation waits for a table.
        assert!(app.enabled(A::Ext("insert.chart")));
        assert!(unsupported_reason(A::Ext("insert.chart")).is_none());
        assert!(!app.menu_enabled(BLOCK_CALC));
        enter_table(&mut app);
        assert!(app.menu_enabled(BLOCK_CALC));
    }

    #[test]
    fn menu_buttons_follow_their_entries() {
        let app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        // Body caret in a new document: page margins apply, text-box commands do not.
        assert!(app.menu_enabled(MARGIN_MENU));
        assert!(!app.menu_enabled(TEXTBOX_ALIGN));
        assert!(!app.menu_enabled(TEXTBOX_DIRECTION));
        assert!(!app.menu_enabled(CONNECTOR_TYPE));
        assert!(!app.menu_enabled(&[]));
    }
}
