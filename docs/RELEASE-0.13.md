# HwpCraft 0.13 — RHWP 미활용 API 반영과 다중 OS 지원

작성: 2026-10-25, core 영역(조정·통합). 이 문서는 통합 검증이 끝난 항목만 적는다. 영역별 상세 보고는 내부 검증 자료로 보관한다.

## 배경

0.12까지 HwpCraft는 RHWP `DocumentCore` 메서드 약 110종을 썼다. RHWP에는 공개 명령·조회 메서드 271종이 있고, README의 "구현하지 않음"·"RHWP 한계" 항목 중 여럿은 실제로 RHWP API가 있었다. 0.13은 그 간극을 닫고, 사용자 지시에 따라 Windows 외 macOS·Linux에서도 빌드·실행되도록 플랫폼 의존 코드를 정리한 판이다.

## core — 파일·핵심 편집·플랫폼

### 암호 문서

- 열기: 암호가 걸린 HWP(EncryptVersion 4)·HWP3·HWPX(ODF AES-256-CBC)를 열면 암호 입력 창이 뜬다(`Session::open_with_password`, `hwpcraft_formats::needs_password`/`wrong_password`). 틀린 암호는 창 안에서 다시 묻고, 취소하면 기존 문서는 그대로다.
- 저장: 암호 문서를 열었거나 `Session::set_password(Some(..))`로 암호를 두면 다음 HWP/HWPX 저장에 적용된다(`hwpcraft_formats::save_with_password`). PDF/TXT에는 적용하지 않는다. 암호를 바꾸면 문서가 수정 상태가 된다.
- CLI: `hwpcraft-cli --password <암호> <명령 …>`로 암호 문서를 열고, `new`/`edit`/`convert` 결과를 암호로 저장한다.
- 엔진 명령 `file.password {"password": "..."|null}` (CLI·MCP용). 보안 탭의 "문서 암호 설정" 대화상자는 04 영역.
- 테스트: `crates/engine/tests/security.rs`(HWP/HWPX 왕복, 틀린 암호, 잘못된 입력, 비HWP 대상 거부), `apps/hwpcraft-cli/tests/password.rs`.

### 클립보드 HTML

- 복사·잘라내기 때 본문 선택 영역을 HTML로도 클립보드에 올린다(`export_selection_html_native` + arboard). 다른 프로그램에 붙이면 굵게·색 등 서식이 유지된다. 표 셀 안 선택은 글만 올린다.
- 붙이기 때 클립보드에 HTML이 있고 그것이 HwpCraft 자신의 복사가 아니면 서식을 살려 붙인다(`paste_html_native`). HTML 해석에 실패하면 글로 붙인다. 자신의 복사는 종전처럼 RHWP 내부 클립보드(표·개체 포함)를 쓴다. 4 MB 초과 HTML은 글로만 붙인다.
- 엔진 명령 `edit.paste_html {"html": "..."}`.

### 하이퍼링크 고치기·지우기

- 엔진 명령 `hyperlink.list`, `hyperlink.update {field_id, uri?, text?}`, `hyperlink.remove {field_id, restore_format?}`(`crates/engine/src/links.rs`). 주소와 표시 글을 한 번의 실행 취소 단계로 바꾸고, 지우기는 글을 남긴다.
- 테스트 `crates/engine/tests/links.rs`: 목록·수정·삭제·undo·HWP/HWPX 왕복, 잘못된 주소·여러 줄 글·없는 링크의 원자성.

### 형광펜 화면 표시

- 02가 형광펜을 RHWP `markpen_marks`로 실제 저장하도록 바꿨다. RHWP 렌더러는 형광펜을 그리지 않으므로 HwpCraft가 글자 뒤에 반투명 색 띠를 덧그린다(`Session::highlight_rects`, `crates/engine/src/highlights.rs`). 줄이 바뀌면 줄마다 하나씩 그린다. 문서 세대가 바뀔 때만 다시 계산한다.
- 테스트 `crates/engine/tests/highlights.rs`.

### 다중 OS

