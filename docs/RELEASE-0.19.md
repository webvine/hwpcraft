# HwpCraft 0.19 — 아래한글 편집 기능 넓히기·MCP 대응·조판 속도

0.19 릴리스. 아래한글 전체 기능의 100% 호환을 뜻하지 않는다. 대표 기능 60개 원장 기준 완성도는 **95/120(79%)**이다(0.18 66/120, 55%; 기본경로 2점·부분 1점). 판정 근거와 남은 항목은 아래 "알려진 한계"와 "다음 버전에서 정할 것"에 적었다.

## 기능

- 차례: 제목 차례(개요 문단 또는 지정 스타일), 표 차례·그림 차례(캡션 글과 쪽), 차례 새로 고침(`tools.toc`·`tools.toc.update`). 항목은 대상 문단으로 가는 링크이고 PDF에서도 눌린다. 한글의 차례 필드가 아니라 차례 스타일의 일반 문단이다.
- 상호 참조: 책갈피·개요 제목·표·그림의 쪽 번호, 표 캡션 번호·캡션 전체(`insert.cross_reference`, 한글과 같은 `%xrf` 필드). 쪽이 바뀌면 한 번에 고친다.
- 색인: 낱말에 색인 표시(첫째·둘째 키)를 달고, 한글 2024 색인 만들기와 같은 모양(첫소리 묶음 머리, `낱말<탭>쪽`)으로 캐럿 위치에 색인을 넣는다. 인용·참고 문헌은 아직 없다.
- 메모 댓글과 표 셀 메모, 날짜 필드(문서 만든 날짜·마지막 저장 날짜), 문서 비교 뒤 고른 차이만 병합, 문서 이력 목록(한글이 HWP에 기록한 버전)과 마지막 버전 HML 저장.
- 스타일마당(보고서·공문·간결 묶음, 사용자 묶음 저장), 덧말, 문단 첫 글자 장식(2줄·3줄·여백), 그림 글머리표, 양식 개체 만들기(선택 상자·입력 상자·목록 상자·라디오·명령 단추).
- 수식 시각 편집(그려진 조각을 눌러 스크립트 위치로)·수식 매크로, 차트 15종(누적·혼합·영역·방사·거품형 추가), 그림 효과(그림자·네온·부드러운 가장자리·반사).
- 쪽: 바탕쪽 글·개체, 구역·단 정의 나누기, 줄 번호(설정과 화면·PDF 그리기, 문단별 줄 번호 건너뜀), 사용자 표 마당, 쪽 윤곽, 격자, 맞쪽 보기.
- 인쇄 상세(홀수·짝수 쪽, 역순, 모아 찍기, 워터마크)와 인쇄 미리 보기, 자동 복구·복구 창, 문서마당(사용자 서식), 화면 나누기·새 창·창 배열·창 전환, 키 매크로 기록·실행·저장.
- 표 안의 표(중첩 표) 셀과 글상자를 쪽 화면에서 바로 편집한다: 누르면 그 자리에 캐럿이 놓이고 글 입력(한글 입력기 포함)·지우기·Enter·화살표·Home/End·선택·복사/붙이기·실행 취소·리본의 글자/문단 단추가 된다. 두 번·세 번 누르면 낱말·문단 선택, Tab은 다음 셀로 간다(RHWP 패치 0046).
- DOCX 내보내기(글자·문단 모양, 표, 그림, 수식, 머리말·꼬리말, 각주·미주, 쪽 설정; 담지 못한 것은 결과의 `omitted`에 적음).
- 고친 결함: 새 문서를 HWP로 저장해 다시 열면 쪽 나눔이 달라지던 조판 결함, 0.18까지 HwpCraft가 붙인 글머리표·문단 번호가 한글에서 본문 밖으로 잘려 보이지 않던 결함, 표 셀 안 메모의 내용이 HWP 저장 때 사라지던 결함, 덧말·글자 겹치기·조판 부호 지우기 뒤 같은 문단의 누름틀이 저장에서 사라지던 결함, 스타일을 지우면 머리말·글상자 문단이 없는 스타일을 가리키던 결함, 편집으로 건 개체 보호가 표는 HWP 저장, 그림은 HWPX 저장에서 풀리던 결함(RHWP 패치 0050).

