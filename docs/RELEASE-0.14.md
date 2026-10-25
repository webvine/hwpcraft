# HwpCraft 0.14 — 아래한글 편집 기능 동등성 (작업 중)

작성: 2026-10-25, core 영역(조정·통합). 이 문서는 통합 검증이 끝난 항목만 적는다. 영역별 상세 보고와 한글 실측 자료는 내부 검증 자료로 보관하며 공개 저장소에는 올리지 않는다.

## 목표

한글 2024의 편집 동작을 실제 프로그램으로 확인하고 HwpCraft가 같은 결과를 내도록 구현한다. RHWP에 API가 없는 기능은 RHWP 문서 모델을 유지한 채 HwpCraft 자체 구현으로 채운다. MCP는 한글 자동화 동작 이름을 그대로 받아 같은 편집을 수행한다.

## core — 캐럿·선택·클립보드·삭제·특수 문자·탐색·MCP

- **캐럿 이동** `caret.move {motion, extend}`: 글자·낱말(처음/끝/다음/이전)·줄 처음/끝·위/아래·쪽 위/아래/처음/끝·문단 처음/끝·앞/다음 문단 처음·리스트(셀 또는 본문) 처음/끝·문서 처음/끝. 한글 `Move*`/`MoveSel*` 동작 70여 개에 대응. 한글 실측: `MoveWordEnd`는 낱말 끝(뒤 공백 앞)으로, Ctrl+↑/↓는 앞/다음 문단 처음으로 간다.
- **선택** `select.all/word/paragraph/line/clear/get`. **클립보드** `edit.copy/cut/paste {text?, html?, mode}`·`edit.clipboard.get`: 헤드리스(CLI·MCP)에서도 RHWP 내부 클립보드로 표·개체·서식이 함께 붙는다.
- **삭제** `text.delete_word`(Ctrl+T: 캐럿부터 다음 낱말 시작까지), `text.delete_word_back`(Ctrl+Backspace), `text.delete_line`(Ctrl+Y: 한 줄 문단은 문단째), `text.delete_to_line_end`(Alt+Y). 모두 한글에서 같은 입력으로 확인한 결과와 일치.
- **특수 문자** `insert.line_break`(Shift+Enter, 문단 안 줄 나눔; HWPX `lineBreak`), `insert.tab`, `insert.nbsp`(U+00A0), `insert.fixed_width_space`(U+2007), `insert.soft_hyphen`(U+00AD). HWP/HWPX 저장 후 재열기 왕복 테스트 통과.
- **상용구** `insert.auto_text {kind}`: 날짜·시각(형식 지정 가능, 요일·오전/오후 한글)·파일 이름·사용자 이름은 글로, 쪽 번호·전체 쪽수·쪽/전체는 본문 자동 번호 컨트롤로 넣는다(RHWP `insert_auto_number_at_cursor`). 한글의 날짜 코드 필드는 RHWP에 없어 글로 넣는다. RHWP가 새 자동 번호의 자리표에 1단위 간격만 기록해 저장·추가 입력 때 번호가 문단 끝으로 밀리던 문제는 HwpCraft가 자리표 간격(8단위)을 복원해 해결했다(한글이 만든 파일과 같은 구조로 저장). 다만 RHWP 렌더러가 본문 쪽 번호를 그리지 않아 화면·PDF는 빈칸이다(README 한계).
- **탐색** `edit.goto {page|section|paragraph|bookmark}`, `hyperlink.jump/next/prev`, `edit.find_all`(RHWP `search_all_text_native`; 본문·셀 위치를 Cursor로 변환), `document.statistics`(글자·공백 제외 글자·낱말·줄·문단·쪽·구역·표·그림·도형·수식·각주·미주·메모), `document.outline`, `edit.overwrite`.
- **데스크톱 단축키**: Shift+Enter, Tab, Ctrl+T/Ctrl+Delete, Ctrl+Backspace, Ctrl+Y(한 줄 지우기; 다시 실행은 Ctrl+Shift+Z), Alt+Y, Ctrl+Alt+Space, Alt+Space, Ctrl+Shift+-, Ctrl+↑/↓.
- **MCP**: `hwp_action {id, params?, query?}` 도구 — 한글 자동화 동작 이름 300여 개를 대응 명령으로 실행(`crates/engine/src/hwp_actions.rs`, 기본 인자 병합). `batch {atomic:true}` — 실패 시 전체 되돌림(`Session::checkpoint/rollback/release`). `get_selection` 도구, `hwpcraft://hwp-actions` 리소스, `list_commands`의 `hwpActions` 역색인. 명령 인자 도움말을 엔진 `PARAMS` 표(`command_help`)로 옮겨 각 영역이 자기 모듈에서 채운다.
- **최종 목표 검증**: MCP만으로 한국어 보고서를 끝까지 만드는 종단 테스트(`korean_report_authored_end_to_end_through_mcp`): 제목 서식(`hwp_action` CharShapeBold·ParagraphShapeAlignCenter)·번호 매기기·표와 캡션·각주·꼬리말 쪽 번호·날짜(atomic batch)·문서 통계·HWPX/PDF 저장·PNG 렌더·재열기 확인.
- 테스트 `crates/engine/tests/caret.rs` 9개(자동 번호 위치 왕복 포함), `crates/mcp/tests/protocol.rs`에 한글 동작 별칭·선택·atomic batch 테스트 추가.
- **이동 대화상자**(Alt+G, 편집 탭 찾기 ▾ 이동): 쪽·구역·문단·책갈피로 이동.
- **맞쪽 보기 배치**: `view.mirror_pages`(shell-page 영역 플래그)가 켜지면 연속 보기가 1쪽은 오른쪽 단독, 이후 짝수·홀수 쪽을 가로로 나란히 놓는다. 쪽 비트맵은 shell-page 영역의 `shell_page_image`(문서 보기 색·그림 자리 표시)를 거친다.
- **명령 도움말 이전**: 핵심 명령 60개의 인자 도움말을 `crates/engine/src/protocol.rs`·`caret.rs`의 `PARAMS`로 옮기고 MCP catalog의 임시 표에서 제거했다. 모든 등록 명령에 예제가 있는지 MCP·엔진 테스트가 검사한다.
- **최적화**: 문단 주소 걷기 `Session::cursors()`/`paragraph_total()`을 추가해 검색·탐색·명령 결과(`info`)가 문단마다 미리 보기 문자열을 만들던 `entries()` 대신 할당 없는 경로를 쓴다(긴 문서의 MCP batch 응답 비용 감소).
- 알려진 차이: `MovePrevParaEnd`는 앞 문단 처음으로, `Select`(F3 블록 모드)는 토글 없이 선택 조회·확장 이동으로 대응. 화면 단위 선택(`MoveSelView*`)·조판 부호 단위 이동은 없다.

