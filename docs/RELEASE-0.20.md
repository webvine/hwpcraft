# HwpCraft 0.20 — 변경 추적·세로쓰기·화면 정리·MCP 판 호환·편집 속도

0.20 릴리스. 아래한글 전체 기능의 100% 호환을 뜻하지 않는다. 한글 2024 공개 도움말의 명령 325행과 1:1로 대조한 기능 완성도는 **64%**다(0.19: 60%; 동작 162·부분 62·없음 76·범위 밖 25, 동작 2점·부분 1점, 범위 밖 제외). 영역별로 서식 92%, 표 91%, 보안 83%, 그림 그리기 82%, 쪽 77%(0.19: 65%), 편집 61%, 보기 60%, 입력 55%, 추가 기능 50%, 도구 45%, 파일 38%, 검토 38%다. HwpCraft에서 실제로 실행·저장되는지로 판정했고 한글 실측 비교는 아니다. 판정 근거와 남은 항목은 아래 "알려진 한계"와 "다음 버전에서 정할 것"에 적었다.

## 기능

- 변경 추적: 한글 변경 추적 문서(HWP·HWPX)를 변경 표시본으로 연다. 삽입은 파란 밑줄, 삭제는 빨간 취소선, 서식 변경은 초록 밑줄로 화면·PNG·PDF에 그린다(본문과 표 셀). 변경 목록(종류·글·작성자·날짜)을 보고 캐럿의 변경이나 모든 변경을 적용·취소하며(한 단계 실행 취소), 이전·다음 변경으로 간다. 모두 적용한 결과는 한글이 저장한 결과본과 같다(변경 추적 문서 9개). 기록을 켜면 본문과 표 셀의 글자 입력·지우기·문단 나누기·합치기를 작성자·날짜와 함께 변경으로 기록한다(검토 탭, 상태 표시줄 표시, MCP `document.track_changes`·`.accept`·`.reject`·`.record`). HWP·HWPX로 저장해도 변경 추적이 남는다: 고친 HWP는 결과본(BodyText)과 변경 표시본(ViewText)을 함께 쓰고, HWPX는 삽입·삭제 표식과 헤더의 변경·작성자 목록을 쓴다(RHWP 패치 0070·0082–0084). 0.19는 이런 문서의 저장을 거절했다.
- 본문 세로쓰기와 원고지: 세로쓰기 구역의 본문을 그리고 편집한다(영문 눕힘·세움, ↑↓는 열 안·←→는 열 사이, 한 열·한 쪽 글자 수는 한글 2024 실측에 맞춤). 가운데·오른쪽 정렬 문단은 열 가운데·끝에 놓인다(0099). 원고지는 400자 A4 세로·200자 A4 가로, 칸마다 한 글자(영문·숫자는 두 자 한 칸)이다(RHWP 패치 0086–0090).
- 쪽: 구역 시작 위치 그대로·홀수로·짝수로(한글처럼 빈 쪽 없이 쪽 번호만 건너뜀, 0060), 구역 나누기가 앞 구역 바탕쪽을 이어 씀(0061), 마지막 쪽·임의 쪽(구역 안 n번째 쪽) 바탕쪽(임의 쪽은 HWPX만, 0081), 머리말·꼬리말의 만든 날짜·저장한 날짜 필드, 라벨 새 쪽 추가하기, 인쇄 창에서 프린터 고르기.
- 선택: 본문 선택이 한글처럼 구역을 넘고 모두 선택은 문서 전체다. 표 셀·글상자 안에서도 여러 문단을 고르고, 글자·문단 모양 창이 그 자리의 값을 보여 준다. 중첩 표에서도 셀 블록을 끌어 고르고 합치기·나누기·줄·칸 넣기·지우기를 한다.
- 상호 참조: 개요 번호·개요 제목, 그림 캡션, 표 셀·개체 안 문단을 대상으로. 하이퍼링크·메모를 책갈피·각주·수식 같은 다른 개체가 있는 문단에도 넣는다(0067). 여러 문단에 걸친 메모.
- 표·개체: 블록만큼 가로 경계선 옮기기, 셀 음영 진하게/연하게 한 단계, 데스크톱 차트 창 15종(혼합형·영역형·방사형·거품형 추가), 수식 미리 보기에서 끌어 고르기, 양식 개체 세부 속성과 넣기 창, 바탕쪽 그림·도형·표의 크기·자리 바꾸기, 표 셀·글상자·주석 안 동적 쪽 번호, .hwp 그림 효과 저장(0054).
- 보기: 개체를 끌 때 쪽·본문·다른 개체에 맞추는 이동 안내선(Alt는 자유 이동), 창이 있는 모니터 작업 영역 안의 창 배열, 모두 아이콘으로, macOS 메뉴·툴팁·상태 표시줄의 ⌘ 표기, 상태 표시줄의 단 번호.
- 조판: 덧말 기준 글자 폭이 줄 나누기에 들어감(0071), 글자처럼 취급한 개체가 있는 줄 높이(0096), 바깥 여백이 있는 개체를 한글처럼 여백 안쪽에 그림(0093), 본문 마지막 줄이 각주 구분선과 겹치던 것(0063), 각주 구분선 길이(0057), HWP5 빈 쪽 규칙(0058), 큰 표가 쪽을 넘을 때의 첫 조각(0091·0092), 이어 쓰는 구역의 머리말·꼬리말(0094, 입력마다 드는 비용은 0101로 되돌림). 새 표 셀은 처음부터 바탕글 문단 모양으로 만든다(0098·0100, 0.20 개발 중 생긴 표 넣기 지연을 없앰).
- 고친 결함: `style.apply`로 개요 문단 스타일을 바꾸면 개요 번호가 뒤 문단으로 번지던 것, `insert.hyperlink`로 넣은 링크가 `hyperlink.list`에 나오지 않고 고칠 수 없던 것, 이름 바꾼 링크의 끝이 HWPX 저장에서 어긋나던 것, `copy_shape.apply`가 조회 결과를 거절하던 것, 묶인 입력이나 롤백이 실패했을 때 변경 표시가 빠지던 것, 표만 든 문단에서 ↓가 오류 나던 것과 글자처럼 취급한 도형이 있는 높은 첫 줄에서 ↓가 움직이지 않던 것(0.19부터, 데스크톱과 MCP `caret.move`), 변경 추적을 켜기만 해도 상태 표시줄이 "변경 추적 기록 있음"으로 나오던 것, 앞쪽 끝이 화면에 걸친 채 표 선 근처를 클릭하면 마우스를 움직이지 않아도 행 높이가 한 쪽만큼 벌어지고 문서가 수정됨으로 바뀌던 것(데스크톱).