## MCP 2026-07-28

- 도구 16개(`list_documents` 추가): 실행 중인 데스크톱 창이 공유한 문서를 MCP로 고친다(루프백 HTTP, 토큰 인증). 고친 내용은 창에 바로 보이고 창의 실행 취소 기록에 들어간다.
- 문서 전체 찾기·바꾸기: `edit.find_all`·`edit.replace_all`의 `include`로 머리말·꼬리말·각주·미주·글상자·모든 깊이의 표 셀까지 한 번에(실행 취소 한 단계, 보호된 곳은 건너뛰고 `skippedProtected`로 셈). 381쪽 편람 1,470건 약 4초.
- 표: `insert.table` 결과에 새 표 주소, 셀 글을 한 번에 쓰는 `table.cells.set`, 셀 주소로 줄·칸 넣기·지우기(`table.edit`).
- 한글 자동화 인자(ParameterSet): 글자 모양·문단 모양·표 만들기에 더해 InsertText, PageSetup(SecDef.PageDef, `ApplyTo` 문서 전체 포함), 찾기·바꾸기(AllReplace·ExecReplace·RepeatFind·FindDlg·FindAll). 한컴 공개 표의 필드 이름·단위를 쓰고, 지원하지 않는 필드는 편집 전에 거절한다. 공개 액션 1,083개 중 642개 대응(59%).
- `page.setup`의 `apply_to:"document"`, `hyperlink.jump`, 차례·상호 참조·색인·메모 댓글·날짜 필드·스타일마당 등 새 명령마다 도움말과 예제. 모든 도움말 문장을 한국어로 맞췄다.
- 고친 결함: `edit.find_all`이 글상자·중첩 셀·수식 스크립트 안의 일치를 본문·표 셀 캐럿 위치로 돌려줘, 그 자리에서 고치면 엉뚱한 글이 바뀌었다. 이제 일치마다 버전 2 주소를 주고 캐럿 위치는 본문·1단계 셀 글에만 준다.
- 도구 결과 처리 약 40% 단축(JSON 재파싱 제거). [보고서 작성 순서](mcp-commands.md#보고서-작성-순서) 안내.
- 상호 운용: 공식 Python SDK 2.3.0 stdio·HTTP **53/53**, TypeScript client 2.4.0 **41/43**(실패 2건은 TS SDK가 Tasks 결과를 아직 해석하지 못하는 SDK 한계), 공식 conformance 0.2.0-alpha.12 **109/171**(실패는 시험용 fixture가 없는 시나리오).

## 성능

0.18(v0.18)과 같은 기계·같은 시험 코드로 잰 release 빌드 값이다(5회 중앙값, 글자 입력은 21회).

- 381쪽 편람 모두 바꾸기 877곳: HWP 37.4초 → 1.7초(−96%), HWPX 38.4초 → 3.5초(−91%).
- 44쪽 보고서 글자 하나: 16.4 ms → 0.4 ms(−98%, 줄 배치가 그대로인 문단은 구역 조판 결과를 다시 쓴다; RHWP 패치 0031). 0.19 마지막 수정으로 첫 문단(구역·단 정의 문단)과 미주 없는 여러 구역 문서도 재사용한다(첫 문단 18 → 0.5 ms, 14구역 편람 입력의 쪽 나눔 3.0 → 0.4 ms).
- 저장: 편람 HWP 607 → 422 ms, HWPX 829 → 453 ms. PDF 크기 48.2 MB → 9.7 MB(−80%, 글꼴 공유와 같은 자원 합치기), PDF 저장 시간은 +3–5%(13.3 → 13.7초)로 크기 대비 받아들였다.
- 링크 있는 차례 넣기 1,500제목 18.6초 → 1.25초. 44쪽 보고서 글자 입력 프레임 75 → 32 ms.
- 그대로인 것: 열기(HWP 428 ms, HWPX 410 → 422 ms, +3%는 원인 확인 중), MCP 왕복(caret.get 약 1 ms, render_page 약 10 ms).

## 화면

- 좁은 창(400 px까지)에서 리본·탭 줄·서식 도구 줄이 잘리지 않는다(» 더 보기). 접힌 묶음·더 보기에도 키팁, 회색 단추에는 쓸 수 없는 이유 툴팁.
- 대화상자는 첫 입력 칸에 포커스가 가고, 저장·오류 상자에 Alt 단축키가 있다.
- 리본에서 지원하지 않는 명령은 59개에서 36개로 줄었다(남은 목록과 사유는 "지원 안 함" 칸).

## 검증과 한계

- Windows x64 `cargo test --locked --workspace` 905 통과·0 실패(10-31 릴리스 트리 기준), `cargo clippy --workspace --all-targets -D warnings` 0. Linux(WSL) 실제 시험, macOS(arm64·x64) clippy 교차 확인. 릴리스 빌드는 GitHub Actions에서 Windows·macOS·Linux workspace 시험을 다시 돌린다.
- RHWP 로컬 패치는 `patches/rhwp/`에 MIT 라이선스 그대로 기록한다(0001–0050 중 44개; 빠진 번호는 합치거나 거둔 것). 깨끗한 RHWP v0.8.7에 순서대로 적용하면 빌드에 쓴 RHWP와 같아진다.
- 0.18 이후 새로 들어온 외부 Rust 크레이트는 없다(Cargo.lock의 외부 크레이트 목록 그대로). 고지: `THIRD-PARTY-NOTICES.txt`, `THIRD-PARTY-DEPENDENCIES.md`, `assets/licenses/`.

### 알려진 한계

- 화면·HWP·HWPX 쪽 수가 갈리는 경우가 남아 있다: 381쪽 편람에서 모두 바꾸기를 하면 화면 380쪽·HWP 다시 열기 381쪽·HWPX 다시 열기 382쪽, HwpCraft 새 문서에 빈 줄을 많이 넣으면 화면과 HWPX 다시 열기가 한 쪽 다르다.
- 변경 추적: HWP 변경 추적 기록이 있는 문서의 HWP/HWPX 저장은 기본 거절하고(`acceptTrackChanges`로 결과본 저장), HWPX의 변경 표식은 읽지도 보존하지도 않는다. 변경 추적 화면·수락/거절은 없다.
- 본문 세로쓰기·원고지, 글맵시·OLE, 인용·참고 문헌, 프레젠테이션, 입력 도우미, 한자 음 달기는 없다. 교정 부호는 없다.
- 줄 번호 건너뜀은 HWPX에만 저장된다(HWP5 자리가 공개 형식 문서에 없음). 그림 효과는 .hwp로 저장할 때 빼야 한다(`dropPictureEffects`).
- 차례는 한글 차례 필드가 아니다. 색인은 캐럿 위치에 넣으며(한글은 새 문서), 다시 부르면 하나 더 넣는다. 문서 이력은 보여 주기·마지막 버전 HML 저장만 하고 새 버전 기록·되돌리기는 없다.
- 창 전환은 요청 전달까지만 확인했다(Windows가 다른 프로그램 창을 앞으로 못 가져오게 막으면 작업 표시줄 단추만 깜박일 수 있음). 창 배열은 주 모니터 기준이다.
- 실제 프린터·드라이버 인쇄는 검증하지 않았다. 새 차트·상호 참조 HWPX를 한컴에서 열고 고치는 호환성은 일부 미확인이다.

### 다음 버전에서 정할 것

다음은 아직 넣지도 빼지도 않았다.

- 낱말 한자 사전 데이터(libhangul hanja.txt, BSD-3)를 넣을지.
- 맞춤법 사전과 그 라이선스.
- 배포용 문서로 저장(지금은 배포용 문서 읽기 전용만).
- 클라우드·번역·실시간 협업을 범위 밖으로 둘지.
- 한글 형식이 공개되지 않은 보안 기능 4개(선택 글자 보호·개인 정보 보호 해제·보호 암호 변경·문서 보안 설정)를 범위 밖으로 확정할지. 대안으로 개인 정보 찾아서 가리기가 있다.

---

# HwpCraft 0.19 — wider Hangul editing, matching MCP commands, faster layout

0.19 release. This is not 100% compatibility with every Hangul feature. On the 60-feature ledger HwpCraft scores **95/120 (79%)** (0.18: 66/120, 55%; full path 2 points, partial 1). The remaining gaps are under "Known limits" and "To be decided for a later version".

## Features

- Tables of contents: headings (outline paragraphs or chosen styles), tables and pictures (caption text and page), refreshed in one step (`tools.toc`, `tools.toc.update`). Entries link to their targets, also in PDF. They are paragraphs in table-of-contents styles, not Hangul's TOC field.
- Cross references to the page of a bookmark, outline heading, table or picture, and to a table's caption number or whole caption (`insert.cross_reference`, Hangul's `%xrf` field), updated together when pages move.
- Index: index marks (first and second key) on words, and an index at the caret laid out like Hangul 2024's (initial-consonant group heads, `word<tab>pages`). Citations and bibliographies are not there yet.
- Memo replies and memos in table cells, date fields (created / last saved), merging chosen differences after a document compare, the document history Hangul records in HWP files and saving its last version as HML.
- Style sets (report, official letter, compact, and your own), ruby text, drop caps (2 or 3 lines, in the margin), picture bullets, form objects (check box, edit box, combo box, radio button, push button).
- Visual equation editing (click a drawn piece to go to its script) and equation macros, 15 chart kinds (stacked, combination, area, radar and bubble added), picture effects (shadow, glow, soft edge, reflection).
- Pages: master-page text and objects, section and column-definition breaks, line numbers (settings, drawn on screen and in PDF, skipped per paragraph), your own table styles, page outline view, grid, facing pages.
- Print details (odd/even pages, reverse order, several pages per sheet, watermark) and print preview, auto recovery with a recovery window, document templates, split view, new window, window arrange and switch, key macros (record, run, save).
- Cells of a table inside a table and text boxes are edited on the page: a click places the caret there, and typing (including the Korean IME), deleting, Enter, arrows, Home / End, selection, copy and paste, undo and the ribbon's character and paragraph buttons work. A double or triple click selects the word or paragraph, and Tab goes to the next cell (RHWP patch 0046).
- DOCX export (character and paragraph shapes, tables, pictures, equations, headers and footers, foot- and endnotes, page setup; what it cannot carry is listed in `omitted`).
- Fixed: page breaks changing when a new document was saved as HWP and opened again; bullets and numbering added by HwpCraft up to 0.18 cut off outside the body in Hangul; memo content in table cells lost on HWP save; a click-here field after a ruby, character overlap or deleted control mark lost on save; deleting a style leaving header and text-box paragraphs pointing at a style that no longer exists; an object lock set by editing lost when a table was saved as HWP or a picture as HWPX (RHWP patch 0050).

