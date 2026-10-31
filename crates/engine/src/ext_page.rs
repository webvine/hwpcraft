//! Owner: shell/page area. 쪽/보기/파일 서비스: 쪽 테두리/배경·머리말/꼬리말·쪽 번호·새 번호·감추기·문서 정보.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Lengths in parameters are millimetres; RHWP stores HWPUNIT (1/7200 inch).
use super::*;

mod print_layout;
use hwpcraft_doc::hidden_text::HiddenTextOptions;
use hwpcraft_doc::model::control::PageNumberPos;
use hwpcraft_doc::model::document::Document;
use hwpcraft_doc::model::header_footer::{HeaderFooterApply, MasterPage};
use hwpcraft_doc::model::paragraph::Paragraph as DocParagraph;
use hwpcraft_doc::pii_scan::PiiKind;
pub use print_layout::{PrintLayout, Watermark};
use serde::{Deserialize, Serialize};

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "view.marks.get",
        "문단 부호·조판 부호·투명 선 보기 조회",
        "",
    ),
    (
        "view.marks",
        "문단 부호·조판 부호·투명 선 보기 (화면 전용, 저장 안 함)",
        "Ctrl+G,T / Ctrl+G,C",
    ),
    (
        "page.header_footer.text.get",
        "머리말/꼬리말 내용·필드 조회",
        "",
    ),
    (
        "page.header_footer.text",
        "머리말/꼬리말 내용·필드(쪽 번호·전체 쪽수·파일 이름)",
        "",
    ),
    ("page.section.get", "구역 설정 조회", ""),
    ("page.section", "구역 설정", ""),
    ("security.privacy.scan", "개인 정보·숨은 글자 찾기", ""),
    ("security.privacy.mask", "개인 정보 가리기", ""),
    ("file.save_hml", "HML로 저장 (HML 원본 문서만)", ""),
    (
        "review.history.list",
        "문서 이력 목록 (HWP에 기록된 버전)",
        "",
    ),
    (
        "review.history.save_last",
        "마지막 이력 버전을 HML로 저장",
        "",
    ),
    ("file.save_pages", "쪽 범위만 저장 (현재 쪽만)", ""),
    ("page.border_fill.get", "쪽 테두리/배경 조회", ""),
    ("page.border_fill", "쪽 테두리/배경", ""),
    ("page.header", "머리말 (마당에서 만들기)", "Ctrl+N,H"),
    ("page.footer", "꼬리말 (마당에서 만들기)", "Ctrl+N,H"),
    ("page.header_footer.list", "머리말/꼬리말 목록", ""),
    ("page.header_footer.delete", "머리말/꼬리말 지우기", ""),
    ("page.number.get", "쪽 번호 매기기 조회", ""),
    ("page.number", "쪽 번호 매기기", "Ctrl+N,P"),
    ("page.new_number", "새 번호로 시작", ""),
    ("page.line_numbers.get", "줄 번호 설정 조회", ""),
    ("page.line_numbers", "줄 번호 넣기", ""),
    (
        "page.line_numbers.skip.get",
        "현재 문단 줄 번호 건너뜀 조회",
        "",
    ),
    ("page.line_numbers.skip", "현재 문단 줄 번호 건너뜀", ""),
    ("page.hide.get", "현재 쪽만 감추기 조회", ""),
    ("page.hide", "현재 쪽만 감추기", ""),
    ("page.master.list", "바탕쪽 목록", ""),
    ("page.label", "라벨 문서 만들기", ""),
    ("page.column_def_break", "단 정의 나누기", "Ctrl+Alt+Enter"),
    ("page.section_break", "구역 나누기", "Alt+Shift+Enter"),
    ("file.template.list", "문서마당 목록", ""),
    ("file.template.new", "문서마당에서 새 문서", "Ctrl+Alt+N"),
    ("file.template.save", "현재 문서를 문서마당에 저장", ""),
    ("file.template.delete", "문서마당 서식 지우기", ""),
    ("page.master.get", "바탕쪽 글 조회 (양쪽·홀수·짝수)", ""),
    (
        "page.master.set",
        "바탕쪽 만들기/고치기 (양쪽·홀수·짝수, 글)",
        "",
    ),
    ("page.master.delete", "바탕쪽 지우기 (양쪽·홀수·짝수)", ""),
    ("file.doc_info", "문서 정보", "Ctrl+Q,I"),
    ("file.print_pdf", "인쇄용 PDF 만들기 (쪽 범위)", "Ctrl+P"),
    (
        "page.margins.presets",
        "쪽 여백 사전 값 목록 (기본·좁게1·좁게2·넓게(머리말/꼬리말 포함)·좁게·넓게)",
        "",
    ),
    (
        "page.margins.preset",
        "쪽 여백 사전 값 적용 (현재 구역)",
        "",
    ),
    ("page.section.next", "이후 구역으로", ""),
    ("page.section.prev", "이전 구역으로", ""),
    ("page.copy", "쪽 복사하기 (쪽 클립보드)", ""),
    ("page.paste", "쪽 붙이기 (현재 쪽 뒤에 새 쪽으로)", ""),
    ("page.delete", "쪽 지우기", ""),
    ("file.insert", "문서 끼워 넣기 (HWP/HWPX를 커서 위치에)", ""),
    ("file.save_block", "블록 저장 (선택 영역만 새 파일로)", ""),
];