## MCP

- 2025년 판 클라이언트 호환: 최신 `2026-07-28`(무상태)과 함께 2025-03-26·06-18·11-25 판의 initialize·세션을 받는다. 0.19는 Codex CLI(2025-06-18로 접속)를 받지 못했다. Claude Code와 Codex CLI로 stdio·HTTP·데스크톱 창 연결을 실제로 확인했다.
- stdio에서 한 문서에 보낸 요청은 보낸 순서대로 실행한다. 2025년 판 HTTP 세션은 64개까지, 5분 쉬면 닫는다. 데스크톱 창 문서의 PDF 저장은 여러 프레임에 나눠 준비해 창이 멈추지 않는다.
- 데스크톱 MCP 연결을 창마다 켤 수 있다(먼저 켠 창부터 7083, 7084 … 7099).
- `crates/mcp` 전체를 다시 읽고, 도구 16개를 정상·경계·오류 입력으로, 전송 3종을 동시성·취소·Tasks·큰 문서·인증·수명으로 실제 호출해 점검했다. 편집 명령 415개의 도움말 예시 중 OS 클립보드를 쓰는 5개를 뺀 410개를 MCP `execute`로 실제 실행해(예시와 빈 인자로 684회) 멈춤·서버 오류·응답 형식 위반·예시 형식 오류가 없음을 확인했고, 415개 모두 도움말·예시 형식을 시험한다.
- 상호 운용: 공식 Python SDK 2.3.0 **52/53**(1.30.0 동작), TypeScript client 2.4.0 **41/43**(실패 2건은 TS SDK가 Tasks 결과를 아직 해석하지 못하는 SDK 한계), 공식 conformance 2026 요구 시나리오 **109 통과·62 실패**(실패는 시험용 fixture가 없는 시나리오), 전체 130/70. rmcp는 3.7.0 그대로다.