## MCP 2026-07-28

- 16 tools (`list_documents` added): edit the document a running desktop window shares (loopback HTTP, token). Edits show in the window at once and go into its undo history.
- Find and replace across the document: `include` on `edit.find_all` / `edit.replace_all` reaches headers, footers, footnotes, endnotes, text boxes and table cells at any depth (one undo step; protected places are skipped and counted in `skippedProtected`). 1,470 replacements in the 381-page handbook take about 4 s.
- Tables: `insert.table` returns the new table's address; `table.cells.set` writes many cells at once; `table.edit` inserts and deletes rows and columns by cell address.
- Hangul automation ParameterSets: besides character shape, paragraph shape and table creation, now InsertText, PageSetup (SecDef.PageDef, including `ApplyTo` whole document) and find/replace (AllReplace, ExecReplace, RepeatFind, FindDlg, FindAll), with the field names and units of Hancom's published table; unsupported fields are refused before any edit. 642 of the 1,083 public actions are mapped (59%).
- `apply_to:"document"` on `page.setup`, `hyperlink.jump`, and help with examples for every new command (tables of contents, cross references, index, memo replies, date fields, style sets and more). All help text is in Korean now.
- Fixed: `edit.find_all` gave matches inside text boxes, nested cells and equation scripts as body or table-cell caret positions, so editing there changed the wrong text. Every match now has a version 2 address, and caret positions are given only for body and first-level cell text.
- Tool results are handled about 40% faster (no second JSON parse). A [report-writing recipe](mcp-commands.md#보고서-작성-순서).
- Interoperability: official Python SDK 2.3.0 over stdio and HTTP **53/53**, TypeScript client 2.4.0 **41/43** (the 2 failures are the TS SDK not reading Tasks results yet), official conformance 0.2.0-alpha.12 **109/171** (failures are scenarios without a test fixture).

## Performance

Release builds of 0.18 (v0.18) and 0.19 on the same machine with the same test code (median of 5 runs, 21 for typing).

- Replace all, 877 places in the 381-page handbook: HWP 37.4 s → 1.7 s (−96%), HWPX 38.4 s → 3.5 s (−91%).
- One character in a 44-page report: 16.4 ms → 0.4 ms (−98%; a paragraph whose lines stay the same reuses the section's previous layout, RHWP patch 0031). The last 0.19 changes extend this to the first paragraph (section and column definitions) and to multi-section documents without endnotes (first paragraph 18 → 0.5 ms, handbook page breaking per key 3.0 → 0.4 ms).
- Saving the handbook: HWP 607 → 422 ms, HWPX 829 → 453 ms. PDF size 48.2 MB → 9.7 MB (−80%, shared fonts and merged resources); PDF save time +3–5% (13.3 → 13.7 s), accepted for the size.
- A linked table of contents for 1,500 headings 18.6 s → 1.25 s. Typing frames in the 44-page report 75 → 32 ms.
- Unchanged: opening (HWP 428 ms; HWPX 410 → 422 ms, +3%, cause under investigation) and MCP round trips (caret.get about 1 ms, render_page about 10 ms).

## Interface

- The ribbon, tab row and formatting toolbar no longer cut off in narrow windows down to 400 px (» more). Key tips also on collapsed groups and the more menu; greyed buttons say why.
- Dialogs focus their first field; save and error boxes have Alt shortcuts.
- Unsupported ribbon commands went from 59 to 36 (listed with reasons under "unsupported").

## Verification and limits

- Windows x64 `cargo test --locked --workspace` 905 passed, 0 failed (release tree, 10-31), `cargo clippy --workspace --all-targets -D warnings` clean. Real tests on Linux (WSL), clippy cross-checks for macOS (arm64, x64). Release builds run the workspace tests again on Windows, macOS and Linux in GitHub Actions.
- Local RHWP patches are kept in `patches/rhwp/` under RHWP's MIT license (44 of 0001–0050; missing numbers were merged or withdrawn). Applied in order to a clean RHWP v0.8.7 they give the RHWP used for the build.
- No new third-party Rust crates since 0.18 (the Cargo.lock crate list is unchanged). Notices: `THIRD-PARTY-NOTICES.txt`, `THIRD-PARTY-DEPENDENCIES.md`, `assets/licenses/`.

### Known limits

- Page counts can still differ between screen, HWP and HWPX: after a replace-all in the 381-page handbook the screen shows 380 pages, the reopened HWP 381 and the reopened HWPX 382; many empty lines in a new HwpCraft document differ by one page between screen and reopened HWPX.
- Track changes: a document with HWP change tracking is refused as HWP/HWPX unless `acceptTrackChanges` saves the shown result; HWPX change marks are neither read nor kept. There is no change-tracking view or accept/reject.
- Not there: vertical body text and manuscript paper, text art and OLE, citations and bibliographies, presentation, input helper, hanja pronunciation, proofreading marks.
- Skipping line numbers is saved only to HWPX (the HWP5 slot is not in the published format). Picture effects must be dropped for .hwp (`dropPictureEffects`).
- The table of contents is not Hangul's TOC field. The index goes in at the caret (Hangul makes a new document) and a second call adds another. Document history shows versions and saves the last one as HML; it does not record versions or roll back.
- Window switching is confirmed only up to delivering the request (when Windows refuses to bring another program's window forward, its taskbar button may only flash). Window arrangement uses the primary monitor.
- Printing on real printers and drivers is not verified. Opening and editing new charts and cross-reference HWPX files in Hancom is partly unverified.

### To be decided for a later version

These are neither added nor dropped yet:

- Whether to include word hanja dictionary data (libhangul hanja.txt, BSD-3) .
- A spell-check dictionary and its license .
- Saving as a distribution document (read-only distribution documents are supported).
- Whether cloud, translation and real-time collaboration are out of scope.
- Whether the four security features whose Hangul format is not published (protect selected text, remove privacy protection, change protection password, document security settings) are confirmed out of scope . Find-and-mask personal information is the alternative.