/// (command id, parameter help, JSON example object) for list_commands and
/// docs/mcp-commands.md; owner fills this for every command above.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "view.marks.get",
        "매개 변수 없음. paragraphMarks, controlCodes, transparentBorders(화면에만).",
        "{}",
    ),
    (
        "view.marks",
        "paragraphMarks, controlCodes, transparentBorders: bool(기본 false). 화면에만 적용되며 저장·실행 취소 대상이 아닙니다.",
        r#"{"paragraphMarks":true,"controlCodes":true,"transparentBorders":false}"#,
    ),
    (
        "page.header_footer.text.get",
        "header: bool, apply_to: both|odd|even(기본 both). 결과: exists, text, fields [{offset, kind}].",
        r#"{"header":true,"apply_to":"both"}"#,
    ),
    (
        "page.header_footer.text",
        "header: bool, apply_to: both|odd|even, text: 한 줄, fields: [{offset, kind: page_number|total_pages|file_name}]를 글 오프셋에 넣습니다.",
        r#"{"header":true,"apply_to":"both","text":"보고서 ","fields":[{"offset":4,"kind":"page_number"}]}"#,
    ),
    (
        "page.section.get",
        "매개 변수 없음. 지금 구역: pageNum, pageNumType, pictureNum, tableNum, equationNum(0 = 이어짐), defaultTabSpacingMm, hideHeader, hideFooter, hideMasterPage, hideBorder, hideFill, hideEmptyLine.",
        "{}",
    ),
    (
        "page.section",
        "pageNum, pageNumType, pictureNum, tableNum, equationNum(선택, 0 = 앞 구역에서 이어짐), defaultTabSpacingMm, hideHeader, hideFooter, hideMasterPage, hideBorder, hideFill, hideEmptyLine. wholeDocument면 모든 구역에 적용합니다.",
        r#"{"pageNum":1,"defaultTabSpacingMm":40,"hideHeader":false}"#,
    ),
    (
        "security.privacy.scan",
        "kinds: ssn|card|phone|email 가운데 일부(기본 전부), mask: 가릴 문자, hiddenText: 숨은 글도 나열(기본 true), offPage: 쪽 밖 글도 포함.",
        r#"{"kinds":["ssn","phone"],"mask":"*"}"#,
    ),
    (
        "security.privacy.mask",
        "items: [{raw, masked}](글자 수가 같아야 함, scan 결과 그대로). 나오는 곳을 모두 한 번 실행 취소로 바꿉니다.",
        r#"{"items":[{"raw":"010-1234-5678","masked":"010-****-****"}]}"#,
    ),
    (
        "file.save_hml",
        "path(.hml로 끝남). HML에서 연 문서만 다시 쓸 수 있습니다.",
        r#"{"path":"out.hml"}"#,
    ),
    (
        "file.save_pages",
        "path(.hwp/.hwpx/.pdf/.txt), from·to: 화면 쪽 번호(1부터, 양 끝 포함). 열린 문서는 바뀌지 않습니다. PDF는 화면 쪽 나눔을 따르고, 모든 쪽을 고르면 원본 전체를 그대로 씁니다. 일부만 편집 문서·글로 쓸 때는 문단·표 전체 단위이며 원본과 화면 경계가 맞아야 하고, 지원하지 않는 자르기는 쓰기 전에 실패합니다.",
        r#"{"path":"2-3쪽.hwpx","from":2,"to":3}"#,
    ),
    (
        "page.border_fill.get",
        "매개 변수 없음. 지금 구역의 테두리·배경과 spacingMm [왼쪽, 오른쪽, 위, 아래].",
        "{}",
    ),
    (
        "page.border_fill",
        "basis: paper|page, spacingMm: [왼쪽, 오른쪽, 위, 아래], headerInside·footerInside: bool, applyPage: all|exceptFirst, borders: 왼쪽·오른쪽·위·아래 네 개 {type, width, color #rrggbb}, fill: #rrggbb 또는 null, fillArea: paper|page|border, wholeDocument: bool.",
        r##"{"basis":"paper","spacingMm":[5,5,5,5],"headerInside":false,"footerInside":false,"applyPage":"all","borders":[{"type":1,"width":2,"color":"#000000"},{"type":1,"width":2,"color":"#000000"},{"type":1,"width":2,"color":"#000000"},{"type":1,"width":2,"color":"#000000"}],"fill":null}"##,
    ),
    (
        "page.header",
        "apply_to: both|odd|even(기본 both), template: 0–10(0 없음, 1–3 쪽 번호 왼쪽/가운데/오른쪽, 4–5 파일 이름 포함, 6–10 같은 것을 굵게·밑줄).",
        r#"{"apply_to":"both","template":2}"#,
    ),
    (
        "page.footer",
        "page.header와 같은 매개 변수.",
        r#"{"apply_to":"both","template":2}"#,
    ),
    (
        "page.header_footer.list",
        "매개 변수 없음. 지금 구역의 머리말·꼬리말.",
        "{}",
    ),
    (
        "page.header_footer.delete",
        "header: bool, apply_to: both|odd|even(기본 both).",
        r#"{"header":true,"apply_to":"both"}"#,
    ),
    (
        "page.number.get",
        "매개 변수 없음. exists, position, format, dash, start.",
        "{}",
    ),
    (
        "page.number",
        "position: none|top_left|top_center|top_right|bottom_left|bottom_center|bottom_right|outside_top|outside_bottom|inside_top|inside_bottom, format: 0 1,2,3 / 1 원 숫자 / 2 I,II,III / 3 i,ii,iii / 4 A,B,C / 5 a,b,c / 8 가,나,다 / 10 ㄱ,ㄴ,ㄷ / 12 일,이,삼 / 13 一,二,三, dash: bool, start: 첫 번호(선택).",
        r#"{"position":"bottom_center","format":0,"dash":true,"start":1}"#,
    ),
    (
        "page.new_number",
        "start: 캐럿 쪽에서 새로 시작할 쪽 번호(1 이상).",
        r#"{"start":5}"#,
    ),
    (
        "page.hide.get",
        "매개 변수 없음. 캐럿 쪽의 감추기 설정.",
        "{}",
    ),
    (
        "page.line_numbers.skip.get",
        "매개 변수 없음. 캐럿 문단이 줄 번호를 건너뛰는지: skip.",
        "{}",
    ),
    (
        "page.line_numbers.skip",
        "skip: bool. 선택한 본문 문단(선택이 없으면 캐럿 문단)이 줄 번호를 받지도 세지도 않게 합니다(한글의 문단 모양 \"줄 번호 건너뜀\"). HWPX에는 저장되지만 HWP 파일에서의 자리는 공개 문서에 없어 HWP로 저장하면 사라집니다.",
        r#"{"skip":true}"#,
    ),
    (
        "review.history.list",
        "매개 변수 없음. 이 문서 파일(마지막으로 열거나 저장한 HWP)에 한글의 문서 이력 관리로 기록된 버전: entries[{index(0부터, 오래된 것 먼저), date(YYYY-MM-DD HH:MM:SS, 한글이 기록한 지역 시간), writer, description}], lastVersion(마지막 이력 버전 HML이 있으면 true), file. 이력이 없거나 HWP가 아니면 entries는 빈 목록입니다. 읽기만 하며, HwpCraft는 새 버전을 기록하지 않습니다(이력은 HWP로 저장해도 그대로 남음).",
        "{}",
    ),
    (
        "review.history.save_last",
        "path: .hml로 끝나는 파일. 문서 파일에 남은 마지막 이력 버전(한글이 HWPML 2.5로 기록)을 UTF-16 HML로 저장합니다. 이전 버전으로 되돌리는 차이 적용은 하지 않고, HwpCraft는 HWPML 2.5를 열지 못합니다.",
        r#"{"path":"C:/문서/마지막 이력.hml"}"#,
    ),
    (
        "page.line_numbers.get",
        "매개 변수 없음. 캐럿 구역의 줄 번호: show, start, countBy, distanceMm(본문 왼쪽 끝부터), restart(저장된 restartType; 한글 자동화 표 기준 0 기본, 1 쪽마다, 2 구역마다, 3 이어서 — 한글 파일로는 확인하지 않음).",
        "{}",
    ),
    (
        "page.line_numbers",
        "show: bool. start(1–65535), count_by(1–100: n째 줄마다 번호), distance_mm(0–100, 번호 오른쪽 끝과 본문 사이), restart(0 기본, 1 쪽마다, 2 구역마다, 3 이어서: 한글 자동화 표 순서, 한글 파일로는 확인하지 않음)는 생략하면 저장된 값을 유지합니다(꺼져 있던 줄 번호를 켤 때 저장된 거리가 0이면 5 mm). all_sections: bool(기본 false = 캐럿 구역). 한글의 줄 번호 넣기처럼 HWP·HWPX에 저장하고, 화면·PDF의 본문 줄(표·글상자·각주 안 제외)에 번호를 그립니다.",
        r#"{"show":true,"start":1,"count_by":5,"distance_mm":5,"restart":2}"#,
    ),
    (
        "page.hide",
        "header, footer, master_page, border, fill, page_number: bool. 캐럿 쪽에만 적용합니다.",
        r#"{"header":true,"footer":false,"master_page":false,"border":false,"fill":false,"page_number":true}"#,
    ),
    (
        "page.master.list",
        "매개 변수 없음. 지금 구역의 바탕쪽과 편집 가능 여부·사유.",
        "{}",
    ),
    (
        "page.column_def_break",
        "count 1–6(여기부터 단 수), spacing_mm 0–50(기본 8). 본문 캐럿에서 문단을 나누고 거기서 새 단 정의를 시작합니다(예: 제목은 1단, 이어서 2단). 한 번 실행 취소.",
        r#"{"count":2,"spacing_mm":8.0}"#,
    ),
    (
        "page.section_break",
        "매개 변수 없음. 본문 캐럿에서 새 구역을 시작합니다(캐럿이 문단 안이면 문단을 나눔). 새 구역은 용지·바탕쪽·단 설정을 이어받고, 머리말·꼬리말은 한글처럼 이어지며 쪽 번호도 이어집니다. 한 번 실행 취소.",
        "{}",
    ),
    (
        "page.label",
        "새 A4 라벨 문서: rows 1–40, columns 1–10, labelWidthMm·labelHeightMm(5–300), gapXMm(칸 사이, 0–30, 기본 0), topMm/leftMm(기본: 가운데). file.new처럼 지금 문서를 바꿉니다(force: true면 저장하지 않은 변경을 버림). 라벨은 테두리 없는 표 하나의 셀이며 셀마다 입력합니다.",
        r#"{"rows":8,"columns":2,"labelWidthMm":99.1,"labelHeightMm":34.0,"gapXMm":2.5}"#,
    ),
    (
        "file.template.list",
        "매개 변수 없음. 사용자 서식(설정 폴더의 templates): name, format, bytes, modified.",
        "{}",
    ),
    (
        "file.template.new",
        "name: 서식 이름. 그 사본을 이름 없는 새 문서로 엽니다(서식 파일은 그대로). file.open처럼 지금 문서를 바꾸며, 저장하지 않은 변경이 있으면 force가 true일 때만 엽니다.",
        r#"{"name":"주간 보고","force":false}"#,
    ),
    (
        "file.template.save",
        "name(1–60자, 경로 문자 없음). 지금 문서를 그 서식으로 저장합니다(.hwpx, 암호 문서는 .hwp; 같은 이름은 바꿈). 문서 자체의 파일은 그대로입니다.",
        r#"{"name":"주간 보고"}"#,
    ),
    (
        "file.template.delete",
        "name: 지울 서식 이름.",
        r#"{"name":"주간 보고"}"#,
    ),
    (
        "page.master.get",
        "applyTo: both(기본), odd 또는 even. 지금 구역의 그 바탕쪽 글, exists, editable, reason.",
        r#"{"applyTo":"both"}"#,
    ),
    (
        "page.master.set",
        "applyTo: both(기본), odd 또는 even, text: \\n으로 나눈 줄(줄마다 한 문단, 10 KB까지). 지금 구역의 그 바탕쪽을 글로 만들거나 바꿉니다. 한글처럼 홀수·짝수 바탕쪽은 그 쪽에서 양쪽 바탕쪽 대신 쓰입니다. 바탕쪽에 그림·도형이 있거나 마지막 쪽·임의 쪽 바탕쪽이 있는 구역은 거절합니다.",
        r#"{"applyTo":"both","text":"대외비"}"#,
    ),
    (
        "page.master.delete",
        "applyTo: both(기본), odd 또는 even. 지금 구역의 그 바탕쪽을 지웁니다.",
        r#"{"applyTo":"both"}"#,
    ),
    (
        "file.doc_info",
        "매개 변수 없음. file {path, format, bytes, modified, created}, pages, sections, paragraphs, lines, characters, words, tables, pictures, shapes, equations, notes, fonts, fontSubstitutions, binData, pictureData.",
        "{}",
    ),
    (
        "file.print_pdf",
        "path(.pdf), pages: all|current|to_current|from_current 또는 1,3,5-7 같은 목록, current: current 계열 범위에 쓰는 캐럿 쪽(0부터). 인쇄 상세(선택): oddEven all|odd|even, reverse, copies 1–99와 collate(기본 true), nUp 1|2|4|8(한 장에 쪽 수), watermark {text 1–40자, opacity 0.05–0.6}(대각선으로 찍음, 문서에는 저장 안 함). 결과 sheets는 PDF 쪽 수.",
        r#"{"path":"print.pdf","pages":"1,3,5-7","current":0,"nUp":2,"watermark":{"text":"대외비"}}"#,
    ),
    (
        "page.margins.presets",
        "매개 변수 없음. 한글 쪽 여백 기본값 여섯 가지와 밀리미터 값.",
        "{}",
    ),
    (
        "page.margins.preset",
        "preset: default|narrow1_hf|narrow2_hf|wide_hf|narrow|wide(한글 목록 순서: 기본, 머리말·꼬리말 여백 있는 좁게1/좁게2/넓게, 좁게, 넓게). 캐럿 구역에 적용하며 용지 크기와 제본 여백은 그대로입니다.",
        r#"{"preset":"narrow"}"#,
    ),
    (
        "page.section.next",
        "매개 변수 없음. 다음 구역의 첫 문단으로 캐럿을 옮깁니다.",
        "{}",
    ),
    (
        "page.section.prev",
        "매개 변수 없음. 앞 구역의 첫 문단으로 캐럿을 옮깁니다.",
        "{}",
    ),
    (
        "page.copy",
        "page: 선택, 1부터(기본 캐럿 쪽). 그 쪽의 글을 문서 안 쪽 클립보드에 복사합니다. 쪽 나눔을 넘는 문단은 그 쪽에 있는 부분만 들어갑니다.",
        r#"{"page":2}"#,
    ),
    (
        "page.paste",
        "page: 선택, 1부터(기본 캐럿 쪽). 복사한 쪽을 그 뒤에 새 쪽으로 넣고, 캐럿은 붙인 글 뒤에 둡니다.",
        "{}",
    ),
    (
        "page.delete",
        "page: 선택, 1부터(기본 캐럿 쪽). 그 쪽의 글(문단 전체 또는 그 쪽에 있는 문단 부분)을 지워 뒤쪽이 당겨 오릅니다. 한 쪽짜리 문서는 거절합니다.",
        "{}",
    ),
    (
        "file.insert",
        "path: HWP/HWPX/HWP3 문서, password: 보호 문서일 때. 본문을 캐럿에 끼워 넣으며 글꼴·스타일·그림을 다시 맞춥니다. 한 번 실행 취소.",
        r#"{"path":"부록.hwpx"}"#,
    ),
    (
        "file.save_block",
        "path(.hwp/.hwpx/.pdf/.txt). 선택한 본문 글만 씁니다(여러 문단 선택의 가운데 문단은 문단 전체).",
        r#"{"path":"블록.hwpx"}"#,
    ),
];
const HU_PER_MM: f64 = 7200.0 / 25.4;
/// Hangul's 쪽 번호 위치 codes (HWP spec table 149) by dialog name.
pub const NUMBER_POSITIONS: &[(&str, u8)] = &[
    ("none", 0),
    ("top_left", 1),
    ("top_center", 2),
    ("top_right", 3),
    ("bottom_left", 4),
    ("bottom_center", 5),
    ("bottom_right", 6),
    ("outside_top", 7),
    ("outside_bottom", 8),
    ("inside_top", 9),
    ("inside_bottom", 10),
];
/// Number formats offered by Hangul's 쪽 번호 모양 list (HWP table 41 codes).
pub const NUMBER_FORMATS: &[(u8, &str)] = &[
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
/// Hangul 머리말/꼬리말 마당 entries supported by RHWP's template builder.
pub const HEADER_TEMPLATES: &[&str] = &[
    "(모양 없음)",
    "왼쪽 쪽 번호",
    "가운데 쪽 번호",
    "오른쪽 쪽 번호",
    "쪽 번호 · 파일 이름",
    "파일 이름 · 쪽 번호",
    "왼쪽 쪽 번호 (굵게·밑줄)",
    "가운데 쪽 번호 (굵게·밑줄)",
    "오른쪽 쪽 번호 (굵게·밑줄)",
    "쪽 번호 · 파일 이름 (굵게·밑줄)",
    "파일 이름 · 쪽 번호 (굵게·밑줄)",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BorderLine {
    /// RHWP border line type code: 0 none, 1 solid, 2 dash, 3 dot, …
    #[serde(rename = "type")]
    kind: u8,
    /// Width index 0–15 (0.1 mm … 5 mm).
    width: u8,
    color: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BorderFill {
    /// "paper" (종이 기준) or "page" (쪽 기준).
    basis: String,
    /// left, right, top, bottom spacing in mm.
    spacing_mm: [f64; 4],
    header_inside: bool,
    footer_inside: bool,
    /// "all" (모두), "exceptFirst" (첫 쪽 제외).
    apply_page: String,
    /// left, right, top, bottom.
    borders: [BorderLine; 4],
    /// `None` for no fill, `#rrggbb` for a solid fill.
    #[serde(default)]
    fill: Option<String>,
    /// "paper", "page" or "border" — area the fill covers.
    #[serde(default)]
    fill_area: Option<String>,
    /// Apply to every section instead of the cursor's.
    #[serde(default)]
    whole_document: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderFooter {
    /// "both", "odd" or "even".
    #[serde(default = "both")]
    apply_to: String,
    #[serde(default)]
    template: u8,
}
fn both() -> String {
    "both".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageNumber {
    position: String,
    #[serde(default)]
    format: u8,
    #[serde(default)]
    dash: bool,
    /// Section start number; `None` keeps the current value.
    #[serde(default)]
    start: Option<u16>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LineNumbers {
    show: bool,
    #[serde(default)]
    start: Option<u16>,
    #[serde(default)]
    count_by: Option<u16>,
    #[serde(default)]
    distance_mm: Option<f64>,
    #[serde(default)]
    restart: Option<u8>,
    #[serde(default)]
    all_sections: bool,
}

/// One stored line-numbering value (`restartType`, `countBy`, `distance`, `startNumber`).
fn line_number_value(def: &hwpcraft_doc::model::document::SectionDef, key: &str) -> u32 {
    def.line_number_shape
        .as_ref()
        .and_then(|attrs| attrs.iter().find(|(k, _)| k == key))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0)
}

/// Put line numbering in a section definition's HWP5 places as well: flags bit 24 (shown)
/// and the record tail at offsets 10..19 (restart u8, count u16, distance u32, start u16),
/// where Hangul 2024 keeps them. A shorter tail is padded with zeros up to there.
fn store_line_numbers(
    def: &mut hwpcraft_doc::model::document::SectionDef,
    shape: &[(String, String)],
    show: bool,
) {
    const SHOW_LINE_NUMBER_FLAG: u32 = 0x0100_0000;
    def.line_number_shape = Some(shape.to_vec());
    def.show_line_number = show;
    if show {
        def.flags |= SHOW_LINE_NUMBER_FLAG;
    } else {
        def.flags &= !SHOW_LINE_NUMBER_FLAG;
    }
    if def.raw_ctrl_extra.len() < 19 {
        def.raw_ctrl_extra.resize(19, 0);
    }
    let value = |key: &str| line_number_value(def, key);
    let (restart, count, distance, start) = (
        value("restartType"),
        value("countBy"),
        value("distance"),
        value("startNumber"),
    );
    let tail = &mut def.raw_ctrl_extra[10..19];
    tail[0] = restart as u8;
    tail[1..3].copy_from_slice(&(count as u16).to_le_bytes());
    tail[3..7].copy_from_slice(&distance.to_le_bytes());
    tail[7..9].copy_from_slice(&(start as u16).to_le_bytes());
}

/// Line numbering of a section as `page.line_numbers.get` reports it.
fn line_numbers_json(def: &hwpcraft_doc::model::document::SectionDef) -> Value {
    let value = |key| line_number_value(def, key);
    json!({
        "show": def.show_line_number,
        "start": value("startNumber"),
        "countBy": value("countBy"),
        "distanceMm": (f64::from(value("distance")) / HU_PER_MM * 100.0).round() / 100.0,
        "restart": value("restartType"),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageHide {
    header: bool,
    footer: bool,
    master_page: bool,
    border: bool,
    fill: bool,
    page_number: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HfTarget {
    header: bool,
    #[serde(default = "both")]
    apply_to: String,
}

/// Header/footer field kinds RHWP can insert (`insert_field_in_hf_native` codes 1–3).
/// Hangul also offers a 날짜 field; RHWP has no header/footer date field.
pub const HF_FIELDS: &[(&str, u8, char)] = &[
    ("page_number", 1, '\u{15}'),
    ("total_pages", 2, '\u{16}'),
    ("file_name", 3, '\u{17}'),
];

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
pub struct HfField {
    /// Character offset in `text` (before fields are inserted) where the field goes.
    pub offset: usize,
    /// One of `HF_FIELDS` names.
    pub kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HfText {
    header: bool,
    #[serde(default = "both")]
    apply_to: String,
    /// One line without control characters; fields are added separately.
    text: String,
    #[serde(default)]
    fields: Vec<HfField>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SectionDef {
    /// Section start page number; `None` keeps the current value.
    #[serde(default)]
    page_num: Option<u16>,
    /// HWP page number kind 0–3 (이어서/새 번호/홀수/짝수 as stored by Hangul).
    #[serde(default)]
    page_num_type: Option<u8>,
    #[serde(default)]
    picture_num: Option<u16>,
    #[serde(default)]
    table_num: Option<u16>,
    #[serde(default)]
    equation_num: Option<u16>,
    #[serde(default)]
    default_tab_spacing_mm: Option<f64>,
    #[serde(default)]
    hide_header: Option<bool>,
    #[serde(default)]
    hide_footer: Option<bool>,
    #[serde(default)]
    hide_master_page: Option<bool>,
    #[serde(default)]
    hide_border: Option<bool>,
    #[serde(default)]
    hide_fill: Option<bool>,
    #[serde(default)]
    hide_empty_line: Option<bool>,
    #[serde(default)]
    whole_document: bool,
}

/// Personal-data kinds RHWP's scanner validates (checksum/Luhn/format), by command name.
pub const PII_KINDS: &[(&str, &str)] = &[
    ("ssn", "주민등록번호"),
    ("card", "신용카드 번호"),
    ("phone", "전화번호"),
    ("email", "전자우편 주소"),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PrivacyScan {
    /// Subset of `PII_KINDS` names; empty means all four.
    #[serde(default)]
    kinds: Vec<String>,
    /// Single character used for the masked preview.
    #[serde(default = "star")]
    mask: String,
    /// Also report hidden text (same colour as background, zero/near-zero size).
    #[serde(default = "yes")]
    hidden_text: bool,
    /// Include paragraphs laid out completely off the page (slower, layout based).
    #[serde(default)]
    off_page: bool,
}
fn star() -> String {
    "*".into()
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskItem {
    raw: String,
    masked: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivacyMask {
    /// Values to replace everywhere they occur (body, cells, text boxes), as returned by the scan.
    items: Vec<MaskItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavePages {
    path: std::path::PathBuf,
    /// 1-based inclusive page range.
    from: u32,
    to: u32,
}

/// 0-based page numbers for a Hangul 인쇄 범위. `current` is 0-based; lists are 1-based.
pub fn print_pages(range: &str, current: u32, count: u32) -> Result<Vec<u32>> {
    if count == 0 {
        bail!("인쇄할 쪽이 없습니다.");
    }
    let current = current.min(count - 1);
    let pages: Vec<u32> = match range {
        "all" => (0..count).collect(),
        "current" => vec![current],
        "to_current" => (0..=current).collect(),
        "from_current" => (current..count).collect(),
        list => {
            let mut out = Vec::new();
            for part in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let (a, b) = part.split_once('-').unwrap_or((part, part));
                let parse = |v: &str| -> Result<u32> {
                    let n: u32 = v
                        .trim()
                        .parse()
                        .map_err(|_| anyhow!("쪽 범위를 읽지 못했습니다: {part}"))?;
                    if n == 0 || n > count {
                        bail!("{n}쪽은 범위(1–{count})를 벗어났습니다.");
                    }
                    Ok(n - 1)
                };
                let (a, b) = (parse(a)?, parse(b)?);
                if a > b {
                    bail!("쪽 범위는 작은 번호부터 적으세요: {part}");
                }
                out.extend(a..=b);
            }
            out
        }
    };
    if pages.is_empty() {
        bail!("인쇄할 쪽 범위를 입력하세요. 예) 1,3,5-7");
    }
    Ok(pages)
}

/// Section-flag bits that record which master-page kinds a section has (HWP5).
const MASTER_KIND_FLAGS: u32 = 0xe000_0000;
/// The master-page kinds HwpCraft edits, with their name, label and section-flag bit,
/// in the order Hangul stores them (양쪽, 짝수, 홀수; Hangul 2024 saves, HWP and HWPX).
const MASTER_KINDS: [(HeaderFooterApply, &str, &str, u32); 3] = [
    (HeaderFooterApply::Both, "both", "양쪽", 0x2000_0000),
    (HeaderFooterApply::Even, "even", "짝수 쪽", 0x4000_0000),
    (HeaderFooterApply::Odd, "odd", "홀수 쪽", 0x8000_0000),
];
/// Raw records a text master page is made of: list header and paragraph records.
const MASTER_RECORD_TAGS: [u16; 6] = [66, 67, 68, 69, 70, 72];

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MasterTarget {
    #[serde(default = "both")]
    apply_to: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MasterText {
    #[serde(default = "both")]
    apply_to: String,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Label {
    rows: u32,
    columns: u32,
    label_width_mm: f64,
    label_height_mm: f64,
    #[serde(default)]
    gap_x_mm: f64,
    #[serde(default)]
    top_mm: Option<f64>,
    #[serde(default)]
    left_mm: Option<f64>,
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateName {
    name: String,
}

/// 문서마당: the templates folder in the settings folder.
fn template_dir() -> Result<std::path::PathBuf> {
    crate::config_dir()
        .map(|d| d.join("templates"))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못했습니다."))
}

/// The .hwpx and .hwp file a template `name` may be stored as.
fn template_path(name: &str) -> Result<[std::path::PathBuf; 2]> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 60
        || name.starts_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c))
    {
        bail!("서식 이름은 1–60자이고 \\ / : * ? \" < > | 를 쓸 수 없습니다.");
    }
    let dir = template_dir()?;
    Ok([
        dir.join(format!("{name}.hwpx")),
        dir.join(format!("{name}.hwp")),
    ])
}

fn template_list() -> Result<Vec<Value>> {
    let dir = template_dir()?;
    let mut list: Vec<Value> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let format = path.extension()?.to_str()?.to_ascii_lowercase();
            if format != "hwp" && format != "hwpx" {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some(json!({
                "name": path.file_stem()?.to_string_lossy(),
                "format": format,
                "bytes": meta.len(),
                "modified": modified,
            }))
        })
        .collect();
    list.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(list)
}

fn column_spacing() -> f64 {
    8.0
}
fn print_all() -> String {
    "all".into()
}
fn print_one() -> u32 {
    1
}
fn print_yes() -> bool {
    true
}
/// The master-page kind named by `applyTo`.
fn master_kind(apply_to: &str) -> Result<HeaderFooterApply> {
    match MASTER_KINDS.iter().find(|(_, name, ..)| *name == apply_to) {
        Some((kind, ..)) => Ok(*kind),
        None => bail!("적용 쪽은 both, odd, even 중 하나여야 합니다."),
    }
}
/// Position of `kind` in [`MASTER_KINDS`] (Hangul's storage order).
fn master_kind_index(kind: HeaderFooterApply) -> usize {
    MASTER_KINDS
        .iter()
        .position(|(k, ..)| *k == kind)
        .unwrap_or(0)
}
/// Why `sec`'s master pages cannot be edited without losing something, if so.
fn master_edit_block(sec: &hwpcraft_doc::model::document::Section) -> Option<&'static str> {
    let masters = &sec.section_def.master_pages;
    if masters.iter().any(|m| m.is_extension) {
        return Some("마지막 쪽·임의 쪽 바탕쪽이 있는 구역은 아직 편집할 수 없습니다.");
    }
    // Pictures and drawing shapes are kept through a text edit (`master_objects`).
    if masters.iter().any(|m| {
        m.paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .any(|c| !matches!(c, Control::Picture(_) | Control::Shape(_)))
    }) {
        return Some(
            "표 같은 개체가 든 바탕쪽은 아직 편집할 수 없습니다. 보기와 저장은 그대로 됩니다.",
        );
    }
    if sec
        .section_def
        .extra_child_records
        .iter()
        .any(|r| !MASTER_RECORD_TAGS.contains(&r.tag_id))
    {
        return Some("이 구역의 바탕쪽에 알 수 없는 기록이 있어 편집하지 않습니다.");
    }
    None
}
fn apply_code(apply_to: &str) -> Result<u8> {
    Ok(match apply_to {
        "both" => 0,
        "even" => 1,
        "odd" => 2,
        _ => bail!("적용 쪽은 both, odd, even 중 하나여야 합니다."),
    })
}
fn color(value: &str) -> Result<&str> {
    let hex = value.strip_prefix('#').unwrap_or("");
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("색은 #rrggbb 형식이어야 합니다: {value}");
    }
    Ok(value)
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_page_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_page_command(id, params.clone()))
    }

    fn run_page_command(&mut self, id: &str, params: Value) -> Result<Value> {
        let section = self.cursor.section;
        match id {
            "page.border_fill.get" => self.page_border_fill(),
            "page.border_fill" => {
                let p: BorderFill = serde_json::from_value(params)?;
                let json = Self::border_fill_json(&p)?;
                let sections = if p.whole_document {
                    0..self.core.document().sections.len()
                } else {
                    section..section + 1
                };
                self.transaction(|s| {
                    for i in sections {
                        s.core
                            .set_page_border_fill_native(i, &json.to_string())
                            .map_err(err)?;
                    }
                    Ok(())
                })?;
                self.page_border_fill()
            }
            "page.header" | "page.footer" => {
                let p: HeaderFooter = serde_json::from_value(params)?;
                let apply = apply_code(&p.apply_to)?;
                if usize::from(p.template) >= HEADER_TEMPLATES.len() {
                    bail!(
                        "머리말/꼬리말 마당 번호는 0–{}입니다.",
                        HEADER_TEMPLATES.len() - 1
                    );
                }
                let is_header = id == "page.header";
                self.transaction(|s| {
                    s.core
                        .apply_hf_template_native(section, is_header, apply, p.template)
                        .map_err(err)?;
                    Ok(())
                })?;
                self.header_footer_list()
            }
            "page.header_footer.list" => self.header_footer_list(),
            "page.header_footer.delete" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Delete {
                    header: bool,
                    #[serde(default = "both")]
                    apply_to: String,
                }
                let p: Delete = serde_json::from_value(params)?;
                let apply = apply_code(&p.apply_to)?;
                self.transaction(|s| s.delete_header_footer(section, p.header, apply))?;
                self.header_footer_list()
            }
            "page.number.get" => self.page_number(),
            "page.number" => {
                let p: PageNumber = serde_json::from_value(params)?;
                self.set_page_number(&p)?;
                self.page_number()
            }
            "page.new_number" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Start {
                    start: u16,
                }
                let p: Start = serde_json::from_value(params)?;
                if p.start == 0 {
                    bail!("새 번호는 1 이상이어야 합니다.");
                }
                self.require_body()?;
                let c = self.cursor;
                self.transaction(|s| {
                    s.core
                        .insert_new_number_native(c.section, c.paragraph, c.offset, p.start)
                        .map_err(err)?;
                    Ok(())
                })?;
                Ok(json!({"start": p.start}))
            }
            "page.hide.get" => {
                self.require_body()?;
                let c = self.cursor;
                Ok(serde_json::from_str(
                    &self
                        .core
                        .get_page_hide_native(c.section, c.paragraph)
                        .map_err(err)?,
                )?)
            }
            "page.line_numbers.skip.get" => {
                let c = self.cursor;
                let doc = self.core.document();
                let skip = doc
                    .sections
                    .get(c.section)
                    .and_then(|s| s.paragraphs.get(c.paragraph))
                    .and_then(|p| doc.doc_info.para_shapes.get(p.para_shape_id as usize))
                    .is_some_and(|shape| shape.suppress_line_numbers);
                Ok(json!({"skip": skip}))
            }
            "page.line_numbers.skip" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Skip {
                    skip: bool,
                }
                let p: Skip = serde_json::from_value(params)?;
                self.require_body()?;
                let targets: Vec<(usize, usize)> = self
                    .selected_paragraphs()?
                    .into_iter()
                    .map(|(c, _)| (c.section, c.paragraph))
                    .collect();
                let mut changed = 0;
                self.transaction(|s| {
                    for &(section, paragraph) in &targets {
                        let doc = s.core.document_mut();
                        let Some(old) = doc
                            .sections
                            .get(section)
                            .and_then(|sec| sec.paragraphs.get(paragraph))
                            .map(|para| para.para_shape_id)
                        else {
                            continue;
                        };
                        let Some(mut shape) = doc.doc_info.para_shapes.get(old as usize).cloned()
                        else {
                            continue;
                        };
                        if shape.suppress_line_numbers == p.skip {
                            continue;
                        }
                        shape.suppress_line_numbers = p.skip;
                        shape.raw_data = None;
                        let shapes = &mut doc.doc_info.para_shapes;
                        let id = match shapes.iter().position(|s| *s == shape) {
                            Some(i) => i,
                            None => {
                                shapes.push(shape);
                                doc.doc_info.raw_stream_dirty = true;
                                shapes.len() - 1
                            }
                        };
                        s.core
                            .set_para_shape_id_native(section, paragraph, id as u16)
                            .map_err(err)?;
                        changed += 1;
                    }
                    Ok(())
                })?;
                Ok(json!({"skip": p.skip, "paragraphs": changed}))
            }
            "page.line_numbers.get" => {
                let section = self.cursor.section;
                let doc = self.core.document();
                let def = &doc
                    .sections
                    .get(section)
                    .ok_or_else(|| anyhow!("구역을 찾지 못했습니다."))?
                    .section_def;
                Ok(line_numbers_json(def))
            }
            "page.line_numbers" => {
                let p: LineNumbers = serde_json::from_value(params)?;
                if p.start == Some(0) {
                    bail!("시작 번호는 1–65535입니다.");
                }
                if p.count_by.is_some_and(|n| !(1..=100).contains(&n)) {
                    bail!("번호 간격은 1–100줄입니다.");
                }
                if p.distance_mm
                    .is_some_and(|d| !(0.0..=100.0).contains(&d) || d.is_nan())
                {
                    bail!("본문과의 거리는 0–100 mm입니다.");
                }
                if p.restart.is_some_and(|r| r > 3) {
                    bail!("다시 시작은 0(기본), 1(쪽마다), 2(구역마다), 3(이어서)입니다.");
                }
                let sections: Vec<usize> = if p.all_sections {
                    (0..self.core.document().sections.len()).collect()
                } else {
                    vec![self.cursor.section]
                };
                self.transaction(|s| {
                    for &section in &sections {
                        let Some(sec) = s.core.document_mut().sections.get_mut(section) else {
                            continue;
                        };
                        let def = &mut sec.section_def;
                        // Numbers turned on with no distance stored would touch the text.
                        let turning_on = p.show && !def.show_line_number;
                        let value = |key, given: Option<u32>, default: u32| {
                            given.unwrap_or_else(|| match line_number_value(def, key) {
                                0 => default,
                                n => n,
                            })
                        };
                        let shape = vec![
                            (
                                "restartType".to_string(),
                                p.restart
                                    .map(u32::from)
                                    .unwrap_or_else(|| line_number_value(def, "restartType"))
                                    .to_string(),
                            ),
                            (
                                "countBy".to_string(),
                                value("countBy", p.count_by.map(u32::from), 1).to_string(),
                            ),
                            (
                                "distance".to_string(),
                                p.distance_mm
                                    .map(|mm| (mm * HU_PER_MM).round() as u32)
                                    .unwrap_or_else(|| match line_number_value(def, "distance") {
                                        0 if turning_on => (5.0 * HU_PER_MM).round() as u32,
                                        n => n,
                                    })
                                    .to_string(),
                            ),
                            (
                                "startNumber".to_string(),
                                value("startNumber", p.start.map(u32::from), 1).to_string(),
                            ),
                        ];
                        store_line_numbers(def, &shape, p.show);
                        // The section definition control of its first paragraph too.
                        for para in &mut sec.paragraphs {
                            for control in &mut para.controls {
                                if let Control::SectionDef(d) = control {
                                    store_line_numbers(d, &shape, p.show);
                                }
                            }
                        }
                        sec.raw_stream = None;
                    }
                    for &section in &sections {
                        s.core.set_section_def_native(section, "{}").map_err(err)?;
                    }
                    Ok(())
                })?;
                let section = self.cursor.section;
                Ok(line_numbers_json(
                    &self.core.document().sections[section].section_def,
                ))
            }
            "page.hide" => {
                let p: PageHide = serde_json::from_value(params)?;
                self.require_body()?;
                let c = self.cursor;
                let hide = hwpcraft_doc::model::control::PageHide {
                    hide_header: p.header,
                    hide_footer: p.footer,
                    hide_master_page: p.master_page,
                    hide_border: p.border,
                    hide_fill: p.fill,
                    hide_page_num: p.page_number,
                };
                let any = p.header || p.footer || p.master_page || p.border || p.fill;
                let any = any || p.page_number;
                // RHWP's set_page_hide_native puts the control in without its slot, which
                // moves the paragraph's inline objects to its end.
                self.transaction(|s| {
                    let sec = &mut s.core.document_mut().sections[c.section];
                    let para = &mut sec.paragraphs[c.paragraph];
                    let existing = para
                        .controls
                        .iter()
                        .position(|x| matches!(x, Control::PageHide(_)));
                    match (existing, any) {
                        (Some(i), false) => {
                            crate::control_slots::remove_control(para, i);
                        }
                        (Some(i), true) => para.controls[i] = Control::PageHide(hide),
                        (None, true) => {
                            crate::control_slots::insert_control(
                                para,
                                0,
                                Control::PageHide(hide),
                                None,
                            );
                        }
                        (None, false) => return Ok(()),
                    }
                    sec.raw_stream = None;
                    s.refresh_layout()
                })?;
                Ok(serde_json::from_str(
                    &self
                        .core
                        .get_page_hide_native(c.section, c.paragraph)
                        .map_err(err)?,
                )?)
            }
            "page.label" => {
                let p: Label = serde_json::from_value(params)?;
                self.create_label_sheet(&p)
            }
            "page.column_def_break" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct ColumnBreak {
                    count: u16,
                    #[serde(default = "column_spacing")]
                    spacing_mm: f64,
                }
                let p: ColumnBreak = serde_json::from_value(params)?;
                self.column_def_break(p.count, p.spacing_mm)
            }
            "page.section_break" => self.section_break(),
            "file.template.list" => Ok(json!({ "templates": template_list()? })),
            "file.template.new" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct New {
                    name: String,
                    #[serde(default)]
                    force: bool,
                }
                let p: New = serde_json::from_value(params)?;
                // Like file.new: unsaved changes are given up only on request.
                if self.dirty() && !p.force {
                    bail!(
                        "저장하지 않은 변경이 있습니다. 먼저 저장하거나 force: true로 명시적으로 버리세요."
                    );
                }
                let path = template_path(&p.name)?
                    .into_iter()
                    .find(|p| p.is_file())
                    .ok_or_else(|| anyhow!("문서마당에 [{}] 서식이 없습니다.", p.name))?;
                self.open(&path)?;
                // A new document from the template: saving asks for a file name.
                self.path = None;
                Ok(json!({ "template": p.name, "pages": self.page_count()? }))
            }
            "file.template.save" => {
                let p: TemplateName = serde_json::from_value(params)?;
                let [hwpx, hwp] = template_path(&p.name)?;
                let target = if self.has_password() {
                    hwp.clone()
                } else {
                    hwpx.clone()
                };
                if let Some(dir) = target.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                self.save_copy(&target)?;
                // One template per name, whatever its format.
                for other in [hwpx, hwp] {
                    if other != target {
                        let _ = std::fs::remove_file(other);
                    }
                }
                Ok(json!({ "template": p.name, "path": target }))
            }
            "file.template.delete" => {
                let p: TemplateName = serde_json::from_value(params)?;
                let mut removed = false;
                for path in template_path(&p.name)? {
                    removed |= std::fs::remove_file(path).is_ok();
                }
                if !removed {
                    bail!("문서마당에 [{}] 서식이 없습니다.", p.name);
                }
                Ok(json!({ "templates": template_list()? }))
            }
            "page.master.get" => {
                let p: MasterTarget = serde_json::from_value(params)?;
                self.master_page_text(section, master_kind(&p.apply_to)?)
            }
            "page.master.set" => {
                let p: MasterText = serde_json::from_value(params)?;
                let kind = master_kind(&p.apply_to)?;
                self.set_master_page_text(section, kind, Some(&p.text))?;
                self.master_page_text(section, kind)
            }
            "page.master.delete" => {
                let p: MasterTarget = serde_json::from_value(params)?;
                let kind = master_kind(&p.apply_to)?;
                if !self.master_page_text(section, kind)?["exists"]
                    .as_bool()
                    .unwrap_or(false)
                {
                    let label = MASTER_KINDS[master_kind_index(kind)].2;
                    bail!("이 구역에는 {label} 바탕쪽이 없습니다.");
                }
                self.set_master_page_text(section, kind, None)?;
                self.master_page_text(section, kind)
            }
            "page.master.list" => {
                let doc = self.core.document();
                let list: Vec<Value> = doc
                    .sections
                    .get(section)
                    .map(|s| {
                        s.section_def
                            .master_pages
                            .iter()
                            .map(|m| {
                                json!({
                                    "applyTo": format!("{:?}", m.apply_to),
                                    "extension": m.is_extension,
                                    "overlap": m.overlap,
                                    "text": m.paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let reason = doc.sections.get(section).and_then(master_edit_block);
                Ok(json!({
                    "section": section,
                    "masterPages": list,
                    "editable": reason.is_none(),
                    "reason": reason,
                }))
            }
            "view.marks.get" => Ok(serde_json::to_value(self.view_marks())?),
            "view.marks" => {
                let m: ViewMarks = serde_json::from_value(params)?;
                Ok(serde_json::to_value(self.set_view_marks(m))?)
            }
            "page.header_footer.text.get" => {
                let p: HfTarget = serde_json::from_value(params)?;
                self.header_footer_fields(p.header, apply_code(&p.apply_to)?)
            }
            "page.header_footer.text" => {
                let p: HfText = serde_json::from_value(params)?;
                self.set_header_footer_fields(&p)?;
                self.header_footer_fields(p.header, apply_code(&p.apply_to)?)
            }
            "page.section.get" => self.section_def(),
            "page.section" => {
                let p: SectionDef = serde_json::from_value(params)?;
                let json = Self::section_def_json(&p)?;
                let whole = p.whole_document;
                self.transaction(|s| {
                    if whole {
                        s.core
                            .set_section_def_all_native(&json.to_string())
                            .map_err(err)?;
                    } else {
                        s.core
                            .set_section_def_native(section, &json.to_string())
                            .map_err(err)?;
                    }
                    Ok(())
                })?;
                self.section_def()
            }
            "security.privacy.scan" => {
                let p: PrivacyScan = serde_json::from_value(params)?;
                self.privacy_scan(&p)
            }
            "security.privacy.mask" => {
                let p: PrivacyMask = serde_json::from_value(params)?;
                self.privacy_mask(&p)
            }
            "file.save_hml" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Target {
                    path: std::path::PathBuf,
                }
                let p: Target = serde_json::from_value(params)?;
                if p.path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_none_or(|e| !e.eq_ignore_ascii_case("hml"))
                {
                    bail!("HML 저장 파일 이름은 .hml로 끝나야 합니다.");
                }
                let bytes = self.core.export_hml_native().map_err(|e| {
                    anyhow!("HML로 저장하지 못했습니다: {e}. RHWP는 HML로 연 문서만 HML로 다시 저장합니다.")
                })?;
                hwpcraft_formats::atomic_write(&p.path, &bytes)?;
                Ok(json!({"path": p.path, "bytes": bytes.len()}))
            }
            "file.save_pages" => {
                let p: SavePages = serde_json::from_value(params)?;
                self.save_pages(&p)
            }
            "review.history.list" => {
                let Some(path) = self.path.clone() else {
                    return Ok(json!({"file": null, "entries": [], "lastVersion": false}));
                };
                let bytes = std::fs::read(&path)?;
                let entries = hwpcraft_formats::doc_history::entries(&bytes)?;
                let last = hwpcraft_formats::doc_history::last_version_hml(&bytes)?.is_some();
                let entries: Vec<Value> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "index": e.index,
                            "date": e.date,
                            "writer": e.writer,
                            "description": e.description,
                        })
                    })
                    .collect();
                Ok(json!({"file": path, "entries": entries, "lastVersion": last}))
            }
            "review.history.save_last" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Target {
                    path: std::path::PathBuf,
                }
                let p: Target = serde_json::from_value(params)?;
                if p.path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_none_or(|e| !e.eq_ignore_ascii_case("hml"))
                {
                    bail!("저장 파일 이름은 .hml로 끝나야 합니다.");
                }
                let source = self
                    .path
                    .clone()
                    .ok_or_else(|| anyhow!("저장된 문서 파일이 없어 이력이 없습니다."))?;
                let bytes = std::fs::read(&source)?;
                let hml = hwpcraft_formats::doc_history::last_version_hml(&bytes)?
                    .ok_or_else(|| anyhow!("이 문서 파일에는 마지막 이력 버전이 없습니다."))?;
                // Stored as UTF-16 by Hangul: written back the same way, with a byte order mark.
                let mut out = vec![0xFF, 0xFE];
                out.extend(hml.encode_utf16().flat_map(u16::to_le_bytes));
                hwpcraft_formats::atomic_write(&p.path, &out)?;
                Ok(json!({"path": p.path, "bytes": out.len()}))
            }
            "file.doc_info" => Ok(self.document_statistics()),
            "file.print_pdf" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct Print {
                    path: std::path::PathBuf,
                    /// "all", "current", "to_current", "from_current" or a list like "1,3,5-7".
                    pages: String,
                    #[serde(default)]
                    current: u32,
                    #[serde(default = "print_all")]
                    odd_even: String,
                    #[serde(default)]
                    reverse: bool,
                    #[serde(default = "print_one")]
                    copies: u32,
                    #[serde(default = "print_yes")]
                    collate: bool,
                    #[serde(default = "print_one")]
                    n_up: u32,
                    #[serde(default)]
                    watermark: Option<Watermark>,
                }
                let p: Print = serde_json::from_value(params)?;
                let layout = PrintLayout {
                    odd_even: p.odd_even,
                    reverse: p.reverse,
                    copies: p.copies,
                    collate: p.collate,
                    n_up: p.n_up,
                    watermark: p.watermark,
                };
                layout.validate()?;
                let count = self.page_count()?;
                let pages = print_pages(&p.pages, p.current, count)?;
                let (bytes, sheets) = if layout == PrintLayout::default() {
                    (self.render_pdf(&pages)?, pages.len())
                } else {
                    let mut prep = self.pdf_prepare(&pages)?;
                    self.pdf_prepare_step(&mut prep, std::time::Duration::MAX)?;
                    let job = self.pdf_finish_with(prep, &pages, &layout)?;
                    let sheets = job.page_count();
                    (job.convert()?, sheets)
                };
                hwpcraft_formats::atomic_write(&p.path, &bytes)?;
                Ok(json!({
                    "path": p.path,
                    "pages": pages.iter().map(|n| n + 1).collect::<Vec<_>>(),
                    "sheets": sheets,
                    "bytes": bytes.len(),
                }))
            }
            "page.margins.presets" => Ok(margin_presets_json()),
            "page.margins.preset" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Preset {
                    preset: String,
                }
                let p: Preset = serde_json::from_value(params)?;
                self.apply_margin_preset(&p.preset)
            }
            "page.section.next" | "page.section.prev" => {
                self.move_section(id == "page.section.next")
            }
            "page.copy" => {
                let p: PageTarget = serde_json::from_value(params)?;
                self.copy_page(p.page)
            }
            "page.paste" => {
                let p: PageTarget = serde_json::from_value(params)?;
                self.paste_page(p.page)
            }
            "page.delete" => {
                let p: PageTarget = serde_json::from_value(params)?;
                self.delete_page(p.page)
            }
            "file.insert" => {
                let p: InsertFile = serde_json::from_value(params)?;
                self.insert_file(&p)
            }
            "file.save_block" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Target {
                    path: std::path::PathBuf,
                }
                let p: Target = serde_json::from_value(params)?;
                self.save_block(&p.path)
            }
            _ => bail!("알 수 없는 편집 명령: {id}"),
        }
    }

    fn require_body(&self) -> Result<()> {
        if self.cursor.cell.is_some() {
            bail!("표 밖 본문에 커서를 두고 실행하세요.");
        }
        Ok(())
    }

    fn page_border_fill(&self) -> Result<Value> {
        let mut v: Value = serde_json::from_str(
            &self
                .core
                .get_page_border_fill_native(self.cursor.section)
                .map_err(err)?,
        )?;
        let mm = |k: &str, v: &Value| v[k].as_f64().unwrap_or(0.0) / HU_PER_MM;
        let spacing = [
            mm("spacingLeft", &v),
            mm("spacingRight", &v),
            mm("spacingTop", &v),
            mm("spacingBottom", &v),
        ];
        v["spacingMm"] = json!(spacing.map(|x| (x * 100.0).round() / 100.0));
        Ok(v)
    }

    fn border_fill_json(p: &BorderFill) -> Result<Value> {
        if !matches!(p.basis.as_str(), "paper" | "page") {
            bail!("위치 기준은 paper 또는 page입니다.");
        }
        if !matches!(p.apply_page.as_str(), "all" | "exceptFirst") {
            bail!("적용 쪽은 all 또는 exceptFirst입니다. 첫 쪽만은 지원하지 않습니다.");
        }
        let mut spacing = [0i64; 4];
        for (out, mm) in spacing.iter_mut().zip(p.spacing_mm) {
            if !mm.is_finite() || !(0.0..=100.0).contains(&mm) {
                bail!("테두리 간격은 0–100 mm여야 합니다.");
            }
            *out = (mm * HU_PER_MM).round() as i64;
        }
        let mut v = json!({
            "basis": p.basis,
            "spacingLeft": spacing[0],
            "spacingRight": spacing[1],
            "spacingTop": spacing[2],
            "spacingBottom": spacing[3],
            "headerInside": p.header_inside,
            "footerInside": p.footer_inside,
            "applyPage": p.apply_page,
            "fillArea": p.fill_area.as_deref().unwrap_or("paper"),
        });
        for (key, line) in ["borderLeft", "borderRight", "borderTop", "borderBottom"]
            .iter()
            .zip(&p.borders)
        {
            if line.kind > 16 || line.width > 15 {
                bail!("테두리 종류 0–16, 굵기 0–15 범위를 벗어났습니다.");
            }
            v[*key] = json!({"type": line.kind, "width": line.width, "color": color(&line.color)?});
        }
        match &p.fill {
            Some(c) => {
                v["fillType"] = json!("solid");
                v["fillColor"] = json!(color(c)?);
            }
            None => v["fillType"] = json!("none"),
        }
        Ok(v)
    }

    /// 단 정의 나누기: split the body paragraph at the caret; the new paragraph starts a
    /// new column definition (Hangul's 다단 나누기 break with its own ColumnDef).
    fn column_def_break(&mut self, count: u16, spacing_mm: f64) -> Result<Value> {
        if !(1..=6).contains(&count) || !(0.0..=50.0).contains(&spacing_mm) {
            bail!("단은 1–6개, 단 사이는 0–50 mm입니다.");
        }
        self.require_body()?;
        self.transaction(|s| {
            let at = s.cursor;
            if at.offset > 0 {
                s.execute(Command::Split)?;
            }
            let c = s.cursor;
            let doc = s.core.document_mut();
            let sec = &mut doc.sections[c.section];
            // The section's current definition gives the direction and line settings.
            let mut def = sec.paragraphs[..=c.paragraph]
                .iter()
                .rev()
                .flat_map(|p| p.controls.iter())
                .find_map(|ctrl| match ctrl {
                    Control::ColumnDef(d) => Some(d.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            def.column_count = count;
            def.same_width = true;
            def.spacing = (spacing_mm * 7200.0 / 25.4).round() as i16;
            def.raw_attr = 0;
            def.widths.clear();
            def.gaps.clear();
            let para = &mut sec.paragraphs[c.paragraph];
            let own = para
                .controls
                .iter()
                .position(|ctrl| matches!(ctrl, Control::ColumnDef(_)));
            match own {
                Some(i) => {
                    para.controls[i] = Control::ColumnDef(def);
                    para.invalidate_layout_inputs();
                }
                None => {
                    // Behind the section definition of a section's first paragraph.
                    let head = usize::from(matches!(
                        para.controls.first(),
                        Some(Control::SectionDef(_))
                    ));
                    crate::control_slots::insert_head_control(
                        para,
                        head,
                        Control::ColumnDef(def),
                        None,
                    );
                }
            }
            para.column_type = hwpcraft_doc::model::paragraph::ColumnBreakType::MultiColumn;
            para.raw_break_type = 0;
            sec.raw_stream = None;
            s.refresh_layout()
        })?;
        Ok(json!({ "columns": count, "pages": self.page_count()? }))
    }

    /// 구역 나누기: the body from the caret on becomes a new section. As in Hangul, the
    /// new section copies the section definition (page setup, numbering shapes) and the
    /// column layout at the caret, and page numbers continue. Hangul leaves the master
    /// pages to carry on from the previous section; RHWP does not draw them across
    /// sections, so they are copied, which Hangul shows the same way. Headers and
    /// footers already carry on in both.
    fn section_break(&mut self) -> Result<Value> {
        self.require_body()?;
        self.transaction(|s| {
            let at = s.cursor;
            let (len, last) = {
                let sec = &s.core.document().sections[at.section];
                let len = sec.paragraphs[at.paragraph].text.chars().count();
                (len, at.paragraph + 1 == sec.paragraphs.len())
            };
            // Hangul: at a paragraph's end the next paragraph starts the section, at its
            // start that paragraph does, and in the middle the paragraph is split. The
            // section's first paragraph keeps its definition, so it is split there too.
            let target = if at.offset > 0 && at.offset >= len && !last {
                at.paragraph + 1
            } else if at.offset > 0 || at.paragraph == 0 {
                s.execute(Command::Split)?;
                s.cursor.paragraph
            } else {
                at.paragraph
            };
            let doc = s.core.document_mut();
            let sec = &mut doc.sections[at.section];
            let mut moved = sec.paragraphs.split_off(target);
            let first = &mut moved[0];
            // A column definition at the break moves into the new section's head;
            // otherwise the one in effect at the caret is copied.
            let own = first
                .controls
                .iter()
                .position(|c| matches!(c, Control::ColumnDef(_)));
            let taken = own.map(|i| crate::control_slots::remove_control(first, i).0);
            let column = match taken {
                Some(Control::ColumnDef(d)) => d,
                _ => sec
                    .paragraphs
                    .iter()
                    .rev()
                    .flat_map(|p| p.controls.iter())
                    .find_map(|c| match c {
                        Control::ColumnDef(d) => Some(d.clone()),
                        _ => None,
                    })
                    .unwrap_or_default(),
            };
            let mut def = sec.section_def.clone();
            def.page_num = 0;
            def.picture_num = 0;
            def.table_num = 0;
            def.equation_num = 0;
            // The two definition controls take the paragraph's first two slots, as in a
            // section that was read from a file.
            let head = crate::control_slots::insert_head_control;
            head(first, 0, Control::ColumnDef(column), None);
            head(first, 0, Control::SectionDef(Box::new(def.clone())), None);
            first.column_type = hwpcraft_doc::model::paragraph::ColumnBreakType::Section;
            first.raw_break_type = 0;
            first.invalidate_layout_inputs();
            sec.raw_stream = None;
            sec.raw_provenance = None;
            let section = hwpcraft_doc::model::document::Section {
                section_def: def,
                paragraphs: moved,
                raw_stream: None,
                raw_provenance: None,
            };
            // DocInfo's section count is left alone: the HWP writer sets it from the
            // sections it writes, and only when the model still holds the old count.
            doc.sections.insert(at.section + 1, section);
            s.cursor = Cursor {
                section: at.section + 1,
                ..Cursor::default()
            };
            s.anchor = None;
            s.refresh_layout()
        })?;
        Ok(json!({
            "sections": self.core.document().sections.len(),
            "section": self.cursor.section,
            "pages": self.page_count()?
        }))
    }

    /// 구역 경계 지우기: section `section` (> 0) joins the previous one, which keeps its
    /// definition (page setup, master pages); the joined section's own definition and
    /// master pages go. Its column definition stays only where it differs from the one
    /// in effect, as a column-definition break. Paragraphs are not merged. With `back`
    /// (Backspace) the caret goes to the joined section's first paragraph; otherwise
    /// (Delete) it stays. Called from `delete`, inside its transaction.
    pub(crate) fn join_sections(&mut self, section: usize, back: bool) -> Result<()> {
        use hwpcraft_doc::model::paragraph::ColumnBreakType;
        let doc = self.core.document_mut();
        if section == 0 || section >= doc.sections.len() {
            bail!("합칠 구역이 없습니다.");
        }
        let removed = doc.sections.remove(section);
        let prev = &mut doc.sections[section - 1];
        let joint = prev.paragraphs.len();
        let current = prev
            .paragraphs
            .iter()
            .rev()
            .flat_map(|p| p.controls.iter())
            .find_map(|c| match c {
                Control::ColumnDef(d) => Some(format!("{d:?}")),
                _ => None,
            });
        let mut moved = removed.paragraphs;
        if let Some(first) = moved.first_mut() {
            let mut keeps_columns = false;
            let mut i = 0;
            while i < first.controls.len() {
                let drop = match &first.controls[i] {
                    Control::SectionDef(_) => true,
                    Control::ColumnDef(d) => {
                        let same = current.as_deref() == Some(format!("{d:?}").as_str());
                        keeps_columns |= !same;
                        same
                    }
                    _ => break,
                };
                if drop {
                    crate::control_slots::remove_control(first, i);
                } else {
                    i += 1;
                }
            }
            first.column_type = if keeps_columns {
                ColumnBreakType::MultiColumn
            } else {
                ColumnBreakType::None
            };
            first.raw_break_type = 0;
            first.invalidate_layout_inputs();
        }
        prev.paragraphs.extend(moved);
        prev.raw_stream = None;
        prev.raw_provenance = None;
        if back {
            self.cursor = Cursor {
                section: section - 1,
                paragraph: joint,
                ..Cursor::default()
            };
        }
        self.refresh_layout()
    }

    /// 라벨: a new A4 document whose page holds one borderless table, one cell per label
    /// (spacer columns make the gaps between labels).
    fn create_label_sheet(&mut self, p: &Label) -> Result<Value> {
        let (rows, columns) = (p.rows, p.columns);
        let (w, h, gap) = (p.label_width_mm, p.label_height_mm, p.gap_x_mm);
        if !(1..=40).contains(&rows) || !(1..=10).contains(&columns) {
            bail!("라벨은 1–40줄, 1–10칸입니다.");
        }
        if !(5.0..=300.0).contains(&w)
            || !(5.0..=300.0).contains(&h)
            || !(0.0..=30.0).contains(&gap)
        {
            bail!("라벨 크기는 5–300 mm, 칸 사이는 0–30 mm입니다.");
        }
        let (paper_w, paper_h) = (210.0, 297.0);
        let total_w = f64::from(columns) * w + f64::from(columns - 1) * gap;
        let total_h = f64::from(rows) * h;
        let left = p.left_mm.unwrap_or((paper_w - total_w) / 2.0);
        let top = p.top_mm.unwrap_or((paper_h - total_h) / 2.0);
        if left < 0.0 || top < 0.0 || left + total_w > paper_w || top + total_h > paper_h {
            bail!("라벨이 A4 용지({paper_w}×{paper_h} mm)를 벗어납니다.");
        }
        if self.dirty() && !p.force {
            bail!(
                "저장하지 않은 변경이 있습니다. 먼저 저장하거나 force: true로 명시적으로 버리세요."
            );
        }
        self.reset()?;
        self.execute(Command::PageSetup(crate::page::PageSetup {
            width_mm: paper_w,
            height_mm: paper_h,
            landscape: false,
            left_mm: left,
            right_mm: (paper_w - left - total_w).max(0.0),
            top_mm: top,
            // A little slack so the last row is never pushed to a second page.
            bottom_mm: (paper_h - top - total_h - 1.0).max(0.0),
            header_mm: 0.0,
            footer_mm: 0.0,
            gutter_mm: 0.0,
            binding: 0,
        }))?;
        let mut widths = Vec::new();
        for c in 0..columns {
            if c > 0 && gap > 0.0 {
                widths.push(gap);
            }
            widths.push(w);
        }
        let table_columns = widths.len();
        self.run(
            "table.create",
            json!({
                "rows": rows,
                "columns": table_columns,
                "width_mm": total_w,
                "height_mm": total_h,
                "column_widths_mm": widths,
            }),
        )?;
        let none = json!({"type": 0, "width": 0, "color": "#000000"});
        self.run(
            "table.cell_properties",
            json!({
                "range": {"start_row": 0, "start_col": 0, "end_row": rows - 1, "end_col": table_columns - 1},
                "borders": {
                    "left": none, "right": none, "top": none, "bottom": none,
                    "inner_vertical": none, "inner_horizontal": none,
                },
            }),
        )?;
        // A new document: no file yet, nothing to save.
        self.path = None;
        self.saved_revision = self.revision;
        Ok(json!({
            "labels": rows * columns,
            "rows": rows,
            "columns": columns,
            "pages": self.page_count()?,
        }))
    }

    /// Write the document to `path` (.hwp/.hwpx, same bytes as [`Session::save`]) without
    /// making it the document's file or marking it saved: 자동 복구 copies.
    pub fn save_copy(&mut self, path: &std::path::Path) -> Result<()> {
        self.save_with_tab_widths_for(path, true)
    }

    /// The text of `section`'s 양쪽 master page and whether it can be edited.
    fn master_page_text(&self, section: usize, kind: HeaderFooterApply) -> Result<Value> {
        let sec = self
            .core
            .document()
            .sections
            .get(section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let master = sec
            .section_def
            .master_pages
            .iter()
            .find(|m| m.apply_to == kind && !m.is_extension);
        let reason = master_edit_block(sec);
        Ok(json!({
            "section": section,
            "applyTo": MASTER_KINDS[master_kind_index(kind)].1,
            "exists": master.is_some(),
            "text": master.map(|m| m.paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n")),
            "editable": reason.is_none(),
            "reason": reason,
        }))
    }

    /// Create or replace (`Some(text)`) or delete (`None`) the `kind` master page of
    /// `section` in one undo step. Each line becomes a paragraph shaped like the
    /// section's first paragraph; the text area is the body (page minus margins,
    /// header and footer), as Hangul stores it. The master pages stay in Hangul's order
    /// (양쪽, 짝수, 홀수) and the section flags name the kinds present.
    fn set_master_page_text(
        &mut self,
        section: usize,
        kind: HeaderFooterApply,
        text: Option<&str>,
    ) -> Result<()> {
        if let Some(text) = text
            && (text.len() > 10_000
                || text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\r'))
        {
            bail!("바탕쪽 글은 제어 문자 없이 10 KB 이하로 입력하세요.");
        }
        let sec = self
            .core
            .document()
            .sections
            .get(section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        if let Some(why) = master_edit_block(sec) {
            bail!("{why}");
        }
        let text = text.map(|t| t.replace("\r\n", "\n").replace('\r', "\n"));
        self.transaction(|s| {
            let doc = s.core.document_mut();
            let sec = &mut doc.sections[section];
            let pd = &sec.section_def.page_def;
            let text_width = pd.width.saturating_sub(pd.margin_left + pd.margin_right);
            let text_height = pd.height.saturating_sub(
                pd.margin_top + pd.margin_bottom + pd.margin_header + pd.margin_footer,
            );
            let mut masters = std::mem::take(&mut sec.section_def.master_pages);
            // The page's pictures and shapes stay with the new text.
            let objects: Vec<Control> = masters
                .iter()
                .filter(|m| m.apply_to == kind && !m.is_extension)
                .flat_map(|m| &m.paragraphs)
                .flat_map(|p| &p.controls)
                .filter(|c| matches!(c, Control::Picture(_) | Control::Shape(_)))
                .cloned()
                .collect();
            masters.retain(|m| m.apply_to != kind);
            if let Some(text) = &text {
                let template = sec
                    .paragraphs
                    .first()
                    .cloned()
                    .unwrap_or_else(DocParagraph::new_empty);
                let mut paragraphs: Vec<DocParagraph> = text
                    .split('\n')
                    .map(|line| {
                        let mut p = DocParagraph::new_empty_like(&template);
                        if !line.is_empty() {
                            p.insert_text_at(0, line);
                        }
                        p
                    })
                    .collect();
                if let Some(first) = paragraphs.first_mut() {
                    for object in objects {
                        let end = first.text.chars().count();
                        crate::control_slots::insert_control(first, end, object, None);
                        first.control_mask |= 1 << 11;
                        first.has_para_text = true;
                    }
                    first.invalidate_layout_inputs();
                }
                masters.push(MasterPage {
                    apply_to: kind,
                    paragraphs,
                    text_width,
                    text_height,
                    ..Default::default()
                });
            }
            masters.sort_by_key(|m| master_kind_index(m.apply_to));
            // HWP keeps the kinds present in the top section-flag bits and reads raw
            // records before the model; the raw copies are dropped so the edited model
            // is what gets saved.
            let flags = masters
                .iter()
                .fold(sec.section_def.flags & !MASTER_KIND_FLAGS, |flags, m| {
                    flags | MASTER_KINDS[master_kind_index(m.apply_to)].3
                });
            sec.section_def.flags = flags;
            sec.section_def.extra_child_records.clear();
            sec.section_def.master_pages = masters.clone();
            for control in sec
                .paragraphs
                .iter_mut()
                .flat_map(|p| p.controls.iter_mut())
            {
                if let Control::SectionDef(def) = control {
                    def.flags = flags;
                    def.extra_child_records.clear();
                    def.master_pages = masters.clone();
                }
            }
            sec.raw_stream = None;
            s.refresh_layout()
        })
    }

    /// [`Session::pdf_finish`] with 인쇄 상세 applied to the prepared `pages`: order,
    /// copies, 모아 찍기 and watermark. Same checks as `pdf_finish`.
    pub fn pdf_finish_with(
        &self,
        prep: PdfPrep,
        pages: &[u32],
        layout: &PrintLayout,
    ) -> Result<PdfJob> {
        if prep.generation() != self.generation {
            bail!("PDF를 준비하는 동안 문서가 바뀌었습니다.");
        }
        if prep.ready() != prep.total() || prep.total() != pages.len() {
            bail!("PDF 쪽 준비가 끝나지 않았습니다.");
        }
        let (svgs, links) = prep.into_parts();
        let (svgs, links) = layout.apply(pages, svgs, links)?;
        Ok(PdfJob::from_svgs(svgs, links))
    }

    /// 미리 보기: sheet `sheet` of the whole document printed with `layout` (one copy),
    /// as the print SVG it becomes, and how many sheets there are.
    pub fn print_preview_sheet(
        &self,
        sheet: usize,
        layout: &PrintLayout,
    ) -> Result<(String, usize)> {
        let all: Vec<u32> = (0..self.page_count()?).collect();
        let sheets = layout.sheet_count(&all);
        if sheets == 0 {
            bail!("미리 볼 쪽이 없습니다.");
        }
        let pages = layout.sheet_pages(&all, sheet.min(sheets - 1));
        let mut prep = self.pdf_prepare(&pages)?;
        self.pdf_prepare_step(&mut prep, std::time::Duration::MAX)?;
        let (svgs, _) = prep.into_parts();
        Ok((layout.compose_sheet(svgs)?, sheets))
    }

    fn header_footer_list(&self) -> Result<Value> {
        Ok(serde_json::from_str(
            &self
                .core
                .get_header_footer_list_native(self.cursor.section, true, 0)
                .map_err(err)?,
        )?)
    }

    /// First 쪽 번호 위치 control of the cursor's section.
    fn page_number_control(&self) -> Option<(usize, usize, PageNumberPos)> {
        let section = self.core.document().sections.get(self.cursor.section)?;
        section.paragraphs.iter().enumerate().find_map(|(pi, p)| {
            p.controls.iter().enumerate().find_map(|(ci, c)| match c {
                Control::PageNumberPos(n) => Some((pi, ci, n.clone())),
                _ => None,
            })
        })
    }

    fn page_number(&self) -> Result<Value> {
        let start = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .map(|s| s.section_def.page_num)
            .unwrap_or(0);
        Ok(match self.page_number_control() {
            Some((_, _, n)) => json!({
                "exists": true,
                "position": NUMBER_POSITIONS.iter().find(|(_, c)| *c == n.position).map(|(k, _)| *k).unwrap_or("none"),
                "format": n.format,
                "dash": n.dash_char != '\0',
                "start": start,
            }),
            None => {
                json!({"exists": false, "position": "bottom_center", "format": 0, "dash": true, "start": start})
            }
        })
    }

    fn set_page_number(&mut self, p: &PageNumber) -> Result<()> {
        let position = NUMBER_POSITIONS
            .iter()
            .find(|(k, _)| *k == p.position)
            .map(|(_, c)| *c)
            .ok_or_else(|| anyhow!("알 수 없는 쪽 번호 위치: {}", p.position))?;
        if !NUMBER_FORMATS.iter().any(|(c, _)| *c == p.format) {
            bail!("지원하지 않는 번호 모양입니다: {}", p.format);
        }
        if p.start == Some(0) {
            bail!("시작 번호는 1 이상이어야 합니다.");
        }
        self.require_body()?;
        let control = PageNumberPos {
            format: p.format,
            position,
            dash_char: if p.dash { '-' } else { '\0' },
            ..Default::default()
        };
        let existing = self.page_number_control();
        let cursor = self.cursor;
        self.transaction(|s| {
            let (pi, ci) = match existing {
                Some((pi, ci, _)) => (pi, ci),
                None => {
                    // RHWP has no public pgnp insert; reuse its inline-control insertion
                    // (placeholder, char count, line starts) and swap the control kind.
                    let r: Value = serde_json::from_str(
                        &s.core
                            .insert_new_number_native(cursor.section, cursor.paragraph, 0, 1)
                            .map_err(err)?,
                    )?;
                    let ci = r["controlIdx"]
                        .as_u64()
                        .and_then(|v| usize::try_from(v).ok())
                        .ok_or_else(|| anyhow!("쪽 번호 컨트롤 위치를 찾지 못했습니다."))?;
                    (cursor.paragraph, ci)
                }
            };
            {
                let para = s
                    .core
                    .document_mut()
                    .sections
                    .get_mut(cursor.section)
                    .and_then(|sec| sec.paragraphs.get_mut(pi))
                    .ok_or_else(|| anyhow!("쪽 번호 문단을 찾지 못했습니다."))?;
                let slot = para
                    .controls
                    .get_mut(ci)
                    .ok_or_else(|| anyhow!("쪽 번호 컨트롤을 찾지 못했습니다."))?;
                *slot = Control::PageNumberPos(control);
                if !para
                    .controls
                    .iter()
                    .any(|c| matches!(c, Control::NewNumber(_)))
                {
                    para.control_mask &= !(1 << 0x12);
                }
                para.control_mask |= 1 << 0x15;
            }
            if let Some(sec) = s.core.document_mut().sections.get_mut(cursor.section) {
                sec.raw_stream = None;
            }
            // Recompose and paginate the section; also sets the start number.
            let def = match p.start {
                Some(n) => json!({"pageNum": n}),
                None => json!({}),
            };
            s.core
                .set_section_def_native(cursor.section, &def.to_string())
                .map_err(err)?;
            Ok(())
        })
    }

    /// 문서 정보: statistics, fonts and pictures (RHWP keeps no summary properties).
    fn document_statistics(&self) -> Value {
        let pages = self.page_count();
        let doc = self.core.document();
        let mut paragraphs = 0usize;
        let mut chars = 0usize;
        let mut spaces = 0usize;
        let mut words = 0usize;
        let mut lines = 0usize;
        let (mut tables, mut pictures, mut shapes, mut equations, mut notes) = (0, 0, 0, 0, 0);
        for section in &doc.sections {
            for p in &section.paragraphs {
                paragraphs += 1;
                chars += p.text.chars().filter(|c| !c.is_control()).count();
                spaces += p.text.chars().filter(|c| c.is_whitespace()).count();
                words += p.text.split_whitespace().count();
                lines += p.line_segs.len().max(1);
                for c in &p.controls {
                    match c {
                        Control::Table(_) => tables += 1,
                        Control::Picture(_) => pictures += 1,
                        Control::Shape(_) => shapes += 1,
                        Control::Equation(_) => equations += 1,
                        Control::Footnote(_) | Control::Endnote(_) => notes += 1,
                        _ => {}
                    }
                }
            }
        }
        let fonts: Vec<String> = {
            let mut names: Vec<String> = doc
                .doc_info
                .font_faces
                .iter()
                .flatten()
                .map(|f| f.name.clone())
                .filter(|n| !n.is_empty())
                .collect();
            names.sort();
            names.dedup();
            names
        };
        let secs = |t: std::io::Result<std::time::SystemTime>| {
            t.ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
        };
        let file = self.path.as_ref().and_then(|p| {
            let meta = std::fs::metadata(p).ok()?;
            Some(json!({
                "path": p.display().to_string(),
                "format": p.extension().and_then(|e| e.to_str()).map(str::to_ascii_uppercase),
                "bytes": meta.len(),
                "created": secs(meta.created()),
                "modified": secs(meta.modified()),
            }))
        });
        // RHWP's own summary adds the fonts it had to substitute on this machine.
        let rhwp: Value =
            serde_json::from_str(&self.core.get_document_info()).unwrap_or(Value::Null);
        let mut used_by = std::collections::HashMap::<u16, usize>::new();
        let mut count_pictures = |paragraphs: &[Paragraph]| {
            for p in paragraphs {
                for c in &p.controls {
                    match c {
                        Control::Picture(pic) => {
                            *used_by.entry(pic.image_attr.bin_data_id).or_default() += 1
                        }
                        Control::Table(t) => {
                            for cell in &t.cells {
                                for p in &cell.paragraphs {
                                    for c in &p.controls {
                                        if let Control::Picture(pic) = c {
                                            *used_by
                                                .entry(pic.image_attr.bin_data_id)
                                                .or_default() += 1;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        };
        for section in &doc.sections {
            count_pictures(&section.paragraphs);
        }
        let picture_data: Vec<Value> = doc
            .doc_info
            .bin_data_list
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let id = u16::try_from(i + 1).unwrap_or(u16::MAX);
                let bytes = doc
                    .bin_data_content
                    .iter()
                    .find(|c| c.id == b.storage_id)
                    .map(|c| c.data.len());
                json!({
                    "id": id,
                    "type": format!("{:?}", b.data_type),
                    "extension": b.extension.clone().or_else(|| b.abs_path.clone()).or_else(|| b.rel_path.clone()),
                    "bytes": bytes,
                    "usedBy": used_by.get(&id).copied().unwrap_or(0),
                })
            })
            .collect();
        json!({
            "file": file,
            "pages": pages.as_ref().ok(),
            "layoutError": pages.as_ref().err().map(ToString::to_string),
            "sections": doc.sections.len(),
            "paragraphs": paragraphs,
            "lines": lines,
            "characters": chars,
            "charactersNoSpaces": chars - spaces,
            "words": words,
            "tables": tables,
            "pictures": pictures,
            "shapes": shapes,
            "equations": equations,
            "notes": notes,
            "fonts": fonts,
            "binData": doc.doc_info.bin_data_list.len(),
            "pictureData": picture_data,
            "fontSubstitutions": rhwp["fontSubstitutions"].clone(),
            "fallbackFont": rhwp["fallbackFont"].clone(),
            "formatVersion": rhwp["version"].clone(),
            "summarySupported": false,
        })
    }
}

// ----- 0.13: 보기 표식 · 머리말/꼬리말 필드 · 구역 설정 · 개인 정보 · 쪽 범위 저장 -----

/// Screen-only view flags (문단 부호·조판 부호·투명 선). Never written to the document
/// and not part of the undo history; changing them bumps `generation` so cached page
/// images are redrawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ViewMarks {
    #[serde(default)]
    pub paragraph_marks: bool,
    #[serde(default)]
    pub control_codes: bool,
    #[serde(default)]
    pub transparent_borders: bool,
}

fn hf_field(kind: &str) -> Result<(u8, char)> {
    HF_FIELDS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, code, c)| (*code, *c))
        .ok_or_else(|| {
            anyhow!(
                "머리말/꼬리말 필드는 page_number, total_pages, file_name 중 하나입니다: {kind}"
            )
        })
}

impl Session {
    /// RHWP keeps the view flags private on `DocumentCore`; only its wasm wrapper
    /// `HwpDocument` has setters. Swap the live core into a throw-away wrapper.
    fn with_view_wrapper<T>(&mut self, f: impl FnOnce(&mut hwpcraft_doc::HwpDocument) -> T) -> T {
        let mut wrapper = hwpcraft_doc::HwpDocument::create_empty();
        std::mem::swap(&mut *wrapper, &mut self.core);
        let out = f(&mut wrapper);
        std::mem::swap(&mut *wrapper, &mut self.core);
        out
    }
    pub fn view_marks(&mut self) -> ViewMarks {
        self.with_view_wrapper(|d| ViewMarks {
            paragraph_marks: d.get_show_paragraph_marks(),
            control_codes: d.get_show_control_codes(),
            transparent_borders: d.get_show_transparent_borders(),
        })
    }
    /// Returns the flags now in effect; a new document (`open`/`reset`) starts with none.
    pub fn set_view_marks(&mut self, marks: ViewMarks) -> ViewMarks {
        let changed = self.with_view_wrapper(|d| {
            let before = ViewMarks {
                paragraph_marks: d.get_show_paragraph_marks(),
                control_codes: d.get_show_control_codes(),
                transparent_borders: d.get_show_transparent_borders(),
            };
            if before != marks {
                d.set_show_paragraph_marks(marks.paragraph_marks);
                d.set_show_control_codes(marks.control_codes);
                d.set_show_transparent_borders(marks.transparent_borders);
            }
            before != marks
        });
        if changed {
            self.generation += 1;
            self.rebase_visual_hint(self.generation - 1);
        }
        self.projection_view_marks = marks;
        marks
    }

    /// First paragraph of the header/footer `(section, header, apply)`; `None` when absent.
    fn hf_paragraph(&self, header: bool, apply: u8) -> Result<Option<&Paragraph>> {
        let info: Value = serde_json::from_str(
            &self
                .core
                .get_header_footer_native(self.cursor.section, header, apply)
                .map_err(err)?,
        )?;
        if info["exists"] != true {
            return Ok(None);
        }
        let index = |k: &str| {
            info[k]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| anyhow!("머리말/꼬리말 주소를 읽지 못했습니다."))
        };
        let (pi, ci) = (index("paraIndex")?, index("controlIndex")?);
        let control = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .and_then(|s| s.paragraphs.get(pi))
            .and_then(|p| p.controls.get(ci));
        let paragraphs = match control {
            Some(Control::Header(h)) => &h.paragraphs,
            Some(Control::Footer(f)) => &f.paragraphs,
            _ => bail!("머리말/꼬리말을 찾지 못했습니다."),
        };
        if paragraphs.len() != 1 {
            bail!("여러 문단으로 된 머리말/꼬리말은 이 창에서 고칠 수 없습니다.");
        }
        Ok(paragraphs.first())
    }

    /// Text and field markers of a header/footer: `{exists, text, fields:[{offset, kind}]}`.
    /// `text` has the markers removed; each field offset refers to that text.
    fn header_footer_fields(&self, header: bool, apply: u8) -> Result<Value> {
        let Some(p) = self.hf_paragraph(header, apply)? else {
            return Ok(json!({"exists": false, "text": "", "fields": [], "applyTo": apply}));
        };
        use hwpcraft_doc::model::control::{AutoNumberType, FieldType};
        // Freshly inserted fields are marker characters; after a save RHWP lowers them to
        // AutoNumber controls (placeholder space) and a path Field wrapping the file name.
        let positions = p.control_text_positions();
        let chars: Vec<char> = p.text.chars().collect();
        let mut at_control: Vec<(usize, usize, &str)> = Vec::new(); // (start, end, kind)
        for (ci, control) in p.controls.iter().enumerate() {
            let Some(&start) = positions.get(ci) else {
                continue;
            };
            // An AutoNumber owns one placeholder space; RHWP reports a trailing one at `len`.
            let placeholder = |start: usize| {
                let at = start.min(chars.len().saturating_sub(1));
                if chars.get(at) == Some(&' ') {
                    (at, at + 1)
                } else {
                    (at, at)
                }
            };
            match control {
                Control::AutoNumber(a) if a.number_type == AutoNumberType::Page => {
                    let (s, e) = placeholder(start);
                    at_control.push((s, e, "page_number"));
                }
                Control::AutoNumber(a) if a.number_type == AutoNumberType::TotalPage => {
                    let (s, e) = placeholder(start);
                    at_control.push((s, e, "total_pages"));
                }
                Control::Field(f) if f.field_type == FieldType::Path => {
                    // The field wraps the literal file name (empty for an unsaved document).
                    let (s, e) = p
                        .field_ranges
                        .iter()
                        .find(|r| r.control_idx == ci)
                        .map_or((start, start), |r| {
                            (r.start_char_idx, r.end_char_idx.max(r.start_char_idx))
                        });
                    at_control.push((s, e, "file_name"));
                }
                _ => {}
            }
        }
        at_control.sort_by_key(|(s, e, _)| (*s, *e));
        let mut text = String::new();
        let mut fields = Vec::new();
        let mut offset = 0usize;
        let mut skip_until = 0usize;
        let mut next_control = 0usize;
        for (i, c) in chars.iter().copied().enumerate() {
            // Zero-width fields (empty file name) sit before the character at `i`.
            let mut skipped = false;
            while let Some((s, e, kind)) = at_control.get(next_control).copied()
                && s <= i
            {
                fields.push(HfField {
                    offset,
                    kind: kind.to_owned(),
                });
                skip_until = skip_until.max(e);
                next_control += 1;
                skipped |= e > i;
            }
            if i < skip_until || skipped {
                continue;
            }
            match HF_FIELDS.iter().find(|(_, _, m)| *m == c) {
                Some((kind, _, _)) => fields.push(HfField {
                    offset,
                    kind: (*kind).to_owned(),
                }),
                None if c.is_control() => {}
                None => {
                    text.push(c);
                    offset += 1;
                }
            }
        }
        Ok(json!({"exists": true, "text": text, "fields": fields, "applyTo": apply}))
    }

    /// Delete the section's header or footer for `apply` (0 both, 1 even, 2 odd). RHWP's
    /// delete_header_footer_native leaves the control's slot and CTRL_DATA record behind
    /// and later field ranges on the wrong control.
    fn delete_header_footer(&mut self, section: usize, header: bool, apply: u8) -> Result<()> {
        let kind = match apply {
            1 => HeaderFooterApply::Even,
            2 => HeaderFooterApply::Odd,
            _ => HeaderFooterApply::Both,
        };
        let sec = &mut self.core.document_mut().sections[section];
        let at = sec.paragraphs.iter().enumerate().find_map(|(pi, p)| {
            let ci = p.controls.iter().position(|c| match c {
                Control::Header(h) => header && h.apply_to == kind,
                Control::Footer(f) => !header && f.apply_to == kind,
                _ => false,
            });
            ci.map(|ci| (pi, ci))
        });
        let Some((pi, ci)) = at else {
            bail!("{}이 없습니다.", if header { "머리말" } else { "꼬리말" });
        };
        crate::control_slots::remove_control(&mut sec.paragraphs[pi], ci);
        sec.raw_stream = None;
        self.refresh_layout()
    }

    /// Replace a header/footer with one line of text plus page-number/total/file-name fields.
    fn set_header_footer_fields(&mut self, p: &HfText) -> Result<()> {
        if p.text.len() > 10_000 || p.text.chars().any(char::is_control) {
            bail!("머리말/꼬리말은 제어 문자 없는 한 줄, 10 KB 이하로 입력하세요.");
        }
        let length = p.text.chars().count();
        let mut fields: Vec<(usize, usize, u8)> = Vec::with_capacity(p.fields.len());
        for (i, f) in p.fields.iter().enumerate() {
            if f.offset > length {
                bail!("필드 위치 {}가 글자 수 {length}를 넘습니다.", f.offset);
            }
            fields.push((f.offset, i, hf_field(&f.kind)?.0));
        }
        // Insert from the end so earlier offsets stay valid. Fields sharing an offset are
        // inserted last-listed first, so the listed order is the final order.
        fields.sort_by_key(|f| std::cmp::Reverse((f.0, f.1)));
        let apply = apply_code(&p.apply_to)?;
        self.require_body()?;
        let exists = self.hf_paragraph(p.header, apply)?.is_some();
        let header = p.header;
        let text = p.text.clone();
        self.transaction(|s| {
            let section = s.cursor.section;
            // A reopened header may hold lowered AutoNumber/Field controls; rebuilding it
            // from scratch keeps text, fields and controls consistent.
            if exists {
                s.delete_header_footer(section, header, apply)?;
            }
            s.core
                .create_header_footer_native(section, header, apply)
                .map_err(err)?;
            if !text.is_empty() {
                s.core
                    .insert_text_in_header_footer_native(section, header, apply, 0, 0, &text)
                    .map_err(err)?;
            }
            for (offset, _, code) in &fields {
                s.core
                    .insert_field_in_hf_native(section, header, apply, 0, *offset, *code)
                    .map_err(err)?;
            }
            Ok(())
        })
    }

    /// 구역 설정 of the cursor's section, numbers as stored plus mm conversions.
    fn section_def(&self) -> Result<Value> {
        let mut v: Value = serde_json::from_str(
            &self
                .core
                .get_section_def_native(self.cursor.section)
                .map_err(err)?,
        )?;
        let tab = v["defaultTabSpacing"].as_f64().unwrap_or(0.0) / HU_PER_MM;
        v["defaultTabSpacingMm"] = json!((tab * 100.0).round() / 100.0);
        v["section"] = json!(self.cursor.section);
        v["sections"] = json!(self.core.document().sections.len());
        v["lineNumbersSupported"] = json!(false);
        Ok(v)
    }

    fn section_def_json(p: &SectionDef) -> Result<Value> {
        let mut v = json!({});
        if let Some(n) = p.page_num {
            v["pageNum"] = json!(n);
        }
        if let Some(t) = p.page_num_type {
            if t > 3 {
                bail!("쪽 번호 종류는 0–3입니다.");
            }
            v["pageNumType"] = json!(t);
        }
        for (key, value) in [
            ("pictureNum", p.picture_num),
            ("tableNum", p.table_num),
            ("equationNum", p.equation_num),
        ] {
            if let Some(n) = value {
                v[key] = json!(n);
            }
        }
        if let Some(mm) = p.default_tab_spacing_mm {
            if !mm.is_finite() || !(0.0..=100.0).contains(&mm) {
                bail!("기본 탭 간격은 0–100 mm입니다.");
            }
            v["defaultTabSpacing"] = json!((mm * HU_PER_MM).round() as u32);
        }
        for (key, value) in [
            ("hideHeader", p.hide_header),
            ("hideFooter", p.hide_footer),
            ("hideMasterPage", p.hide_master_page),
            ("hideBorder", p.hide_border),
            ("hideFill", p.hide_fill),
            ("hideEmptyLine", p.hide_empty_line),
        ] {
            if let Some(b) = value {
                v[key] = json!(b);
            }
        }
        if v.as_object().is_some_and(|o| o.is_empty()) {
            bail!("바꿀 구역 설정 항목을 하나 이상 지정하세요.");
        }
        Ok(v)
    }

    /// Personal data (validated by RHWP) and hidden text. Read only; `raw` values are the data itself.
    fn privacy_scan(&self, p: &PrivacyScan) -> Result<Value> {
        let mask = {
            let mut chars = p.mask.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => c,
                _ => bail!("가릴 문자는 한 글자여야 합니다."),
            }
        };
        let kinds: Vec<PiiKind> = if p.kinds.is_empty() {
            PiiKind::all().to_vec()
        } else {
            p.kinds
                .iter()
                .map(|k| PiiKind::parse(k).ok_or_else(|| anyhow!("알 수 없는 개인 정보 종류: {k}")))
                .collect::<Result<_>>()?
        };
        let doc = self.core.document();
        let findings: Vec<Value> = self
            .core
            .scan_pii(&kinds, mask)
            .into_iter()
            .map(|f| {
                // Body matches can be selected; cell/text-box matches only name the body paragraph.
                let in_body = doc
                    .sections
                    .get(f.section)
                    .and_then(|s| s.paragraphs.get(f.paragraph))
                    .is_some_and(|q| {
                        q.text
                            .chars()
                            .skip(f.char_offset)
                            .take(f.raw.chars().count())
                            .eq(f.raw.chars())
                    });
                let label = PII_KINDS
                    .iter()
                    .find(|(k, _)| *k == f.kind)
                    .map_or(f.kind, |(_, l)| *l);
                json!({
                    "kind": f.kind, "label": label, "raw": f.raw, "masked": f.masked,
                    "section": f.section, "paragraph": f.paragraph, "page": f.page,
                    "charOffset": f.char_offset, "inBody": in_body,
                })
            })
            .collect();
        let hidden = if p.hidden_text {
            let options = HiddenTextOptions {
                include_off_page: p.off_page,
                ..Default::default()
            };
            serde_json::to_value(self.core.detect_hidden_text(&options))?
        } else {
            Value::Null
        };
        Ok(json!({"findings": findings, "hiddenText": hidden}))
    }

    /// Replace every occurrence of each value with its mask in one undo step.
    fn privacy_mask(&mut self, p: &PrivacyMask) -> Result<Value> {
        if p.items.is_empty() {
            bail!("가릴 항목을 선택하세요.");
        }
        for item in &p.items {
            if item.raw.is_empty()
                || item.raw.chars().count() != item.masked.chars().count()
                || item.raw.chars().any(char::is_control)
                || item.masked.chars().any(char::is_control)
            {
                bail!("가릴 값과 가린 결과는 글자 수가 같은 한 줄 텍스트여야 합니다.");
            }
            if item.raw.len() > 1_000 {
                bail!("가릴 값은 1,000바이트 이하입니다.");
            }
        }
        let items: Vec<(String, String)> = p
            .items
            .iter()
            .filter(|i| i.raw != i.masked)
            .map(|i| (i.raw.clone(), i.masked.clone()))
            .collect();
        let mut replaced = 0u64;
        self.transaction(|s| {
            for (raw, masked) in &items {
                let r: Value = serde_json::from_str(
                    &s.core.replace_all_native(raw, masked, true).map_err(err)?,
                )?;
                replaced += r["count"].as_u64().unwrap_or(0);
            }
            Ok(())
        })?;
        Ok(json!({"replaced": replaced}))
    }

    /// Save only `from..=to` pages (1-based). The open document, cursor, path and undo
    /// history stay unchanged: partial extraction runs on a private model clone.
    fn save_pages(&mut self, p: &SavePages) -> Result<Value> {
        let count = self.page_count()?;
        if p.from == 0 || p.to < p.from || p.to > count {
            bail!(
                "쪽 범위는 1–{count} 안에서 작은 번호부터 적으세요: {}–{}",
                p.from,
                p.to
            );
        }
        if !matches!(
            p.path
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("hwp" | "hwpx" | "pdf" | "txt")
        ) {
            bail!("쪽 범위 저장 형식: HWP, HWPX, PDF, TXT");
        }
        self.refuse_tracked_save(&p.path)?;
        if p.path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            let bytes = self.render_pdf(&((p.from - 1)..p.to).collect::<Vec<_>>())?;
            hwpcraft_formats::atomic_write(&p.path, &bytes)?;
            return Ok(json!({"path":p.path,"pagesBefore":count,"pagesAfter":p.to-p.from+1}));
        }
        // Selecting every displayed page means preserving the whole document,
        // including sections, hidden/structural paragraphs and opaque resources.
        // Do not call the native reproducer extractor, which prunes resources.
        if p.from == 1 && p.to == count {
            self.save_with_tab_widths(&p.path)?;
            let kept: usize = self
                .core
                .document()
                .sections
                .iter()
                .map(|s| s.paragraphs.len())
                .sum();
            return Ok(json!({"path":p.path,"pagesBefore":count,"pagesAfter":count,
                "paragraphsKept":kept,"paragraphsRemoved":0}));
        }
        // Native extraction keeps entire touching paragraphs, not page slices.
        // Its page-start query always reports scalar zero, including a partial
        // paragraph/table. Check coverage as well as its coarse start addresses.
        let layout = self.page_field_layout()?;
        if let Some(layout) = &layout {
            let mut same = count == self.core.page_count();
            if same {
                for page in 0..count {
                    if self
                        .core
                        .get_position_of_page_native(page as usize)
                        .map_err(err)?
                        != layout
                            .core
                            .get_position_of_page_native(page as usize)
                            .map_err(err)?
                    {
                        same = false;
                        break;
                    }
                }
            }
            if !same {
                bail!(
                    "표시 조판과 원본의 쪽 경계가 달라 HWP/HWPX/TXT의 일부 쪽을 정확히 추출할 수 없습니다. PDF 쪽 저장을 사용하세요."
                );
            }
        }
        let boundary = |core: &DocumentCore, page: u32| -> Result<(usize, usize)> {
            let value: Value = serde_json::from_str(
                &core
                    .get_position_of_page_native(page as usize)
                    .map_err(err)?,
            )?;
            Ok((usize_field(&value, "sec")?, usize_field(&value, "para")?))
        };
        let check_cuts = |core: &mut DocumentCore| -> Result<()> {
            let start = boundary(core, p.from - 1)?;
            let pages = core.pages_covering_paragraphs(&[start]).ok_or_else(|| {
                anyhow!("시작 문단의 쪽 범위를 확인하지 못했습니다. PDF 쪽 저장을 사용하세요.")
            })?;
            if pages.iter().any(|page| *page < p.from - 1 || *page >= p.to) {
                bail!(
                    "선택한 쪽 경계가 문단 또는 표의 중간에 걸립니다. 해당 문단·표의 모든 쪽을 선택하거나 PDF 쪽 저장을 사용하세요."
                );
            }
            if p.to < count {
                let next = boundary(core, p.to)?;
                let pages = core.pages_covering_paragraphs(&[next]).ok_or_else(|| {
                    anyhow!("끝 문단의 쪽 범위를 확인하지 못했습니다. PDF 쪽 저장을 사용하세요.")
                })?;
                if pages.iter().any(|page| *page >= p.from - 1 && *page < p.to) {
                    bail!(
                        "선택한 쪽 경계가 문단 또는 표의 중간에 걸립니다. 해당 문단·표의 모든 쪽을 선택하거나 PDF 쪽 저장을 사용하세요."
                    );
                }
            }
            Ok(())
        };
        let dpi = crate::nested_presentation::source_dpi(&self.core)?;
        let mut output = Self::with_core(
            crate::nested_presentation::clone_core(&self.core, dpi)?,
            None,
        );
        output.password = self.password.clone();
        if output.core.page_count() != count {
            bail!("쪽 경계를 동일하게 복제하지 못했습니다. PDF 쪽 저장을 사용하세요.");
        }
        check_cuts(&mut output.core)?;
        if let Some(layout) = layout {
            let dpi = crate::nested_presentation::source_dpi(&layout.core)?;
            let mut displayed = crate::nested_presentation::clone_core(&layout.core, dpi)?;
            check_cuts(&mut displayed)?;
        }
        let first = boundary(&output.core, p.from - 1)?;
        let end = if p.to < count {
            Some(boundary(&output.core, p.to)?)
        } else {
            None
        };
        let signature = |paragraph: &Paragraph| -> Result<Value> {
            let controls = paragraph
                .controls
                .iter()
                .filter(|control| {
                    !matches!(control, Control::SectionDef(_) | Control::ColumnDef(_))
                })
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            // Structural section controls may move to the first kept paragraph,
            // shifting UTF-16 offsets. Compare scalar styles and full nested
            // control payloads rather than their parent paragraph's raw offsets.
            let shapes: Vec<_> = (0..paragraph.text.chars().count())
                .map(|offset| paragraph.char_shape_id_at(offset))
                .collect();
            Ok(
                json!({"text":paragraph.text,"paragraphStyle":paragraph.para_shape_id,
                "style":paragraph.style_id,"characters":shapes,
                "emptyShape":paragraph.text.is_empty().then(|| paragraph.char_shape_id_at(0)),
                "markpen":crate::markpen_ranges(paragraph),"controls":controls}),
            )
        };
        let expected: Vec<Vec<Value>> = output
            .core
            .document()
            .sections
            .iter()
            .enumerate()
            .map(|(si, section)| {
                section
                    .paragraphs
                    .iter()
                    .enumerate()
                    .filter(|(pi, _)| (si, *pi) >= first && end.is_none_or(|end| (si, *pi) < end))
                    .map(|(_, paragraph)| signature(paragraph))
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        let report = output.core.extract_page_range(p.from, p.to).map_err(err)?;
        // The native extractor intentionally retains an excluded section's last
        // paragraph, and may retain paragraphs it cannot delete. Do not silently
        // write excluded text or omit selected text in a user-facing export.
        let actual: Vec<Vec<Value>> = output
            .core
            .document()
            .sections
            .iter()
            .map(|section| {
                section
                    .paragraphs
                    .iter()
                    .map(&signature)
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        if actual != expected {
            bail!(
                "선택 밖의 구역·문단을 정확히 제외하지 못했습니다. 원본과 출력 파일은 변경하지 않았습니다. PDF 쪽 저장을 사용하세요."
            );
        }
        output.save_with_tab_widths(&p.path)?;
        Ok(json!({
            "path": p.path,
            "pagesBefore": report.pages_before,
            "pagesAfter": report.pages_after,
            "paragraphsKept": report.kept,
            "paragraphsRemoved": report.removed,
        }))
    }
}

// ----- 0.14: 쪽 여백 사전 값 · 구역 이동 · 쪽 복사/붙이기/지우기 · 문서 끼워 넣기 · 블록 저장 -----

/// Hangul's 쪽 여백 gallery, in its order: id, label, [top, bottom, left, right, header,
/// footer] in millimetres. The gutter and paper keep their current values.
pub const MARGIN_PRESETS: &[(&str, &str, [f64; 6])] = &[
    ("default", "기본", [20.0, 15.0, 30.0, 30.0, 15.0, 15.0]),
    (
        "narrow1_hf",
        "좁게1(머리말/꼬리말 여백포함)",
        [15.0, 15.0, 20.0, 20.0, 10.0, 10.0],
    ),
    (
        "narrow2_hf",
        "좁게2(머리말/꼬리말 여백포함)",
        [5.0, 5.0, 10.0, 10.0, 10.0, 10.0],
    ),
    (
        "wide_hf",
        "넓게(머리말/꼬리말 여백포함)",
        [35.0, 35.0, 40.0, 40.0, 10.0, 10.0],
    ),
    ("narrow", "좁게", [10.0, 10.0, 10.0, 10.0, 0.0, 0.0]),
    ("wide", "넓게", [35.0, 35.0, 40.0, 40.0, 0.0, 0.0]),
];

fn margin_presets_json() -> Value {
    Value::Array(
        MARGIN_PRESETS
            .iter()
            .map(|(id, label, m)| {
                json!({"preset": id, "label": label, "topMm": m[0], "bottomMm": m[1],
                    "leftMm": m[2], "rightMm": m[3], "headerMm": m[4], "footerMm": m[5]})
            })
            .collect(),
    )
}

/// 1-based page for CLI/MCP callers; `None` means the caret's page.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PageTarget {
    #[serde(default)]
    page: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InsertFile {
    path: std::path::PathBuf,
    /// Password of the inserted document, if it is protected.
    #[serde(default)]
    password: Option<String>,
}

// The copied page belongs to this Session. A process-global clipboard would
// let an unrelated MCP document paste content without its source handle.

/// `Document` holding only `range` of section `sec` of `doc`, with the format tables kept so
/// `paste_foreign_document_native` can remap them.
fn excerpt(doc: &Document, sec: usize, range: std::ops::Range<usize>) -> Result<Document> {
    let section = doc
        .sections
        .get(sec)
        .ok_or_else(|| anyhow!("구역 {}이 없습니다.", sec + 1))?;
    let paragraphs = section
        .paragraphs
        .get(range)
        .ok_or_else(|| anyhow!("문단 범위를 읽지 못했습니다."))?
        .to_vec();
    let mut copy = section.clone();
    copy.paragraphs = paragraphs;
    copy.raw_stream = None;
    copy.raw_provenance = None;
    let mut foreign = doc.clone();
    foreign.sections = vec![copy];
    Ok(foreign)
}

fn usize_field(v: &Value, key: &str) -> Result<usize> {
    v[key]
        .as_u64()
        .and_then(|x| usize::try_from(x).ok())
        .ok_or_else(|| anyhow!("RHWP 응답에 {key}가 없습니다: {v}"))
}

impl Session {
    fn apply_margin_preset(&mut self, preset: &str) -> Result<Value> {
        let Some((_, label, m)) = MARGIN_PRESETS.iter().find(|(id, _, _)| *id == preset) else {
            let ids: Vec<&str> = MARGIN_PRESETS.iter().map(|(id, _, _)| *id).collect();
            bail!(
                "쪽 여백 사전 값은 {} 중 하나입니다: {preset}",
                ids.join(", ")
            );
        };
        let mut setup = self.page_setup()?;
        setup.top_mm = m[0];
        setup.bottom_mm = m[1];
        setup.left_mm = m[2];
        setup.right_mm = m[3];
        setup.header_mm = m[4];
        setup.footer_mm = m[5];
        self.execute(Command::PageSetup(setup.clone()))?;
        Ok(json!({"preset": preset, "label": label, "setup": setup}))
    }

    /// Hangul SectionToNext/SectionToPrev: caret to the first paragraph of the neighbouring
    /// section.
    fn move_section(&mut self, next: bool) -> Result<Value> {
        let count = self.core.document().sections.len();
        let current = self.cursor.section;
        let target = if next {
            current + 1
        } else {
            current.wrapping_sub(1)
        };
        if target >= count {
            bail!(if next {
                "마지막 구역입니다."
            } else {
                "첫 구역입니다."
            });
        }
        self.select(
            Cursor {
                section: target,
                paragraph: 0,
                offset: 0,
                cell: None,
            },
            false,
        )?;
        Ok(json!({"section": target + 1, "sections": count}))
    }

    /// 0-based page: the caller's 1-based `page`, else the caret's page.
    fn page_target(&self, page: Option<u32>) -> Result<u32> {
        let count = self.page_count()?;
        match page {
            Some(0) => bail!("쪽 번호는 1부터입니다."),
            Some(p) if p > count => bail!("{p}쪽은 범위(1–{count})를 벗어났습니다."),
            Some(p) => Ok(p - 1),
            None => {
                let caret = self.caret()?;
                let page = caret["pageIndex"].as_u64().unwrap_or(0);
                Ok(u32::try_from(page)
                    .unwrap_or(0)
                    .min(count.saturating_sub(1)))
            }
        }
    }

    /// (section, first paragraph) of a 0-based page from RHWP's pagination.
    fn page_start(&self, page: u32) -> Result<(usize, usize)> {
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        let v: Value = serde_json::from_str(
            &core
                .get_position_of_page_native(page as usize)
                .map_err(err)?,
        )?;
        Ok((usize_field(&v, "sec")?, usize_field(&v, "para")?))
    }

    /// Paragraphs that start on `page` and end before the next page starts, as
    /// `(section, range)`. The range is empty when one paragraph covers the whole page.
    fn page_paragraphs(&self, page: u32) -> Result<(usize, std::ops::Range<usize>)> {
        let (sec, first) = self.page_start(page)?;
        let total = self
            .core
            .document()
            .sections
            .get(sec)
            .map_or(0, |s| s.paragraphs.len());
        let end = if page + 1 < self.page_count()? {
            match self.page_start(page + 1)? {
                (s, p) if s == sec => p.min(total),
                _ => total,
            }
        } else {
            total
        };
        Ok((sec, first..end.max(first)))
    }

    /// Body text of a 0-based page as `(start, next)`: its first position and the first
    /// position of the next page in the same section (`None` when the page ends the
    /// section). A paragraph that runs over a page break is cut there.
    fn page_span(&self, page: u32) -> Result<(Cursor, Option<Cursor>)> {
        let start = self
            .page_first_position(page as usize)
            .ok_or_else(|| anyhow!("{}쪽의 시작 위치를 찾지 못했습니다.", page + 1))?;
        let next = if page + 1 < self.page_count()? {
            self.page_first_position(page as usize + 1)
                .filter(|n| n.section == start.section)
        } else {
            None
        };
        Ok((start, next))
    }

    fn text_len(&self, section: usize, paragraph: usize) -> Result<usize> {
        Ok(self
            .paragraph(Cursor {
                section,
                paragraph,
                offset: 0,
                cell: None,
            })?
            .text
            .chars()
            .count())
    }

    /// Hangul CopyPage: the page's text goes to the 쪽 클립보드. Whole paragraphs are copied
    /// as they are; a paragraph running over a page break contributes the part on this page.
    fn copy_page(&mut self, page: Option<u32>) -> Result<Value> {
        let page = self.page_target(page)?;
        let (start, next) = self.page_span(page)?;
        if start.offset > 0 || next.is_some_and(|n| n.offset > 0) {
            let end = match next {
                Some(n) => n,
                None => {
                    let last = self.core.document().sections[start.section]
                        .paragraphs
                        .len()
                        .saturating_sub(1);
                    Cursor {
                        section: start.section,
                        paragraph: last,
                        offset: self.text_len(start.section, last)?,
                        cell: None,
                    }
                }
            };
            let (block, text) = self.range_session(start, end)?;
            self.page_clipboard = Some(std::rc::Rc::new(block.core.document().clone()));
            return Ok(json!({"page": page + 1, "paragraphs": text.len()}));
        }
        let (sec, range) = self.page_paragraphs(page)?;
        let range = if range.is_empty() {
            range.start..range.start + 1
        } else {
            range
        };
        let foreign = excerpt(self.core.document(), sec, range.clone())?;
        self.page_clipboard = Some(std::rc::Rc::new(foreign));
        Ok(json!({"page": page + 1, "paragraphs": range.len()}))
    }

    /// Hangul PastePage: the copied page becomes a new page right after the current one and
    /// the caret lands at the end of the pasted content (observed behaviour).
    fn paste_page(&mut self, page: Option<u32>) -> Result<Value> {
        let foreign = self
            .page_clipboard
            .as_deref()
            .cloned()
            .ok_or_else(|| anyhow!("복사한 쪽이 없습니다. 먼저 쪽 복사하기를 실행하세요."))?;
        let count: usize = foreign.sections.iter().map(|s| s.paragraphs.len()).sum();
        let page = self.page_target(page)?;
        let (sec, range) = self.page_paragraphs(page)?;
        let last = range.end.max(range.start + 1) - 1;
        let len = self.text_len(sec, last)?;
        let mut end = Cursor::default();
        self.transaction(|s| {
            let split = s.split_offset(Cursor {
                section: sec,
                paragraph: last,
                offset: len,
                cell: None,
            })?;
            s.core
                .insert_page_break_native(sec, last, split)
                .map_err(err)?;
            let r: Value = serde_json::from_str(
                &s.core
                    .paste_foreign_document_native(sec, last + 1, 0, foreign)
                    .map_err(err)?,
            )?;
            if r["ok"] != true {
                bail!("쪽을 붙이지 못했습니다: {}", r["error"]);
            }
            let paragraph = usize_field(&r, "paraIdx")?;
            let offset = usize_field(&r, "charOffset")?.min(s.text_len(sec, paragraph)?);
            end = Cursor {
                section: sec,
                paragraph,
                offset,
                cell: None,
            };
            s.cursor = end;
            s.anchor = None;
            s.span_anchor = None;
            Ok(())
        })?;
        Ok(json!({
            "afterPage": page + 1,
            "paragraphs": count,
            "pages": self.page_count()?,
            "cursor": end,
        }))
    }

    /// Hangul DeleteCurrentPage: remove the text of the current page so the following
    /// pages move up. A page made of whole paragraphs loses those paragraphs; the
    /// section's first paragraph is emptied instead of removed because it carries the
    /// section definition and RHWP keeps at least one paragraph per section. A page that
    /// starts or ends inside a paragraph loses the text from its first position up to the
    /// next page's first position.
    fn delete_page(&mut self, page: Option<u32>) -> Result<Value> {
        let page = self.page_target(page)?;
        if self.page_count()? <= 1 {
            bail!("쪽이 하나뿐인 문서에서는 쪽을 지울 수 없습니다.");
        }
        let (start, next) = self.page_span(page)?;
        if start.offset > 0 || next.is_some_and(|n| n.offset > 0) {
            let sec = start.section;
            let end = match next {
                Some(n) => n,
                None => {
                    let last = self.core.document().sections[sec]
                        .paragraphs
                        .len()
                        .saturating_sub(1);
                    Cursor {
                        section: sec,
                        paragraph: last,
                        offset: self.text_len(sec, last)?,
                        cell: None,
                    }
                }
            };
            let removed = end.paragraph - start.paragraph + 1;
            self.transaction(|s| {
                s.core
                    .delete_range_native(
                        sec,
                        start.paragraph,
                        start.offset,
                        end.paragraph,
                        end.offset,
                        None,
                    )
                    .map_err(err)?;
                s.cursor = start;
                s.anchor = None;
                s.span_anchor = None;
                Ok(())
            })?;
            return Ok(
                json!({"page": page + 1, "paragraphsRemoved": removed, "pages": self.page_count()?}),
            );
        }
        let (sec, range) = self.page_paragraphs(page)?;
        if range.is_empty() {
            bail!(
                "한 문단이 여러 쪽에 걸쳐 있어 이 쪽만 지울 수 없습니다. 문단 안에서 범위를 선택해 지우세요."
            );
        }
        let removed = range.len();
        self.transaction(|s| {
            let keep_first = range.start == 0;
            let from = if keep_first { 1 } else { range.start };
            for i in (from..range.end).rev() {
                if s.core.document().sections[sec].paragraphs.len() > 1 {
                    s.core.delete_paragraph_native(sec, i).map_err(err)?;
                }
            }
            if keep_first {
                let remaining = s.core.document().sections[sec].paragraphs.len();
                let len = s.text_len(sec, 0)?;
                if remaining > 1 {
                    // Merge the next page's first paragraph into the emptied first one.
                    s.core
                        .delete_range_native(sec, 0, 0, 1, 0, None)
                        .map_err(err)?;
                } else if len > 0 {
                    s.core
                        .delete_range_native(sec, 0, 0, 0, len, None)
                        .map_err(err)?;
                }
            }
            let remaining = s.core.document().sections[sec].paragraphs.len();
            s.cursor = Cursor {
                section: sec,
                paragraph: range.start.min(remaining.saturating_sub(1)),
                offset: 0,
                cell: None,
            };
            s.anchor = None;
            s.span_anchor = None;
            Ok(())
        })?;
        Ok(json!({"page": page + 1, "paragraphsRemoved": removed, "pages": self.page_count()?}))
    }

    /// Hangul 문서 끼워 넣기 (FileInsertionDlg): the other document's body goes in at the caret
    /// with its fonts, styles and pictures merged into this document's tables.
    fn insert_file(&mut self, p: &InsertFile) -> Result<Value> {
        self.require_body()?;
        let core = match p.password.as_deref() {
            Some(password) => hwpcraft_formats::open_with_password(&p.path, password),
            None => hwpcraft_formats::open(&p.path),
        }?;
        let foreign = core.document().clone();
        let count: usize = foreign.sections.iter().map(|s| s.paragraphs.len()).sum();
        let c = self.cursor;
        let mut end = c;
        self.transaction(|s| {
            let split = s.split_offset(c)?;
            let r: Value = serde_json::from_str(
                &s.core
                    .paste_foreign_document_native(c.section, c.paragraph, split, foreign)
                    .map_err(err)?,
            )?;
            if r["ok"] != true {
                bail!("문서를 끼워 넣지 못했습니다: {}", r["error"]);
            }
            let paragraph = usize_field(&r, "paraIdx")?;
            let offset = usize_field(&r, "charOffset")?.min(s.text_len(c.section, paragraph)?);
            end = Cursor {
                section: c.section,
                paragraph,
                offset,
                cell: None,
            };
            s.cursor = end;
            s.anchor = None;
            s.span_anchor = None;
            Ok(())
        })?;
        Ok(json!({
            "path": p.path,
            "paragraphs": count,
            "pages": self.page_count()?,
            "cursor": end,
        }))
    }

    /// Hangul SaveBlock: only the selected body text (one or more paragraphs) goes to a new
    /// file. The open document, cursor and history are untouched.
    fn save_block(&mut self, path: &Path) -> Result<Value> {
        if !matches!(
            path.extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("hwp" | "hwpx" | "pdf" | "txt")
        ) {
            bail!("블록 저장 형식: HWP, HWPX, PDF, TXT");
        }
        self.refuse_tracked_save(path)?;
        let (mut block, text) = self.block_session()?;
        block.save(path)?;
        Ok(json!({"path": path, "paragraphs": text.len(), "text": text.join("\n")}))
    }

    /// PDF of the selected block, prepared here and converted by the caller (the desktop
    /// runs `convert` on a worker thread so a long block does not freeze the window).
    pub fn save_block_pdf_job(&self) -> Result<crate::rendering::PdfJob> {
        let (block, _) = self.block_session()?;
        let pages = block.page_count()?;
        block.pdf_job(&(0..pages).collect::<Vec<_>>())
    }

    /// PDF of a Hangul 인쇄 범위 (`print_pages` names), prepared for a background
    /// conversion; also returns the 0-based pages it covers.
    pub fn print_pdf_job(
        &self,
        range: &str,
        current: u32,
    ) -> Result<(Vec<u32>, crate::rendering::PdfJob)> {
        let pages = print_pages(range, current, self.page_count()?)?;
        let job = self.pdf_job(&pages)?;
        Ok((pages, job))
    }

    /// 0-based pages of a Hangul 인쇄 범위 (`print_pages` names) in this document, for a
    /// PDF prepared a few pages per frame (`Session::pdf_prepare`).
    pub fn print_pdf_pages(&self, range: &str, current: u32) -> Result<Vec<u32>> {
        print_pages(range, current, self.page_count()?)
    }

    /// The selected block as its own session, for a PDF prepared a few pages per frame.
    /// It is a copy, so later edits of this document do not change it.
    pub fn block_pdf_session(&self) -> Result<Session> {
        Ok(self.block_session()?.0)
    }

    /// Whether the document carries Hangul change-tracking records (HWP 5 `DocInfo`
    /// TRACK_CHANGE / TRACK_CHANGE_AUTHOR). RHWP keeps them as raw records and writes
    /// them back but does not show the changes. RHWP does not read HWPX change lists.
    pub fn has_track_change_records(&self) -> bool {
        // HWPTAG_BEGIN (16) + 80 and + 81.
        self.core
            .document()
            .doc_info
            .extra_records
            .iter()
            .any(|r| matches!(r.tag_id, 96 | 97))
    }

    /// The selected body text as a separate document session, plus its paragraph texts.
    fn block_session(&self) -> Result<(Session, Vec<String>)> {
        if !self.has_selection() {
            bail!("저장할 블록을 먼저 선택하세요.");
        }
        if self.cursor.cell.is_some() {
            bail!("표 밖 본문의 블록만 저장할 수 있습니다.");
        }
        let (start, end) = match self.span() {
            Some(span) => span,
            None => {
                let r = self.selection();
                (
                    Cursor {
                        offset: r.start,
                        ..self.cursor
                    },
                    Cursor {
                        offset: r.end,
                        ..self.cursor
                    },
                )
            }
        };
        self.range_session(start, end)
    }

    /// Body text from `start` to `end` (one section) as a separate document session, plus
    /// its paragraph texts.
    fn range_session(&self, start: Cursor, end: Cursor) -> Result<(Session, Vec<String>)> {
        let foreign = excerpt(
            self.core.document(),
            start.section,
            start.paragraph..end.paragraph + 1,
        )?;
        let span = end.paragraph - start.paragraph;
        let mut out = blank()?;
        let r: Value = serde_json::from_str(
            &out.paste_foreign_document_native(0, 0, 0, foreign)
                .map_err(err)?,
        )?;
        if r["ok"] != true {
            bail!("블록을 옮기지 못했습니다: {}", r["error"]);
        }
        let last = usize_field(&r, "paraIdx")?;
        let first = last.saturating_sub(span);
        let chars = |doc: &DocumentCore, i: usize| {
            doc.document().sections[0]
                .paragraphs
                .get(i)
                .map_or(0, |p| p.text.chars().count())
        };
        // Trim the tail, then the head, then drop whatever the blank document left behind.
        let tail = chars(&out, last);
        if end.offset < tail {
            out.delete_range_native(0, last, end.offset, last, tail, None)
                .map_err(err)?;
        }
        if start.offset > 0 {
            out.delete_range_native(0, first, 0, first, start.offset, None)
                .map_err(err)?;
        }
        while out.document().sections[0].paragraphs.len() > last + 1 {
            out.delete_paragraph_native(0, last + 1).map_err(err)?;
        }
        for _ in 0..first {
            // Leading blank paragraphs: fold them into the first kept one.
            out.delete_range_native(0, 0, 0, 1, 0, None).map_err(err)?;
        }
        // Native trimming can discard a closing mark at the cut boundary.
        // Rebuild clipped markpen intervals from the untouched source so a
        // highlighted selection keeps its formatting in every block format.
        for (index, source_index) in (start.paragraph..=end.paragraph).enumerate() {
            let source = &self.core.document().sections[start.section].paragraphs[source_index];
            let from = if source_index == start.paragraph {
                start.offset
            } else {
                0
            };
            let to = if source_index == end.paragraph {
                end.offset
            } else {
                source.text.chars().count()
            };
            let marks = crate::markpen_ranges(source)
                .into_iter()
                .flat_map(|(range, color)| {
                    let a = range.start.max(from);
                    let b = range.end.min(to);
                    (a < b)
                        .then(|| {
                            [
                                hwpcraft_doc::model::paragraph::MarkpenMark {
                                    char_idx: a - from,
                                    color: Some(color),
                                    utf16_pos: None,
                                },
                                hwpcraft_doc::model::paragraph::MarkpenMark {
                                    char_idx: b - from,
                                    color: None,
                                    utf16_pos: None,
                                },
                            ]
                        })
                        .into_iter()
                        .flatten()
                })
                .collect();
            let section = &mut out.document_mut().sections[0];
            let target = section
                .paragraphs
                .get_mut(index)
                .ok_or_else(|| anyhow!("블록 문단의 형광펜을 복원하지 못했습니다."))?;
            target.markpen_marks = marks;
            crate::highlights::resolve_markpen_positions(target);
            target.range_tags.retain(|tag| tag.tag >> 24 != 2);
            section.raw_stream = None;
        }
        let text: Vec<String> = out.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect();
        out.set_document(out.document().clone());
        Ok((Self::with_core(out, None), text))
    }
}