## 성능

0.19(v0.19)와 같은 기계·같은 시험 코드로 잰 release 빌드 값이다(ms, 5회 중앙값). 열기·저장·PDF·모두 바꾸기는 조용한 상태에서, 표 넣기와 글자 입력은 마지막 수정 뒤 0.19와 번갈아 다시 쟀다.

| 문서 | 열기 | 글자 하나(쪽 나눔 포함) | HWP 저장 | HWPX 저장 | PDF | 모두 바꾸기 | 표 넣기 / 그 실행 취소 |
|---|---|---|---|---|---|---|---|
| 편람 .hwp 381쪽 | 428 → 437 | 6.91 → 6.65 | 429 → 245 | 439 → 330 | 14,465 → 13,737 | 1,697 → 1,769 | 39.5 → 36.5 / 498.6 → 232.7 |
| 편람 .hwpx 383쪽 | 426 → 419 | 5.63 → 5.85 | 281 → 301 | 378 → 270 | 12,893 → 12,681 | 3,563 → 3,429 | 19.0 → 19.3 / 116.7 → 127.7 |
| 정책 보고서 .hwp 215쪽 | 262 → 264 | 1.08 → 0.97 | 273 → 108 | 313 → 222 | 3,052 → 2,856 | – | 318 → 342 / 336 → 367 |
| 보고서 .hwpx 44쪽 | 42 → 42 | 0.42 → 0.40 | 9 → 6 | 9 → 9 | 856 → 865 | – | 34.2 → 34.5 / 34.7 → 34.6 |
| 양식 .hwpx 30쪽 | 37 → 38 | 4.68 → 4.57 | 38 → 41 | 57 → 51 | 239 → 244 | – | 29.7 → 26.6 / 31.3 → 32.6 |

- 저장: 그림을 다시 압축하지 않고 원본 바이트를 그대로 쓴다(RHWP 패치 0051·0052). 정책 보고서 HWP 저장 −60%, 편람 HWP −43%.
- 실행 취소: 편집한 편람 .hwp를 연 직후 상태로 되돌리기 290 → 37 ms(0095), 저장 직후 첫 실행 취소 434 → 22 ms(0055). 편람 .hwp 표 넣기의 실행 취소 499 → 233 ms.
- 화면: 381쪽 편람 휠 스크롤 최장 프레임 160–200 ms → 20–27 ms(스크롤 중 미리 그리기 미룸, 그림 변환 메모 공유 0059). 표가 많은 표본 문서 열기 9.8초 → 0.05초(표 측정 캐시 0053).
- 0.20 개발 중 생긴 두 퇴행은 마감 전에 되돌렸다: 표 넣기(편람 .hwpx 16.5 → 179.8 ms, 0098·0100으로 19.3 ms), 편람 글자 입력의 RHWP 몫(0.4 → 0.7 ms, 0101로 0.4 ms).
- 정책 보고서의 표 넣기 318 → 342 ms는 다른 빌드와 겹친 재측정의 흔들림 범위로 본다(같은 회차의 0.19 값도 조용한 측정의 2배까지 흔들렸다). 열기·첫 쪽은 ±3% 안이다.

## 화면

- 창보다 큰 대화상자는 세로로 스크롤하고, 공통 대화상자는 창 폭에 맞춰 오른쪽 단추 열을 남긴다(800×600에서 문단 모양·미리 보기·문자표).
- 문맥 메뉴: 굵게·기울임·밑줄 표시, 단축키, 회색 항목의 사유, 줄·칸 하위 메뉴.
- 작은 글자 9 → 11 px, 대화상자 항목 간격 8 px, 흰 색 단추 테두리, 대화상자 탭 이름 간격, 좁은 리본 중간 단계에 이름 표시, 리본 "지원 안 함" 칩이 세로로 눌리던 것.
- 한자로 바꿀 낱말이 없을 때 오류 상자 대신 상태 표시줄 안내, 머리말·꼬리말 마당 목록의 템플릿 이름.