## format — 서식

- 신규 명령 20: `format.superscript.cycle`, `format.indent.at_caret`, `format.font.next/prev {fonts?}`, `format.text_color {color|name}`, `format.shade_color {color|name|없음}`, `format.char_border {value?,type?,width?,color?}`, `format.paragraph.up/down`, `format.style.clear_char`, `format.style.update_from_caret {id?}`, `format.style.move {id,direction}`, `format.style.export {path?}`, `format.style.import {path?|styles?}`, 글자 겹치기 `format.char_overlap {chars,border,size,expansion}` / `.get` / `.update` / `.delete`(RHWP `Control::CharOverlap`, 8유닛 슬롯). `format.highlight` 사전 색 이름, `format.style.shortcut` 1–20, 여백·들여쓰기 음수 허용(한글 실측).
- 한글과 다른 점: 오른쪽 여백 늘리기는 한글 자동화가 −1pt를 저장하지만 라벨대로 +1pt; 음영 없음은 흰색; 글꼴 순환은 UI가 설치 글꼴 목록을 넘김. 덧말(렌더 없음)·드롭캡(모델 없음)은 미구현.
- 테스트 ext_format 17·formatting 7·ext_format_overlap_xml 1, PARAMS 69개.

## table-object — 표·개체

- 표: `table.cell_move {motion}`(Tab/Shift+Tab 경로, 마지막 셀 Tab → 줄 추가; 한글 공개 이름 TableRightCell 등 실측), `table.style` 옵션(제목 줄·첫/마지막 칸·마지막 줄·회색조·테두리·배경), `table.cell_shade {darker}`(잉크 ×1.54/×0.54 실측), `table.auto_fill {values}`.
- 개체: `object.align`(11종)·`size`·`rotate`·`protect_size`·`outer_margin`·`style`·`shadow`·`format.get/format`(모양 복사)·`neighbor`·`connector_type`, `textbox.align/margin/direction`(세로 글상자, `shape.insert direction`), `picture.save/save_all/recolor`, `insert.auto_number {page|total|picture|table|equation}`, `note.convert`(각주↔미주), `note.shape.get/shape`, `note.delete`, `field.delete`. UI: Tab 개체 순환, 개체 선택 단추, 표 마당 옵션 창, 사용자 자동 채우기 창, 각주/미주 모양 창.
- RHWP API 부재: 그림 그림자·네온·옅은 테두리·반사, 자유선·그리기 마당, 표 마당 글자 모양, 메모 삭제. 한글 자동화가 거부하는 리본 전용 식별자(TableCellBlockStrict 등)는 내부 대응표에 기록.
- 테스트 ext_table 16·ext_object 19, ui-egui 26.

## shell-page — 셸·쪽·보기·파일

