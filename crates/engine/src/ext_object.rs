//! Owner: table/object area. 개체: 그림·도형·글상자·수식·각주/미주·메모·차트·글맵시.
//! Other areas must not edit this file; request changes through the owner.
//!
//! 개체 속성 follows RHWP property JSON (HWPUNIT sizes, degrees, BGR integers for
//! drawing colors). Every key is validated before the document changes.
use super::*;
use hwpcraft_doc::model::{paragraph::Paragraph, shape::ShapeObject};
use serde::Deserialize;
use serde_json::Map;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("picture.insert", "그림 넣기 (PNG·JPG·BMP·GIF)", "Ctrl+N,I"),
    ("object.properties.get", "개체 속성 조회", ""),
    ("object.properties", "개체 속성", "P"),
    (
        "shape.insert",
        "도형 넣기 (직선·사각형·타원·호·다각형·곡선·글상자)",
        "",
    ),
    ("textbox.paragraphs.get", "글상자 문단 조회", ""),
    ("textbox.paragraphs", "글상자 여러 문단 고치기", ""),
    ("note.paragraphs.get", "각주·미주 문단 조회", ""),
    ("note.paragraphs", "각주·미주 여러 문단 고치기", ""),
    (
        "note.insert",
        "각주·미주 넣기 (여러 문단)",
        "Ctrl+N,N / Ctrl+N,E",
    ),
    ("equation.insert", "수식 넣기 (색·기준선·글꼴)", "Ctrl+N,M"),
    ("object.caption_text", "그림·개체 캡션 내용 고치기", ""),
    ("object.caption_text.get", "그림·개체 캡션 내용", ""),
    (
        "equation.layout",
        "수식 조각 배치와 스크립트 위치 (시각 편집)",
        "",
    ),
    ("equation.macro.list", "수식 매크로 목록", ""),
    ("equation.macro.get", "수식 매크로 스크립트 보기", ""),
    ("equation.macro.save", "수식 매크로 저장", ""),
    ("equation.macro.delete", "수식 매크로 지우기", ""),
    ("chart.list", "차트 목록", ""),
    ("chart.data.get", "차트 데이터 조회", ""),
    ("chart.data", "차트 데이터 고치기 (값·계열 이름)", ""),
    ("object.group", "개체 묶기 (도형·그림 2개 이상)", ""),
    ("object.ungroup", "개체 풀기", ""),
    (
        "object.z_order",
        "개체 앞뒤 순서 (front·back·forward·backward)",
        "",
    ),
    ("object.lock", "개체 보호 (잠금 켜기/끄기)", ""),
    ("object.unlock_all", "모든 개체 보호 해제", ""),
    ("object.flip", "개체 대칭 (horizontal·vertical)", ""),
    ("object.line_endpoint", "직선 끝점 옮기기 (mm)", ""),
    ("insert.field", "누름틀 넣기 (안내문·이름)", "Ctrl+K,E"),
    ("field.list", "누름틀 목록", ""),
    ("field.set", "누름틀 값 고치기", ""),
    ("form.list", "양식 개체 목록", ""),
    ("form.get", "양식 개체 정보 조회", ""),
    ("form.set", "양식 개체 값 고치기 (value·text·caption)", ""),
    (
        "form.insert",
        "양식 개체 넣기 (선택 상자·입력 상자·콤보 상자·라디오 단추·명령 단추)",
        "",
    ),
    (
        "object.align",
        "개체 맞춤 (왼쪽·가운데·오른쪽·위·중간·아래, 같은 너비·높이·크기, 가로·세로 간격 동일)",
        "",
    ),
    ("object.size", "개체 너비·높이 (mm)", ""),
    ("object.rotate", "개체 회전 (각도·증감)", ""),
    ("object.protect_size", "개체 크기 고정", ""),
    ("object.outer_margin", "개체 바깥 여백 (사전 값·mm)", ""),
    (
        "object.style",
        "개체 빠른 스타일 (채우기·선 색, 선 종류·굵기, 화살표)",
        "",
    ),
    ("object.shadow", "개체 그림자 (종류·이동·없음)", ""),
    (
        "object.format.get",
        "개체 모양 복사 (선·채우기·그림자 읽기)",
        "",
    ),
    ("object.format", "개체 모양 붙이기", ""),
    ("object.neighbor", "다음·이전 개체 찾기", "Tab / Shift+Tab"),
    (
        "object.connector_type",
        "연결선 종류 바꾸기 (직선·꺾인·곡선)",
        "",
    ),
    ("textbox.align", "글상자 글자 정렬 (가로 3 × 세로 3)", ""),
    ("textbox.margin", "글상자 안 여백 (사전 값·mm)", ""),
    (
        "textbox.direction",
        "글상자 글자 방향 (가로·세로·영문 세움)",
        "",
    ),
    ("picture.save", "그림 파일로 저장", ""),
    ("picture.save_all", "모든 그림 파일로 저장", ""),
    (
        "picture.recolor",
        "그림 색 조정 (원래·회색조·흑백·워터마크)",
        "",
    ),
    (
        "picture.effects",
        "그림 효과 (그림자·네온·부드러운 가장자리·반사)",
        "",
    ),
    ("picture.effects.get", "그림 효과 조회", ""),
    (
        "insert.auto_number",
        "번호 넣기 (쪽·전체 쪽·그림·표·수식 번호)",
        "",
    ),
    ("note.convert", "각주 ↔ 미주 변환", ""),
    ("note.shape.get", "각주·미주 모양 조회 (번호·구분선)", ""),
    (
        "note.shape",
        "각주·미주 모양 (번호 모양·구분선 길이·종류·색)",
        "",
    ),
    ("note.delete", "각주·미주 지우기", ""),
    ("field.delete", "누름틀 지우기 (글자 유지)", ""),
];

/// (command id, parameter help, JSON example object) for list_commands and
/// docs/mcp-commands.md; owner fills this for every command above.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "picture.insert",
        "path(PNG·JPG·BMP·GIF, 20 MB·8192 px 이하), width_mm(없으면 원본 비율), treat_as_char(생략하면 true=글자처럼 취급, 0.19부터; false면 캐럿 문단 기준·단 왼쪽·어울림으로 띄움). 표 셀 안에서는 treat_as_char와 관계없이 그 셀 왼쪽 위에 어울림으로 띄움(셀 안 글자처럼 그림은 아직 지원 안 함)",
        r##"{"path":"C:/img/logo.png","width_mm":60.0}"##,
    ),
    (
        "object.properties.get",
        "address{section,paragraph,control} → kind, props",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "object.properties",
        "address, props{width, height, treatAsChar, textWrap, horz/vertRelTo, horz/vertAlign, horz/vertOffset, rotationAngle, sizeProtect, outerMargin*, caption*, 그림: crop*, brightness, contrast, effect, transparency; 도형: borderColor(BGR), borderWidth, lineType 0–17, arrowStart/End, fillType, fillBgColor, shadowType 0–8, shadowOffsetX/Y(%), tbMargin*, tbVerticalAlign}",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"props":{"rotationAngle":90}}"##,
    ),
    (
        "shape.insert",
        "kind rectangle|ellipse|line|textbox|arc|polygon|curve|connector_*, width_mm, height_mm, treat_as_char, text_wrap, points_mm[[x,y]], paragraphs[], x_mm·y_mm(종이 기준), flip_x/flip_y, start/end{address,point}, direction horizontal|vertical|vertical_all(글상자)",
        r##"{"kind":"rectangle","width_mm":40.0,"height_mm":20.0}"##,
    ),
    (
        "textbox.paragraphs.get",
        "address → paragraphs[]",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "textbox.paragraphs",
        "address, paragraphs[](1–200문단)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"paragraphs":["첫 문단","둘째 문단"]}"##,
    ),
    (
        "note.paragraphs.get",
        "address → paragraphs[] (첫 문단은 번호 뒤 글자)",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "note.paragraphs",
        "address, paragraphs[]",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"paragraphs":["주석 내용"]}"##,
    ),
    (
        "note.insert",
        "endnote(false: 각주), paragraphs[] → address",
        r##"{"endnote":false,"paragraphs":["각주 내용"]}"##,
    ),
    (
        "equation.insert",
        "script(수식 스크립트), size_pt, color(BGR), baseline, font_name → address",
        r##"{"script":"a^2+b^2=c^2","size_pt":10.0}"##,
    ),
    (
        "object.caption_text",
        "address(그림·도형·묶음·차트·OLE), text(캡션 번호 뒤 문자열, 1,000자 이하) → 문서 정보. 캡션은 object.properties hasCaption으로 먼저 넣는다. 표는 table.caption_text",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"text":"분기별 매출 추이"}"##,
    ),
    (
        "object.caption_text.get",
        "address → text(캡션 번호 뒤 문자열)",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "equation.layout",
        "script, size_pt → width·height·baseline(미리 보기 SVG 단위), items[{kind, text, x, y, width, height, start, end}](그려지는 조각과 그 조각을 만든 스크립트 범위, 유니코드 스칼라; 맞추지 못한 조각은 start·end null; kind slot은 빈 {} 칸), slots[](빈 {} 안쪽 위치)",
        r##"{"script":"{a+b} over {}","size_pt":10.0}"##,
    ),
    (
        "equation.macro.list",
        "(없음) → macros[{name, script}] (설정 폴더 equation_macros.json)",
        r##"{}"##,
    ),
    (
        "equation.macro.get",
        "name → script",
        r##"{"name":"근의 공식"}"##,
    ),
    (
        "equation.macro.save",
        "name(1–40자), script(수식 넣기와 같은 검사) → 같은 이름은 바꿈, 200개까지",
        r##"{"name":"근의 공식","script":"x = {-b +- sqrt {b^2 -4ac}} over {2a}"}"##,
    ),
    ("equation.macro.delete", "name", r##"{"name":"근의 공식"}"##),
    ("chart.list", "(없음) → 차트 목록", r##"{}"##),
    ("chart.data.get", "index → 계열·값", r##"{"index":0}"##),
    (
        "chart.data",
        "index, series[{name?, values[] (문자열)}], structure?(true일 때만 계열 이름·개수 변경)",
        r##"{"index":0,"series":[{"name":"1분기","values":["1","2","3"]}]}"##,
    ),
    (
        "object.group",
        "targets[address…](같은 구역의 떠 있는 도형·그림 2–256개) → address",
        r##"{"targets":[{"section":0,"paragraph":0,"control":0},{"section":0,"paragraph":0,"control":1}]}"##,
    ),
    (
        "object.ungroup",
        "address(묶음) → 한 단계 풀기",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "object.z_order",
        "address, mode front|back|forward|backward → moved",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"mode":"front"}"##,
    ),
    (
        "object.lock",
        "address, locked(없으면 토글) → locked",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    ("object.unlock_all", "(없음) → unlocked 개수", r##"{}"##),
    (
        "object.flip",
        "address, axis horizontal|vertical",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"axis":"horizontal"}"##,
    ),
    (
        "object.line_endpoint",
        "address(직선), start_mm[x,y], end_mm[x,y] (종이 기준)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"start_mm":[20.0,30.0],"end_mm":[80.0,30.0]}"##,
    ),
    (
        "insert.field",
        "guide(안내문), name, memo(도움말), editable",
        r##"{"guide":"이름을 입력하세요","name":"이름"}"##,
    ),
    ("field.list", "(없음) → 누름틀 목록", r##"{}"##),
    (
        "field.set",
        "field_id 또는 name+occurrence, value",
        r##"{"name":"이름","value":"홍길동"}"##,
    ),
    ("form.list", "(없음) → 양식 개체 목록", r##"{}"##),
    (
        "form.get",
        "at{section,paragraph,control,cell?}(form.list·form.insert 가 돌려준 값, address 도 받음) → 종류·값",
        r##"{"at":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "form.set",
        "at, value(선택 상자·라디오 단추: 1 또는 true=선택, 0 또는 false=해제)|text(입력·콤보)|caption(단추)",
        r##"{"at":{"section":0,"paragraph":0,"control":0},"value":1}"##,
    ),
    (
        "form.insert",
        "kind check_box|edit|combo_box|radio_button|push_button, caption?(단추·상자 글, 기본 한글 기본값), text?(입력·콤보 값), checked?(선택·라디오), items?[](콤보 목록, 64개), group?(라디오 묶음), name?(문서에서 유일, 기본 CheckBox1…), width_mm?/height_mm?(3–200, 기본 한글 기본 크기) → at. 캐럿 자리에 글자처럼 넣는다(본문·표 셀)",
        r##"{"kind":"check_box","caption":"동의합니다","checked":true}"##,
    ),
    (
        "object.align",
        "targets[address…](없으면 1개는 종이 기준), mode left|center|right|top|middle|bottom|same_width|same_height|same_size|space_h|space_v → moved 개수",
        r##"{"targets":[{"section":0,"paragraph":0,"control":0},{"section":0,"paragraph":0,"control":1}],"mode":"left"}"##,
    ),
    (
        "object.size",
        "address, width_mm, height_mm(둘 중 하나 이상), keep_ratio",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"width_mm":50.0,"keep_ratio":true}"##,
    ),
    (
        "object.rotate",
        "address, angle(절대 0–359) 또는 by(증감 ±) → angle",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"by":90}"##,
    ),
    (
        "object.protect_size",
        "address, protected(없으면 토글) → protected",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "object.outer_margin",
        "address, preset none|narrow|normal|wide 또는 mm(0–50)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"preset":"normal"}"##,
    ),
    (
        "object.style",
        "address, fill \"#rrggbb\"|\"none\", outline \"#rrggbb\"|\"none\", line_type 0–17, line_width_mm 0–5, arrow_start/arrow_end 0–8, arrow_size 0–8",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"fill":"#ffe08a","outline":"#000000","line_width_mm":0.5}"##,
    ),
    (
        "object.shadow",
        "address, mode none|on|left|right|up|down|reset, kind 1–8(on), step(%, 기본 1)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"mode":"on","kind":4}"##,
    ),
    (
        "object.format.get",
        "address → format{선·채우기·그림자·회전 속성}",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "object.format",
        "address, format(object.format.get 결과)",
        r##"{"address":{"section":0,"paragraph":0,"control":1},"format":{"borderColor":255,"borderWidth":28}}"##,
    ),
    (
        "object.neighbor",
        "address(없으면 커서 문단), direction next|prev, wrap(기본 true) → address, kind, page",
        r##"{"direction":"next"}"##,
    ),
    (
        "object.connector_type",
        "address(연결선), kind straight|stroke|arc",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"kind":"stroke"}"##,
    ),
    (
        "textbox.align",
        "address, horizontal left|center|right|justify, vertical top|center|bottom (둘 중 하나 이상)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"horizontal":"center","vertical":"center"}"##,
    ),
    (
        "textbox.margin",
        "address, preset none|narrow|normal|wide 또는 mm(0–50)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"preset":"normal"}"##,
    ),
    (
        "textbox.direction",
        "address, direction horizontal|vertical(세로 글상자)|vertical_all(영문 세움)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"direction":"vertical"}"##,
    ),
    (
        "picture.save",
        "address, path(확장자는 원본대로 바뀜), overwrite → path",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"path":"C:/out/picture1.png"}"##,
    ),
    (
        "picture.save_all",
        "dir, prefix(기본 picture), overwrite → files[]",
        r##"{"dir":"C:/out"}"##,
    ),
    (
        "picture.recolor",
        "address, mode original|grayscale|black_white|watermark",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"mode":"grayscale"}"##,
    ),
    (
        "picture.effects",
        "address; 효과마다 객체=설정, null=해제, 없음=유지; clear=true면 모두 해제. shadow{style outside|inside, transparency 0–100, blur_mm, distance_mm, direction 0–360, color #RRGGBB}, glow{transparency, size_mm, color}, soft_edge{size_mm}, reflection{transparency_start, transparency_end, size 1–100(%), distance_mm, blur_mm} → effects. HWP(.hwp)는 아직 효과를 쓰지 못해 .hwp 저장은 거절된다(HWPX로 저장하거나 save_document dropPictureEffects:true로 효과를 빼고 저장)",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"shadow":{"transparency":60,"blur_mm":1.5,"distance_mm":2,"direction":45,"color":"#000000"},"glow":null}"##,
    ),
    (
        "picture.effects.get",
        "address → effects{shadow, glow, soft_edge, reflection} (없는 효과는 null)",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "insert.auto_number",
        "kind page|total|picture|table|equation (본문 커서 위치)",
        r##"{"kind":"picture"}"##,
    ),
    (
        "note.convert",
        "address(각주 또는 미주) → address(변환된 주석)",
        r##"{"address":{"section":0,"paragraph":0,"control":0}}"##,
    ),
    (
        "note.shape.get",
        "kind footnote|endnote, section(없으면 커서 구역) → 번호·구분선 속성",
        r##"{"kind":"footnote"}"##,
    ),
    (
        "note.shape",
        "kind, section, props{numberFormat, prefixChar, suffixChar, startNumber, separatorEnabled, separatorLength(HWPUNIT 0–32767, -1 = 5 cm(한글 기본), 미주만 -4 = 단 너비), separatorLineType 0–17, separatorLineWidth, separatorColor \"#rrggbb\", numberCodeSuperscript, numbering continue|section|page}",
        r##"{"kind":"footnote","props":{"separatorLength":5669,"startNumber":1}}"##,
    ),
    (
        "note.delete",
        "address(없으면 커서 문단의 첫 주석)",
        r##"{}"##,
    ),
    (
        "field.delete",
        "field_id(없으면 커서 위치의 누름틀) → removed",
        r##"{}"##,
    ),
];
/// Connector connection points, as RHWP numbers them.
const CONNECT_POINTS: u32 = 4; // 0 top, 1 right, 2 bottom, 3 left

const HU: i64 = 7200;
const LENGTH: Kind = Kind::Int(0, 14_173); // 0–50 mm
const OFFSET: Kind = Kind::Int(-170_079, 170_079); // ±600 mm
const SIZE: Kind = Kind::Int(57, 170_079); // 0.2–600 mm
const COLOR: Kind = Kind::Int(0, 0xff_ffff);
const WRAP: Kind = Kind::Choice(&[
    "Square",
    "Tight",
    "Through",
    "TopAndBottom",
    "BehindText",
    "InFrontOfText",
]);

enum Kind {
    Int(i64, i64),
    Bool,
    Choice(&'static [&'static str]),
    Text(usize),
}