## 검증과 한계

- CI와 같은 방식(깨끗한 RHWP v0.8.7에 패치 86개를 이름순으로 적용)으로 Windows x64 `cargo test --release --locked --workspace` 1,069 통과·0 실패(무시 13), `cargo clippy --workspace --all-targets -D warnings` 0. Linux(WSL) 같은 시험 1,068 통과·0 실패(무시 13). 릴리스 빌드는 GitHub Actions에서 Windows·macOS·Linux workspace 시험을 다시 돌린다.
- RHWP 로컬 패치는 `patches/rhwp/`에 MIT 라이선스 그대로 기록한다(0001–0101 중 86개; 빠진 번호는 합치거나 거두거나 순서를 맞추느라 옮긴 것). 깨끗한 RHWP v0.8.7에 이름 순서대로 적용하면 빌드에 쓴 RHWP와 같아진다. RHWP 업스트림 최신판은 같은 문서에서 열기·입력·PDF가 2–6배 느려 0.20에 넣지 않았다.
- 0.19 이후 새로 들어온 외부 Rust 크레이트는 없다(Cargo.lock은 워크스페이스 버전만 바뀜). 고지: `THIRD-PARTY-NOTICES.txt`, `THIRD-PARTY-DEPENDENCIES.md`, `assets/licenses/`.

### 알려진 한계

- 변경 추적은 서식 변경을 기록하지 않고, 추적 중 지원하지 않는 편집은 거절한다. HwpCraft가 기록·저장한 변경 추적 문서를 데스크톱 한글에서 열어 표시·적용·취소하는 것은 확인 중이다.
- 세로쓰기 구역의 양쪽·배분 정렬은 글자 사이를 벌리지 않고 열 위부터 놓으며, 문단 여백·들여쓰기는 열 시작 자리에 반영하지 않는다(한글 세로 문서와 대조하지 못함). 세로쓰기 구역의 표·그림·각주는 아직 세로로 그리지 않는다. 세로 머리말·꼬리말과 세로 구역 안의 표 셀·글상자는 읽기 전용이다.
- 큰 표가 많은 일부 HWP 문서는 쪽 나눔이 한글과 다를 수 있다. 381쪽 편람의 .hwp는 여전히 381쪽이다(한글 383쪽, 같은 문서 .hwpx 383쪽). 0.20에서 구역 11·12 경계와 제5장 질의응답 표 위치·높이는 한글과 맞췄다.
- 바깥 여백 안쪽 그리기는 왼쪽·위 정렬의 글 앞·뒤 표·도형과 그림에만 적용했다. 가운데·오른쪽·아래 정렬과 글자처럼 취급한 개체는 종전대로 그린다.
- 여러 모니터 창 배열은 해석·고르기만 시험했다(검증 PC는 모니터 하나). 실제 프린터·드라이버 인쇄는 검증하지 않았다.
- MCP: HTTP로 동시에 보낸 요청의 순서는 정해지지 않는다. 데스크톱 창 문서의 HWP·HWPX 저장은 한 번에 실행된다(큰 문서 1초 남짓). Tasks는 2026-07-28 클라이언트에만 준다.
- 글자처럼 취급한 도형이 있는 높은 첫 줄의 처음에서 ↓를 누르면 둘째 줄을 건너뛰고 셋째 줄로 간다(0.19는 움직이지 않았음, 다음 버전에서 고침).
- 맞춤법, 교정 부호, 글맵시·OLE, 인용·참고 문헌, 프레젠테이션은 없다.

### 다음 버전에서 정할 것

다음은 아직 넣지도 빼지도 않았다.

- 낱말 한자 사전 데이터(libhangul hanja.txt, BSD-3)를 넣을지.
- 맞춤법 사전과 그 라이선스.
- 배포용 문서로 저장(지금은 배포용 문서 읽기 전용만).
- 클라우드·번역·실시간 협업을 범위 밖으로 둘지.
- 한글 형식이 공개되지 않은 보안 기능 4개(선택 글자 보호·개인 정보 보호 해제·보호 암호 변경·문서 보안 설정)를 범위 밖으로 확정할지. 대안으로 개인 정보 찾아서 가리기가 있다.

