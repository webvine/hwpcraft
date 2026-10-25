# HwpCraft

**English** · [한국어](#hwpcraft-한국어)

A native Rust desktop editor for Korean HWP / HWPX word-processor documents, built on the MIT-licensed [RHWP](https://github.com/edwardkim/rhwp) document engine and an [egui](https://github.com/emilk/egui) user interface.

Current version: **0.12**. Versions increase as `0.10 → 0.11 → 0.12 …`.

HwpCraft is an independent open-source project. It is not affiliated with, endorsed by, or derived from Hancom Inc. It ships no Hancom code, icons, logos or fonts. **It is not a 100 % replacement for Hancom Office Hangul.** Unimplemented features and known differences are listed under [Scope and limits](#scope-and-limits) rather than hidden.

License: MIT (HwpCraft) · MIT (RHWP). See `LICENSE`, `THIRD-PARTY-NOTICES.txt`, `THIRD-PARTY-DEPENDENCIES.md` and `assets/licenses/`. Contributions: [CONTRIBUTING.md](CONTRIBUTING.md).

## What it does

- Opens and saves **HWP** (binary) and **HWPX** (XML) documents, exports **PDF** and UTF-8 text, and renders pages to PNG.
- Keeps the RHWP document model as-is instead of converting it to another format, so original objects survive a round trip as far as the engine allows.
- Provides a Hangul-style layout: menu tabs (File | Edit View Insert Format Page Security Review Tools), contextual tabs for tables, pictures and shapes, a ribbon that collapses from the right in narrow windows, format toolbar, rulers, document tabs and a status bar with zoom (10–500 %).
- Saves atomically: the file is written to a temporary location first and then swapped in, so a failed save never destroys the existing file. Failed editing commands leave the document and undo history untouched.
- Runs fully offline. No login, server, database or Docker. Documents never leave your machine.
- Exposes the same editing engine three ways: the desktop app, a CLI, and a local stdio **MCP** (Model Context Protocol) server for AI assistants.

### Editing

- Multi-paragraph selection (Shift+click, drag, Shift+arrows), Ctrl+A, double-click word / triple-click paragraph selection, overwrite mode (Insert), Windows Korean IME input.
- Cursor movement across paragraphs, Home/End, Ctrl+←/→ by word, Ctrl+↑/↓ by paragraph, Ctrl+Home/End, PageUp/PageDown.
- Copy, cut and paste that keep formatting, tables and objects inside the app; plain text to and from other programs.
- Undo/redo (40 steps, consecutive typing grouped by word).
- Find (Ctrl+F) and Replace (Ctrl+H) with case, whole-word and direction options; Replace All undoes in one step.
- Sort paragraphs, remove control marks by type, page break (Ctrl+Enter), column break (Ctrl+Shift+Enter), bookmarks, HTTP/HTTPS hyperlinks, character map, context menu.
- Hangul-style two-key shortcuts (Ctrl+K,H · Ctrl+K,N · Ctrl+K,O · Ctrl+N,P/H/T · Ctrl+G,… · Ctrl+Q,I) plus the standard format shortcuts and Ctrl+1…0.
- Tab stops are saved with widths computed the Hangul way (left, right, centre, decimal, leader) so they land in the same place when opened in Hangul.

### Formatting

- Character format (Alt+L: basic / extended / border & background) and paragraph format (Alt+T: basic / extended / tabs / border & background), with previews typeset by RHWP.
- Styles (F6): list, add, edit, delete, shortcut keys. Bullets and numbering (10 numbering presets, 16 bullet presets), outline numbering, promote / demote.
- Custom tabs, character and paragraph borders and shading. Increment steps match Hangul (1 % spacing/width, 1 pt size, 10 % line spacing, 1 pt indent/margin).

### Tables and objects

- Tables up to 200 × 64: cell blocks (F5, drag, Shift), table / cell properties, borders and backgrounds, merge / split (N×M), insert / delete rows and columns, drag grid lines, formulas and block sum / average / product, thousands separators, flip / split / join tables, text ↔ table, sort, auto-fill, captions, four table templates, cell protection.
- Cell inner margins in mm from the Table Layout tab or context menu; the horizontal ruler switches to the selected cell and its edge markers can be dragged.
- Direct manipulation: cross handle to select and move a whole table, square handles to resize (Shift keeps ratio), width / height entry.
- Table drawing and eraser tools: outer border, rows, columns, diagonals, apply line style / colour / width, erase shared borders to merge cells. Each action is one undo step.
- Pictures: PNG, JPG, BMP, GIF with size, position, wrap, rotation, flip, crop, effects, brightness, contrast, transparency and caption.
- Shapes and text boxes: line, rectangle, ellipse, arc, polygon, freeform and text box drawn by dragging; eight resize handles, rotation handle, line / fill / shadow properties, multi-paragraph text boxes.
- Equation editor with Hangul-style structure and symbol palettes and live preview, multi-paragraph footnotes and endnotes, editing of existing chart values, memo list.

### Page, view and file services

- Ctrl+wheel zoom (10–500 %) over the document, wheel-only zoom over the status-bar zoom indicator.
- Page setup (F7), page numbering (11 positions, 10 formats), headers / footers (11 templates × both / odd / even), page borders and backgrounds, restart numbering, hide current page, columns.
- Document information, print (range, copies; via PDF to the Windows print dialog or PDF file), zoom presets, fit page / fit width, show or hide rulers and document tabs.
- Continuous multi-page view like Hangul (14 px page gap), drawing only visible pages and scrolling with the caret.

### CLI and MCP

`hwpcraft-cli` analyses, converts, renders and batch-edits documents with the same engine as the desktop app:

```text
hwpcraft-cli --help
hwpcraft-cli commands                       # list editing commands
hwpcraft-cli info  input.hwp
hwpcraft-cli convert input.hwp output.hwpx
hwpcraft-cli render input.hwpx page.png 1
hwpcraft-cli edit --new commands.json output.hwpx
hwpcraft-cli mcp --help                     # local stdio MCP server
```

The `edit` command file is a JSON array such as `[{"command":"text.insert","params":{"text":"Hello"}},{"command":"page.setup","params":{"landscape":true}}]`, limited to 1 MB / 1,000 commands. The output file is written only when every command succeeds. Text offsets count Unicode scalar values.

`hwpcraft-cli mcp` is a headless stdio MCP server: create Korean documents, edit text / formatting / tables / objects, save HWP / HWPX, export PDF / TXT, render PNG for checking, and restrict read / write roots. It does not control a running desktop window or Hancom software. See [docs/mcp.md](docs/mcp.md) and [docs/mcp-commands.md](docs/mcp-commands.md) (Korean).

## Build

Tested on Windows x64 with Rust 1.98.1 (MSVC) and the Visual Studio C++ build tools. The toolchain is pinned in `rust-toolchain.toml`. Linux dependencies (X11 / Wayland) are enabled in `eframe` but Linux and macOS builds have not been verified.

HwpCraft depends on RHWP as a **sibling path dependency** (`../rhwp`, pinned to RHWP **v0.8.7**, commit `1a76570e`). Clone both repositories next to each other:

```text
git clone https://github.com/webvine/hwpcraft.git
git clone --branch v0.8.7 https://github.com/edwardkim/rhwp.git
cd hwpcraft
cargo build --release --locked -p hwpcraft -p hwpcraft-cli
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

One engine test opens a chart sample from `../rhwp/samples`, so the RHWP checkout must include its `samples` folder. On Windows, `HwpCraft.ps1` (`Build`, `Test`, `Install`, `Start`, `Status`) wraps the same commands and uses `../.build/hwpcraft` as the Cargo target directory; the `Launcher` and `Install` actions target a local launcher that is not part of this repository.

Run the desktop app:

```text
hwpcraft ./samples/HwpCraft-시작하기.hwpx
hwpcraft ./samples/HwpCraft-시작하기.hwpx --view replace --screenshot out.png --exit-after-screenshot
```

`--view` opens a dialog or ribbon tab directly for visual checks (for example `character`, `paragraph`, `table-create`, `page-setup`, `find`, `replace`, or a ribbon tab name such as `insert`).

Do not run `cargo fmt --all`: it follows the path dependency into RHWP. Format the HwpCraft packages with explicit `-p` arguments.

## Repository layout

| Path | Role |
|---|---|
| `crates/doc` | Document engine façade, Unicode cursor and table-cell addressing |
| `crates/formats` | HWP / HWPX load and save, PDF / TXT export, safe file replacement |
| `crates/render` | RHWP SVG → page bitmaps via resvg, hairline handling, installed font list |
| `crates/engine` | Session and command protocol, selection / clipboard / caret, find, sort and control marks, tab widths, formatting, tables and objects, page services |
| `crates/mcp` | stdio JSON-RPC MCP server: headless editing, tools, resources, command help |
| `crates/ui-egui` | Main window, continuous page view, keyboard / IME, ribbon, toolbars, rulers, status bar, dialogs |
| `apps/hwpcraft` | eframe / glow desktop executable |
| `apps/hwpcraft-cli` | Analysis, conversion, PNG rendering and batch-edit CLI, MCP entry point |
| `samples` | HWP / HWPX / PDF examples generated by HwpCraft itself |
| `docs` | Design notes, verification records and release notes (Korean) |

Feature areas plug in through `extensions.rs`, so each area adds its own commands, dialogs and shortcuts in its own files.

## Scope and limits

### Not implemented (disabled in the ribbon and menus)

- MCP: connecting to a running desktop window, click / key / screen capture, HTTP / SSE transport, track changes. Each MCP process is an independent headless session.
- Entire Security, Review and Tools tabs: document passwords, distribution documents, personal-information tools, proofreading marks, document history / compare, track changes, spell check, quick correction, dictionaries, translation, Hanja conversion (no dictionary data), macros, index / table of contents / citations, preferences.
- View: page outline, paragraph / control-mark display, transparent lines, grid, guides, split editing window, multiple document windows. File: preview, templates gallery, recent documents.
- Page: text direction, line numbers, section breaks, labels, manuscript paper. Print details (printer list, N-up, watermark).
- Format: style gallery import / export, custom numbering / bullet shapes, picture bullets, drop caps, section default tab spacing.
- Tables and objects: moving a single cell border, editing nested tables, connectors, vertical text boxes, group / order / align, object selection and protection buttons, picture reflection / glow / photo editing, visual equation editing and macros, new memos, new charts, WordArt, OLE.
- Tab drop-down lists are ribbon command lists, not Hangul submenus; there are no Alt key tips. The status-bar column indicator is always 1.
- Direct table dragging targets top-level tables within one page; an inline table becomes floating when moved. Multi-page tables are adjusted through properties. Table drawing supports zero cell spacing only.

### RHWP typesetting and save limits (not worked around in HwpCraft)

- Paragraph border top / bottom spacing is not reflected in line layout, so paragraphs may overlap both on screen and in Hangul. Side spacing and some line styles (double lines) draw as single lines.
- Footnote separator length −1 (column width) is drawn short, and objects with outer margins are offset on screen (saved files open correctly in Hangul).
- Highlighter is saved as shading; new numbered lists create new numbering definitions; curves are saved as dense polylines.
- Master pages, document summary, odd / even / first-page-only page borders, picture fill, new memos, chart / WordArt / OLE creation are unavailable because RHWP has no API for them.
- Automatic tab-width recording covers body text and first-level table cells only; tabs inside headers, text boxes and notes are saved with width 0.
- Multi-paragraph selection works within one section body or one cell; Select All covers the current section.

### Verification limits

- Pointer-event regression tests cover table selection, moving and resizing. End-to-end click / drag verification of every feature has not been completed.
- Round-trip saving of complex real-world documents, performance on very large documents and tables, and long IME sessions have not been tested thoroughly. Keep originals and save important documents under a new name, then check them in Hangul.
- Input documents are limited to 128 MB and a single text insertion to 1 MB. Parsing and rendering are synchronous, so the UI may pause on complex documents.

Release notes are in `docs/` (Korean).

## Acknowledgements

- [RHWP](https://github.com/edwardkim/rhwp) (MIT) — HWP / HWPX document engine.
- [Lucide](https://lucide.dev) (ISC) — ribbon icons in `assets/icons/lucide`, built into `assets/toolbar-icons.png`.
- WordCraft, DeckCraft and DesignCraft (MIT) — ribbon width planning, dialog and draft / commit patterns. Licenses are kept in `assets/` and listed in `THIRD-PARTY-NOTICES.txt`.
- [egui](https://github.com/emilk/egui) / eframe, [resvg](https://github.com/linebender/resvg), and the other crates in `Cargo.lock`.

Hancom, Hangul (한글) and HWP are trademarks of their respective owners and are used here only to describe file-format compatibility.

---

# HwpCraft (한국어)

[English](#hwpcraft) · **한국어**

MIT 라이선스의 [RHWP](https://github.com/edwardkim/rhwp) 문서 엔진과 [egui](https://github.com/emilk/egui)로 만든 Rust 네이티브 HWP/HWPX 편집기입니다.

현재 버전은 **0.12**이며, 버전은 `0.10 → 0.11 → 0.12 …`처럼 `x.xx` 형식으로 증가합니다.

HwpCraft는 독립적인 오픈소스 프로젝트이며 한글과컴퓨터와 관련이 없습니다. 한컴의 코드·아이콘·로고·글꼴은 포함하지 않습니다. **아래한글의 모든 기능·화면과 100 % 같은 제품은 아닙니다.** 구현하지 않은 기능과 차이는 [현재 범위와 한계](#현재-범위와-한계)에 숨기지 않고 적었습니다.

라이선스: HwpCraft MIT, RHWP MIT. `LICENSE`, `THIRD-PARTY-NOTICES.txt`, `THIRD-PARTY-DEPENDENCIES.md`, `assets/licenses/`를 참조하세요. 기여 방법: [CONTRIBUTING.md](CONTRIBUTING.md).

## 하는 일

- **HWP**(바이너리)·**HWPX**(XML) 문서를 열고 저장하며, **PDF**·UTF-8 텍스트로 내보내고 쪽을 PNG로 렌더링합니다.
- RHWP의 문서 모델을 다른 모델로 변환하지 않고 그대로 사용해, 엔진이 허용하는 한 원본 개체를 유지합니다.
- 한글식 배치: 메뉴 탭 줄(파일 | 편집 보기 입력 서식 쪽 보안 검토 도구), 상황 탭(표 디자인·표 레이아웃·그림·도형), 좁은 창에서 오른쪽부터 접히는 리본, 서식 도구줄, 눈금자, 문서 탭, 배율 10–500 % 상태 표시줄.
- 저장은 임시 파일에 다 쓴 뒤 교체하므로 저장 실패가 기존 파일을 망가뜨리지 않습니다. 실패한 편집 명령은 문서와 실행 취소 기록을 그대로 둡니다.
- 완전히 오프라인으로 동작합니다. 로그인·서버·데이터베이스·Docker가 필요 없고 문서는 외부로 전송되지 않습니다.
- 같은 편집 엔진을 데스크톱 앱, CLI, 로컬 stdio **MCP** 서버 세 가지로 제공합니다.

### 편집

- 여러 문단 선택(Shift+클릭·끌기·Shift+방향키), 전체 선택(Ctrl+A), 두 번 클릭 낱말·세 번 클릭 문단 선택, 수정 모드(Insert), Windows 한글 IME.
- 커서 이동: 문단을 넘는 화살표, Home/End, 낱말(Ctrl+←/→), 문단(Ctrl+↑/↓), 문서 처음/끝(Ctrl+Home/End), PageUp/PageDown.
- 복사·잘라내기·붙이기: 앱 안에서는 서식·표·개체 유지, 다른 프로그램과는 일반 텍스트.
- 실행 취소/다시 실행 40단계. 이어서 친 글자는 낱말 단위로 묶습니다.
- 찾기(Ctrl+F)·찾아 바꾸기(Ctrl+H): 대소문자, 온전한 낱말, 방향 옵션. 모두 바꾸기는 한 번에 되돌립니다.
- 정렬, 조판 부호 지우기(종류별), 쪽 나누기(Ctrl+Enter), 단 나누기(Ctrl+Shift+Enter), 책갈피, HTTP/HTTPS 하이퍼링크, 문자표, 오른쪽 클릭 메뉴.
- 한글식 접두 단축키(Ctrl+K,H · Ctrl+K,N · Ctrl+K,O · Ctrl+N,P/H/T · Ctrl+G,… · Ctrl+Q,I)와 서식 단축키, Ctrl+1…0.
- 탭은 한글 방식으로 폭을 계산해 저장하므로(왼쪽·오른쪽·가운데·소수점 탭, 채울 모양) 한글에서 열어도 같은 자리에 놓입니다.

### 서식

- 글자 모양(Alt+L: 기본/확장/테두리·배경), 문단 모양(Alt+T: 기본/확장/탭 설정/테두리·배경). 미리 보기는 RHWP로 실제 조판합니다.
- 스타일(F6) 목록·추가·편집·지우기·단축키, 글머리표 및 문단 번호(번호 10종·글머리표 16종), 개요 번호, 수준 증가·감소.
- 사용자 탭, 글자·문단 테두리와 배경. 증감 폭은 한글과 같습니다(장평·자간 1 %, 크기 1 pt, 줄 간격 10 %, 들여쓰기·여백 1 pt).

### 표·개체

- 표(최대 200줄·64칸): 셀 블록(F5·끌기·Shift), 표/셀 속성, 테두리·배경, 합치기·나누기(N×M), 줄/칸 넣기·지우기, 격자선 끌기, 계산식·블록 합계/평균/곱, 1,000 단위 쉼표, 표 뒤집기·나누기·붙이기, 문자열↔표, 정렬, 자동 채우기, 캡션, 표 마당 4종, 셀 보호.
- 셀 안 여백을 표 레이아웃 탭·우클릭 메뉴에서 mm 단위로 조절. 셀 선택 시 가로 눈금자가 해당 셀 기준으로 바뀌고 경계 표식을 끌 수 있습니다.
- 표 직접 조작: 십자 손잡이로 전체 선택·이동, 사각 손잡이로 크기 조절(Shift: 비율 유지), 너비·높이 입력.
- 표 그리기/지우개: 외곽·줄·칸·대각선, 선 모양·색·굵기 적용, 공유 경계 지우기로 셀 합치기. 각 동작은 한 번에 실행 취소합니다.
- 그림: PNG·JPG·BMP·GIF, 크기·위치·배치·회전·대칭·자르기·효과·밝기·대비·투명도·캡션.
- 도형·글상자: 직선·사각형·타원·호·다각형·자유 곡선·글상자를 끌어 그리기, 8개 핸들·회전 손잡이, 선·채우기·그림자 속성, 여러 문단 글상자.
- 수식 편집기(구조·기호 도구와 미리 보기), 여러 문단 각주·미주, 기존 차트 값 편집, 메모 목록.

### 쪽·보기·파일 서비스

- 문서 위 Ctrl+휠 확대/축소(10–500 %), 배율 표시 위에서는 휠만으로 확대/축소.
- 편집 용지(F7), 쪽 번호 매기기(위치 11종·모양 10종), 머리말/꼬리말(마당 11종 × 양쪽/홀수/짝수), 쪽 테두리/배경, 새 번호로 시작, 현재 쪽만 감추기, 다단.
- 문서 정보, 인쇄(범위·매수, PDF를 거쳐 Windows 인쇄 또는 PDF 저장), 보기 배율·쪽 맞춤·폭 맞춤, 눈금자·문서 탭 보이기.
- 한글처럼 여러 쪽을 세로로 이어 보는 연속 보기(쪽 사이 14 px). 보이는 쪽만 그리고 커서를 따라 스크롤합니다.

### CLI와 MCP

`hwpcraft-cli`는 데스크톱 앱과 같은 엔진으로 문서를 분석·변환·렌더·일괄 편집합니다.

```text
hwpcraft-cli --help
hwpcraft-cli commands                       # 편집 명령 목록
hwpcraft-cli info  input.hwp
hwpcraft-cli convert input.hwp output.hwpx
hwpcraft-cli render input.hwpx page.png 1
hwpcraft-cli edit --new commands.json output.hwpx
hwpcraft-cli mcp --help                     # 로컬 stdio MCP 서버
```

`edit`의 명령 파일은 `[{"command":"text.insert","params":{"text":"안녕하세요"}},{"command":"page.setup","params":{"landscape":true}}]` 형태의 JSON 배열이며 최대 1 MB / 1,000개 명령입니다. 전체 명령이 성공한 뒤에만 출력 파일을 저장합니다. 텍스트 위치(offset)는 유니코드 스칼라 값 단위입니다.

`hwpcraft-cli mcp`는 헤드리스 stdio MCP 서버입니다. 한글 문서 생성, 텍스트/서식/표/개체 편집, HWP/HWPX 저장, PDF/TXT 출력, PNG 확인, 읽기/쓰기 폴더 제한을 지원합니다. 실행 중인 데스크톱 창이나 한컴 프로그램은 제어하지 않습니다. [MCP 연결과 사용법](docs/mcp.md), [명령 인자](docs/mcp-commands.md)를 참조하세요.

## 빌드

Windows x64, Rust 1.98.1(MSVC), Visual Studio C++ 빌드 도구에서 검증했습니다. 툴체인은 `rust-toolchain.toml`에 고정되어 있습니다. `eframe`에 Linux(X11/Wayland) 기능이 켜져 있지만 Linux·macOS 빌드는 검증하지 않았습니다.

HwpCraft는 RHWP를 **형제 폴더 경로 의존성**(`../rhwp`, RHWP **v0.8.7**, 커밋 `1a76570e`)으로 사용합니다. 두 저장소를 같은 상위 폴더에 받으세요.

```text
git clone https://github.com/webvine/hwpcraft.git
git clone --branch v0.8.7 https://github.com/edwardkim/rhwp.git
cd hwpcraft
cargo build --release --locked -p hwpcraft -p hwpcraft-cli
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

엔진 테스트 하나가 `../rhwp/samples`의 차트 예제를 열므로 RHWP 체크아웃에 `samples` 폴더가 있어야 합니다. Windows에서는 `HwpCraft.ps1`(`Build`, `Test`, `Install`, `Start`, `Status`)이 같은 명령을 감싸며 `../.build/hwpcraft`를 Cargo 빌드 폴더로 씁니다. `Launcher`·`Install` 동작은 이 저장소에 포함되지 않은 로컬 런처를 대상으로 합니다.

데스크톱 앱 실행:

```text
hwpcraft ./samples/HwpCraft-시작하기.hwpx
hwpcraft ./samples/HwpCraft-시작하기.hwpx --view replace --screenshot out.png --exit-after-screenshot
```

`--view`는 실제 화면 확인용으로 대화상자나 리본 탭을 바로 엽니다(예: `character`, `paragraph`, `table-create`, `page-setup`, `find`, `replace`, 리본 탭 이름 `insert` 등).

`cargo fmt --all`은 경로 의존성을 따라 RHWP까지 포맷하므로 실행하지 마세요. HwpCraft 패키지는 `-p`를 명시해 포맷합니다.

## 구조

| 경로 | 역할 |
|---|---|
| `crates/doc` | 문서 엔진 노출, 유니코드 커서와 표 셀 주소 |
| `crates/formats` | HWP/HWPX 읽기·저장, PDF/TXT 내보내기, 안전한 파일 교체 |
| `crates/render` | RHWP SVG → resvg 쪽 이미지, 얇은 선 처리, 설치 글꼴 목록 |
| `crates/engine` | Session·명령 프로토콜, 선택·클립보드·커서, 찾기, 정렬·조판 부호, 탭 폭 저장, 서식, 표·개체, 쪽 서비스 |
| `crates/mcp` | stdio JSON-RPC MCP 서버, 헤드리스 편집·도구·리소스·명령 안내 |
| `crates/ui-egui` | 메인 창·연속 쪽 보기·키보드/IME, 리본·도구줄·눈금자·상태 표시줄, 영역별 대화상자 |
| `apps/hwpcraft` | eframe/glow 데스크톱 실행 파일 |
| `apps/hwpcraft-cli` | 문서 분석·변환·PNG 렌더·명령 일괄 편집 CLI, MCP 진입점 |
| `samples` | HwpCraft로 직접 생성한 HWP/HWPX/PDF 예제 |
| `docs` | 설계 노트·검증 기록·릴리스 노트 |

영역별 모듈은 `extensions.rs`를 통해 연결되어 각 영역이 자기 파일에서 명령·대화상자·단축키를 추가합니다.

## 현재 범위와 한계

### 구현하지 않음(리본·메뉴에서 비활성)

- MCP: 실행 중인 데스크톱 창 연결, 클릭/키 입력/창 캡처, HTTP/SSE, 변경 내용 추적. 각 MCP 프로세스는 독립적인 헤드리스 세션입니다.
- 보안·검토·도구 탭 전체: 문서 암호, 배포용 문서, 개인 정보, 교정 부호, 문서 이력·비교, 변경 내용 추적, 맞춤법, 빠른 교정, 사전, 번역, 한자 변환(사전 데이터 없음), 매크로, 색인·차례·인용, 환경 설정.
- 보기: 쪽 윤곽, 문단·조판 부호 보기, 투명 선, 격자, 안내선, 편집 화면 나누기, 다중 문서 창. 파일: 미리 보기, 문서마당, 최근 문서.
- 쪽: 글자 방향, 줄 번호, 구역 나누기, 라벨, 원고지. 인쇄 세부(프린터 목록, 모아 찍기, 워터마크).
- 서식: 스타일마당·가져오기/내보내기, 사용자 정의 번호/글머리표 모양, 그림 글머리표, 문단 첫 글자 장식, 구역 기본 탭 간격.
- 표·개체: 한 셀만 경계선 옮기기, 중첩 표 편집, 연결선·세로 글상자, 묶기·앞뒤 순서·맞춤, 개체 선택·개체 보호 단추, 그림 반사·네온·사진 편집, 수식 시각 편집·매크로, 새 메모·새 차트·글맵시·OLE.
- 탭 ∨ 목록은 한글 하위 메뉴가 아닌 리본 명령 목록이며 Alt 키팁이 없습니다. 상태 표시줄 '단'은 항상 1입니다.
- 표 직접 이동은 한 쪽 안의 최상위 표가 대상이며, 글자처럼 취급한 표는 이동 시 떠 있는 표로 전환됩니다. 여러 쪽에 걸친 표는 속성으로 조절하세요. 표 그리기는 셀 간격 0인 표만 지원합니다.

### RHWP 조판·저장 한계(HwpCraft에서 우회하지 않음)

- 문단 테두리 위·아래 간격이 줄 배치에 반영되지 않아 화면과 한글에서 연 저장본 모두 문단이 겹칠 수 있습니다. 좌우 간격과 일부 선 종류(이중선 등)는 단선으로 그려집니다.
- 각주 구분선 길이 −1(단 너비)은 짧게, 바깥 여백이 있는 개체는 여백만큼 다른 자리에 그립니다(저장본은 한글에서 정상).
- 형광펜은 음영으로, 새 번호 목록은 새 번호 정의로 저장합니다. 곡선은 촘촘한 직선 구간으로 저장합니다.
- 바탕쪽, 문서 요약 정보, 쪽 테두리의 홀수/짝수·첫 쪽만·그림 채우기, 새 메모, 차트·글맵시·OLE 생성은 RHWP API가 없어 지원하지 않습니다.
- 탭 폭 자동 기록은 본문과 1단계 표 셀만 대상입니다. 머리말·글상자·주석 안의 탭은 폭 0으로 저장됩니다.
- 여러 문단 선택은 한 구역 본문 또는 한 셀 안에서만 되며, 전체 선택은 현재 구역입니다.

### 검증 한계

- 표 선택·이동·크기 조절에는 포인터 이벤트 회귀 검사가 있습니다. 앱 전체 기능의 클릭·끌기 E2E 검증을 완료한 것은 아닙니다.
- 복잡한 실제 문서의 편집 후 왕복 저장, 대용량 문서·표 성능, 장시간 IME 입력은 충분히 검사하지 않았습니다. 중요한 문서는 원본을 유지하고 다른 이름으로 저장한 뒤 한글에서도 확인하세요.
- 문서 입력 크기는 128 MB, 한 번의 텍스트 삽입은 1 MB로 제한합니다. 파싱·렌더링이 동기식이라 복잡한 문서에서 UI가 잠시 멈출 수 있습니다.

릴리스 노트는 `docs/`에 있습니다.

## 감사의 글

- [RHWP](https://github.com/edwardkim/rhwp) (MIT) — HWP/HWPX 문서 엔진.
- [Lucide](https://lucide.dev) (ISC) — `assets/icons/lucide`의 리본 아이콘. `assets/toolbar-icons.png`로 조합합니다.
- WordCraft, DeckCraft, DesignCraft (MIT) — 리본 너비 계획, 대화상자, 초안/확정 패턴. 라이선스는 `assets/`와 `THIRD-PARTY-NOTICES.txt`에 보존합니다.
- [egui](https://github.com/emilk/egui)/eframe, [resvg](https://github.com/linebender/resvg) 및 `Cargo.lock`의 크레이트들.

한글과컴퓨터, 한글(Hangul), HWP는 각 권리자의 상표이며 파일 형식 호환성을 설명하기 위해서만 언급합니다.