- 글꼴: "맑은 고딕"/"Malgun Gothic" 하드코딩을 OS 후보 목록으로 바꿨다. `hwpcraft_doc::fonts::KOREAN_CANDIDATES`(Windows: 맑은 고딕·굴림, macOS: Apple SD Gothic Neo·AppleGothic, Linux: Noto Sans CJK KR·NanumGothic 등), `hwpcraft_render::default_korean_font()`(설치된 첫 후보), `korean_font_face()`(egui UI 글꼴 파일·컬렉션 색인), `hwpcraft_formats::pdf_options()`(PDF 대체 글꼴).
- 빌드 스크립트: `hwpcraft.sh build|test|install|start|status`(macOS·Linux)를 `HwpCraft.ps1`과 같은 단계로 추가했다. 데스크톱 실행 파일은 eframe/Glow로 X11·Wayland·macOS에서 동작하는 구성이다.
- 검증: Windows x64 전체 테스트·clippy 통과. `cargo check --locked --workspace --all-targets --target x86_64-unknown-linux-gnu` 통과(`logs/xplat-check-workspace-linux.log`). `aarch64-apple-darwin`은 이 Windows 호스트에 Apple 교차 C 컴파일러가 없어 RHWP 의존성 `blake3`의 build script에서 멈췄다(`logs/xplat-check-aarch64-apple-darwin.log`). **macOS 빌드·실행은 실제 Mac에서 확인해야 한다.** Linux도 컴파일만 확인했고 실행은 하지 않았다.

### 문자표 (사용자 추가 지시 2026-10-25)

- 기존 4개 탭·약 70자였던 문자표를 한글 2024의 네 가지 표 구성으로 다시 만들고 범위를 넓혔다. `crates/engine/src/symbols.rs`(카탈로그·검색·`insert.symbol`)와 `crates/ui-egui/src/symbol_dialog.rs`(대화상자).
- 한글(HNC) 문자표 30개 영역(일반·수학·단위·원 문자·괄호 문자·괘선·도형·화살표·기타·딩뱃·그리스·키릴·히라가나·가타카나·로마 숫자/분수·첨자·영문 서체·한글 자모·옛한글 자모·IPA·전각·반각 가나·한중일 구두점·호환 단위/월·한자 부수·주역 괘·점자·음악·통화·기술), 완성형(KS X 1001) 3개(특수 문자 988·한글 2,350·한자 4,888, `encoding_rs` EUC-KR 표에서 생성), 유니코드 15 블록 162개, 이모지 10개 영역, 이름 있는 특수 문자 47개. 한글보다 넓은 범위: 이모지·검색·즐겨찾기·여러 문자 입력·코드 표시(유니코드·완성형)·글꼴 지정 삽입.
- 검색: `U+2605`/`2605`/`#9733` 코드, 입력한 기호 자체, 특수 문자 이름(예: 줄표), 영역 키워드(한글·영문; 예: 화살표, emoji flag). WordCraft의 Symbol 대화상자(글꼴별 문자·Unicode subset·최근 사용·Special Characters)를 참고했다.
- 글꼴: 설치된 기호·이모지 글꼴(Segoe UI Symbol/Emoji, Cambria Math, Apple Symbols, Noto Sans Symbols/Math, Symbola, DejaVu Sans)을 egui 대체 글꼴로 등록해 표시 범위를 넓혔다(`hwpcraft_render::symbol_font_faces`). "글꼴에 있는 문자만" 필터는 egui 글리프 존재 여부로 거른다.
- 엔진 명령: `insert.symbol {text, font?}`(한 undo 단계, 선택한 글꼴을 넣은 문자에만 적용), `symbol.sets`, `symbol.list {set, start, limit}`, `symbol.search {query}`, `symbol.special`. 최근 사용·등록 문자는 설정 폴더 `symbols.json`.
- 검증: `crates/engine/tests/symbols.rs` 5개(영역 중복·제어 문자 없음, KS 2,350/4,888자, 검색, 글꼴 지정 삽입 undo·HWP/HWPX 왕복, 프로토콜), ui-egui 단위 1개.
- 02가 Windows 설치 파일(Inno Setup, `scripts/build-installer.ps1`, `packaging/windows/HwpCraft.iss`)을 추가했다.