---

# HwpCraft 0.20 — track changes, vertical text, interface cleanup, MCP client compatibility, editing speed

0.20 release. This is not 100% compatibility with every Hangul feature. Against the 325 commands of Hangul 2024's public help, HwpCraft scores **64%** (0.19: 60%; 162 working, 62 partial, 76 missing, 25 out of scope; working 2 points, partial 1, out of scope excluded). By area: Format 92%, Table 91%, Security 83%, Drawing 82%, Page 77% (0.19: 65%), Edit 61%, View 60%, Insert 55%, Add-ons 50%, Tools 45%, File 38%, Review 38%. Each row was judged by running and saving in HwpCraft, not by measuring Hangul. The remaining gaps are under "Known limits" and "To be decided for a later version".

## Features

- Track changes: Hangul documents with tracked changes (HWP and HWPX) open with the changes shown: insertions underlined in blue, deletions struck through in red, format changes underlined in green, on screen, in PNG and in PDF (body and table cells). The change list (kind, text, author, date) accepts or rejects the change at the caret or every change (one undo step) and steps to the previous or next change; accepting every change gives the result Hangul stored (nine tracked documents). With recording on, typing, deleting, splitting and joining paragraphs in the body and table cells are recorded with author and date (Review tab, status bar, MCP `document.track_changes`, `.accept`, `.reject`, `.record`). HWP and HWPX saves keep the tracking: an edited HWP gets both the accepted text (BodyText) and the marked view (ViewText), and HWPX gets insert / delete marks and the change and author lists in its header (RHWP patches 0070, 0082–0084). 0.19 refused to save such documents.
- Vertical body text and manuscript paper: vertical-text sections are drawn and edited (Latin text rotated or upright; ↑↓ within a column, ←→ between columns; characters per column and page match Hangul 2024 measurements). Centred and right-aligned paragraphs sit in the middle and at the end of their column (0099). Manuscript paper comes as 400 characters A4 portrait and 200 characters A4 landscape, one character per cell, two Latin letters or digits per cell (RHWP patches 0086–0090).
- Pages: section start as is, odd or even (as in Hangul, the page number skips without an empty page; 0060), a section break carries the previous section's master pages on (0061), last-page and optional-page (n-th page of the section) master pages (optional pages in HWPX only; 0081), creation and save date fields in headers and footers, adding a page of labels, choosing a printer in the print window.
- Selection: a body selection runs over sections as in Hangul, and Select All takes the whole document. Several paragraphs can be selected inside table cells and text boxes, and the character and paragraph dialogs show the values there. Cell blocks are dragged in nested tables too, and merged, split, and get rows and columns inserted or deleted.
- Cross references to outline numbers and headings, picture captions, and paragraphs inside table cells and objects. Hyperlinks and memos also go into paragraphs holding other objects such as bookmarks, footnotes and equations (0067). Memos spanning several paragraphs.
- Tables and objects: moving a horizontal border for a cell block, one-step darker / lighter cell shading, all 15 chart kinds in the desktop chart dialog (combo, area, radar and bubble added), drag selection in the equation preview, detailed form-object properties and their insert dialog, resizing and moving master-page pictures, shapes and tables, live page numbers inside table cells, text boxes and notes, picture effects saved to .hwp (0054).
- View: move guides that snap to the page, the body and other objects while dragging (Alt moves freely), window arranging inside the work area of the window's monitor, minimise all, ⌘ in macOS menus, tooltips and status bar, the column number in the status bar.
- Layout: ruby base text counts toward line breaking (0071), line height of lines with objects treated as characters (0096), objects with an outer margin drawn inside it as in Hangul (0093), the last body line overlapping the footnote separator (0063), footnote separator length (0057), HWP5 empty-page rule (0058), the first piece of a large table crossing a page (0091, 0092), headers and footers of kept sections (0094; its per-keystroke cost removed again by 0101). New table cells get the default paragraph shape when they are created (0098, 0100), which removes a table-insert slowdown introduced during 0.20 development.
- Fixed: changing an outline paragraph's style with `style.apply` spread its outline number to the following paragraphs; a link inserted with `insert.hyperlink` did not show in `hyperlink.list` and could not be edited; a renamed link's end moved on HWPX save; `copy_shape.apply` refused query results; the changed mark was lost when a grouped input or a rollback failed; ↓ failed on a paragraph holding only a table and did not move on a tall first line with an inline shape (since 0.19, desktop and MCP `caret.move`); turning recording on alone made the status bar say the document had tracked changes; clicking near a table line with the previous page's bottom edge in view pulled a row a page height taller and marked the document changed without any mouse movement (desktop).