/// Placement keys shared by every floating object (RHWP common object attributes).
const COMMON: &[(&str, Kind)] = &[
    ("width", SIZE),
    ("height", SIZE),
    ("treatAsChar", Kind::Bool),
    ("textWrap", WRAP),
    ("vertRelTo", Kind::Choice(&["Paper", "Page", "Para"])),
    (
        "vertAlign",
        Kind::Choice(&["Top", "Center", "Bottom", "Inside", "Outside"]),
    ),
    (
        "horzRelTo",
        Kind::Choice(&["Paper", "Page", "Column", "Para"]),
    ),
    (
        "horzAlign",
        Kind::Choice(&["Left", "Center", "Right", "Inside", "Outside"]),
    ),
    ("vertOffset", OFFSET),
    ("horzOffset", OFFSET),
    ("restrictInPage", Kind::Bool),
    ("allowOverlap", Kind::Bool),
    ("sizeProtect", Kind::Bool),
    ("description", Kind::Text(1000)),
    ("outerMarginLeft", LENGTH),
    ("outerMarginRight", LENGTH),
    ("outerMarginTop", LENGTH),
    ("outerMarginBottom", LENGTH),
    ("rotationAngle", Kind::Int(-360, 360)),
    ("horzFlip", Kind::Bool),
    ("vertFlip", Kind::Bool),
    ("hasCaption", Kind::Bool),
    (
        "captionDirection",
        Kind::Choice(&["Left", "Right", "Top", "Bottom"]),
    ),
    (
        "captionVertAlign",
        Kind::Choice(&["Top", "Center", "Bottom"]),
    ),
    ("captionWidth", Kind::Int(0, 170_079)),
    ("captionSpacing", LENGTH),
    ("captionIncludeMargin", Kind::Bool),
];
const PICTURE: &[(&str, Kind)] = &[
    ("cropLeft", Kind::Int(0, 10_000_000)),
    ("cropTop", Kind::Int(0, 10_000_000)),
    ("cropRight", Kind::Int(0, 10_000_000)),
    ("cropBottom", Kind::Int(0, 10_000_000)),
    ("paddingLeft", LENGTH),
    ("paddingRight", LENGTH),
    ("paddingTop", LENGTH),
    ("paddingBottom", LENGTH),
    ("brightness", Kind::Int(-100, 100)),
    ("contrast", Kind::Int(-100, 100)),
    (
        "effect",
        Kind::Choice(&["RealPic", "GrayScale", "BlackWhite", "Pattern8x8"]),
    ),
    ("transparency", Kind::Int(0, 100)),
    ("borderColor", COLOR),
    ("borderWidth", Kind::Int(0, 1_417)),
];
const SHAPE: &[(&str, Kind)] = &[
    ("borderColor", COLOR),
    ("borderWidth", Kind::Int(0, 1_417)),
    ("lineType", Kind::Int(0, 17)),
    ("lineEndShape", Kind::Int(0, 5)),
    ("arrowStart", Kind::Int(0, 17)),
    ("arrowEnd", Kind::Int(0, 17)),
    ("arrowStartSize", Kind::Int(0, 8)),
    ("arrowEndSize", Kind::Int(0, 8)),
    ("fillType", Kind::Choice(&["none", "solid", "gradient"])),
    ("fillBgColor", COLOR),
    ("fillPatColor", COLOR),
    ("fillPatType", Kind::Int(-1, 6)),
    ("fillAlpha", Kind::Int(0, 255)),
    ("gradientType", Kind::Int(1, 4)),
    ("gradientAngle", Kind::Int(0, 359)),
    ("gradientCenterX", Kind::Int(0, 100)),
    ("gradientCenterY", Kind::Int(0, 100)),
    ("gradientBlur", Kind::Int(0, 100)),
    ("shadowType", Kind::Int(0, 8)),
    ("shadowColor", COLOR),
    ("shadowOffsetX", Kind::Int(-100, 100)),
    ("shadowOffsetY", Kind::Int(-100, 100)),
    ("roundRate", Kind::Int(0, 50)),
    ("tbMarginLeft", LENGTH),
    ("tbMarginRight", LENGTH),
    ("tbMarginTop", LENGTH),
    ("tbMarginBottom", LENGTH),
    (
        "tbVerticalAlign",
        Kind::Choice(&["Top", "Center", "Bottom"]),
    ),
];
const EQUATION: &[(&str, Kind)] = &[
    ("script", Kind::Text(2000)),
    ("fontSize", Kind::Int(400, 10_000)),
    ("color", COLOR),
    ("baseline", Kind::Int(-100, 100)),
    ("fontName", Kind::Text(64)),
];
const CAPTION: &[&str] = &[
    "captionDirection",
    "captionVertAlign",
    "captionWidth",
    "captionSpacing",
    "captionIncludeMargin",
];