- 명령: `page.margins.presets` / `page.margins.preset {preset}`(한글 쪽 여백 갤러리 6종 실측값), `page.section.next/prev`, `page.copy/paste/delete {page?}`(쪽 클립보드, 한글 CopyPage/PastePage 실측 동작), `file.insert {path, password?}`, `file.save_block {path}`, `file.doc_info` 확장(형식·날짜·그림 데이터·글꼴 대체). 화면 전용: 문서 보기 색 5종·그림 자리 표시(`shell_page_image`가 쪽 비트맵을 후처리), 맞쪽 보기(`view.mirror_pages`, 배치는 core가 `page_view`에 연결).
- 데스크톱: 편집·쪽 탭 쪽 여백 메뉴, 보기 탭 문서 보기 색·그림 체크, 쪽 오른쪽 클릭 메뉴 쪽 복사/붙이기/지우기, 파일 메뉴 끼워 넣기·블록 저장·그림 파일로 저장(PNG, 96·144·192 dpi), 머리말/꼬리말 편집 창 이전·다음, 문서 정보 일반·글꼴·그림 탭, 편집 탭 찾기 메뉴(찾기·찾아 바꾸기·이동), 02·03·05 요청 리본 항목 배치.
- 한계: 쪽 복사/지우기는 문단 단위, Margin 동작 id는 자동화 실행 불가(갤러리 순서 대응), PDF 인쇄 옵션·쪽 윤곽·격자·줄 번호·바탕쪽·문서 이력은 RHWP 옵션·모델 없음.
- 테스트 ext_page 16, PARAMS 34.

## review-tools — 검토·도구·찾기

- 한자 변환 `tools.hanja.candidates/convert/to_hangul`(F9; Unicode Unihan `kHangul` 독음표 8,525자 → `assets/data/hanja-readings.txt`, Unicode License v3, THIRD-PARTY-NOTICES 7번), 입력 형식 4종, 한자→한글 두음 법칙.
- 찾기 확장: `FindOptions`에 `ignore_spaces`, `wildcards`, `regex`, `any_word`, `hanja_by_hangul`, 글자 모양 조건(bold/italic/underline/size_pt) — 기존 호출 호환. 문서 비교 `review.compare {path}`(RHWP docdiff, 추가·삭제·변경 문단 목록 창). 상용구 `tools.autotext.list/add/delete/insert/expand`(Ctrl+F3·Alt+I, 설정 폴더 `autotext.json`, `HWPCRAFT_CONFIG_DIR`).
- 조사 결과(RHWP 한계): 변경 내용 추적(본문 표식 미파싱), 메모 생성(API 없음), 맞춤법(사전 없음), 구결 변환.
- 테스트 find 4·hanja 4·review 3·autotext 4.

## 통합 검증 (01)

`scripts/integrate-0.14.ps1`로 편집 중지 상태의 공유 트리를 검증했다(2026-10-25).

- fmt 검사(8개 패키지)·clippy(-D warnings, 전체 워크스페이스·all-targets) 통과, 전체 테스트 **215개 통과·무시 2개**(0.13: 161개). 새 테스트 파일: caret, autotext, hanja, review, ext_format_overlap_xml.
- 릴리스 빌드 통과. `runtime/hwpcraft.exe`·`hwpcraft-cli.exe` 0.14로 설치(`--version` = HwpCraft 0.14 / RHWP 0.8.7). 런처 설치 폴더의 `hwpcraft.exe`는 실행 중인 HwpCraft 창이 파일을 잡고 있어 교체하지 못했다 — 창을 닫은 뒤 `HwpCraft.ps1 -Action Install`을 다시 실행하면 된다(CLI·문서·라이선스 파일은 교체됨).
- MCP 명령 안내 `docs/mcp-commands.md` 재생성: **285개** 명령(0.13: 190개), 모두 엔진 `PARAMS` 도움말. 한글 동작 대응표 `hwp_actions.rs` 582개.
- 추적 파일 검사: REA·프로브·팀 문서·분석 스크립트는 git에 없음(`tab_probe.rs`는 RHWP 탭 폭 진단 테스트). 라이선스: 새 의존성 regex·chrono 계열(MIT/Apache-2.0), Unihan 독음표(Unicode License v3, THIRD-PARTY-NOTICES 7번), 한컴 코드·아이콘·글꼴 없음. `THIRD-PARTY-DEPENDENCIES.md`는 `scripts/third-party-deps.py`로 431개 크레이트 재생성.
- 형제 저장소 동기화(`scripts/sync-siblings.ps1`): RHWP 0.8.7 최신, Craft 계열 23개 저장소 fast-forward(craft-launcher는 로컬 변경으로 건너뜀).

## 형제 소스 최신화

`scripts/sync-siblings.ps1`이 `../rhwp`와 ArtCraft 계열 저장소를 fast-forward로 갱신한다(로컬 변경이 있는 저장소는 건너뛰고 보고). 2026-10-25 기준 RHWP 0.8.7이 최신이다.