### 확인한 RHWP 제약 (이번에 반영하지 않음)

- 줄 단위 커서 이동(`move_vertical_native`)은 `pub(crate)`라 HwpCraft가 쓸 수 없다. ↑/↓는 종전의 조판 좌표 hit-test 방식을 유지한다.
- 선택 영역 사각형 조회(`get_selection_rects_native`)도 비공개다. 선택·형광펜 표시는 글자별 캐럿 좌표로 계산한다.

## format — 서식

format 영역 보고 요약.

- **형광펜 실제 저장**: `format.highlight {color?: "#rrggbb"}`가 RHWP `Paragraph::markpen_marks`를 기록한다(색 없음 = 지우기, 선택 범위별 trim·병합). HWP(PARA_RANGE_TAG 종류 2)·HWPX(markpenBegin/End) 저장 후 재열기 왕복 일치. 조회 `format.highlight.get`. 음영 색은 더 이상 건드리지 않는다. 화면 표시는 01의 오버레이가 맡는다.
- **머리말/꼬리말·각주 안 서식**: `format.hf.character`/`.get`, `format.hf.paragraph`/`.get` {header, applyTo?, paragraph?, start?, end?, format}, `format.footnote.paragraph`/`.get` {paragraph?, control, footnoteParagraph?, format}. 커서 모델에 머리말·각주 위치가 없어 명령 인자로 대상을 지정한다. 각주 글자 모양은 RHWP 네이티브가 없어 문단 모양만.
- **모양 복사**: `format.copy_shape.apply {character?, paragraph?}` 한 transaction(실행 취소 1단계). Alt+C 연결.
- **글꼴 하드코딩 제거**: `format_dialogs.rs`의 대체 글꼴 목록이 `hwpcraft_render::default_korean_font()`를 맨 앞에 둔다.
- **RHWP 사실 확인**: 스타일 추가/고치기/지우기(`create_style` 등)는 wasm 래퍼 전용 인라인 구현이라 네이티브 호출 불가 → 모델 직접 수정 유지. `set_numbering_restart_native`는 메모리 전용(저장·조회 미반영) → "번호 정의를 새로 만들어 저장" 방식 유지.
- 테스트: ext_format 13(9→13), formatting 7, ui-egui 22 통과. clippy·fmt 통과.. `s02-format-style-add.png`.

## table-object — 표·개체

영역 보고 요약.

- `object.group {targets}` / `object.ungroup {address}` (group_shapes_native/ungroup_shape_native), UI Shift+클릭 다중 선택 → 묶기.
- `object.z_order {address, mode front|back|forward|backward}` (set_control_z_order_at). 이미 그 자리면 undo 없이 moved:false.
- `object.lock {address, locked?}` 토글, `object.unlock_all` (set_control_lock). 잠긴 개체는 삭제·속성·순서·대칭·묶기·끝점 편집을 모두 거부(문서·undo 불변 테스트), UI는 "개체 보호" 표식과 끌기·Delete·방향키 거부. 저장 후 잠금 유지. README의 "개체 보호는 RHWP API 없음"은 삭제.
- `object.flip {address, axis}` (set_control_flip_at, 도형). 그림은 기존 horzFlip 속성.
- 연결선 6종 `shape.insert kind connector_*` + start/end {address, point 0~3}. 두 도형에 붙이면 update_connector_subject_ids·recalculate_connector_routing, 이후 도형 이동·크기 변경 시 update_connectors_in_section으로 재계산. 양쪽 화살표는 RHWP 생성 API에 없어 one-way 생성 후 link_type·arrowStart 보정.
- `object.line_endpoint {address, start_mm, end_mm}` (move_line_endpoint_native), UI 직선 양 끝점 손잡이.
- `insert.field {guide, name, memo, editable}`, `field.list`, `field.set` (insert_click_here_field_at*, set_field_value_*). 양식 개체는 생성 API가 없어 `form.list`/`form.get`/`form.set`(값·글·캡션 편집)만.
- 메모 새로 넣기: DocInfo MemoShape 파싱·기록 미지원, HWP5 메모 본문 미파싱 → 한계 유지.
- 테스트: ext_object 15(신규 5), 전체 142 통과. clippy 통과. 예제 `s03-object13-sample.hwpx/.hwp`. 새 `--view`: object-group, line-handles, connector-draw, field-insert, form-edit..
- 알려진 제한: 연결선 부착 좌표는 종이 기준 개체에 정확하고 문단 기준 개체는 어긋날 수 있음. 직선 끝점 편집은 글자처럼 취급·연결선 제외. 개체 선택 단추(object.select)는 미구현.