## MCP

- 2025-era clients: the initialize handshake and sessions of 2025-03-26, 06-18 and 11-25 are accepted alongside the current stateless `2026-07-28`. 0.19 turned away Codex CLI (which connects as 2025-06-18). stdio, HTTP and desktop-window connections were checked for real with Claude Code and Codex CLI.
- stdio requests for one document run in the order sent. 2025-era HTTP sessions are capped at 64 and closed after 5 idle minutes. PDF saves of a desktop-window document are prepared over several frames so the window does not freeze.
- The desktop MCP connection can be turned on per window (ports 7083, 7084 … 7099 in the order the windows turn it on).
- `crates/mcp` was read again in full; the 16 tools were called with normal, edge and wrong input, and the three transports were exercised for concurrency, cancellation, Tasks, large documents, authentication and lifetime. Of the 415 editing commands, the help examples of 410 (all but the 5 that use the OS clipboard) were run through MCP `execute` (684 calls with the example and with empty parameters) without hangs, server errors, output-schema violations or example-shape errors; all 415 have their help and examples checked by tests.
- Interoperability: official Python SDK 2.3.0 **52/53** (1.30.0 works), TypeScript client 2.4.0 **41/43** (the 2 failures are the TS SDK not reading Tasks results yet), official conformance for the 2026 requirements **109 passed, 62 failed** (failures are scenarios without a test fixture), all scenarios 130/70. rmcp stays at 3.7.0.

## Performance

Release builds of 0.19 (v0.19) and 0.20 on the same machine with the same test code (ms, median of 5 runs). Opening, saving, PDF and replace-all were measured on a quiet machine; table insertion and typing were measured again after the last changes, alternating with 0.19.

| Document | Open | One character (with page breaking) | HWP save | HWPX save | PDF | Replace all | Insert table / its undo |
|---|---|---|---|---|---|---|---|
| Handbook .hwp, 381 pages | 428 → 437 | 6.91 → 6.65 | 429 → 245 | 439 → 330 | 14,465 → 13,737 | 1,697 → 1,769 | 39.5 → 36.5 / 498.6 → 232.7 |
| Handbook .hwpx, 383 pages | 426 → 419 | 5.63 → 5.85 | 281 → 301 | 378 → 270 | 12,893 → 12,681 | 3,563 → 3,429 | 19.0 → 19.3 / 116.7 → 127.7 |
| Policy report .hwp, 215 pages | 262 → 264 | 1.08 → 0.97 | 273 → 108 | 313 → 222 | 3,052 → 2,856 | – | 318 → 342 / 336 → 367 |
| Report .hwpx, 44 pages | 42 → 42 | 0.42 → 0.40 | 9 → 6 | 9 → 9 | 856 → 865 | – | 34.2 → 34.5 / 34.7 → 34.6 |
| Form .hwpx, 30 pages | 37 → 38 | 4.68 → 4.57 | 38 → 41 | 57 → 51 | 239 → 244 | – | 29.7 → 26.6 / 31.3 → 32.6 |

- Saving writes pictures' original bytes instead of recompressing them (RHWP patches 0051, 0052): policy report HWP save −60%, handbook HWP −43%.
- Undo: undoing back to the just-opened state of an edited handbook .hwp 290 → 37 ms (0095), the first undo after a save 434 → 22 ms (0055), undoing a table insert in the handbook .hwp 499 → 233 ms.
- Screen: longest wheel-scroll frame in the 381-page handbook 160–200 ms → 20–27 ms (pre-drawing waits while scrolling, picture conversions shared, 0059). Opening a sample with many tables 9.8 s → 0.05 s (table measurement caches, 0053).
- Two slowdowns introduced during 0.20 development were removed before the release: table insertion (handbook .hwpx 16.5 → 179.8 ms, back to 19.3 ms with 0098 and 0100) and RHWP's share of typing in the handbook (0.4 → 0.7 ms, back to 0.4 ms with 0101).
- Table insertion in the policy report, 318 → 342 ms, is taken as noise of a re-measurement that overlapped another build (the 0.19 values of the same round varied up to twice the quiet ones). Opening and the first page stay within ±3%.