fn validate(props: &Value, specs: &[&[(&str, Kind)]]) -> Result<Map<String, Value>> {
    let object = props
        .as_object()
        .ok_or_else(|| anyhow!("속성은 JSON 객체여야 합니다."))?;
    for (key, value) in object {
        let kind = specs
            .iter()
            .flat_map(|s| s.iter())
            .find(|(k, _)| k == key)
            .map(|(_, k)| k)
            .ok_or_else(|| anyhow!("이 개체에 쓸 수 없는 속성: {key}"))?;
        let ok = match kind {
            Kind::Int(lo, hi) => value.as_i64().is_some_and(|v| (*lo..=*hi).contains(&v)),
            Kind::Bool => value.is_boolean(),
            Kind::Choice(list) => value.as_str().is_some_and(|v| list.contains(&v)),
            Kind::Text(max) => value
                .as_str()
                .is_some_and(|v| v.chars().count() <= *max && !v.chars().any(char::is_control)),
        };
        if !ok {
            bail!("잘못된 개체 속성 값: {key}");
        }
    }
    Ok(object.clone())
}
fn mm_to_hu(mm: f64) -> Result<u32> {
    if !mm.is_finite() || !(0.2..=600.0).contains(&mm) {
        bail!("개체 크기는 0.2–600 mm입니다.");
    }
    Ok((mm * HU as f64 / 25.4).round() as u32)
}
fn paragraphs_ok(list: &[String]) -> Result<()> {
    if list.is_empty() || list.len() > 200 {
        bail!("문단은 1–200개입니다.");
    }
    if list.iter().map(String::len).sum::<usize>() > 100_000
        || list.iter().any(|p| p.chars().any(char::is_control))
    {
        bail!("문단마다 제어 문자 없이, 전체 100 KB 이하로 입력하세요.");
    }
    Ok(())
}
/// RHWP split APIs count inline controls as characters (see `Session::split_offset`).
fn logical_end(p: &Paragraph) -> usize {
    let len = p.text.chars().count();
    len + p
        .controls
        .iter()
        .zip(p.control_text_positions())
        .filter(|(c, pos)| {
            *pos <= len
                && matches!(
                    c,
                    Control::Shape(_)
                        | Control::Table(_)
                        | Control::Picture(_)
                        | Control::Equation(_)
                        | Control::Footnote(_)
                        | Control::Endnote(_)
                        | Control::AutoNumber(_)
                        | Control::CharOverlap(_)
                )
        })
        .count()
}
/// Hangul and RHWP close a polygon only when the last vertex repeats the first.
pub(crate) fn closed_polygon(
    mut points: Vec<hwpcraft_doc::model::Point>,
) -> Vec<hwpcraft_doc::model::Point> {
    if let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
        && (first.x != last.x || first.y != last.y)
    {
        points.push(first);
    }
    points
}
/// Closed triangle inscribed in the frame (HWPUNIT).
pub(crate) fn default_polygon(width: u32, height: u32) -> Vec<hwpcraft_doc::model::Point> {
    use hwpcraft_doc::model::Point;
    let (w, h) = (width as i32, height as i32);
    closed_polygon(vec![
        Point { x: w / 2, y: 0 },
        Point { x: w, y: h },
        Point { x: 0, y: h },
    ])
}
fn simple(p: &Paragraph) -> bool {
    p.controls.is_empty() && p.field_ranges.is_empty() && !p.text.chars().any(char::is_control)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Address {
    address: ObjectAddress,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Paragraphs {
    address: ObjectAddress,
    paragraphs: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShapeInsert {
    kind: ShapeKind,
    width_mm: f64,
    height_mm: f64,
    /// Hangul draws new shapes floating in front of text.
    #[serde(default)]
    treat_as_char: bool,
    #[serde(default)]
    text_wrap: Option<String>,
    /// Polygon vertices in mm, relative to the shape frame.
    #[serde(default)]
    points_mm: Option<Vec<[f64; 2]>>,
    /// Text box paragraphs.
    #[serde(default)]
    paragraphs: Vec<String>,
    /// Position from the paper's top-left (mouse drawing); default: the caret.
    #[serde(default)]
    x_mm: Option<f64>,
    #[serde(default)]
    y_mm: Option<f64>,
    /// Line direction: start at the right / bottom edge of the frame.
    #[serde(default)]
    flip_x: bool,
    #[serde(default)]
    flip_y: bool,
    /// Connector ends attached to shapes; both or none.
    #[serde(default)]
    start: Option<ConnectorEnd>,
    #[serde(default)]
    end: Option<ConnectorEnd>,
    /// 글상자 글자 방향: horizontal(기본) · vertical(세로 글상자) · vertical_all(영문 세움).
    #[serde(default)]
    direction: Option<String>,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectorEnd {
    address: ObjectAddress,
    /// 0 top, 1 right, 2 bottom, 3 left.
    #[serde(default)]
    point: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FormAt {
    section: usize,
    paragraph: usize,
    /// Control index of the form object in its (body or cell) paragraph.
    control: usize,
    #[serde(default)]
    cell: Option<Cell>,
}
/// A form value as an integer, or true/false for a check box or radio button.
fn form_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(i64),
        Flag(bool),
    }
    Ok(Option::<Value>::deserialize(d)?.map(|v| match v {
        Value::Number(n) => n,
        Value::Flag(b) => i64::from(b),
    }))
}
pub(crate) fn form_type_name(t: hwpcraft_doc::model::control::FormType) -> &'static str {
    use hwpcraft_doc::model::control::FormType;
    match t {
        FormType::PushButton => "PushButton",
        FormType::CheckBox => "CheckBox",
        FormType::ComboBox => "ComboBox",
        FormType::RadioButton => "RadioButton",
        FormType::Edit => "Edit",
    }
}
fn common_of(c: &Control) -> Option<&hwpcraft_doc::model::shape::CommonObjAttr> {
    match c {
        Control::Shape(s) => Some(s.common()),
        Control::Picture(p) => Some(&p.common),
        Control::Table(t) => Some(&t.common),
        _ => None,
    }
}
/// RHWP mutators answer `{"ok":true,...}` or `{"ok":false,"reason":...}`.
fn ok_value(raw: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(raw)?;
    if v["ok"] != true {
        let why = v["reason"]
            .as_str()
            .or_else(|| v["error"].as_str())
            .unwrap_or("실패");
        bail!("{why}");
    }
    Ok(v)
}
fn short_text(text: &str, max: usize, what: &str) -> Result<()> {
    if text.chars().count() > max || text.chars().any(char::is_control) {
        bail!("{what}은(는) 제어 문자 없이 {max}자 이하로 입력하세요.");
    }
    Ok(())
}
fn mm_point(p: [f64; 2]) -> Result<(i32, i32)> {
    if !(p[0].is_finite() && p[1].is_finite())
        || !(0.0..=1000.0).contains(&p[0])
        || !(0.0..=1000.0).contains(&p[1])
    {
        bail!("좌표는 종이 안(0–1000 mm)이어야 합니다.");
    }
    Ok((
        (p[0] * HU as f64 / 25.4).round() as i32,
        (p[1] * HU as f64 / 25.4).round() as i32,
    ))
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_object_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_object_command(id, params.clone()))
    }
    fn run_object_command(&mut self, id: &str, params: Value) -> Result<Value> {
        match id {
            "picture.insert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Picture {
                    path: std::path::PathBuf,
                    #[serde(default)]
                    width_mm: Option<f64>,
                    /// Hangul's default for 그림 넣기 (0.19; false before).
                    #[serde(default = "inline_by_default")]
                    treat_as_char: bool,
                }
                fn inline_by_default() -> bool {
                    true
                }
                let p: Picture = serde_json::from_value(params)?;
                let a = self.insert_picture(&p.path, p.width_mm, p.treat_as_char)?;
                Ok(json!({"address":a}))
            }
            "object.properties.get" => {
                let p: Address = serde_json::from_value(params)?;
                self.object_properties(p.address)
            }
            "object.properties" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Set {
                    address: ObjectAddress,
                    props: Value,
                }
                let p: Set = serde_json::from_value(params)?;
                self.set_object_properties(p.address, &p.props)?;
                Ok(self.info())
            }
            "shape.insert" => {
                let a = self.insert_shape_ex(serde_json::from_value(params)?)?;
                Ok(json!({"address":a}))
            }
            "textbox.paragraphs.get" | "note.paragraphs.get" => {
                let p: Address = serde_json::from_value(params)?;
                Ok(json!({"paragraphs": if id == "textbox.paragraphs.get" {
                    self.textbox_paragraphs(p.address)?
                } else {
                    self.note_paragraphs(p.address)?
                }}))
            }
            "object.caption_text" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Caption {
                    address: ObjectAddress,
                    text: String,
                }
                let p: Caption = serde_json::from_value(params)?;
                self.set_object_caption_text(p.address, &p.text)?;
                Ok(self.info())
            }
            "object.caption_text.get" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Get {
                    address: ObjectAddress,
                }
                let p: Get = serde_json::from_value(params)?;
                Ok(json!({"text": self.object_caption_text(p.address)?}))
            }
            "equation.layout" => {
                let p: EquationOptions = serde_json::from_value(params)?;
                Ok(serde_json::to_value(self.equation_layout(&p)?)?)
            }
            "equation.macro.list" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct NoParams {}
                let _: NoParams = serde_json::from_value(params)?;
                Ok(json!({"macros": crate::equation_macros::equation_macros()?}))
            }
            "equation.macro.get" | "equation.macro.delete" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Name {
                    name: String,
                }
                let p: Name = serde_json::from_value(params)?;
                if id == "equation.macro.get" {
                    let script = crate::equation_macros::equation_macro(&p.name)?;
                    Ok(json!({"name": p.name.trim(), "script": script}))
                } else {
                    crate::equation_macros::delete_equation_macro(&p.name)?;
                    Ok(json!({"deleted": p.name.trim()}))
                }
            }
            "equation.macro.save" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Save {
                    name: String,
                    script: String,
                }
                let p: Save = serde_json::from_value(params)?;
                crate::equation_macros::save_equation_macro(&p.name, &p.script)?;
                Ok(json!({"saved": p.name.trim()}))
            }
            "equation.insert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Equation {
                    script: String,
                    size_pt: f64,
                    #[serde(default)]
                    color: u32,
                    #[serde(default)]
                    baseline: Option<i64>,
                    #[serde(default)]
                    font_name: Option<String>,
                }
                let p: Equation = serde_json::from_value(params)?;
                if p.color > 0xff_ffff {
                    bail!("수식 색은 0x000000–0xFFFFFF(BGR)입니다.");
                }
                let mut extra = Map::new();
                if let Some(b) = p.baseline {
                    extra.insert("baseline".into(), json!(b));
                }
                if let Some(f) = &p.font_name {
                    extra.insert("fontName".into(), json!(f));
                }
                let extra = validate(&Value::Object(extra), &[EQUATION])?;
                let options = EquationOptions {
                    script: p.script,
                    size_pt: p.size_pt,
                };
                self.equation_preview(&options)?;
                self.body_insert()?;
                let mut inserted = None;
                self.transaction(|s| {
                    let c = s.reserve_object_position()?;
                    let size = (options.size_pt * 100.0).round() as u32;
                    let response = s
                        .core
                        .insert_equation_native(
                            c.section,
                            c.paragraph,
                            c.offset,
                            &options.script,
                            size,
                            p.color,
                        )
                        .map_err(err)?;
                    let v: Value = serde_json::from_str(&response)?;
                    let a = ObjectAddress {
                        section: c.section,
                        paragraph: v["paraIdx"]
                            .as_u64()
                            .ok_or_else(|| anyhow!("수식 주소 누락"))?
                            as usize,
                        control: v["controlIdx"]
                            .as_u64()
                            .ok_or_else(|| anyhow!("수식 주소 누락"))?
                            as usize,
                    };
                    if !extra.is_empty() {
                        s.core
                            .set_equation_properties_native(
                                a.section,
                                a.paragraph,
                                a.control,
                                None,
                                None,
                                &Value::Object(extra.clone()).to_string(),
                            )
                            .map_err(err)?;
                    }
                    inserted = Some(a);
                    Ok(())
                })?;
                Ok(json!({"address":inserted}))
            }
            "chart.list" | "memo.list" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                if id == "memo.list" {
                    return Ok(self.memos());
                }
                Ok(serde_json::from_str(
                    &self.core.list_charts_native().map_err(err)?,
                )?)
            }
            "chart.data.get" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Chart {
                    index: usize,
                }
                let p: Chart = serde_json::from_value(params)?;
                Ok(serde_json::from_str(
                    &self
                        .core
                        .get_chart_data_by_index_native(p.index)
                        .map_err(err)?,
                )?)
            }
            "chart.data" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Series {
                    #[serde(default)]
                    name: Option<String>,
                    values: Vec<String>,
                }
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Chart {
                    index: usize,
                    series: Vec<Series>,
                    #[serde(default)]
                    structure: bool,
                }
                let p: Chart = serde_json::from_value(params)?;
                if p.series.is_empty()
                    || p.series.len() > 64
                    || p.series.iter().any(|s| s.values.len() > 1000)
                {
                    bail!("차트 계열은 1–64개, 계열마다 값 1,000개 이하입니다.");
                }
                // Preserve the default value-only contract; names/structure change
                // only when a caller explicitly requests the public native mode.
                let edits = json!({"structure":p.structure,"series":p.series.iter().map(|s| {
                    let mut v = json!({"values":s.values});
                    if let Some(n) = &s.name { v["name"] = json!(n); }
                    v
                }).collect::<Vec<_>>()});
                self.validate_authored_chart_edit(p.index, &edits)?;
                let mut report = Value::Null;
                self.transaction(|s| {
                    let v: Value = serde_json::from_str(
                        &s.core
                            .set_chart_data_by_index_native(p.index, &edits.to_string())
                            .map_err(err)?,
                    )?;
                    if v["ok"] != true {
                        bail!("차트 데이터를 고치지 못했습니다: {v}");
                    }
                    report = v;
                    Ok(())
                })?;
                Ok(report)
            }
            "note.insert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Note {
                    endnote: bool,
                    paragraphs: Vec<String>,
                }
                let p: Note = serde_json::from_value(params)?;
                paragraphs_ok(&p.paragraphs)?;
                self.body_insert()?;
                let mut inserted = None;
                self.transaction(|s| {
                    let a = s.insert_note_raw(p.endnote, "")?;
                    s.write_note(a, &p.paragraphs)?;
                    inserted = Some(a);
                    Ok(())
                })?;
                Ok(json!({"address":inserted}))
            }
            "textbox.paragraphs" | "note.paragraphs" => {
                let p: Paragraphs = serde_json::from_value(params)?;
                if id == "textbox.paragraphs" {
                    self.set_textbox_paragraphs(p.address, &p.paragraphs)?;
                } else {
                    self.set_note_paragraphs(p.address, &p.paragraphs)?;
                }
                Ok(self.info())
            }
            "object.group" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Group {
                    targets: Vec<ObjectAddress>,
                }
                let p: Group = serde_json::from_value(params)?;
                let a = self.group_objects(&p.targets)?;
                Ok(json!({"address":a}))
            }
            "object.ungroup" => {
                let p: Address = serde_json::from_value(params)?;
                self.ungroup_object(p.address)?;
                Ok(self.info())
            }
            "object.z_order" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct ZOrder {
                    address: ObjectAddress,
                    mode: String,
                }
                let p: ZOrder = serde_json::from_value(params)?;
                let moved = self.set_z_order(p.address, &p.mode)?;
                Ok(json!({"moved":moved}))
            }
            "object.lock" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Lock {
                    address: ObjectAddress,
                    #[serde(default)]
                    locked: Option<bool>,
                }
                let p: Lock = serde_json::from_value(params)?;
                let locked = self.set_object_lock(p.address, p.locked)?;
                Ok(json!({"locked":locked}))
            }
            "object.unlock_all" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                Ok(json!({"unlocked":self.unlock_all_objects()?}))
            }
            "object.flip" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Flip {
                    address: ObjectAddress,
                    axis: String,
                }
                let p: Flip = serde_json::from_value(params)?;
                let vertical = match p.axis.as_str() {
                    "horizontal" => false,
                    "vertical" => true,
                    _ => bail!("대칭 축은 horizontal 또는 vertical입니다."),
                };
                self.flip_object(p.address, vertical)?;
                Ok(self.info())
            }
            "object.line_endpoint" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Endpoints {
                    address: ObjectAddress,
                    start_mm: [f64; 2],
                    end_mm: [f64; 2],
                }
                let p: Endpoints = serde_json::from_value(params)?;
                self.move_line_endpoints(p.address, p.start_mm, p.end_mm)?;
                Ok(self.info())
            }
            "insert.field" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Field {
                    guide: String,
                    #[serde(default)]
                    name: String,
                    #[serde(default)]
                    memo: String,
                    #[serde(default = "yes")]
                    editable: bool,
                }
                fn yes() -> bool {
                    true
                }
                let p: Field = serde_json::from_value(params)?;
                self.insert_click_here_field(&p.guide, &p.name, &p.memo, p.editable)
            }
            "field.list" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                Ok(serde_json::from_str(&self.core.get_field_list_json())?)
            }
            "field.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct FieldSet {
                    #[serde(default)]
                    field_id: Option<u32>,
                    #[serde(default)]
                    name: Option<String>,
                    #[serde(default)]
                    occurrence: usize,
                    value: String,
                }
                let p: FieldSet = serde_json::from_value(params)?;
                self.set_field_value(p.field_id, p.name.as_deref(), p.occurrence, &p.value)
            }
            "form.list" => {
                if params.as_object().is_some_and(|o| !o.is_empty()) {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                Ok(self.forms())
            }
            "form.get" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct FormGet {
                    #[serde(alias = "at")]
                    address: ObjectAddress,
                }
                let p: FormGet = serde_json::from_value(params)?;
                let a = p.address;
                if !matches!(self.object(a)?, Control::Form(_)) {
                    bail!("양식 개체가 아닙니다.");
                }
                ok_value(
                    &self
                        .core
                        .get_form_object_info_native(a.section, a.paragraph, a.control)
                        .map_err(err)?,
                )
            }
            "form.insert" => {
                let p: crate::form_insert::FormSpec = serde_json::from_value(params)?;
                Ok(json!({"at": self.insert_form(p)?}))
            }
            "form.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct FormSet {
                    at: FormAt,
                    #[serde(default, deserialize_with = "form_value")]
                    value: Option<i64>,
                    #[serde(default)]
                    text: Option<String>,
                    #[serde(default)]
                    caption: Option<String>,
                }
                let p: FormSet = serde_json::from_value(params)?;
                self.set_form_value(&p.at, p.value, p.text.as_deref(), p.caption.as_deref())?;
                Ok(self.info())
            }
            "object.align" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Align {
                    targets: Vec<ObjectAddress>,
                    mode: String,
                }
                let p: Align = serde_json::from_value(params)?;
                Ok(json!({"moved":self.align_objects(&p.targets, &p.mode)?}))
            }
            "object.size" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Size {
                    address: ObjectAddress,
                    #[serde(default)]
                    width_mm: Option<f64>,
                    #[serde(default)]
                    height_mm: Option<f64>,
                    #[serde(default)]
                    keep_ratio: bool,
                }
                let p: Size = serde_json::from_value(params)?;
                self.size_object(p.address, p.width_mm, p.height_mm, p.keep_ratio)?;
                Ok(self.info())
            }
            "object.rotate" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Rotate {
                    address: ObjectAddress,
                    #[serde(default)]
                    angle: Option<i64>,
                    #[serde(default)]
                    by: Option<i64>,
                }
                let p: Rotate = serde_json::from_value(params)?;
                Ok(json!({"angle":self.rotate_object(p.address, p.angle, p.by)?}))
            }
            "object.protect_size" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Protect {
                    address: ObjectAddress,
                    #[serde(default)]
                    protected: Option<bool>,
                }
                let p: Protect = serde_json::from_value(params)?;
                let now = self.object_properties(p.address)?["props"]["sizeProtect"] == true;
                let next = p.protected.unwrap_or(!now);
                if next != now {
                    self.set_object_properties(p.address, &json!({"sizeProtect":next}))?;
                }
                Ok(json!({"protected":next}))
            }
            "object.outer_margin" | "textbox.margin" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Margin {
                    address: ObjectAddress,
                    #[serde(default)]
                    preset: Option<String>,
                    #[serde(default)]
                    mm: Option<f64>,
                }
                let p: Margin = serde_json::from_value(params)?;
                let hu = margin_hu(p.preset.as_deref(), p.mm)?;
                let keys: [&str; 4] = if id == "object.outer_margin" {
                    [
                        "outerMarginLeft",
                        "outerMarginRight",
                        "outerMarginTop",
                        "outerMarginBottom",
                    ]
                } else {
                    if self.textbox_list(p.address).is_err() {
                        bail!("글상자가 아닙니다.");
                    }
                    [
                        "tbMarginLeft",
                        "tbMarginRight",
                        "tbMarginTop",
                        "tbMarginBottom",
                    ]
                };
                let mut props = Map::new();
                for k in keys {
                    props.insert(k.into(), json!(hu));
                }
                self.set_object_properties(p.address, &Value::Object(props))?;
                Ok(json!({"mm":hu as f64 * 25.4 / HU as f64}))
            }
            "object.style" => {
                let p: QuickStyle = serde_json::from_value(params)?;
                self.quick_style(p)?;
                Ok(self.info())
            }
            "object.shadow" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Shadow {
                    address: ObjectAddress,
                    mode: String,
                    #[serde(default)]
                    kind: Option<i64>,
                    #[serde(default)]
                    step: Option<i64>,
                }
                let p: Shadow = serde_json::from_value(params)?;
                self.shadow_object(p.address, &p.mode, p.kind, p.step.unwrap_or(1))
            }
            "object.format.get" => {
                let p: Address = serde_json::from_value(params)?;
                Ok(json!({"format":self.object_format(p.address)?}))
            }
            "object.format" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Format {
                    address: ObjectAddress,
                    format: Value,
                }
                let p: Format = serde_json::from_value(params)?;
                self.apply_object_format(p.address, &p.format)?;
                Ok(self.info())
            }
            "object.neighbor" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Neighbor {
                    #[serde(default)]
                    address: Option<ObjectAddress>,
                    direction: String,
                    #[serde(default = "yes")]
                    wrap: bool,
                }
                fn yes() -> bool {
                    true
                }
                let p: Neighbor = serde_json::from_value(params)?;
                let next = match p.direction.as_str() {
                    "next" => true,
                    "prev" => false,
                    _ => bail!("direction은 next 또는 prev입니다."),
                };
                match self.neighbor_object(p.address, next, p.wrap)? {
                    Some((a, kind)) => {
                        let page = self.page_of_object(a).unwrap_or(0);
                        Ok(json!({"address":a,"kind":kind,"page":page}))
                    }
                    None => Ok(json!({"address":Value::Null})),
                }
            }
            "object.connector_type" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Conn {
                    address: ObjectAddress,
                    kind: String,
                }
                let p: Conn = serde_json::from_value(params)?;
                self.set_connector_type(p.address, &p.kind)?;
                Ok(self.info())
            }
            "textbox.align" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct TbAlign {
                    address: ObjectAddress,
                    #[serde(default)]
                    horizontal: Option<String>,
                    #[serde(default)]
                    vertical: Option<String>,
                }
                let p: TbAlign = serde_json::from_value(params)?;
                self.align_textbox(p.address, p.horizontal.as_deref(), p.vertical.as_deref())?;
                Ok(self.info())
            }
            "textbox.direction" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Dir {
                    address: ObjectAddress,
                    direction: String,
                }
                let p: Dir = serde_json::from_value(params)?;
                let d = textbox_direction_code(&p.direction)?;
                self.set_textbox_direction(p.address, d)?;
                Ok(json!({"direction":self.textbox_direction(p.address)?}))
            }
            "picture.save" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Save {
                    address: ObjectAddress,
                    path: std::path::PathBuf,
                    #[serde(default)]
                    overwrite: bool,
                }
                let p: Save = serde_json::from_value(params)?;
                let written = self.save_picture(p.address, &p.path, p.overwrite)?;
                Ok(json!({"path":written}))
            }
            "picture.save_all" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct SaveAll {
                    dir: std::path::PathBuf,
                    #[serde(default)]
                    prefix: Option<String>,
                    #[serde(default)]
                    overwrite: bool,
                }
                let p: SaveAll = serde_json::from_value(params)?;
                let files = self.save_all_pictures(
                    &p.dir,
                    p.prefix.as_deref().unwrap_or("picture"),
                    p.overwrite,
                )?;
                Ok(json!({"files":files}))
            }
            "picture.effects" => {
                let mut p = params
                    .as_object()
                    .cloned()
                    .ok_or_else(|| anyhow!("picture.effects 인자는 객체입니다."))?;
                let address: ObjectAddress = serde_json::from_value(
                    p.remove("address")
                        .ok_or_else(|| anyhow!("address가 필요합니다."))?,
                )?;
                if let Some(key) = p.keys().find(|k| {
                    !["shadow", "glow", "soft_edge", "reflection", "clear"].contains(&k.as_str())
                }) {
                    bail!("picture.effects: 알 수 없는 항목 {key}");
                }
                self.set_picture_effects(address, &p)?;
                Ok(json!({"effects": self.picture_effects(address)?}))
            }
            "picture.effects.get" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Get {
                    address: ObjectAddress,
                }
                let p: Get = serde_json::from_value(params)?;
                Ok(json!({"effects": self.picture_effects(p.address)?}))
            }
            "picture.recolor" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Recolor {
                    address: ObjectAddress,
                    mode: String,
                }
                let p: Recolor = serde_json::from_value(params)?;
                let effect = match p.mode.as_str() {
                    "original" => "RealPic",
                    "grayscale" => "GrayScale",
                    "black_white" => "BlackWhite",
                    "watermark" => "Pattern8x8",
                    _ => bail!("mode는 original·grayscale·black_white·watermark입니다."),
                };
                if !matches!(self.object(p.address)?, Control::Picture(_)) {
                    bail!("그림이 아닙니다.");
                }
                self.set_object_properties(p.address, &json!({"effect":effect}))?;
                Ok(self.info())
            }
            "insert.auto_number" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct AutoNum {
                    kind: String,
                }
                let p: AutoNum = serde_json::from_value(params)?;
                Ok(json!({"address":self.insert_auto_number(&p.kind)?}))
            }
            "note.convert" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Convert {
                    #[serde(default)]
                    address: Option<ObjectAddress>,
                }
                let p: Convert = serde_json::from_value(params)?;
                let a = match p.address {
                    Some(a) => a,
                    None => self
                        .note_at_caret()
                        .ok_or_else(|| anyhow!("커서 문단에 각주·미주가 없습니다."))?,
                };
                Ok(json!({"address":self.convert_note(a)?}))
            }
            "note.shape.get" | "note.shape" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct NoteShape {
                    kind: String,
                    #[serde(default)]
                    section: Option<usize>,
                    #[serde(default)]
                    props: Option<Value>,
                }
                let p: NoteShape = serde_json::from_value(params)?;
                let endnote = match p.kind.as_str() {
                    "footnote" => false,
                    "endnote" => true,
                    _ => bail!("kind는 footnote 또는 endnote입니다."),
                };
                let section = p.section.unwrap_or(self.cursor.section);
                if id == "note.shape.get" {
                    if p.props.is_some() {
                        bail!("조회에는 props를 쓰지 않습니다.");
                    }
                    self.note_shape(section, endnote)
                } else {
                    let props = p.props.ok_or_else(|| anyhow!("props가 필요합니다."))?;
                    self.set_note_shape(section, endnote, &props)?;
                    self.note_shape(section, endnote)
                }
            }
            "note.delete" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Del {
                    #[serde(default)]
                    address: Option<ObjectAddress>,
                }
                let p: Del = serde_json::from_value(params)?;
                let a = match p.address {
                    Some(a) => a,
                    None => self
                        .note_at_caret()
                        .ok_or_else(|| anyhow!("커서 문단에 각주·미주가 없습니다."))?,
                };
                if !matches!(self.object(a)?, Control::Footnote(_) | Control::Endnote(_)) {
                    bail!("각주/미주가 아닙니다.");
                }
                self.delete_object(a)?;
                Ok(self.info())
            }
            "field.delete" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct DelField {
                    #[serde(default)]
                    field_id: Option<u32>,
                }
                let p: DelField = serde_json::from_value(params)?;
                Ok(json!({"removed":self.delete_field(p.field_id)?}))
            }
            _ => bail!("알 수 없는 개체 명령: {id}"),
        }
    }

    /// Memo fields in body paragraphs: anchor text and memo body (read-only;
    /// RHWP cannot write the MemoShape records a new memo needs).
    pub fn memos(&self) -> Value {
        use hwpcraft_doc::model::control::{Field, FieldType};
        fn is_memo(f: &Field) -> bool {
            f.field_type == FieldType::Memo || f.command.starts_with("MEMO/")
        }
        fn scan(p: &Paragraph, at: Value, out: &mut Vec<Value>) {
            for (ci, c) in p.controls.iter().enumerate() {
                match c {
                    Control::Field(f) if is_memo(f) => {
                        let anchor: String = p
                            .field_ranges
                            .iter()
                            .find(|r| r.control_idx == ci)
                            .map(|r| {
                                p.text
                                    .chars()
                                    .skip(r.start_char_idx)
                                    .take(r.end_char_idx.saturating_sub(r.start_char_idx))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let mut item = at.clone();
                        item["control"] = json!(ci);
                        out.push(json!({
                            "at": item,
                            "index": f.memo_index,
                            "anchor": anchor,
                            "paragraphs": f.memo_paragraphs.iter().map(|m| m.text.clone()).collect::<Vec<_>>(),
                        }));
                    }
                    Control::Table(t) => {
                        for (cell_index, cell) in t.cells.iter().enumerate() {
                            for (cp, para) in cell.paragraphs.iter().enumerate() {
                                let mut inner = at.clone();
                                inner["cell"] =
                                    json!({"control":ci,"index":cell_index,"paragraph":cp});
                                scan(para, inner, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for (si, section) in self.core.document().sections.iter().enumerate() {
            for (pi, p) in section.paragraphs.iter().enumerate() {
                scan(p, json!({"section":si,"paragraph":pi}), &mut out);
            }
        }
        json!(out)
    }

    /// `{kind, props}` where props is RHWP's property JSON for that object.
    pub fn object_properties(&self, a: ObjectAddress) -> Result<Value> {
        let (kind, raw) = match self.object(a)? {
            Control::Picture(_) => (
                "picture",
                self.core
                    .get_picture_properties_native(a.section, a.paragraph, a.control),
            ),
            Control::Shape(s) => (
                if s.drawing().and_then(|d| d.text_box.as_ref()).is_some() {
                    "textbox"
                } else {
                    "shape"
                },
                self.core
                    .get_shape_properties_native(a.section, a.paragraph, a.control),
            ),
            Control::Equation(_) => (
                "equation",
                self.core.get_equation_properties_native(
                    a.section,
                    a.paragraph,
                    a.control,
                    None,
                    None,
                ),
            ),
            Control::Table(_) => {
                return Ok(json!({"kind":"table","props":self.table_properties(a)?}));
            }
            _ => bail!("속성을 고칠 수 없는 개체입니다."),
        };
        let props: Value = serde_json::from_str(&raw.map_err(err)?)?;
        Ok(json!({"kind":kind,"props":props}))
    }

    pub fn set_object_properties(&mut self, a: ObjectAddress, props: &Value) -> Result<()> {
        let Some((kind, json)) = self.object_props_json(a, props)? else {
            return Ok(());
        };
        self.transaction(|s| s.apply_object_props(a, kind, &json))
    }
    /// Validate and apply properties without opening a transaction (for callers that
    /// already hold one). Same checks as `set_object_properties`.
    fn put_object_props(&mut self, a: ObjectAddress, props: &Value) -> Result<()> {
        match self.object_props_json(a, props)? {
            Some((kind, json)) => self.apply_object_props(a, kind, &json),
            None => Ok(()),
        }
    }
    /// Validated RHWP property JSON and object kind (0 picture, 1 shape, 2 equation);
    /// `None` when nothing would change. Tables are handled by `set_table_properties`.
    fn object_props_json(
        &mut self,
        a: ObjectAddress,
        props: &Value,
    ) -> Result<Option<(u8, String)>> {
        let kind = match self.object(a)? {
            Control::Picture(_) => 0,
            Control::Shape(s) if matches!(s.as_ref(), ShapeObject::Picture(_)) => {
                bail!("묶음 안 그림 속성은 아직 지원하지 않습니다.")
            }
            Control::Shape(_) => 1,
            Control::Equation(_) => 2,
            Control::Table(_) => {
                self.set_table_properties(a, props)?;
                return Ok(None);
            }
            _ => bail!("속성을 고칠 수 없는 개체입니다."),
        };
        let mut map = match kind {
            0 => validate(props, &[COMMON, PICTURE])?,
            1 => validate(props, &[COMMON, SHAPE])?,
            _ => validate(props, &[COMMON, EQUATION])?,
        };
        if map.is_empty() {
            return Ok(None);
        }
        // Hangul's 개체 보호 keeps a locked object out of every edit except unlocking.
        self.guard_object_lock(a)?;
        if kind == 2 && map.contains_key("hasCaption") {
            bail!("수식 캡션은 아직 지원하지 않습니다.");
        }
        // RHWP applies caption details only alongside hasCaption.
        if map.keys().any(|k| CAPTION.contains(&k.as_str())) && !map.contains_key("hasCaption") {
            let current = self.object_properties(a)?;
            map.insert("hasCaption".into(), current["props"]["hasCaption"].clone());
        }
        if let Some(script) = map.get("script").and_then(Value::as_str) {
            let size = map
                .get("fontSize")
                .and_then(Value::as_f64)
                .map(|v| v / 100.0)
                .unwrap_or(12.0);
            self.equation_preview(&EquationOptions {
                script: script.to_owned(),
                size_pt: size,
            })?;
        }
        Ok(Some((kind, Value::Object(map).to_string())))
    }
    fn apply_object_props(&mut self, a: ObjectAddress, kind: u8, json: &str) -> Result<()> {
        match kind {
            0 => self
                .core
                .set_picture_properties_native(a.section, a.paragraph, a.control, json),
            1 => self
                .core
                .set_shape_properties_native(a.section, a.paragraph, a.control, json),
            _ => self.core.set_equation_properties_native(
                a.section,
                a.paragraph,
                a.control,
                None,
                None,
                json,
            ),
        }
        .map_err(err)?;
        if kind != 2 {
            self.reroute_connectors(a)?;
        }
        Ok(())
    }

    /// Floating shapes start at the caret position on the page, like a click-to-draw in Hangul.
    fn insert_shape_ex(&mut self, p: ShapeInsert) -> Result<ObjectAddress> {
        self.body_insert()?;
        let (width, height) = (mm_to_hu(p.width_mm)?, mm_to_hu(p.height_mm)?);
        let direction = p
            .direction
            .as_deref()
            .map(textbox_direction_code)
            .transpose()?;
        if p.kind != ShapeKind::Textbox && direction.is_some() {
            bail!("글자 방향은 글상자에만 지정합니다.");
        }
        if p.kind != ShapeKind::Textbox && !p.paragraphs.is_empty() {
            bail!("글은 글상자에만 넣을 수 있습니다.");
        }
        if !p.paragraphs.is_empty() {
            paragraphs_ok(&p.paragraphs)?;
        }
        if (p.flip_x || p.flip_y) && !p.kind.is_line() {
            bail!("방향(flip_x·flip_y)은 직선과 연결선에만 지정합니다.");
        }
        if (p.start.is_some() || p.end.is_some()) && !p.kind.is_connector() {
            bail!("시작·끝 도형은 연결선에만 지정합니다.");
        }
        if p.kind.is_connector() && p.treat_as_char {
            bail!("연결선은 글자처럼 취급할 수 없습니다.");
        }
        // Attached connector: both ends name a shape in the caret's section; its
        // geometry comes from the shapes' connection points, not the request.
        let ends = match (p.start, p.end) {
            (Some(s), Some(e)) => Some(self.connector_ends(s, e)?),
            (None, None) => None,
            _ => bail!("연결선의 시작과 끝 도형은 함께 지정하세요."),
        };
        let wrap = p.text_wrap.as_deref().unwrap_or(if p.treat_as_char {
            "TopAndBottom"
        } else {
            "InFrontOfText"
        });
        if !matches!(
            wrap,
            "Square" | "TopAndBottom" | "BehindText" | "InFrontOfText"
        ) {
            bail!("본문과의 배치는 Square, TopAndBottom, BehindText, InFrontOfText 중 하나입니다.");
        }
        let points: Vec<hwpcraft_doc::model::Point> = match (&p.points_mm, p.kind) {
            (Some(list), ShapeKind::Polygon | ShapeKind::Curve) => {
                let curve = p.kind == ShapeKind::Curve;
                if curve && !(2..=1024).contains(&list.len()) {
                    bail!("곡선 점은 2–1,024개입니다.");
                }
                if !curve && !(3..=256).contains(&list.len()) {
                    bail!("다각형 꼭짓점은 3–256개입니다.");
                }
                let points = list
                    .iter()
                    .map(|[x, y]| {
                        if !(x.is_finite() && y.is_finite())
                            || !(0.0..=p.width_mm).contains(x)
                            || !(0.0..=p.height_mm).contains(y)
                        {
                            bail!("꼭짓점은 도형 크기 안에 있어야 합니다.");
                        }
                        Ok(hwpcraft_doc::model::Point {
                            x: (x * HU as f64 / 25.4).round() as i32,
                            y: (y * HU as f64 / 25.4).round() as i32,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                // A freehand curve stays open; a polygon closes on its first vertex.
                if curve {
                    points
                } else {
                    closed_polygon(points)
                }
            }
            (Some(_), _) => bail!("점은 다각형과 곡선에만 지정합니다."),
            (None, ShapeKind::Polygon) => default_polygon(width, height),
            (None, _) => Vec::new(),
        };
        // Caret position on its page (px at 96 dpi → HWPUNIT ×75).
        let (mut width, mut height) = (width, height);
        let (mut flip_x, mut flip_y) = (p.flip_x, p.flip_y);
        let (x, y) = if let Some(e) = &ends {
            let (sx, sy, ex, ey) = e.points;
            width = (sx - ex).unsigned_abs().max(1);
            height = (sy - ey).unsigned_abs().max(1);
            flip_x = sx > ex;
            flip_y = sy > ey;
            (sx.min(ex).max(0) as u32, sy.min(ey).max(0) as u32)
        } else if p.treat_as_char {
            (0, 0)
        } else if let (Some(x), Some(y)) = (p.x_mm, p.y_mm) {
            if !(x.is_finite() && y.is_finite())
                || !(0.0..=1000.0).contains(&x)
                || !(0.0..=1000.0).contains(&y)
            {
                bail!("도형 위치는 종이 안(0–1000 mm)이어야 합니다.");
            }
            (
                (x * HU as f64 / 25.4).round() as u32,
                (y * HU as f64 / 25.4).round() as u32,
            )
        } else if p.x_mm.is_some() || p.y_mm.is_some() {
            bail!("도형 위치는 x_mm와 y_mm를 함께 지정하세요.");
        } else {
            let caret = self.caret()?;
            (
                (caret["x"].as_f64().unwrap_or(0.0) * 75.0).max(0.0) as u32,
                (caret["y"].as_f64().unwrap_or(0.0) * 75.0).max(0.0) as u32,
            )
        };
        let mut address = None;
        self.transaction(|s| {
            // A paper-relative connector shows on its anchor paragraph's page: anchor it
            // after its start object when the caret is on another page.
            if let Some(e) = &ends
                && s.page_of_object(ObjectAddress {
                    control: 0,
                    section: s.cursor.section,
                    paragraph: s.cursor.paragraph,
                })? != e.page
            {
                let at = Cursor {
                    section: e.anchor.section,
                    paragraph: e.anchor.paragraph,
                    offset: 0,
                    cell: None,
                };
                let len = s.paragraph(at)?.text.chars().count();
                s.cursor = Cursor { offset: len, ..at };
                s.anchor = None;
            }
            let c = s.reserve_object_position()?;
            let response = s
                .core
                .create_shape_control_native(
                    c.section,
                    c.paragraph,
                    c.offset,
                    width,
                    height,
                    x,
                    y,
                    p.treat_as_char,
                    wrap,
                    p.kind.name(),
                    flip_x,
                    flip_y,
                    &points,
                )
                .map_err(err)?;
            let v: Value = serde_json::from_str(&response)?;
            let index = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("도형 삽입 결과에 {k}가 없습니다: {v}"))
            };
            let a = ObjectAddress {
                section: c.section,
                paragraph: index("paraIdx")?,
                control: index("controlIdx")?,
            };
            if p.kind == ShapeKind::Curve {
                s.polygon_to_curve(a, p.points_mm.is_some())?;
            }
            if p.kind.is_connector() {
                s.finish_connector(a, p.kind, ends.as_ref())?;
            }
            if !p.paragraphs.is_empty() {
                s.write_textbox(a, &p.paragraphs)?;
            }
            if let Some(d) = direction {
                s.textbox_direction_raw(a, d)?;
            }
            address = Some(a);
            Ok(())
        })?;
        address.ok_or_else(|| anyhow!("도형을 넣지 못했습니다."))
    }

    fn textbox_list(&self, a: ObjectAddress) -> Result<&[Paragraph]> {
        let Control::Shape(shape) = self.object(a)? else {
            bail!("글상자가 아닙니다.");
        };
        Ok(&shape
            .drawing()
            .and_then(|d| d.text_box.as_ref())
            .ok_or_else(|| anyhow!("글상자가 아닙니다."))?
            .paragraphs)
    }
    pub fn textbox_paragraphs(&self, a: ObjectAddress) -> Result<Vec<String>> {
        let list = self.textbox_list(a)?;
        if !list.iter().all(simple) {
            bail!("개체·필드가 있는 글상자는 문단 텍스트 편집을 지원하지 않습니다.");
        }
        Ok(list.iter().map(|p| p.text.clone()).collect())
    }
    pub fn set_textbox_paragraphs(
        &mut self,
        a: ObjectAddress,
        paragraphs: &[String],
    ) -> Result<()> {
        paragraphs_ok(paragraphs)?;
        if self.textbox_paragraphs(a)? == paragraphs {
            return Ok(());
        }
        self.transaction(|s| s.write_textbox(a, paragraphs))
    }
    /// Collapse the text box to one paragraph, then rebuild with splits.
    fn write_textbox(&mut self, a: ObjectAddress, paragraphs: &[String]) -> Result<()> {
        let count = self.textbox_list(a)?.len();
        for i in (1..count).rev() {
            self.core
                .merge_paragraph_in_cell_native(a.section, a.paragraph, a.control, 0, i)
                .map_err(err)?;
        }
        let old = self
            .textbox_list(a)?
            .first()
            .map_or(0, |p| p.text.chars().count());
        if old > 0 {
            self.core
                .delete_text_in_cell_native(a.section, a.paragraph, a.control, 0, 0, 0, old)
                .map_err(err)?;
        }
        for (i, text) in paragraphs.iter().enumerate() {
            if i > 0 {
                let end = self
                    .textbox_list(a)?
                    .get(i - 1)
                    .map(logical_end)
                    .ok_or_else(|| anyhow!("글상자 문단을 찾지 못했습니다."))?;
                self.core
                    .split_paragraph_in_cell_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        0,
                        i - 1,
                        end,
                        None,
                    )
                    .map_err(err)?;
            }
            if !text.is_empty() {
                self.core
                    .insert_text_in_cell_native(a.section, a.paragraph, a.control, 0, i, 0, text)
                    .map_err(err)?;
            }
        }
        if self.textbox_list(a)?.len() != paragraphs.len() {
            bail!("글상자 문단 수가 맞지 않습니다.");
        }
        Ok(())
    }

    fn note_list(&self, a: ObjectAddress) -> Result<&[Paragraph]> {
        match self.object(a)? {
            Control::Footnote(n) => Ok(&n.paragraphs),
            Control::Endnote(n) => Ok(&n.paragraphs),
            _ => bail!("각주/미주가 아닙니다."),
        }
    }
    /// First paragraph keeps its automatic number and the two-space separator.
    pub fn note_paragraphs(&self, a: ObjectAddress) -> Result<Vec<String>> {
        let list = self.note_list(a)?;
        let first = list
            .first()
            .ok_or_else(|| anyhow!("주석 문단이 없습니다."))?;
        if first.controls.len() != 1
            || !matches!(first.controls[0], Control::AutoNumber(_))
            || !first.field_ranges.is_empty()
            || !first.text.starts_with("  ")
            || first.text.chars().any(char::is_control)
            || !list[1..].iter().all(simple)
        {
            bail!("번호 외 개체·필드가 있는 주석은 문단 텍스트 편집을 지원하지 않습니다.");
        }
        Ok(std::iter::once(first.text[2..].to_owned())
            .chain(list[1..].iter().map(|p| p.text.clone()))
            .collect())
    }
    pub fn set_note_paragraphs(&mut self, a: ObjectAddress, paragraphs: &[String]) -> Result<()> {
        paragraphs_ok(paragraphs)?;
        if self.note_paragraphs(a)? == paragraphs {
            return Ok(());
        }
        self.transaction(|s| s.write_note(a, paragraphs))
    }
    fn write_note(&mut self, a: ObjectAddress, paragraphs: &[String]) -> Result<()> {
        {
            let s = self;
            let count = s.note_list(a)?.len();
            for i in (1..count).rev() {
                s.core
                    .merge_paragraph_in_footnote_native(a.section, a.paragraph, a.control, i)
                    .map_err(err)?;
            }
            let old = s
                .note_list(a)?
                .first()
                .map_or(0, |p| p.text.chars().count());
            if old > 2 {
                s.core
                    .delete_text_in_footnote_native(
                        a.section,
                        a.paragraph,
                        a.control,
                        0,
                        2,
                        old - 2,
                    )
                    .map_err(err)?;
            }
            for (i, text) in paragraphs.iter().enumerate() {
                if i > 0 {
                    let end = s
                        .note_list(a)?
                        .get(i - 1)
                        .map(logical_end)
                        .ok_or_else(|| anyhow!("주석 문단을 찾지 못했습니다."))?;
                    s.core
                        .split_paragraph_in_footnote_native(
                            a.section,
                            a.paragraph,
                            a.control,
                            i - 1,
                            end,
                            None,
                        )
                        .map_err(err)?;
                }
                if !text.is_empty() {
                    s.core
                        .insert_text_in_footnote_native(
                            a.section,
                            a.paragraph,
                            a.control,
                            i,
                            if i == 0 { 2 } else { 0 },
                            text,
                        )
                        .map_err(err)?;
                }
            }
            if s.note_paragraphs(a)? != paragraphs {
                bail!("주석 문단을 고치지 못했습니다.");
            }
            Ok(())
        }
    }
}

/// Resolved connector ends: subject ids, connection point indices and the
/// global HWPUNIT coordinates of the two connection points.
pub(crate) struct ConnectorEnds {
    start_id: u32,
    start_point: u32,
    end_id: u32,
    end_point: u32,
    /// (start x, start y, end x, end y), paper coordinates of the shared page.
    points: (i32, i32, i32, i32),
    /// The page both objects are shown on.
    page: u32,
    /// The start object: a connector made elsewhere is anchored in its paragraph.
    anchor: ObjectAddress,
}

impl Session {
    /// Hangul numbers body paragraphs across sections; RHWP `*_at` mutators take that index.
    fn para_in_list(&self, a: ObjectAddress) -> usize {
        self.core
            .document()
            .sections
            .iter()
            .take(a.section)
            .map(|s| s.paragraphs.len())
            .sum::<usize>()
            + a.paragraph
    }
    /// 개체 보호 (HWP5 attr bit 30) on a table, shape or picture.
    pub fn object_locked(&self, a: ObjectAddress) -> bool {
        self.object(a)
            .ok()
            .and_then(common_of)
            .is_some_and(|c| c.locked)
    }
    pub(crate) fn guard_object_lock(&self, a: ObjectAddress) -> Result<()> {
        if self.object_locked(a) {
            bail!("보호된 개체입니다. 개체 보호를 먼저 해제하세요.");
        }
        Ok(())
    }
    /// Lay out again after a model-only RHWP mutator (z-order, flip, connector routing).
    fn relayout_object(&mut self, a: ObjectAddress) -> Result<()> {
        match self.object(a)? {
            Control::Shape(_) => {
                self.core
                    .set_shape_properties_native(a.section, a.paragraph, a.control, "{}")
            }
            Control::Picture(_) => {
                self.core
                    .set_picture_properties_native(a.section, a.paragraph, a.control, "{}")
            }
            Control::Table(_) => {
                self.core
                    .set_table_properties_native(a.section, a.paragraph, a.control, "{}")
            }
            _ => return Ok(()),
        }
        .map_err(err)?;
        Ok(())
    }
    fn section_has_connectors(&self, section: usize) -> bool {
        self.core.document().sections.get(section).is_some_and(|s| {
            s.paragraphs
                .iter()
                .flat_map(|p| p.controls.iter())
                .any(|c| {
                    matches!(c, Control::Shape(s)
                            if matches!(s.as_ref(), ShapeObject::Line(l) if l.connector.is_some()))
                })
        })
    }
    /// Connectors follow the shapes they are attached to.
    pub(crate) fn reroute_connectors(&mut self, a: ObjectAddress) -> Result<()> {
        if !self.section_has_connectors(a.section) {
            return Ok(());
        }
        self.core.update_connectors_in_section(a.section);
        self.reroute_connectors_from_layout(a.section)?;
        self.relayout_object(a)
    }

    /// RHWP routes connectors from their objects' stored offsets, which are paper
    /// coordinates only for floating paper-relative objects. Connectors to any other
    /// object (paragraph- or column-relative, treated as a character) are routed again
    /// from where the layout shows the two objects, when both are on one page.
    fn reroute_connectors_from_layout(&mut self, section: usize) -> Result<()> {
        use hwpcraft_doc::model::shape::{HorzRelTo, VertRelTo};
        let Some(sec) = self.core.document().sections.get(section) else {
            return Ok(());
        };
        // Subject id → (address, routed correctly by RHWP).
        let mut subjects = std::collections::HashMap::new();
        let mut connectors = Vec::new();
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            for (ci, c) in p.controls.iter().enumerate() {
                let a = ObjectAddress {
                    section,
                    paragraph: pi,
                    control: ci,
                };
                let (common, inst) = match c {
                    Control::Shape(s) => {
                        if let ShapeObject::Line(l) = s.as_ref() {
                            if let Some(k) = &l.connector
                                && l.common.vert_rel_to == VertRelTo::Paper
                                && l.common.horz_rel_to == HorzRelTo::Paper
                                && !l.common.treat_as_char
                            {
                                connectors.push((
                                    a,
                                    k.start_subject_id,
                                    k.start_subject_index,
                                    k.end_subject_id,
                                    k.end_subject_index,
                                ));
                            }
                            continue;
                        }
                        (s.common(), s.drawing().map(|d| d.inst_id).unwrap_or(0))
                    }
                    Control::Picture(p) => (&p.common, 0),
                    _ => continue,
                };
                let paper = !common.treat_as_char
                    && common.vert_rel_to == VertRelTo::Paper
                    && common.horz_rel_to == HorzRelTo::Paper;
                // The ids RHWP's connector routing accepts for an object.
                let mut ids = vec![inst];
                if common.instance_id != 0 {
                    ids.extend([common.instance_id, (common.instance_id & 0x3FFF_FFFF) + 1]);
                }
                for id in ids.into_iter().filter(|id| *id != 0) {
                    subjects.entry(id).or_insert((a, paper));
                }
            }
        }
        for (a, start_id, start_index, end_id, end_index) in connectors {
            let (Some(&(start, start_paper)), Some(&(end, end_paper))) =
                (subjects.get(&start_id), subjects.get(&end_id))
            else {
                continue;
            };
            if start_paper && end_paper {
                continue;
            }
            let (Ok((s_page, s_pts)), Ok((e_page, e_pts))) =
                (self.connection_points(start), self.connection_points(end))
            else {
                continue;
            };
            if s_page != e_page || self.page_of_object(a)? != s_page {
                continue;
            }
            let (sx, sy) = s_pts[start_index.min(3) as usize];
            let (ex, ey) = e_pts[end_index.min(3) as usize];
            let doc = self.core.document_mut();
            let sec = &mut doc.sections[section];
            if let Some(Control::Shape(s)) = sec.paragraphs[a.paragraph].controls.get_mut(a.control)
                && let ShapeObject::Line(line) = s.as_mut()
            {
                let (min_x, min_y) = (sx.min(ex), sy.min(ey));
                let w = (sx.max(ex) - min_x).max(1) as u32;
                let h = (sy.max(ey) - min_y).max(1) as u32;
                line.common.horizontal_offset = min_x.max(0) as u32;
                line.common.vertical_offset = min_y.max(0) as u32;
                line.common.width = w;
                line.common.height = h;
                line.start.x = sx - min_x;
                line.start.y = sy - min_y;
                line.end.x = ex - min_x;
                line.end.y = ey - min_y;
                let attr = &mut line.drawing.shape_attr;
                attr.current_width = w;
                attr.original_width = w;
                attr.current_height = h;
                attr.original_height = h;
                attr.rotation_center.x = w as i32 / 2;
                attr.rotation_center.y = h as i32 / 2;
                attr.raw_rendering = Vec::new();
                sec.raw_stream = None;
            }
            self.core.recalculate_connector_routing(
                a.section,
                a.paragraph,
                a.control,
                start_index,
                end_index,
            );
        }
        Ok(())
    }

    /// Connection points of an object in paper HWPUNIT with its page. A floating
    /// paper-relative object's offsets are exact paper coordinates; any other object
    /// (paragraph- or column-relative, treated as a character) connects where the page
    /// layout draws it.
    fn connection_points(&self, a: ObjectAddress) -> Result<(u32, [(i32, i32); 4])> {
        use hwpcraft_doc::model::shape::{HorzRelTo, VertRelTo};
        let common = common_of(self.object(a)?).ok_or_else(|| anyhow!("그림·도형이 아닙니다."))?;
        let (page, x, y, w, h) = if !common.treat_as_char
            && common.vert_rel_to == VertRelTo::Paper
            && common.horz_rel_to == HorzRelTo::Paper
        {
            (
                self.page_of_object(a)?,
                common.horizontal_offset as i32,
                common.vertical_offset as i32,
                common.width as i32,
                common.height as i32,
            )
        } else {
            let (page, [x, y, w, h]) = self.object_box(a)?;
            let hu = |px: f64| (px * 75.0).round() as i32;
            (page, hu(x), hu(y), hu(w), hu(h))
        };
        let (cx, cy) = (x + w / 2, y + h / 2);
        Ok((page, [(cx, y), (x + w, cy), (cx, y + h), (x, cy)]))
    }
    fn connector_ends(&self, start: ConnectorEnd, end: ConnectorEnd) -> Result<ConnectorEnds> {
        /// (subject id, connection point index, page, the four connection points).
        type Subject = (u32, u32, u32, [(i32, i32); 4]);
        let section = start.address.section;
        let resolve = |e: ConnectorEnd| -> Result<Subject> {
            if e.address.section != section {
                bail!("연결선과 연결할 도형은 같은 구역에 있어야 합니다.");
            }
            if e.point >= CONNECT_POINTS {
                bail!("연결점은 0(위)·1(오른쪽)·2(아래)·3(왼쪽)입니다.");
            }
            let id = match self.object(e.address)? {
                Control::Shape(s) => {
                    if matches!(s.as_ref(), ShapeObject::Line(_)) {
                        bail!("직선·연결선에는 연결선을 붙일 수 없습니다.");
                    }
                    s.drawing()
                        .map(|d| d.inst_id)
                        .filter(|i| *i != 0)
                        .unwrap_or(s.common().instance_id)
                }
                Control::Picture(p) => p.common.instance_id,
                _ => bail!("연결선은 도형·그림에만 붙일 수 있습니다."),
            };
            if id == 0 {
                bail!("개체 식별자가 없어 연결선을 붙일 수 없습니다.");
            }
            let (page, points) = self.connection_points(e.address)?;
            Ok((id, e.point, page, points))
        };
        let (start_id, start_point, s_page, s_pts) = resolve(start)?;
        let (end_id, end_point, e_page, e_pts) = resolve(end)?;
        if start_id == end_id {
            bail!("연결선의 시작과 끝은 다른 개체여야 합니다.");
        }
        if s_page != e_page {
            bail!("연결선으로 이을 두 개체는 같은 쪽에 있어야 합니다.");
        }
        let (sx, sy) = s_pts[start_point as usize % 4];
        let (ex, ey) = e_pts[end_point as usize % 4];
        Ok(ConnectorEnds {
            start_id,
            start_point,
            end_id,
            end_point,
            points: (sx, sy, ex, ey),
            page: s_page,
            anchor: start.address,
        })
    }
    /// After RHWP created a one-way connector: attach ends, widen to both-way, add arrow heads.
    fn finish_connector(
        &mut self,
        a: ObjectAddress,
        kind: ShapeKind,
        ends: Option<&ConnectorEnds>,
    ) -> Result<()> {
        if let Some(e) = ends {
            self.core.update_connector_subject_ids(
                a.section,
                a.paragraph,
                a.control,
                e.start_id,
                e.start_point,
                e.end_id,
                e.end_point,
            );
            self.core.recalculate_connector_routing(
                a.section,
                a.paragraph,
                a.control,
                e.start_point,
                e.end_point,
            );
        }
        let (start, end) = kind.arrows();
        if start && end {
            use hwpcraft_doc::model::shape::LinkLineType;
            let mut changed = false;
            if let Some(section) = self.core.document_mut().sections.get_mut(a.section) {
                if let Some(Control::Shape(sh)) = section
                    .paragraphs
                    .get_mut(a.paragraph)
                    .and_then(|p| p.controls.get_mut(a.control))
                    && let ShapeObject::Line(l) = sh.as_mut()
                    && let Some(c) = l.connector.as_mut()
                {
                    c.link_type = match c.link_type {
                        LinkLineType::StraightOneWay => LinkLineType::StraightBoth,
                        LinkLineType::StrokeOneWay => LinkLineType::StrokeBoth,
                        LinkLineType::ArcOneWay => LinkLineType::ArcBoth,
                        other => other,
                    };
                    changed = true;
                }
                if changed {
                    section.raw_stream = None;
                }
            }
            if !changed {
                bail!("연결선을 만들지 못했습니다.");
            }
        }
        let mut props = Map::new();
        if start {
            props.insert("arrowStart".into(), json!(1));
            props.insert("arrowStartSize".into(), json!(3));
        }
        if end {
            props.insert("arrowEnd".into(), json!(1));
            props.insert("arrowEndSize".into(), json!(3));
        }
        // The property call also refreshes layout after the model-only edits above.
        self.core
            .set_shape_properties_native(
                a.section,
                a.paragraph,
                a.control,
                &Value::Object(props).to_string(),
            )
            .map_err(err)?;
        Ok(())
    }

    /// 묶기: two or more floating shapes/pictures of one section become a group object.
    pub fn group_objects(&mut self, targets: &[ObjectAddress]) -> Result<ObjectAddress> {
        let Some(first) = targets.first() else {
            bail!("묶기는 같은 구역의 도형·그림 2–256개를 고르세요.");
        };
        if !(2..=256).contains(&targets.len()) {
            bail!("묶기는 같은 구역의 도형·그림 2–256개를 고르세요.");
        }
        let section = first.section;
        let mut seen = std::collections::HashSet::new();
        for a in targets {
            if a.section != section {
                bail!("다른 구역의 개체는 함께 묶을 수 없습니다.");
            }
            if !seen.insert((a.paragraph, a.control)) {
                bail!("같은 개체를 두 번 골랐습니다.");
            }
            let common = match self.object(*a)? {
                Control::Shape(s) => s.common(),
                Control::Picture(p) => &p.common,
                _ => bail!("도형·그림만 묶을 수 있습니다."),
            };
            if common.treat_as_char {
                bail!("글자처럼 취급한 개체는 묶을 수 없습니다.");
            }
            self.guard_object_lock(*a)?;
        }
        let pairs: Vec<(usize, usize)> = targets.iter().map(|a| (a.paragraph, a.control)).collect();
        let mut out = None;
        self.transaction(|s| {
            let v = ok_value(&s.core.group_shapes_native(section, &pairs).map_err(err)?)?;
            let index = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| anyhow!("묶기 결과에 {k}가 없습니다: {v}"))
            };
            let a = ObjectAddress {
                section,
                paragraph: index("paraIdx")?,
                control: index("controlIdx")?,
            };
            if !matches!(s.object(a)?, Control::Shape(sh) if matches!(sh.as_ref(), ShapeObject::Group(_)))
            {
                bail!("묶음 개체를 만들지 못했습니다.");
            }
            s.cursor.offset = s.cursor.offset.min(s.len());
            s.clear_selection();
            s.core.update_connectors_in_section(section);
            s.relayout_object(a)?;
            out = Some(a);
            Ok(())
        })?;
        out.ok_or_else(|| anyhow!("개체를 묶지 못했습니다."))
    }
    /// 풀기: one level; nested groups stay grouped (RHWP rule).
    pub fn ungroup_object(&mut self, a: ObjectAddress) -> Result<()> {
        if !matches!(self.object(a)?, Control::Shape(s) if matches!(s.as_ref(), ShapeObject::Group(_)))
        {
            bail!("묶음 개체가 아닙니다.");
        }
        self.guard_object_lock(a)?;
        self.transaction(|s| {
            ok_value(
                &s.core
                    .ungroup_shape_native(a.section, a.paragraph, a.control)
                    .map_err(err)?,
            )?;
            s.cursor.offset = s.cursor.offset.min(s.len());
            s.clear_selection();
            s.core.update_connectors_in_section(a.section);
            if matches!(s.object(a), Ok(Control::Shape(_) | Control::Picture(_))) {
                s.relayout_object(a)?;
            }
            Ok(())
        })
    }
    /// 앞뒤 순서. Returns false (no undo step) when the object is already there.
    pub fn set_z_order(&mut self, a: ObjectAddress, mode: &str) -> Result<bool> {
        if !matches!(mode, "front" | "back" | "forward" | "backward") {
            bail!("앞뒤 순서는 front, back, forward, backward 중 하나입니다.");
        }
        let z = match self.object(a)? {
            Control::Shape(s) => s.common().z_order,
            Control::Picture(p) => p.common.z_order,
            _ => bail!("앞뒤 순서는 도형·그림에만 적용합니다."),
        };
        self.guard_object_lock(a)?;
        let zs: Vec<i32> = self
            .core
            .document()
            .sections
            .get(a.section)
            .map(|s| {
                s.paragraphs
                    .iter()
                    .flat_map(|p| p.controls.iter())
                    .filter_map(common_of)
                    .map(|c| c.z_order)
                    .collect()
            })
            .unwrap_or_default();
        let top = zs.iter().copied().max().unwrap_or(z);
        let bottom = zs.iter().copied().min().unwrap_or(z);
        let moves = match mode {
            "front" | "forward" => z < top,
            _ => z > bottom,
        };
        if !moves {
            return Ok(false);
        }
        let pil = self.para_in_list(a);
        self.transaction(|s| {
            let v = ok_value(
                &s.core
                    .set_control_z_order_at(pil, a.control, mode)
                    .map_err(err)?,
            )?;
            if v["moved"] != true {
                bail!("앞뒤 순서를 바꾸지 못했습니다.");
            }
            s.relayout_object(a)
        })?;
        Ok(true)
    }
    /// 개체 보호 toggle (`None`) or set; returns the new state.
    pub fn set_object_lock(&mut self, a: ObjectAddress, locked: Option<bool>) -> Result<bool> {
        let current = common_of(self.object(a)?)
            .ok_or_else(|| anyhow!("개체 보호는 표·도형·그림에만 적용합니다."))?
            .locked;
        let want = locked.unwrap_or(!current);
        if want == current {
            return Ok(want);
        }
        let pil = self.para_in_list(a);
        self.transaction(|s| {
            let v = ok_value(
                &s.core
                    .set_control_lock(Some(pil), Some(a.control), want)
                    .map_err(err)?,
            )?;
            if v["touched"] != 1 {
                bail!("개체 보호를 바꾸지 못했습니다: {v}");
            }
            Ok(())
        })?;
        Ok(want)
    }
    /// 모두 풀기 (ShapeObjUnlockAll); returns how many objects were locked.
    pub fn unlock_all_objects(&mut self) -> Result<usize> {
        let mut locked = Vec::new();
        for (section, s) in self.core.document().sections.iter().enumerate() {
            for (paragraph, p) in s.paragraphs.iter().enumerate() {
                for (control, c) in p.controls.iter().enumerate() {
                    if common_of(c).is_some_and(|c| c.locked) {
                        locked.push(ObjectAddress {
                            section,
                            paragraph,
                            control,
                        });
                    }
                }
            }
        }
        if locked.is_empty() {
            return Ok(0);
        }
        // One object at a time: RHWP's all-objects call drops the stored stream of every
        // section with a table, picture or shape, locked or not, and a sealed section
        // without one makes RHWP lay the whole document out as edited.
        self.transaction(|s| {
            for &a in &locked {
                let pil = s.para_in_list(a);
                let v = ok_value(
                    &s.core
                        .set_control_lock(Some(pil), Some(a.control), false)
                        .map_err(err)?,
                )?;
                if v["touched"] != 1 {
                    bail!("개체 보호를 풀지 못했습니다: {v}");
                }
            }
            Ok(())
        })?;
        Ok(locked.len())
    }
    /// 대칭 toggle through RHWP's Hangul-measured flip (bits + matrix). Shapes only;
    /// pictures keep the `horzFlip`/`vertFlip` picture properties.
    pub fn flip_object(&mut self, a: ObjectAddress, vertical: bool) -> Result<()> {
        match self.object(a)? {
            Control::Shape(_) => {}
            Control::Picture(_) => {
                bail!("그림 대칭은 그림 속성(horzFlip·vertFlip)으로 지정하세요.")
            }
            _ => bail!("대칭은 도형에만 적용합니다."),
        }
        self.guard_object_lock(a)?;
        let pil = self.para_in_list(a);
        self.transaction(|s| {
            ok_value(
                &s.core
                    .set_control_flip_at(pil, a.control, vertical, false)
                    .map_err(err)?,
            )?;
            s.relayout_object(a)
        })
    }
    /// Straight line end points in paper mm (global coordinates).
    pub fn move_line_endpoints(
        &mut self,
        a: ObjectAddress,
        start_mm: [f64; 2],
        end_mm: [f64; 2],
    ) -> Result<()> {
        let Control::Shape(s) = self.object(a)? else {
            bail!("직선이 아닙니다.");
        };
        let ShapeObject::Line(l) = s.as_ref() else {
            bail!("직선이 아닙니다.");
        };
        if l.connector.is_some() {
            bail!("연결선 끝점은 연결된 도형을 옮기면 따라갑니다.");
        }
        if l.common.treat_as_char {
            bail!("글자처럼 취급한 직선의 끝점은 옮길 수 없습니다.");
        }
        self.guard_object_lock(a)?;
        let (sx, sy) = mm_point(start_mm)?;
        let (ex, ey) = mm_point(end_mm)?;
        if (sx, sy) == (ex, ey) {
            bail!("직선의 두 끝점이 같습니다.");
        }
        self.transaction(|s| {
            ok_value(
                &s.core
                    .move_line_endpoint_native(a.section, a.paragraph, a.control, sx, sy, ex, ey)
                    .map_err(err)?,
            )?;
            Ok(())
        })
    }

    /// 누름틀 at the caret (body or table cell), empty with a guide text.
    fn insert_click_here_field(
        &mut self,
        guide: &str,
        name: &str,
        memo: &str,
        editable: bool,
    ) -> Result<Value> {
        short_text(guide, 200, "안내문")?;
        short_text(name, 64, "누름틀 이름")?;
        short_text(memo, 1000, "도움말")?;
        if guide.trim().is_empty() {
            bail!("누름틀 안내문을 입력하세요.");
        }
        self.guard_protected()?;
        if self.has_selection() {
            bail!("선택을 해제하고 누름틀을 넣을 위치에 커서를 놓으세요.");
        }
        let c = self.cursor;
        self.paragraph(c)?;
        if c.offset > self.len() {
            bail!("커서가 문단 범위를 벗어났습니다.");
        }
        let mut out = Value::Null;
        self.transaction(|s| {
            let raw = if let Some(t) = c.cell {
                s.core.insert_click_here_field_at_in_cell(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    c.offset,
                    false,
                    guide,
                    memo,
                    name,
                    editable,
                )
            } else {
                s.core.insert_click_here_field_at(
                    c.section,
                    c.paragraph,
                    c.offset,
                    guide,
                    memo,
                    name,
                    editable,
                )
            }
            .map_err(err)?;
            let v = ok_value(&raw)?;
            s.cursor.offset = s.cursor.offset.min(s.len());
            out = json!({"field_id":v["fieldId"],"char_offset":v["charOffset"]});
            Ok(())
        })?;
        Ok(out)
    }
    fn set_field_value(
        &mut self,
        id: Option<u32>,
        name: Option<&str>,
        occurrence: usize,
        value: &str,
    ) -> Result<Value> {
        short_text(value, 10_000, "누름틀 값")?;
        if id.is_some() == name.is_some() {
            bail!("field_id 또는 name 중 하나만 지정하세요.");
        }
        if let Some(n) = name {
            short_text(n, 64, "누름틀 이름")?;
        }
        let mut out = Value::Null;
        self.transaction(|s| {
            let raw = match (id, name) {
                (Some(i), _) => s.core.set_field_value_by_id(i, value),
                (None, Some(n)) => s.core.set_field_value_by_name_at(n, occurrence, value),
                (None, None) => bail!("누름틀을 지정하세요."),
            }
            .map_err(err)?;
            out = ok_value(&raw)?;
            s.cursor.offset = s.cursor.offset.min(s.len());
            Ok(())
        })?;
        Ok(out)
    }

    /// 양식 개체 in body paragraphs and first-level table cells.
    pub fn forms(&self) -> Value {
        use hwpcraft_doc::model::control::FormObject;
        fn item(f: &FormObject, at: Value) -> Value {
            json!({"at":at,"type":form_type_name(f.form_type),"name":f.name,"caption":f.caption,
                "text":f.text,"value":f.value,"enabled":f.enabled})
        }
        let mut out = Vec::new();
        for (si, section) in self.core.document().sections.iter().enumerate() {
            for (pi, p) in section.paragraphs.iter().enumerate() {
                for (ci, c) in p.controls.iter().enumerate() {
                    match c {
                        Control::Form(f) => {
                            out.push(item(f, json!({"section":si,"paragraph":pi,"control":ci})));
                        }
                        Control::Table(t) => {
                            for (cell_index, cell) in t.cells.iter().enumerate() {
                                for (cp, para) in cell.paragraphs.iter().enumerate() {
                                    for (fi, fc) in para.controls.iter().enumerate() {
                                        if let Control::Form(f) = fc {
                                            out.push(item(f, json!({"section":si,"paragraph":pi,
                                                "control":fi,
                                                "cell":{"control":ci,"index":cell_index,"paragraph":cp}})));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        json!(out)
    }
    fn form_at(&self, at: &FormAt) -> Result<&hwpcraft_doc::model::control::FormObject> {
        let p = self.paragraph(Cursor {
            section: at.section,
            paragraph: at.paragraph,
            offset: 0,
            cell: at.cell,
        })?;
        match p.controls.get(at.control) {
            Some(Control::Form(f)) => Ok(f),
            _ => bail!("양식 개체를 찾지 못했습니다."),
        }
    }
    fn set_form_value(
        &mut self,
        at: &FormAt,
        value: Option<i64>,
        text: Option<&str>,
        caption: Option<&str>,
    ) -> Result<()> {
        use hwpcraft_doc::model::control::FormType;
        let kind = self.form_at(at)?.form_type;
        if value.is_none() && text.is_none() && caption.is_none() {
            bail!("value, text, caption 중 하나 이상을 지정하세요.");
        }
        let mut props = Map::new();
        if let Some(v) = value {
            if !matches!(kind, FormType::CheckBox | FormType::RadioButton) || !(0..=1).contains(&v)
            {
                bail!("value는 선택 상자·라디오 단추에 0 또는 1로 지정합니다.");
            }
            props.insert("value".into(), json!(v));
        }
        if let Some(t) = text {
            if !matches!(kind, FormType::Edit | FormType::ComboBox) {
                bail!("text는 입력 상자·목록 상자에만 지정합니다.");
            }
            short_text(t, 1000, "양식 개체 값")?;
            props.insert("text".into(), json!(t));
        }
        if let Some(c) = caption {
            if !matches!(
                kind,
                FormType::PushButton | FormType::CheckBox | FormType::RadioButton
            ) {
                bail!("caption은 명령 단추·선택 상자·라디오 단추에만 지정합니다.");
            }
            short_text(c, 1000, "캡션")?;
            props.insert("caption".into(), json!(c));
        }
        let json = Value::Object(props).to_string();
        let (section, paragraph, control, cell) = (at.section, at.paragraph, at.control, at.cell);
        self.transaction(|s| {
            let raw = if let Some(t) = cell {
                s.core.set_form_value_in_cell_native(
                    section,
                    paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    control,
                    &json,
                )
            } else {
                s.core
                    .set_form_value_native(section, paragraph, control, &json)
            }
            .map_err(err)?;
            ok_value(&raw)?;
            Ok(())
        })
    }
}

/// Hangul stream position of a character index: extended controls take 8 code units
/// (automatic numbers 7 beside their placeholder), tabs 8, click-here fields 8 + 8
/// (+ the guide text while empty). Mirrors RHWP's cursor arithmetic for body paragraphs.
/// Hangul 바깥 여백 / 글상자 안 여백 사전 값 (mm): 없음·좁게·보통·넓게.
fn margin_hu(preset: Option<&str>, mm: Option<f64>) -> Result<i64> {
    let mm = match (preset, mm) {
        (Some(p), None) => match p {
            "none" => 0.0,
            "narrow" => 1.0,
            "normal" => 2.0,
            "wide" => 3.0,
            _ => bail!("preset은 none·narrow·normal·wide입니다."),
        },
        (None, Some(v)) => v,
        _ => bail!("preset 또는 mm 중 하나만 지정하세요."),
    };
    if !mm.is_finite() || !(0.0..=50.0).contains(&mm) {
        bail!("여백은 0–50 mm입니다.");
    }
    Ok((mm * HU as f64 / 25.4).round() as i64)
}
/// Text box direction names → 0 horizontal, 1 vertical, 2 vertical_all.
fn textbox_direction_code(name: &str) -> Result<u8> {
    Ok(match name {
        "horizontal" => 0,
        "vertical" => 1,
        "vertical_all" => 2,
        _ => bail!("direction은 horizontal·vertical·vertical_all입니다."),
    })
}
/// "#rrggbb" → RHWP drawing color (BGR integer).
fn bgr_of(hex: &str) -> Result<i64> {
    let h = hex
        .strip_prefix('#')
        .filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow!("색은 #rrggbb 형식입니다: {hex}"))?;
    let v = i64::from_str_radix(h, 16)?;
    Ok(((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff))
}
/// One alignment move: (address, dx px, dy px, new width HWPUNIT, new height HWPUNIT).
type AlignStep = (ObjectAddress, f64, f64, Option<i64>, Option<i64>);
/// Keys copied by 개체 모양 복사 (CopyShapeFormat): look, not position or size.
const FORMAT_KEYS: &[&str] = &[
    "borderColor",
    "borderWidth",
    "lineType",
    "lineEndShape",
    "arrowStart",
    "arrowEnd",
    "arrowStartSize",
    "arrowEndSize",
    "fillType",
    "fillBgColor",
    "fillPatColor",
    "fillPatType",
    "fillAlpha",
    "gradientType",
    "gradientAngle",
    "gradientCenterX",
    "gradientCenterY",
    "gradientBlur",
    "shadowType",
    "shadowColor",
    "shadowOffsetX",
    "shadowOffsetY",
    "roundRate",
    "brightness",
    "contrast",
    "effect",
    "transparency",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickStyle {
    address: ObjectAddress,
    #[serde(default)]
    fill: Option<String>,
    #[serde(default)]
    outline: Option<String>,
    #[serde(default)]
    line_type: Option<i64>,
    #[serde(default)]
    line_width_mm: Option<f64>,
    #[serde(default)]
    arrow_start: Option<i64>,
    #[serde(default)]
    arrow_end: Option<i64>,
    #[serde(default)]
    arrow_size: Option<i64>,
}

impl Session {
    /// Page-pixel box (96 dpi) of a floating object from the page layout, with its page.
    fn object_box(&self, a: ObjectAddress) -> Result<(u32, [f64; 4])> {
        let page = self.page_of_object(a)?;
        let layout: Value = serde_json::from_str(&self.page_control_layout(page)?)?;
        let n = |v: &Value, k: &str| v[k].as_u64().map(|x| x as usize);
        let c = layout["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| {
                n(c, "secIdx") == Some(a.section)
                    && n(c, "paraIdx") == Some(a.paragraph)
                    && n(c, "controlIdx") == Some(a.control)
                    && c.get("cellIdx").is_none()
            })
            .ok_or_else(|| anyhow!("쪽 배치에서 개체를 찾지 못했습니다."))?;
        let f = |k: &str| c[k].as_f64().unwrap_or(0.0);
        Ok((page, [f("x"), f("y"), f("w"), f("h")]))
    }
    /// Zero-based page that shows the object's anchor paragraph.
    pub fn page_of_object(&self, a: ObjectAddress) -> Result<u32> {
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(
            &core
                .get_page_of_position_native(a.section, a.paragraph)
                .map_err(err)?,
        )?;
        Ok(v["page"].as_u64().unwrap_or(0) as u32)
    }
    fn floating_unlocked(&self, a: ObjectAddress) -> Result<()> {
        self.guard_object_lock(a)?;
        let common = common_of(self.object(a)?).ok_or_else(|| anyhow!("그림·도형이 아닙니다."))?;
        if common.treat_as_char {
            bail!("글자처럼 취급한 개체는 맞출 수 없습니다. 먼저 어울림 등으로 바꾸세요.");
        }
        Ok(())
    }

    /// 맞춤 (ShapeAlign*): move or resize floating objects on one page. With a single
    /// target the paper edges are the reference; with several, the group's extent.
    /// `same_*` take the first target as reference. Returns how many objects changed.
    pub fn align_objects(&mut self, targets: &[ObjectAddress], mode: &str) -> Result<usize> {
        if targets.is_empty() || targets.len() > 256 {
            bail!("맞출 개체를 1–256개 지정하세요.");
        }
        let mut seen = std::collections::HashSet::new();
        for a in targets {
            if !seen.insert((a.section, a.paragraph, a.control)) {
                bail!("같은 개체가 두 번 들어 있습니다.");
            }
            self.floating_unlocked(*a)?;
        }
        const PX_PER_MM: f64 = 96.0 / 25.4;
        let mut boxes = Vec::new();
        let mut page = None;
        for a in targets {
            let (pg, b) = self.object_box(*a)?;
            if *page.get_or_insert(pg) != pg {
                bail!("같은 쪽에 있는 개체끼리만 맞출 수 있습니다.");
            }
            boxes.push((*a, b));
        }
        let (x0, y0, x1, y1) = if boxes.len() == 1 {
            let setup = self.page_setup()?;
            let (w, h) = if setup.landscape {
                (setup.height_mm, setup.width_mm)
            } else {
                (setup.width_mm, setup.height_mm)
            };
            (0.0, 0.0, w * PX_PER_MM, h * PX_PER_MM)
        } else {
            boxes.iter().fold(
                (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
                |(x0, y0, x1, y1), (_, b)| {
                    (
                        x0.min(b[0]),
                        y0.min(b[1]),
                        x1.max(b[0] + b[2]),
                        y1.max(b[1] + b[3]),
                    )
                },
            )
        };
        let hu_per_px = HU as f64 / 96.0;
        let mut plan: Vec<AlignStep> = Vec::new();
        let reference = boxes[0].1;
        match mode {
            "left" | "center" | "right" | "top" | "middle" | "bottom" => {
                for (a, b) in &boxes {
                    let (dx, dy) = match mode {
                        "left" => (x0 - b[0], 0.0),
                        "center" => ((x0 + x1) / 2.0 - (b[0] + b[2] / 2.0), 0.0),
                        "right" => (x1 - (b[0] + b[2]), 0.0),
                        "top" => (0.0, y0 - b[1]),
                        "middle" => (0.0, (y0 + y1) / 2.0 - (b[1] + b[3] / 2.0)),
                        _ => (0.0, y1 - (b[1] + b[3])),
                    };
                    plan.push((*a, dx, dy, None, None));
                }
            }
            "same_width" | "same_height" | "same_size" => {
                if boxes.len() < 2 {
                    bail!("같은 크기로 맞출 개체를 2개 이상 지정하세요.");
                }
                let ref_props = self.object_properties(boxes[0].0)?;
                let w = ref_props["props"]["width"]
                    .as_i64()
                    .unwrap_or((reference[2] * hu_per_px).round() as i64);
                let h = ref_props["props"]["height"]
                    .as_i64()
                    .unwrap_or((reference[3] * hu_per_px).round() as i64);
                for (a, _) in boxes.iter().skip(1) {
                    plan.push((
                        *a,
                        0.0,
                        0.0,
                        (mode != "same_height").then_some(w),
                        (mode != "same_width").then_some(h),
                    ));
                }
            }
            "space_h" | "space_v" => {
                if boxes.len() < 3 {
                    bail!("간격을 같게 할 개체를 3개 이상 지정하세요.");
                }
                let horizontal = mode == "space_h";
                let mut sorted = boxes.clone();
                sorted.sort_by(|a, b| {
                    let (ka, kb) = if horizontal {
                        (a.1[0], b.1[0])
                    } else {
                        (a.1[1], b.1[1])
                    };
                    ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                });
                let total: f64 = sorted
                    .iter()
                    .map(|(_, b)| if horizontal { b[2] } else { b[3] })
                    .sum();
                let span = if horizontal { x1 - x0 } else { y1 - y0 };
                let gap = (span - total) / (sorted.len() - 1) as f64;
                let mut cursor = if horizontal { x0 } else { y0 };
                for (a, b) in &sorted {
                    let (dx, dy) = if horizontal {
                        (cursor - b[0], 0.0)
                    } else {
                        (0.0, cursor - b[1])
                    };
                    plan.push((*a, dx, dy, None, None));
                    cursor += if horizontal { b[2] } else { b[3] } + gap;
                }
            }
            _ => bail!(
                "mode는 left·center·right·top·middle·bottom·same_width·same_height·same_size·space_h·space_v입니다."
            ),
        }
        let plan: Vec<_> = plan
            .into_iter()
            .filter(|(_, dx, dy, w, h)| {
                dx.abs() >= 0.5 || dy.abs() >= 0.5 || w.is_some() || h.is_some()
            })
            .collect();
        if plan.is_empty() {
            return Ok(0);
        }
        let count = plan.len();
        self.transaction(|s| {
            for (a, dx, dy, w, h) in &plan {
                let current = s.object_properties(*a)?;
                let p = &current["props"];
                let mut props = Map::new();
                if dx.abs() >= 0.5 {
                    props.insert(
                        "horzOffset".into(),
                        json!(
                            p["horzOffset"].as_i64().unwrap_or(0) + (dx * hu_per_px).round() as i64
                        ),
                    );
                }
                if dy.abs() >= 0.5 {
                    props.insert(
                        "vertOffset".into(),
                        json!(
                            p["vertOffset"].as_i64().unwrap_or(0) + (dy * hu_per_px).round() as i64
                        ),
                    );
                }
                if let Some(w) = w {
                    props.insert("width".into(), json!(w));
                }
                if let Some(h) = h {
                    props.insert("height".into(), json!(h));
                }
                s.put_object_props(*a, &Value::Object(props))?;
            }
            Ok(())
        })?;
        Ok(count)
    }

    /// ShapeWidth / ShapeHeight: absolute size in mm; `keep_ratio` derives the other side.
    pub fn size_object(
        &mut self,
        a: ObjectAddress,
        width_mm: Option<f64>,
        height_mm: Option<f64>,
        keep_ratio: bool,
    ) -> Result<()> {
        if width_mm.is_none() && height_mm.is_none() {
            bail!("width_mm 또는 height_mm을 지정하세요.");
        }
        let current = self.object_properties(a)?;
        let (w0, h0) = (
            current["props"]["width"].as_f64().unwrap_or(1.0).max(1.0),
            current["props"]["height"].as_f64().unwrap_or(1.0).max(1.0),
        );
        let mut props = Map::new();
        let w = width_mm.map(mm_to_hu).transpose()?;
        let h = height_mm.map(mm_to_hu).transpose()?;
        match (w, h, keep_ratio) {
            (Some(w), None, true) => {
                props.insert("width".into(), json!(w));
                props.insert(
                    "height".into(),
                    json!((f64::from(w) * h0 / w0).round().max(57.0) as u32),
                );
            }
            (None, Some(h), true) => {
                props.insert("height".into(), json!(h));
                props.insert(
                    "width".into(),
                    json!((f64::from(h) * w0 / h0).round().max(57.0) as u32),
                );
            }
            _ => {
                if let Some(w) = w {
                    props.insert("width".into(), json!(w));
                }
                if let Some(h) = h {
                    props.insert("height".into(), json!(h));
                }
            }
        }
        self.set_object_properties(a, &Value::Object(props))
    }

    /// ShapeRotate: absolute `angle` or relative `by`; returns the new angle (0–359).
    pub fn rotate_object(
        &mut self,
        a: ObjectAddress,
        angle: Option<i64>,
        by: Option<i64>,
    ) -> Result<i64> {
        let current = self.object_properties(a)?["props"]["rotationAngle"]
            .as_i64()
            .unwrap_or(0);
        let next = match (angle, by) {
            (Some(v), None) => v,
            (None, Some(d)) => current + d,
            _ => bail!("angle 또는 by 중 하나만 지정하세요."),
        }
        .rem_euclid(360);
        if next != current.rem_euclid(360) {
            self.set_object_properties(a, &json!({"rotationAngle":next}))?;
        }
        Ok(next)
    }

    /// Fill / outline / dash / pen width / arrows in one step (ribbon quick buttons).
    fn quick_style(&mut self, p: QuickStyle) -> Result<()> {
        let kind = self.object_properties(p.address)?["kind"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let mut props = Map::new();
        if let Some(fill) = &p.fill {
            if kind == "picture" {
                bail!("그림에는 채우기를 쓸 수 없습니다.");
            }
            if fill == "none" {
                props.insert("fillType".into(), json!("none"));
            } else {
                props.insert("fillType".into(), json!("solid"));
                props.insert("fillBgColor".into(), json!(bgr_of(fill)?));
            }
        }
        if let Some(outline) = &p.outline {
            if outline == "none" {
                if kind == "picture" {
                    props.insert("borderWidth".into(), json!(0));
                } else {
                    props.insert("lineType".into(), json!(0));
                }
            } else {
                props.insert("borderColor".into(), json!(bgr_of(outline)?));
                if kind != "picture"
                    && self.object_properties(p.address)?["props"]["lineType"].as_i64() == Some(0)
                {
                    props.insert("lineType".into(), json!(1));
                }
            }
        }
        if let Some(t) = p.line_type {
            if kind == "picture" {
                bail!("그림 테두리는 선 종류를 바꿀 수 없습니다.");
            }
            props.insert("lineType".into(), json!(t));
        }
        if let Some(w) = p.line_width_mm {
            if !w.is_finite() || !(0.0..=5.0).contains(&w) {
                bail!("선 굵기는 0–5 mm입니다.");
            }
            props.insert(
                "borderWidth".into(),
                json!((w * HU as f64 / 25.4).round() as i64),
            );
        }
        for (k, v) in [("arrowStart", p.arrow_start), ("arrowEnd", p.arrow_end)] {
            if let Some(v) = v {
                if kind == "picture" {
                    bail!("그림에는 화살표를 쓸 수 없습니다.");
                }
                props.insert(k.into(), json!(v));
            }
        }
        if let Some(size) = p.arrow_size {
            props.insert("arrowStartSize".into(), json!(size));
            props.insert("arrowEndSize".into(), json!(size));
        }
        if props.is_empty() {
            bail!("바꿀 스타일 항목을 지정하세요.");
        }
        self.set_object_properties(p.address, &Value::Object(props))
    }

    /// Shadow quick commands: `on` (kind 1–8, Hangul default 오른쪽 아래), `none`,
    /// `left`/`right`/`up`/`down` nudge the offset by `step` %, `reset` restores 0/0.
    fn shadow_object(
        &mut self,
        a: ObjectAddress,
        mode: &str,
        kind: Option<i64>,
        step: i64,
    ) -> Result<Value> {
        let current = self.object_properties(a)?;
        if current["kind"] == "picture" {
            bail!("그림 그림자는 지원하지 않습니다 (RHWP가 HWP5에 저장하지 않음).");
        }
        let p = &current["props"];
        let (x, y) = (
            p["shadowOffsetX"].as_i64().unwrap_or(0),
            p["shadowOffsetY"].as_i64().unwrap_or(0),
        );
        if !(1..=20).contains(&step) {
            bail!("step은 1–20 %입니다.");
        }
        let props = match mode {
            "none" => json!({"shadowType":0}),
            "on" => {
                let k = kind.unwrap_or(4);
                if !(1..=8).contains(&k) {
                    bail!("kind는 1–8입니다.");
                }
                json!({"shadowType":k})
            }
            "left" => json!({"shadowOffsetX":(x - step).max(-100)}),
            "right" => json!({"shadowOffsetX":(x + step).min(100)}),
            "up" => json!({"shadowOffsetY":(y - step).max(-100)}),
            "down" => json!({"shadowOffsetY":(y + step).min(100)}),
            "reset" => json!({"shadowOffsetX":0,"shadowOffsetY":0}),
            _ => bail!("mode는 none·on·left·right·up·down·reset입니다."),
        };
        if matches!(mode, "left" | "right" | "up" | "down") && p["shadowType"].as_i64() == Some(0) {
            bail!("그림자가 없는 개체입니다. 먼저 mode on으로 그림자를 켜세요.");
        }
        self.set_object_properties(a, &props)?;
        let after = self.object_properties(a)?;
        Ok(
            json!({"shadowType":after["props"]["shadowType"],"shadowOffsetX":after["props"]["shadowOffsetX"],"shadowOffsetY":after["props"]["shadowOffsetY"]}),
        )
    }

    /// 개체 모양 복사: the look of a shape or picture (no position, size or text).
    pub fn object_format(&self, a: ObjectAddress) -> Result<Value> {
        let v = self.object_properties(a)?;
        if !matches!(
            v["kind"].as_str(),
            Some("shape" | "picture" | "textbox" | "group")
        ) {
            bail!("도형·그림의 모양만 복사할 수 있습니다.");
        }
        let mut out = Map::new();
        if let Some(props) = v["props"].as_object() {
            for (k, val) in props {
                if FORMAT_KEYS.contains(&k.as_str()) {
                    out.insert(k.clone(), val.clone());
                }
            }
        }
        out.insert("kind".into(), v["kind"].clone());
        Ok(Value::Object(out))
    }
    /// 개체 모양 붙이기: keys the target cannot take (shape ↔ picture) are skipped.
    pub fn apply_object_format(&mut self, a: ObjectAddress, format: &Value) -> Result<()> {
        let src = format
            .as_object()
            .ok_or_else(|| anyhow!("format은 object.format.get 결과여야 합니다."))?;
        let target = self.object_properties(a)?;
        let picture = target["kind"] == "picture";
        let allowed: &[(&str, Kind)] = if picture { PICTURE } else { SHAPE };
        let mut props = Map::new();
        for (k, v) in src {
            if !FORMAT_KEYS.contains(&k.as_str()) {
                continue;
            }
            if allowed.iter().any(|(key, _)| key == k) && target["props"].get(k) != Some(v) {
                props.insert(k.clone(), v.clone());
            }
        }
        if props.is_empty() {
            return Ok(());
        }
        self.set_object_properties(a, &Value::Object(props))
    }

    /// Drawable objects in document order (body paragraphs only).
    fn drawable_objects(&self) -> Vec<(ObjectAddress, &'static str)> {
        self.objects()
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                let kind = match o["kind"].as_str()? {
                    "picture" => "picture",
                    "shape" => "shape",
                    "textbox" => "textbox",
                    "group" => "group",
                    "equation" => "equation",
                    _ => return None,
                };
                Some((serde_json::from_value(o["address"].clone()).ok()?, kind))
            })
            .collect()
    }
    /// ShapeObjectNextObject / PrevObject: the drawable object after or before `from`
    /// (or the caret paragraph), optionally wrapping around the document.
    pub fn neighbor_object(
        &self,
        from: Option<ObjectAddress>,
        next: bool,
        wrap: bool,
    ) -> Result<Option<(ObjectAddress, &'static str)>> {
        let list = self.drawable_objects();
        if list.is_empty() {
            return Ok(None);
        }
        let key = |a: &ObjectAddress| (a.section, a.paragraph, a.control);
        let pos = match from {
            Some(a) => {
                let k = key(&a);
                if next {
                    list.iter().position(|(o, _)| key(o) > k)
                } else {
                    list.iter().rposition(|(o, _)| key(o) < k)
                }
            }
            None => {
                let c = self.cursor;
                let k = (c.section, c.paragraph, if next { 0 } else { usize::MAX });
                if next {
                    list.iter().position(|(o, _)| key(o) >= k)
                } else {
                    list.iter().rposition(|(o, _)| key(o) <= k)
                }
            }
        };
        let pos = match (pos, wrap) {
            (Some(p), _) => p,
            (None, true) if next => 0,
            (None, true) => list.len() - 1,
            (None, false) => return Ok(None),
        };
        Ok(Some(list[pos]))
    }

    /// ChangeConnectionLineType*: straight / stroke (꺾인) / arc (곡선), keeping arrows.
    pub fn set_connector_type(&mut self, a: ObjectAddress, kind: &str) -> Result<()> {
        use hwpcraft_doc::model::shape::LinkLineType as L;
        self.guard_object_lock(a)?;
        let class = match self.object(a)? {
            Control::Shape(s) => match s.as_ref() {
                ShapeObject::Line(l) => l
                    .connector
                    .as_ref()
                    .map(|c| c.link_type as u8 % 3)
                    .ok_or_else(|| anyhow!("연결선이 아닙니다."))?,
                _ => bail!("연결선이 아닙니다."),
            },
            _ => bail!("연결선이 아닙니다."),
        };
        let base = match kind {
            "straight" => 0u8,
            "stroke" => 3,
            "arc" => 6,
            _ => bail!("kind는 straight·stroke·arc입니다."),
        };
        let next = match base + class {
            0 => L::StraightNoArrow,
            1 => L::StraightOneWay,
            2 => L::StraightBoth,
            3 => L::StrokeNoArrow,
            4 => L::StrokeOneWay,
            5 => L::StrokeBoth,
            6 => L::ArcNoArrow,
            7 => L::ArcOneWay,
            _ => L::ArcBoth,
        };
        self.transaction(|s| {
            let section = s
                .core
                .document_mut()
                .sections
                .get_mut(a.section)
                .ok_or_else(|| anyhow!("구역이 없습니다."))?;
            let mut changed = false;
            if let Some(Control::Shape(sh)) = section
                .paragraphs
                .get_mut(a.paragraph)
                .and_then(|p| p.controls.get_mut(a.control))
                && let ShapeObject::Line(l) = sh.as_mut()
                && let Some(c) = l.connector.as_mut()
            {
                changed = c.link_type != next;
                c.link_type = next;
            }
            if changed {
                section.raw_stream = None;
                s.core.update_connectors_in_section(a.section);
            }
            s.relayout_object(a)
        })
    }

    /// TextBoxAlign* (3 × 3): paragraph alignment of every text box paragraph plus
    /// the box's vertical alignment.
    pub fn align_textbox(
        &mut self,
        a: ObjectAddress,
        horizontal: Option<&str>,
        vertical: Option<&str>,
    ) -> Result<()> {
        if horizontal.is_none() && vertical.is_none() {
            bail!("horizontal 또는 vertical을 지정하세요.");
        }
        let count = self.textbox_list(a)?.len();
        let h = horizontal
            .map(|h| match h {
                "left" => Ok("left"),
                "center" => Ok("center"),
                "right" => Ok("right"),
                "justify" => Ok("justify"),
                _ => Err(anyhow!("horizontal은 left·center·right·justify입니다.")),
            })
            .transpose()?;
        let v = vertical
            .map(|v| match v {
                "top" => Ok("Top"),
                "center" => Ok("Center"),
                "bottom" => Ok("Bottom"),
                _ => Err(anyhow!("vertical은 top·center·bottom입니다.")),
            })
            .transpose()?;
        self.guard_object_lock(a)?;
        self.transaction(|s| {
            if let Some(h) = h {
                let props = json!({"alignment":h}).to_string();
                for i in 0..count {
                    s.core
                        .apply_para_format_in_cell_native(
                            a.section,
                            a.paragraph,
                            a.control,
                            0,
                            i,
                            &props,
                        )
                        .map_err(err)?;
                }
            }
            if let Some(v) = v {
                s.put_object_props(a, &json!({"tbVerticalAlign":v}))?;
            } else {
                s.relayout_object(a)?;
            }
            Ok(())
        })
    }
    /// Current text direction of a text box: horizontal · vertical · vertical_all.
    pub fn textbox_direction(&self, a: ObjectAddress) -> Result<&'static str> {
        let tb = match self.object(a)? {
            Control::Shape(sh) => sh.drawing().and_then(|d| d.text_box.as_ref()),
            _ => None,
        }
        .ok_or_else(|| anyhow!("글상자가 아닙니다."))?;
        Ok(match (tb.list_attr & 0x07 == 1, tb.vertical_all) {
            (true, true) => "vertical_all",
            (true, false) => "vertical",
            _ => "horizontal",
        })
    }
    /// 세로 글상자 / 영문 세움 (TextBoxDirection, DrawObjCreatorMulti*VertTextBox).
    /// `code`: 0 horizontal, 1 vertical, 2 vertical_all (see `textbox_direction_code`).
    pub fn set_textbox_direction(&mut self, a: ObjectAddress, code: u8) -> Result<()> {
        self.textbox_direction(a)?;
        self.guard_object_lock(a)?;
        self.transaction(|s| s.textbox_direction_raw(a, code))
    }
    fn textbox_direction_raw(&mut self, a: ObjectAddress, code: u8) -> Result<()> {
        let section = self
            .core
            .document_mut()
            .sections
            .get_mut(a.section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let changed;
        if let Some(Control::Shape(sh)) = section
            .paragraphs
            .get_mut(a.paragraph)
            .and_then(|p| p.controls.get_mut(a.control))
            && let Some(tb) = sh.drawing_mut().and_then(|d| d.text_box.as_mut())
        {
            let attr = (tb.list_attr & !0x07) | u32::from(code != 0);
            let all = code == 2;
            changed = tb.list_attr != attr || tb.vertical_all != all;
            tb.list_attr = attr;
            tb.vertical_all = all;
        } else {
            bail!("글상자가 아닙니다.");
        }
        if changed {
            section.raw_stream = None;
        }
        self.relayout_object(a)
    }

    fn picture_bytes(&self, a: ObjectAddress) -> Result<(String, Vec<u8>)> {
        let id = match self.object(a)? {
            Control::Picture(p) => p.image_attr.bin_data_id,
            _ => bail!("그림이 아닙니다."),
        };
        let doc = self.core.document();
        let content = doc
            .bin_data_content
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("그림 데이터가 문서에 없습니다 (연결 그림이거나 손상)."))?;
        let bytes = content.data.load();
        if bytes.is_empty() {
            bail!("그림 데이터가 비었습니다.");
        }
        let ext = content
            .extension
            .trim_start_matches('.')
            .to_ascii_lowercase();
        let ext = if ext.is_empty() {
            match bytes.first().copied() {
                Some(0x89) => "png",
                Some(0xff) => "jpg",
                Some(0x42) => "bmp",
                Some(0x47) => "gif",
                _ => "bin",
            }
            .to_owned()
        } else {
            ext
        };
        Ok((ext, bytes))
    }
    fn write_new_file(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
        hwpcraft_formats::file_access::check_write(&path.to_string_lossy())
            .map_err(anyhow::Error::msg)?;
        if path.exists() && !overwrite {
            bail!(
                "이미 있는 파일입니다: {} (overwrite:true로 덮어쓰기)",
                path.display()
            );
        }
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
            && !dir.is_dir()
        {
            bail!("폴더가 없습니다: {}", dir.display());
        }
        hwpcraft_formats::atomic_write(path, bytes)?;
        Ok(())
    }
    /// PictureSaveSelected: the original bytes of one picture, extension corrected.
    pub fn save_picture(&self, a: ObjectAddress, path: &Path, overwrite: bool) -> Result<PathBuf> {
        let (ext, bytes) = self.picture_bytes(a)?;
        let target = path.with_extension(&ext);
        Self::write_new_file(&target, &bytes, overwrite)?;
        Ok(target)
    }
    /// PictureSaveAll: every picture as `<prefix>N.<ext>` in `dir`.
    pub fn save_all_pictures(
        &self,
        dir: &Path,
        prefix: &str,
        overwrite: bool,
    ) -> Result<Vec<PathBuf>> {
        if prefix.is_empty()
            || prefix.chars().any(|c| {
                c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            })
        {
            bail!("prefix는 파일 이름에 쓸 수 있는 글자만 담아야 합니다.");
        }
        if !dir.is_dir() {
            bail!("폴더가 없습니다: {}", dir.display());
        }
        let pictures: Vec<ObjectAddress> = self
            .drawable_objects()
            .into_iter()
            .filter(|(_, k)| *k == "picture")
            .map(|(a, _)| a)
            .collect();
        if pictures.is_empty() {
            bail!("문서에 그림이 없습니다.");
        }
        let mut planned = Vec::new();
        for (i, a) in pictures.iter().enumerate() {
            let (ext, bytes) = self.picture_bytes(*a)?;
            let target = dir.join(format!("{prefix}{}.{ext}", i + 1));
            hwpcraft_formats::file_access::check_write(&target.to_string_lossy())
                .map_err(anyhow::Error::msg)?;
            if target.exists() && !target.is_file() {
                bail!(
                    "그림 저장 대상이 일반 파일이 아닙니다: {}",
                    target.display()
                );
            }
            if target.exists() && !overwrite {
                bail!(
                    "이미 있는 파일입니다: {} (overwrite:true로 덮어쓰기)",
                    target.display()
                );
            }
            planned.push((target, bytes));
        }
        let mut out = Vec::new();
        for (target, bytes) in planned {
            Self::write_new_file(&target, &bytes, overwrite)?;
            out.push(target);
        }
        Ok(out)
    }

    /// InsertAutoNum / InsertCaptionNum at the caret (body paragraphs).
    pub fn insert_auto_number(&mut self, kind: &str) -> Result<ObjectAddress> {
        use hwpcraft_doc::model::control::AutoNumberType;
        let number_type = match kind {
            "page" => AutoNumberType::Page,
            "total" => AutoNumberType::TotalPage,
            "picture" => AutoNumberType::Picture,
            "table" => AutoNumberType::Table,
            "equation" => AutoNumberType::Equation,
            _ => bail!("kind는 page·total·picture·table·equation입니다."),
        };
        self.body_insert()?;
        let c = self.cursor;
        let mut inserted = None;
        self.transaction(|s| {
            let control = s.insert_native_auto_number(
                c,
                if number_type == AutoNumberType::TotalPage {
                    "total"
                } else {
                    "page"
                },
            )?;
            let a = ObjectAddress {
                section: c.section,
                paragraph: c.paragraph,
                control,
            };
            if !matches!(
                number_type,
                AutoNumberType::Page | AutoNumberType::TotalPage
            ) {
                let section = s
                    .core
                    .document_mut()
                    .sections
                    .get_mut(a.section)
                    .ok_or_else(|| anyhow!("구역이 없습니다."))?;
                match section
                    .paragraphs
                    .get_mut(a.paragraph)
                    .and_then(|p| p.controls.get_mut(a.control))
                {
                    Some(Control::AutoNumber(n)) => n.number_type = number_type,
                    _ => bail!("번호 컨트롤을 찾지 못했습니다."),
                }
                section.raw_stream = None;
                s.relayout_section(a.section)?;
            }
            s.cursor = Cursor {
                offset: c.offset + 1,
                ..c
            };
            s.clear_selection();
            inserted = Some(a);
            Ok(())
        })?;
        inserted.ok_or_else(|| anyhow!("번호를 넣지 못했습니다."))
    }
    /// Recompose and repaginate a section after a model-only edit of its notes, note
    /// shapes or numbers. The section definition is copied into its SectionDef
    /// controls (what the HWP writer reads), then each paragraph holding a footnote,
    /// an endnote or an auto number (the first paragraph when none does) is laid out
    /// again in one batch. RHWP's empty endnote-shape apply did this before, but it
    /// also numbers the section's endnotes again from its start number, which undid
    /// numbers carried on from earlier sections.
    pub(crate) fn relayout_section(&mut self, section: usize) -> Result<()> {
        let sec = self
            .core
            .document_mut()
            .sections
            .get_mut(section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let endnotes = &mut sec.section_def.endnote_shape;
        endnotes.attr = endnotes.encode_attr();
        let def = sec.section_def.clone();
        for control in sec
            .paragraphs
            .iter_mut()
            .flat_map(|p| p.controls.iter_mut())
        {
            if let Control::SectionDef(d) = control {
                **d = def.clone();
            }
        }
        sec.raw_stream = None;
        let mut paragraphs: Vec<usize> = sec
            .paragraphs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.controls.iter().any(|c| {
                    matches!(
                        c,
                        Control::Footnote(_) | Control::Endnote(_) | Control::AutoNumber(_)
                    )
                })
            })
            .map(|(i, _)| i)
            .collect();
        if paragraphs.is_empty() {
            paragraphs.push(0);
        }
        self.deferred_layout(|s| {
            for paragraph in paragraphs {
                s.core
                    .relayout_paragraph_native(section, paragraph, None)
                    .map_err(err)?;
            }
            Ok(())
        })
    }

    /// Footnote / endnote in the caret paragraph (first one at or after the caret).
    pub fn note_at_caret(&self) -> Option<ObjectAddress> {
        let c = self.cursor;
        if c.cell.is_some() {
            return None;
        }
        let p = self
            .core
            .document()
            .sections
            .get(c.section)?
            .paragraphs
            .get(c.paragraph)?;
        let notes: Vec<(usize, usize)> = p
            .controls
            .iter()
            .enumerate()
            .zip(p.control_text_positions())
            .filter(|((_, ctrl), _)| matches!(ctrl, Control::Footnote(_) | Control::Endnote(_)))
            .map(|((i, _), pos)| (i, pos))
            .collect();
        let control = notes
            .iter()
            .find(|(_, pos)| *pos >= c.offset)
            .or_else(|| notes.last())
            .map(|(i, _)| *i)?;
        Some(ObjectAddress {
            section: c.section,
            paragraph: c.paragraph,
            control,
        })
    }
    /// FootnoteToEndnote / EndnoteToFootnote: swap the control kind in place and
    /// renumber both kinds in the section (RHWP numbers notes per section).
    pub fn convert_note(&mut self, a: ObjectAddress) -> Result<ObjectAddress> {
        use hwpcraft_doc::model::footnote::{Endnote, Footnote};
        if !matches!(self.object(a)?, Control::Footnote(_) | Control::Endnote(_)) {
            bail!("각주/미주가 아닙니다.");
        }
        self.transaction(|s| {
            let fn_start = s.note_shape(a.section, false)?["startNumber"]
                .as_u64()
                .unwrap_or(1)
                .max(1) as u16;
            let en_start = s.note_shape(a.section, true)?["startNumber"]
                .as_u64()
                .unwrap_or(1)
                .max(1) as u16;
            let section = s
                .core
                .document_mut()
                .sections
                .get_mut(a.section)
                .ok_or_else(|| anyhow!("구역이 없습니다."))?;
            let slot = section
                .paragraphs
                .get_mut(a.paragraph)
                .and_then(|p| p.controls.get_mut(a.control))
                .ok_or_else(|| anyhow!("주석을 찾지 못했습니다."))?;
            let taken = std::mem::replace(slot, Control::AutoNumber(Default::default()));
            *slot = match taken {
                Control::Footnote(n) => Control::Endnote(Box::new(Endnote {
                    number: n.number,
                    paragraphs: n.paragraphs,
                    before_decoration_letter: n.before_decoration_letter,
                    after_decoration_letter: n.after_decoration_letter,
                    number_shape: n.number_shape,
                    instance_id: n.instance_id,
                    list_header_property: n.list_header_property,
                    ..Default::default()
                })),
                Control::Endnote(n) => Control::Footnote(Box::new(Footnote {
                    number: n.number,
                    paragraphs: n.paragraphs,
                    before_decoration_letter: n.before_decoration_letter,
                    after_decoration_letter: n.after_decoration_letter,
                    number_shape: n.number_shape,
                    instance_id: n.instance_id,
                    list_header_property: n.list_header_property,
                    ..Default::default()
                })),
                other => {
                    *slot = other;
                    bail!("각주/미주가 아닙니다.");
                }
            };
            let (mut f, mut e) = (fn_start, en_start);
            for p in &mut section.paragraphs {
                for ctrl in &mut p.controls {
                    match ctrl {
                        Control::Footnote(n) => {
                            n.number = f;
                            flip_note_autonumber(&mut n.paragraphs, f, false);
                            f = f.saturating_add(1);
                        }
                        Control::Endnote(n) => {
                            n.number = e;
                            flip_note_autonumber(&mut n.paragraphs, e, true);
                            e = e.saturating_add(1);
                        }
                        _ => {}
                    }
                }
            }
            section.raw_stream = None;
            s.relayout_section(a.section)?;
            Ok(())
        })?;
        Ok(a)
    }

    /// NoteNumShape* / NoteLine*: the footnote or endnote shape of a section.
    pub fn note_shape(&self, section: usize, endnote: bool) -> Result<Value> {
        if endnote {
            let v: Value =
                serde_json::from_str(&self.core.get_endnote_shape_native(section).map_err(err)?)?;
            return Ok(v);
        }
        let shape = &self
            .core
            .document()
            .sections
            .get(section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?
            .section_def
            .footnote_shape;
        let ch = |c: char| {
            if c == '\0' {
                String::new()
            } else {
                c.to_string()
            }
        };
        let color = shape.separator_color;
        Ok(json!({
            "ok": true,
            "numberFormat": note_number_format_name(shape.number_format),
            "userChar": ch(shape.user_char),
            "prefixChar": ch(shape.prefix_char),
            "suffixChar": ch(shape.suffix_char),
            "startNumber": shape.start_number,
            "separatorEnabled": shape.separator_length != 0 || shape.separator_line_type != 0 || shape.separator_line_width != 0,
            "separatorLength": shape.separator_length,
            // Same slots as the setter and RHWP's endnote getter: Hangul's "구분선 아래"
            // is `note_spacing`, "주석 사이" the undocumented `raw_unknown`.
            "separatorMarginTop": shape.separator_margin_top,
            "separatorMarginBottom": shape.note_spacing,
            "noteSpacing": shape.raw_unknown,
            "separatorLineType": shape.separator_line_type,
            "separatorLineWidth": shape.separator_line_width,
            "separatorColor": format!("#{:02x}{:02x}{:02x}", color & 0xff, (color >> 8) & 0xff, (color >> 16) & 0xff),
            "numberCodeSuperscript": shape.number_code_superscript,
            "printInlineAfterText": shape.print_inline_after_text,
            "numbering": note_numbering_name(shape.numbering),
            "placement": note_placement_name(shape.placement),
        }))
    }
    /// Apply note shape properties (same keys as `note.shape.get`). Endnotes go through
    /// RHWP; footnotes are edited on the section definition with the same semantics.
    pub fn set_note_shape(&mut self, section: usize, endnote: bool, props: &Value) -> Result<()> {
        let o = props
            .as_object()
            .ok_or_else(|| anyhow!("props는 JSON 객체여야 합니다."))?;
        if o.is_empty() {
            bail!("바꿀 속성을 지정하세요.");
        }
        for (k, v) in o {
            let ok = match k.as_str() {
                "numberFormat" => v
                    .as_str()
                    .is_some_and(|f| note_number_format_from(f).is_some()),
                // HWP keeps each decoration character in one UTF-16 unit (WCHAR).
                "userChar" | "prefixChar" | "suffixChar" => v.as_str().is_some_and(|c| {
                    c.chars().count() <= 1
                        && c.chars()
                            .all(|ch| !ch.is_control() && u32::from(ch) <= 0xFFFF)
                }),
                "startNumber" => v.as_u64().is_some_and(|n| (1..=9999).contains(&n)),
                // HWP keeps the length in 16 bits; -1 (5 cm) and, for endnotes, -4
                // (column width) are the presets Hangul writes (RHWP samples).
                "separatorLength" => v.as_i64().is_some_and(|n| {
                    (-1..=i64::from(i16::MAX)).contains(&n) || (endnote && n == -4)
                }),
                "separatorMarginTop" | "separatorMarginBottom" | "noteSpacing" => {
                    v.as_i64().is_some_and(|n| (0..=28_346).contains(&n))
                }
                "separatorLineType" => v.as_u64().is_some_and(|n| n <= 17),
                "separatorLineWidth" => v.as_u64().is_some_and(|n| n <= 15),
                "separatorColor" => v.as_str().is_some_and(|c| bgr_of(c).is_ok()),
                "numberCodeSuperscript" | "printInlineAfterText" | "separatorEnabled" => {
                    v.is_boolean()
                }
                "numbering" => v
                    .as_str()
                    .is_some_and(|n| matches!(n, "continue" | "section" | "page")),
                "placement" => v
                    .as_str()
                    .is_some_and(|n| matches!(n, "eachColumn" | "belowText" | "rightColumn")),
                _ => bail!("각주·미주 모양에 없는 속성: {k}"),
            };
            if !ok {
                bail!("잘못된 각주·미주 모양 값: {k}");
            }
        }
        if self.core.document().sections.get(section).is_none() {
            bail!("구역이 없습니다.");
        }
        let json = props.to_string();
        self.transaction(|s| {
            if endnote {
                let raw = s
                    .core
                    .apply_endnote_shape_native(section, &json)
                    .map_err(err)?;
                ok_value(&raw)?;
                // RHWP turns a negative preset into 0 (no line); keep the preset.
                if let Some(n) = o.get("separatorLength").and_then(Value::as_i64)
                    && n < 0
                    && o.get("separatorEnabled") != Some(&Value::Bool(false))
                {
                    let sec = s
                        .core
                        .document_mut()
                        .sections
                        .get_mut(section)
                        .ok_or_else(|| anyhow!("구역이 없습니다."))?;
                    sec.section_def.endnote_shape.separator_length = n as i32;
                    sec.raw_stream = None;
                    s.relayout_section(section)?;
                }
                return Ok(());
            }
            {
                let sec = s
                    .core
                    .document_mut()
                    .sections
                    .get_mut(section)
                    .ok_or_else(|| anyhow!("구역이 없습니다."))?;
                let shape = &mut sec.section_def.footnote_shape;
                let first = |v: &Value| v.as_str().and_then(|c| c.chars().next()).unwrap_or('\0');
                for (k, v) in o {
                    match k.as_str() {
                        "numberFormat" => {
                            if let Some(f) = v.as_str().and_then(note_number_format_from) {
                                shape.number_format = f;
                            }
                        }
                        "userChar" => shape.user_char = first(v),
                        "prefixChar" => shape.prefix_char = first(v),
                        "suffixChar" => shape.suffix_char = first(v),
                        "startNumber" => shape.start_number = v.as_u64().unwrap_or(1) as u16,
                        "separatorLength" => {
                            shape.separator_length = v.as_i64().unwrap_or(0) as i32
                        }
                        "separatorMarginTop" => {
                            shape.separator_margin_top = v.as_i64().unwrap_or(0) as i16;
                            shape.separator_margin_bottom = v.as_i64().unwrap_or(0) as i16;
                        }
                        "separatorMarginBottom" => {
                            shape.note_spacing = v.as_i64().unwrap_or(0) as i16
                        }
                        "noteSpacing" => shape.raw_unknown = v.as_i64().unwrap_or(0) as u16,
                        "separatorLineType" => {
                            shape.separator_line_type = v.as_u64().unwrap_or(0) as u8
                        }
                        "separatorLineWidth" => {
                            shape.separator_line_width = v.as_u64().unwrap_or(0) as u8
                        }
                        "separatorColor" => {
                            shape.separator_color = bgr_of(v.as_str().unwrap_or("#000000"))? as u32;
                        }
                        "numberCodeSuperscript" => {
                            shape.number_code_superscript = v.as_bool().unwrap_or(false)
                        }
                        "printInlineAfterText" => {
                            shape.print_inline_after_text = v.as_bool().unwrap_or(false)
                        }
                        "numbering" => {
                            use hwpcraft_doc::model::footnote::FootnoteNumbering as N;
                            shape.numbering = match v.as_str() {
                                Some("section") => N::RestartSection,
                                Some("page") => N::RestartPage,
                                _ => N::Continue,
                            };
                        }
                        "placement" => {
                            use hwpcraft_doc::model::footnote::FootnotePlacement as P;
                            shape.placement = match v.as_str() {
                                Some("belowText") => P::BelowText,
                                Some("rightColumn") => P::RightColumn,
                                _ => P::EachColumn,
                            };
                        }
                        "separatorEnabled" if v == false => {
                            shape.separator_length = 0;
                            shape.separator_line_type = 0;
                            shape.separator_line_width = 0;
                        }
                        _ => {}
                    }
                }
                shape.attr = shape.encode_attr();
                let code = hwpcraft_doc::model::footnote::FootnoteShape::number_format_attr_code(
                    shape.number_format,
                ) as u8;
                let (prefix, suffix) = (shape.prefix_char, shape.suffix_char);
                let start = shape.start_number.max(1);
                let mut n = start;
                for p in &mut sec.paragraphs {
                    for ctrl in &mut p.controls {
                        if let Control::Footnote(note) = ctrl {
                            note.number = n;
                            sync_note_number(
                                &mut note.paragraphs,
                                &mut note.before_decoration_letter,
                                &mut note.after_decoration_letter,
                                &mut note.number_shape,
                                n,
                                code,
                                prefix,
                                suffix,
                                false,
                            );
                            n = n.saturating_add(1);
                        }
                    }
                }
                sec.raw_stream = None;
            }
            s.relayout_section(section)
        })
    }

    /// DeleteField: remove the click-here field at the caret (or by id), keeping its text.
    pub fn delete_field(&mut self, field_id: Option<u32>) -> Result<bool> {
        let c = self.cursor;
        let (section, paragraph, offset, cell) = match field_id {
            Some(id) => {
                let list: Value = serde_json::from_str(&self.core.get_field_list_json())?;
                let items = list
                    .as_array()
                    .cloned()
                    .or_else(|| list["fields"].as_array().cloned())
                    .unwrap_or_default();
                let f = items
                    .iter()
                    .find(|f| f["fieldId"].as_u64() == Some(u64::from(id)))
                    .ok_or_else(|| anyhow!("누름틀 {id}을(를) 찾지 못했습니다."))?;
                if f["cellField"] == true || f["listId"].as_u64().unwrap_or(0) != 0 {
                    bail!("표·글상자 안 누름틀은 커서를 그 안에 놓고 field_id 없이 지우세요.");
                }
                let para_in_list = f["paraInList"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("누름틀 위치 정보가 없습니다."))?
                    as usize;
                // Root list paragraphs run across sections.
                let mut rest = para_in_list;
                let mut found = None;
                for (si, sec) in self.core.document().sections.iter().enumerate() {
                    if rest < sec.paragraphs.len() {
                        found = Some((si, rest));
                        break;
                    }
                    rest -= sec.paragraphs.len();
                }
                let (section, paragraph) =
                    found.ok_or_else(|| anyhow!("누름틀 문단을 찾지 못했습니다."))?;
                let offset = f["startCharIdx"].as_u64().unwrap_or(0) as usize;
                (section, paragraph, offset, None)
            }
            None => (c.section, c.paragraph, c.offset, c.cell),
        };
        let has_field = {
            let p = match cell {
                Some(t) => {
                    let host = self
                        .core
                        .document()
                        .sections
                        .get(section)
                        .and_then(|s| s.paragraphs.get(paragraph))
                        .ok_or_else(|| anyhow!("문단이 없습니다."))?;
                    match host.controls.get(t.control) {
                        Some(Control::Table(tb)) => tb
                            .cells
                            .get(t.index)
                            .and_then(|cell| cell.paragraphs.get(t.paragraph)),
                        Some(Control::Shape(sh)) => sh
                            .drawing()
                            .and_then(|d| d.text_box.as_ref())
                            .and_then(|tb| tb.paragraphs.get(t.paragraph)),
                        _ => None,
                    }
                }
                None => self
                    .core
                    .document()
                    .sections
                    .get(section)
                    .and_then(|s| s.paragraphs.get(paragraph)),
            }
            .ok_or_else(|| anyhow!("문단이 없습니다."))?;
            p.field_ranges
                .iter()
                .any(|r| r.start_char_idx <= offset && offset <= r.end_char_idx)
        };
        if !has_field {
            return Ok(false);
        }
        self.transaction(|s| {
            let raw = match cell {
                Some(t) => {
                    let textbox = matches!(
                        s.core.document().sections[section].paragraphs[paragraph]
                            .controls
                            .get(t.control),
                        Some(Control::Shape(_))
                    );
                    s.core.remove_field_at_in_cell(
                        section,
                        paragraph,
                        t.control,
                        t.index,
                        t.paragraph,
                        offset,
                        textbox,
                    )
                }
                None => s.core.remove_field_at(section, paragraph, offset),
            }
            .map_err(err)?;
            ok_value(&raw)?;
            s.clear_selection();
            Ok(())
        })?;
        Ok(true)
    }
}

/// Keep a note's inner automatic number in step with its kind and number.
fn flip_note_autonumber(paragraphs: &mut [Paragraph], number: u16, endnote: bool) {
    use hwpcraft_doc::model::control::AutoNumberType;
    for p in paragraphs {
        for c in &mut p.controls {
            if let Control::AutoNumber(n) = c
                && matches!(
                    n.number_type,
                    AutoNumberType::Footnote | AutoNumberType::Endnote
                )
            {
                n.number_type = if endnote {
                    AutoNumberType::Endnote
                } else {
                    AutoNumberType::Footnote
                };
                n.number = number;
                n.assigned_number = number;
            }
        }
    }
}
/// Mirror of RHWP's endnote sync for footnotes: decoration letters, number shape and the
/// inner automatic number follow the section's note shape.
#[allow(clippy::too_many_arguments)]
fn sync_note_number(
    paragraphs: &mut [Paragraph],
    before: &mut u16,
    after: &mut u16,
    number_shape: &mut u32,
    number: u16,
    code: u8,
    prefix: char,
    suffix: char,
    endnote: bool,
) {
    use hwpcraft_doc::model::control::AutoNumberType;
    *before = if prefix == '\0' { 0 } else { prefix as u16 };
    *after = if suffix == '\0' { 0 } else { suffix as u16 };
    *number_shape = u32::from(code);
    let kind = if endnote {
        AutoNumberType::Endnote
    } else {
        AutoNumberType::Footnote
    };
    for p in paragraphs {
        for c in &mut p.controls {
            if let Control::AutoNumber(n) = c
                && n.number_type == kind
            {
                n.format = code;
                n.prefix_char = prefix;
                n.suffix_char = suffix;
                n.number = number;
                n.assigned_number = number;
            }
        }
    }
}
fn note_number_format_name(f: hwpcraft_doc::model::footnote::NumberFormat) -> &'static str {
    use hwpcraft_doc::model::footnote::NumberFormat as F;
    match f {
        F::Digit => "digit",
        F::CircledDigit => "circledDigit",
        F::UpperRoman => "upperRoman",
        F::LowerRoman => "lowerRoman",
        F::UpperAlpha => "upperAlpha",
        F::LowerAlpha => "lowerAlpha",
        F::CircledUpperAlpha => "circledUpperAlpha",
        F::CircledLowerAlpha => "circledLowerAlpha",
        F::HangulSyllable => "hangulSyllable",
        F::CircledHangulSyllable => "circledHangulSyllable",
        F::HangulJamo => "hangulJamo",
        F::CircledHangulJamo => "circledHangulJamo",
        F::HangulDigit => "hangulDigit",
        F::HanjaDigit => "hanjaDigit",
        F::CircledHanjaDigit => "circledHanjaDigit",
        F::HanjaGapEul => "hanjaGapEul",
        F::HanjaGapEulHanja => "hanjaGapEulHanja",
        F::FourSymbol => "fourSymbol",
        F::UserChar => "userChar",
    }
}
fn note_number_format_from(name: &str) -> Option<hwpcraft_doc::model::footnote::NumberFormat> {
    use hwpcraft_doc::model::footnote::NumberFormat as F;
    Some(match name {
        "digit" => F::Digit,
        "circledDigit" => F::CircledDigit,
        "upperRoman" => F::UpperRoman,
        "lowerRoman" => F::LowerRoman,
        "upperAlpha" => F::UpperAlpha,
        "lowerAlpha" => F::LowerAlpha,
        "circledUpperAlpha" => F::CircledUpperAlpha,
        "circledLowerAlpha" => F::CircledLowerAlpha,
        "hangulSyllable" => F::HangulSyllable,
        "circledHangulSyllable" => F::CircledHangulSyllable,
        "hangulJamo" => F::HangulJamo,
        "circledHangulJamo" => F::CircledHangulJamo,
        "hangulDigit" => F::HangulDigit,
        "hanjaDigit" => F::HanjaDigit,
        "circledHanjaDigit" => F::CircledHanjaDigit,
        "hanjaGapEul" => F::HanjaGapEul,
        "hanjaGapEulHanja" => F::HanjaGapEulHanja,
        "fourSymbol" => F::FourSymbol,
        "userChar" => F::UserChar,
        _ => return None,
    })
}
fn note_numbering_name(n: hwpcraft_doc::model::footnote::FootnoteNumbering) -> &'static str {
    use hwpcraft_doc::model::footnote::FootnoteNumbering as N;
    match n {
        N::RestartSection => "section",
        N::RestartPage => "page",
        _ => "continue",
    }
}
fn note_placement_name(p: hwpcraft_doc::model::footnote::FootnotePlacement) -> &'static str {
    use hwpcraft_doc::model::footnote::FootnotePlacement as P;
    match p {
        P::BelowText => "belowText",
        P::RightColumn => "rightColumn",
        _ => "eachColumn",
    }
}