## shell-page — 셸·쪽·보기·서비스

영역 보고 요약.

- **보기 체크 상자**: 문단 부호·조판 부호·투명 선이 실제로 켜진다(`view.marks`/`.get`, HwpDocument 래퍼 swap). 저장 문서·undo에 들어가지 않는다.
- **보안 탭**: 문서 암호 설정·변경·해제(`security.password`, `security.password_change` → `Session::set_password`), 개인 정보 찾아서 보호(`security.privacy.scan`/`.mask`: PII 종류 4개·보호 문자·숨은 글자, 결과 표에서 선택·이동·가리기, undo 1단계).
- **머리말/꼬리말 필드**: 쪽 번호·전체 쪽수·파일 이름(`page.header_footer.text`/`.get`, `insert_field_in_hf_native`). 날짜 필드는 RHWP에 없다.
- **파일 메뉴**: 미리 보기(`file.preview`), HML로 저장(`file.save_hml`, HML 원본만), 현재 쪽/쪽 범위만 저장(`file.save_pages`), 최근 문서 9개(`file.recent`, 설정 파일은 APPDATA → XDG_CONFIG_HOME → HOME/.config).
- **구역 설정**(`page.section`/`.get`): 번호 시작, 기본 탭 간격, 감추기 6종, 현재 구역/문서 전체. 줄 번호는 API가 없어 비활성.
- **Alt 키팁**: Alt → 탭 글자(F E U D J W R H K) → 리본 항목 Alt 배지 → 실행, Esc 뒤로. ALT_KEYS 139개.
- **OS 분기**: 인쇄 Windows print 동사 / macOS `open` / Linux `xdg-open`, PDF 글꼴 fallback `pdf_options()`.
- 03 요청 리본 항목 14건 배치. 테스트: 엔진 117 통과(ext_page 11), ui-egui 25 통과, clippy 통과..
- 한계: 머리말 날짜 필드, 줄 번호·구역 시작 위치, 쪽 윤곽·격자·안내선·문서 보기 색·화면 나누기, 배포용 문서·선택 글자 보호·보안 설정, 보안 종류 선택, 쪽 번호 홀/짝 분리는 RHWP API/렌더 모드가 없어 미구현. 02의 머리말 서식 명령은 머리말 편집 창에 미연결.

## 통합 검증 (01, 2026-10-25)

`scripts/integrate-0.13.ps1`로 편집 중지 상태의 공유 트리를 검증했다. 상세는 내부 검증 기록.

- clippy(-D warnings, 전체 워크스페이스) 통과, 전체 테스트 **161개 통과·무시 2개**(0.12: 133개), fmt 검사 통과.
- 릴리스 빌드·설치: `runtime/hwpcraft.exe` SHA-256 `D86CCD2E…A81F`, `hwpcraft-cli.exe` `489C122E…8DD9`, `--version` = HwpCraft 0.13 / RHWP 0.8.7. 설치본으로 형광펜 예제 화면과 암호 문서 CLI 읽기 확인.
- MCP 명령 안내(`crates/mcp/src/catalog.rs`)에 0.13 신규 명령 35개의 인자 예제를 추가하고 `docs/mcp-commands.md`(190개)를 재생성했다.
- 명령 수 144 → 190..
- Linux x86_64 전체 워크스페이스 `cargo check` 통과. macOS는 호스트 제약으로 미검증(README에 명시).
- 통합 중 네 세션 밖에서 `assets/LICENSE-*.txt`가 `assets/licenses/`로 옮겨져, `HwpCraft.ps1`·`hwpcraft.sh`가 두 위치를 모두 받도록 고쳤다.