## Interface

- Dialogs taller than the window scroll vertically, and shared dialogs shrink to the window width while keeping their right-hand button column (paragraph shape, preview and character map at 800×600).
- Context menu: bold, italic and underline shown as such, shortcuts, reasons on greyed items, row / column submenus.
- Small text 9 → 11 px, 8 px between dialog items, borders on white colour buttons, spacing between dialog tab names, names in the middle stage of a narrow ribbon, and the "unsupported" chip no longer squeezed vertically.
- A status-bar note instead of an error box when there is no word to convert to hanja; template names in the header / footer gallery.

## Verification and limits

- Built as CI does (a clean RHWP v0.8.7 with the 86 patches applied in name order): Windows x64 `cargo test --release --locked --workspace` 1,069 passed, 0 failed (13 ignored), `cargo clippy --workspace --all-targets -D warnings` clean; the same tests on Linux (WSL) 1,068 passed, 0 failed (13 ignored). Release builds run the workspace tests again on Windows, macOS and Linux in GitHub Actions.
- Local RHWP patches are kept in `patches/rhwp/` under RHWP's MIT license (86 of 0001–0101; missing numbers were merged, withdrawn or moved to keep the apply order). Applied in name order to a clean RHWP v0.8.7 they give the RHWP used for the build. RHWP's latest upstream was not taken for 0.20: on the same documents opening, typing and PDF were 2–6 times slower.
- No new third-party Rust crates since 0.19 (Cargo.lock only changes the workspace versions). Notices: `THIRD-PARTY-NOTICES.txt`, `THIRD-PARTY-DEPENDENCIES.md`, `assets/licenses/`.

### Known limits

- Track changes does not record format changes, and edits it cannot record are refused while recording. Opening HwpCraft-recorded tracked documents in desktop Hangul (display, accept, reject) is still being checked.
- In vertical-text sections justified and distributed paragraphs start at the top without spreading the characters, and paragraph margins and indents do not move the column start (not compared with Hangul). Tables, pictures and footnotes in vertical sections are not drawn vertically yet; vertical headers / footers and table cells and text boxes inside vertical sections are read-only.
- Some HWP documents with many large tables break pages differently from Hangul. The 381-page handbook's .hwp is still 381 pages (Hangul 383, the same document as .hwpx 383). 0.20 brings the section 11 / 12 boundary and the position and height of the chapter 5 Q&A tables in line with Hangul.
- Drawing inside the outer margin applies to left / top aligned tables and shapes in front of or behind text and to pictures; centre, right and bottom alignment and objects treated as characters are drawn as before.
- Window arranging over several monitors was tested by parsing and choosing only (the test PC has one monitor). Printing on real printers and drivers is not verified.
- MCP: the order of requests sent concurrently over HTTP is not defined. HWP / HWPX saves of a desktop-window document run in one go (a little over a second for a large document). Tasks are offered to 2026-07-28 clients only.
- ↓ at the start of a tall first line with an inline shape skips the second line and lands on the third (0.19 did not move; to be fixed in the next version).
- Not there: spell checking, proofreading marks, text art and OLE, citations and bibliographies, presentation.

### To be decided for a later version

These are neither added nor dropped yet:

- Whether to include word hanja dictionary data (libhangul hanja.txt, BSD-3).
- A spell-check dictionary and its license.
- Saving as a distribution document (read-only distribution documents are supported).
- Whether cloud, translation and real-time collaboration are out of scope.
- Whether the four security features whose Hangul format is not published (protect selected text, remove privacy protection, change protection password, document security settings) are confirmed out of scope. Find-and-mask personal information is the alternative.
