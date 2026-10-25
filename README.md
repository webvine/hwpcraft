# HwpCraft

**한국어** · [English](#hwpcraft-english)

ArtCraft 팀의 [WordCraft](https://github.com/storytold/wordcraft) 기본 구조 위에, [RHWP](https://github.com/edwardkim/rhwp) 문서 엔진의 아래한글 편집 기능과 [egui](https://github.com/emilk/egui) 화면을 결합해 만든 Rust 네이티브 HWP/HWPX 편집기입니다. 두 프로젝트는 모두 MIT 라이선스입니다.

현재 버전은 **0.12**이며, 버전은 `0.10 → 0.11 → 0.12 …`처럼 `x.xx` 형식으로 증가합니다.

HwpCraft는 독립적인 오픈소스 프로젝트이며 한글과컴퓨터와 관련이 없습니다. 한컴의 코드·아이콘·로고·글꼴은 포함하지 않습니다. **아래한글의 모든 기능·화면과 100 % 같은 제품은 아닙니다.** 구현하지 않은 기능과 차이는 [현재 범위와 한계](#현재-범위와-한계)에 숨기지 않고 적었습니다.

라이선스: **MIT** ([LICENSE](LICENSE)). 적용 범위와 제3자 구성 요소는 [라이선스](#라이선스) 절을 참조하세요. 기여 방법: [CONTRIBUTING.md](CONTRIBUTING.md).

## 최종 목표

HwpCraft의 최종 목표는 **AI가 MCP를 통해 아래한글 문서를 작성하고, 보고서·리포트로 완성해 출력하는 일을 사람 손 없이 끝까지 해내는 것**입니다. 데스크톱 편집기는 사람이 결과를 검토하고 손보는 자리이고, 자동화의 중심은 `hwpcraft-cli mcp`가 제공하는 MCP 서버입니다.

이를 위해 다음을 최대한 완성합니다.

- **문서 작성·편집의 전 과정을 MCP 명령으로 노출** — 새 문서, 텍스트 입력, 글자·문단 모양, 스타일, 표, 그림, 차트, 수식, 각주·미주, 머리말·꼬리말, 쪽 번호, 다단, 책갈피, 하이퍼링크, 찾기/바꾸기, 한자 변환, 상용구, 문서 비교까지 데스크톱에서 할 수 있는 편집은 모두 에이전트도 할 수 있어야 합니다. 아래한글 자동화 동작 이름(`hwp_action`)을 그대로 받아 기존 매크로·스크립트 지식을 재사용합니다.
- **보고서·리포트 산출에 필요한 요소를 갖춤** — 표지, 목차, 제목 체계, 번호 매기기, 표와 그림 캡션, 참조, 쪽 설정과 인쇄용 레이아웃을 명령으로 조립할 수 있게 하고, 서식 템플릿과 스타일을 적용해 일관된 문서를 만듭니다.
- **출력과 검증을 자동화** — HWP/HWPX 저장, PDF 출력, PNG 렌더로 결과를 확인하고, 일괄 명령(`batch`)이 하나라도 실패하면 문서를 되돌려 깨진 산출물을 남기지 않습니다.
- **아래한글과의 호환성** — RHWP 엔진 위에서 아래한글이 읽고 쓰는 그대로 저장하며, 지원하지 않는 기능은 숨기지 않고 [현재 범위와 한계](#현재-범위와-한계)에 적습니다.

진행 상황은 [하는 일](#하는-일)과 릴리스 노트(`docs/`)에, 남은 항목은 한계 절에 기록합니다.

## 구성 원리

HwpCraft는 두 오픈소스 프로젝트를 결합해 구현했습니다. 어느 부분이 어디서 왔는지 분명히 적습니다.

- **ArtCraft의 기본 구조** — storytold의 ArtCraft 팀이 만든 [WordCraft](https://github.com/storytold/wordcraft)(MIT)의 크레이트 계층(`doc` → `formats` → `render` → `engine` → `mcp` / `ui-egui` → `apps`)을 그대로 따랐습니다. 리본 너비 계획(`crates/ui-egui/src/ribbon_layout.rs`), 안전한 파일 교체(`crates/formats/src/file_access.rs`), MCP 서버의 생애주기·도구·세션 구조는 WordCraft에서 가져왔고, 화면 테마 토큰은 DesignCraft, 대화상자의 초안/확정 패턴은 DeckCraft를 따랐습니다. 앱의 뼈대와 작업 흐름은 ArtCraft 계열 프로젝트의 것입니다.
- **RHWP의 아래한글 편집 기능** — [RHWP](https://github.com/edwardkim/rhwp)(MIT)의 HWP/HWPX 파서와 문서 모델, 조판, 저장, SVG 렌더링, PDF 내보내기, 그리고 글자·문단 모양, 표, 개체, 각주·미주, 머리말·꼬리말, 쪽 번호, 다단, 책갈피, 하이퍼링크, 문서 암호, 개인 정보 검색 같은 편집 API를 그대로 호출합니다. HwpCraft는 RHWP의 문서 모델을 WordCraft의 DOCX 모델로 바꾸지 않고 유지합니다. 문서를 읽고 고치고 저장하는 능력은 RHWP의 것입니다.
- **HwpCraft가 직접 구현한 것** — 두 프로젝트를 잇는 `Session`과 명령 프로토콜(유니코드 스칼라 오프셋, 표 셀 주소, 선택·클립보드·커서, 실행 취소 트랜잭션), RHWP API를 아래한글 명령 체계로 묶은 영역별 명령 모듈, 아래한글식 리본·대화상자·단축키·IME 처리, 찾기/바꾸기·정렬·조판 부호 지우기·문자표처럼 RHWP에 없는 기능, 그리고 CLI와 MCP 노출입니다.

한글과컴퓨터의 코드·리소스는 어느 층에도 쓰지 않았습니다. 아래한글은 사용자가 기대하는 동작과 화면 배치의 참고 기준일 뿐 코드의 출처가 아닙니다.

## 하는 일

- **HWP**(바이너리)·**HWPX**(XML) 문서를 열고 저장하며, **PDF**·UTF-8 텍스트로 내보내고 쪽을 PNG로 렌더링합니다.
- RHWP의 문서 모델을 다른 모델로 변환하지 않고 그대로 사용해, 엔진이 허용하는 한 원본 개체를 유지합니다.
- 한글식 배치: 메뉴 탭 줄(파일 | 편집 보기 입력 서식 쪽 보안 검토 도구), 상황 탭(표 디자인·표 레이아웃·그림·도형), 좁은 창에서 오른쪽부터 접히는 리본, 서식 도구줄, 눈금자, 문서 탭, 배율 10–500 % 상태 표시줄.
- 저장은 임시 파일에 다 쓴 뒤 교체하므로 저장 실패가 기존 파일을 망가뜨리지 않습니다. 실패한 편집 명령은 문서와 실행 취소 기록을 그대로 둡니다.
- 완전히 오프라인으로 동작합니다. 로그인·서버·데이터베이스·Docker가 필요 없고 문서는 외부로 전송되지 않습니다.
- 같은 편집 엔진을 데스크톱 앱, CLI, 로컬 stdio **MCP** 서버 세 가지로 제공합니다.

### 편집

- 여러 문단 선택(Shift+클릭·끌기·Shift+방향키), 전체 선택(Ctrl+A), 두 번 클릭 낱말·세 번 클릭 문단 선택, 수정 모드(Insert), Windows 한글 IME.
- 커서 이동: 문단을 넘는 화살표, Home/End, 낱말(Ctrl+←/→), 앞/다음 문단 처음(Ctrl+↑/↓), 문서 처음/끝(Ctrl+Home/End), PageUp/PageDown.
- 한글 글쇠 동작(0.14): Shift+Enter 줄 나눔(문단 안), Tab, Ctrl+T·Ctrl+Delete 한 단어 지우기, Ctrl+Backspace 낱말 앞 지우기, Ctrl+Y 한 줄 지우기(다시 실행은 Ctrl+Shift+Z), Alt+Y 이후 지우기, Ctrl+Alt+Space 묶음 빈 칸, Alt+Space 고정폭 빈 칸, Ctrl+Shift+- 하이픈. 같은 동작을 엔진 명령으로도 제공합니다: `caret.move`(22가지 이동, 선택 확장), `select.all/word/paragraph/line/clear/get`, `edit.copy/cut/paste`(헤드리스에서도 표·개체 포함), `edit.clipboard.get`, `text.delete_word/_back`, `text.delete_line`, `text.delete_to_line_end`, `insert.line_break/tab/nbsp/fixed_width_space/soft_hyphen`, `insert.auto_text`(날짜·시각·쪽 번호·전체 쪽수·파일 이름·사용자 이름), `edit.overwrite`, `edit.goto`(쪽·구역·문단·책갈피), `hyperlink.jump/next/prev`, `edit.find_all`, `document.statistics`, `document.outline`.
- 복사·잘라내기·붙이기: 앱 안에서는 서식·표·개체 유지. 다른 프로그램과는 **HTML**로 주고받아 굵게·색·문단이 살아남고, HTML이 없거나 해석에 실패하면 일반 텍스트로 붙입니다.
- 암호 문서: 암호가 걸린 HWP/HWPX/HWP3를 열면 암호를 묻고, 연 문서는 같은 암호로 저장합니다. 보안 탭 **문서 암호**로 암호를 걸거나 바꾸거나 풉니다. CLI는 `--password <암호>`.
- 실행 취소/다시 실행 40단계. 이어서 친 글자는 낱말 단위로 묶습니다.
- 찾기(Ctrl+F)·찾아 바꾸기(Ctrl+H): 대소문자, 온전한 낱말, 방향 옵션. 모두 바꾸기는 한 번에 되돌립니다.
- 정렬, 조판 부호 지우기(종류별), 쪽 나누기(Ctrl+Enter), 단 나누기(Ctrl+Shift+Enter), 책갈피, HTTP/HTTPS 하이퍼링크 넣기·고치기·지우기, 오른쪽 클릭 메뉴.
- 문자표(Ctrl+F10): 한글의 네 가지 표 — 사용자 문자표(등록 문자), 유니코드 문자표(유니코드 15 블록 160여 개), 한글(HNC) 문자표(일반 기호·수학·단위·원 문자·괄호 문자·괘선·도형·화살표·딩뱃·그리스·키릴·가나·로마 숫자/분수·첨자·영문 서체·한글 자모(옛한글 포함)·발음 기호·전각/반각·한중일 구두점/단위·한자 부수·주역 괘·점자·음악·통화·기술 기호 30개 영역), 완성형(KS X 1001) 문자표(특수 문자 행·한글 2,350자·한자 4,888자) — 에 더해 이모지 10개 영역, 이름 있는 특수 문자(공백·줄표·표식), 이름·`U+`코드·키워드 검색을 제공합니다. 여러 문자 입력 상자, 최근 사용 문자, 넣을 문자의 글꼴 선택, 유니코드·완성형 코드 표시, 설치된 기호·이모지 글꼴 활용. 같은 목록을 CLI/MCP(`symbol.sets`, `symbol.list`, `symbol.search`, `insert.symbol`)에서도 씁니다.
- 한글식 접두 단축키(Ctrl+K,H · Ctrl+K,N · Ctrl+K,O · Ctrl+N,P/H/T · Ctrl+G,… · Ctrl+Q,I)와 서식 단축키, Ctrl+1…0.
- 탭은 한글 방식으로 폭을 계산해 저장하므로(왼쪽·오른쪽·가운데·소수점 탭, 채울 모양) 한글에서 열어도 같은 자리에 놓입니다.

### 서식

- 글자 모양(Alt+L: 기본/확장/테두리·배경), 문단 모양(Alt+T: 기본/확장/탭 설정/테두리·배경). 미리 보기는 RHWP로 실제 조판합니다.
- 스타일(F6) 목록·추가·편집·지우기·단축키, 글머리표 및 문단 번호(번호 10종·글머리표 16종), 개요 번호, 수준 증가·감소.
- 사용자 탭, 글자·문단 테두리와 배경. 증감 폭은 한글과 같습니다(장평·자간 1 %, 크기 1 pt, 줄 간격 10 %, 들여쓰기·여백 1 pt).
- 형광펜은 RHWP markpen으로 실제 저장되어 HWP/HWPX 왕복이 되며, 화면 표시는 HwpCraft가 직접 그립니다. 모양 복사는 글자+문단을 한 번의 실행 취소 단계로 적용합니다(Alt+C). 머리말/꼬리말·각주 안 글자/문단 모양은 엔진 명령(`format.hf.*`, `format.footnote.paragraph`)으로 지정합니다.

### 표·개체

- 표(최대 200줄·64칸): 셀 블록(F5·끌기·Shift), 표/셀 속성, 테두리·배경, 합치기·나누기(N×M), 줄/칸 넣기·지우기, 격자선 끌기, 계산식·블록 합계/평균/곱, 1,000 단위 쉼표, 표 뒤집기·나누기·붙이기, 문자열↔표, 정렬, 자동 채우기, 캡션, 표 마당 4종, 셀 보호.
- 셀 안 여백을 표 레이아웃 탭·우클릭 메뉴에서 mm 단위로 조절. 셀 선택 시 가로 눈금자가 해당 셀 기준으로 바뀌고 경계 표식을 끌 수 있습니다.
- 표 직접 조작: 십자 손잡이로 전체 선택·이동, 사각 손잡이로 크기 조절(Shift: 비율 유지), 너비·높이 입력.
- 표 그리기/지우개: 외곽·줄·칸·대각선, 선 모양·색·굵기 적용, 공유 경계 지우기로 셀 합치기. 각 동작은 한 번에 실행 취소합니다.
- 그림: PNG·JPG·BMP·GIF, 크기·위치·배치·회전·대칭·자르기·효과·밝기·대비·투명도·캡션.
- 도형·글상자: 직선·사각형·타원·호·다각형·자유 곡선·글상자를 끌어 그리기, 8개 핸들·회전 손잡이, 선·채우기·그림자 속성, 여러 문단 글상자.
- 수식 편집기(구조·기호 도구와 미리 보기), 여러 문단 각주·미주, 기존 차트 값 편집, 메모 목록.
- 개체 조작(0.13): 묶기/풀기(Shift+클릭 다중 선택), 맨 앞/맨 뒤/앞/뒤 순서, 개체 보호(보호된 개체는 끌기·삭제·편집 거부)와 보호 모두 해제, 가로/세로 대칭, 연결선 6종(도형→도형 끌기, 도형 이동 시 재계산), 직선 끝점 손잡이, 누름틀 넣기와 이름·값 편집, 기존 양식 개체 값 편집.

### 쪽·보기·파일 서비스

- 문서 위 Ctrl+휠 확대/축소(10–500 %), 배율 표시 위에서는 휠만으로 확대/축소.
- 편집 용지(F7), 쪽 번호 매기기(위치 11종·모양 10종), 머리말/꼬리말(마당 11종 × 양쪽/홀수/짝수), 쪽 테두리/배경, 새 번호로 시작, 현재 쪽만 감추기, 다단.
- 문서 정보, 인쇄(범위·매수, PDF를 거쳐 Windows 인쇄 동사·macOS `open`·Linux `xdg-open` 또는 PDF 저장), 보기 배율·쪽 맞춤·폭 맞춤, 눈금자·문서 탭 보이기.
- 한글처럼 여러 쪽을 세로로 이어 보는 연속 보기(쪽 사이 14 px). 보이는 쪽만 그리고 커서를 따라 스크롤합니다.
- 0.13: 보기 탭 문단 부호·조판 부호·투명 선, 보안 탭 문서 암호와 *개인 정보 찾아서 보호*(주민등록번호·카드·전화·전자우편·숨은 글자, 선택 값 가리기 1단계 실행 취소), 머리말/꼬리말 쪽 번호·전체 쪽수·파일 이름 필드, 파일 메뉴 미리 보기·최근 문서·HML로 저장(HML 원본만)·쪽 범위 저장, 구역 설정, 탭·리본 Alt 키팁.

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

`hwpcraft-cli mcp`는 헤드리스 stdio MCP 서버입니다. 한글 문서 생성, 텍스트/서식/표/개체 편집, HWP/HWPX 저장, PDF/TXT 출력, PNG 확인, 읽기/쓰기 폴더 제한을 지원합니다. 0.14: `hwp_action` 도구가 한글 자동화 동작 이름(`MoveLineEnd`, `CharShapeBold`, `DeleteWord` 등 300여 개)을 그대로 받아 대응 명령을 실행하고, `batch {atomic:true}`는 하나라도 실패하면 문서 변경을 모두 되돌리며, `get_selection`이 선택 영역을 돌려줍니다. 명령 인자 도움말은 엔진 모듈의 `PARAMS` 표에서 나옵니다. 실행 중인 데스크톱 창이나 한컴 프로그램은 제어하지 않습니다. [MCP 연결과 사용법](docs/mcp.md), [명령 인자](docs/mcp-commands.md)를 참조하세요.

## 빌드

Rust 1.98.1(`rust-toolchain.toml`에 고정). Windows x64는 MSVC와 Visual Studio C++ 빌드 도구, macOS는 Xcode 명령줄 도구, Linux는 C 컴파일러와 X11/Wayland·OpenGL·GTK3(파일 대화상자) 개발 패키지가 필요합니다. `HwpCraft.ps1`(Windows)과 `hwpcraft.sh`(macOS·Linux: `build`, `test`, `install`, `start`, `status`)가 같은 Cargo 명령을 감쌉니다.

Windows 설치 파일: `scripts/build-installer.ps1`이 `HwpCraft.ps1 Install`과 같은 파일을 모아 Inno Setup 6으로 `packaging/windows/HwpCraft.iss`를 컴파일해 `output/installer/HwpCraft-<버전>-windows-x64-setup.exe`를 만듭니다(바탕 화면 바로 가기·`.hwp`/`.hwpx` 연결은 선택).

플랫폼 현황(0.13): 글꼴은 OS별 한글 글꼴 후보(Windows 맑은 고딕, macOS Apple SD Gothic Neo, Linux Noto Sans CJK KR·나눔고딕 등) 중 설치된 첫 글꼴을 쓰고, 인쇄와 설정 폴더도 OS별로 나눕니다. Windows x64는 전체 검증, Linux x86_64는 `cargo check --workspace --all-targets`만 통과했습니다. macOS는 이 Windows 호스트에 Apple 교차 C 컴파일러가 없어 RHWP 의존성(`blake3`) 빌드 스크립트에서 검사가 멈췄으므로 실제 Mac에서 빌드해 확인해야 합니다. Launcher 등록·portable ZIP 패키징은 Windows 스크립트에만 있습니다.

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
- 보안 탭: 배포용 문서, 선택 글자 보호, 보안 설정, 보안 종류 선택(RHWP는 한 가지). 검토·도구 탭 전체: 교정 부호, 문서 이력·비교, 변경 내용 추적, 맞춤법, 빠른 교정, 사전, 번역, 한자 변환(사전 데이터 없음), 매크로, 색인·차례·인용, 환경 설정.
- 보기: 쪽 윤곽, 격자, 안내선, 문서 보기 색, 편집 화면 나누기, 다중 문서 창(RHWP 렌더 모드 없음). 파일: 문서마당, HML 원본이 아닌 문서의 HML 저장.
- 쪽: 글자 방향, 줄 번호, 구역 시작 위치, 라벨, 원고지, 머리말/꼬리말 날짜 필드, 쪽 번호 홀/짝 분리. 인쇄 세부(프린터 목록, 모아 찍기, 워터마크).
- 서식: 스타일마당·가져오기/내보내기, 사용자 정의 번호/글머리표 모양, 그림 글머리표, 문단 첫 글자 장식, 각주 안 글자 모양(문단 모양만 가능).
- 표·개체: 한 셀만 경계선 옮기기, 중첩 표 편집, 세로 글상자, 맞춤, 개체 선택 단추(개체는 쪽에서 직접 클릭), 그림 반사·네온·사진 편집, 수식 시각 편집·매크로, 새 메모, 양식 개체 새로 만들기(기존 개체 값 편집만), 새 차트·글맵시·OLE. 연결선 부착 좌표는 종이 기준 개체에 정확하고 문단 기준 개체는 어긋날 수 있으며, 직선 끝점 편집은 글자처럼 취급한 직선·연결선을 제외합니다.
- 탭 ∨ 목록은 한글 하위 메뉴가 아닌 리본 명령 목록입니다. 상태 표시줄 '단'은 항상 1입니다.
- 표 직접 이동은 한 쪽 안의 최상위 표가 대상이며, 글자처럼 취급한 표는 이동 시 떠 있는 표로 전환됩니다. 여러 쪽에 걸친 표는 속성으로 조절하세요. 표 그리기는 셀 간격 0인 표만 지원합니다.

### RHWP 조판·저장 한계(HwpCraft에서 우회하지 않음)

- 문단 테두리 위·아래 간격이 줄 배치에 반영되지 않아 화면과 한글에서 연 저장본 모두 문단이 겹칠 수 있습니다. 좌우 간격과 일부 선 종류(이중선 등)는 단선으로 그려집니다.
- 각주 구분선 길이 −1(단 너비)은 짧게, 바깥 여백이 있는 개체는 여백만큼 다른 자리에 그립니다(저장본은 한글에서 정상).
- 형광펜은 markpen으로 저장되지만 RHWP 렌더러가 그리지 않아 HwpCraft가 화면에만 덧그립니다(인쇄·PDF에는 없음). 새 번호 목록은 새 번호 정의로 저장합니다(RHWP `set_numbering_restart`는 저장되지 않음). 곡선은 촘촘한 직선 구간으로 저장합니다.
- 본문에 넣은 쪽 번호·전체 쪽수 자동 번호(`insert.auto_text`)는 저장되고 한글에서 번호로 표시되지만, RHWP 렌더러가 본문 자동 번호를 그리지 않아 HwpCraft 화면과 PDF에서는 빈칸입니다. 한글의 날짜 코드 필드는 RHWP에 없어 현재 날짜 글로 넣습니다.
- 쪽 복사/지우기는 문단 단위입니다(앞 쪽에서 이어진 문단은 제외, 한 문단이 쪽 전체면 지우기 거부). 끼워 넣기는 파일 메뉴에서만(Ctrl+O는 불러오기). PDF 인쇄 옵션(형광펜·그리기 개체·그림·메모 포함)·쪽 윤곽 끄기·격자·줄 번호·바탕쪽 편집·문서 이력은 RHWP에 옵션·모델이 없어 제공하지 않습니다. 맞쪽 보기는 쪽 배치만 바꾸며 쪽 번호 안/바깥 구분은 없습니다.
- 상용구는 글 상용구만 지원하며 서식·표·개체가 든 본문 상용구는 저장하지 않습니다(그룹·가져오기/내보내기 없음).
- 한자 변환은 Unicode Unihan 독음표로 음절마다 후보를 고르는 방식이며 낱말 사전이 없어 아래한글처럼 낱말 단위 후보·빈도순 정렬은 하지 않습니다. 변경 내용 추적·메모 새로 넣기·맞춤법 검사는 RHWP에 모델·사전이 없어 제공하지 않습니다. 문서 비교는 추가·삭제·변경 문단 목록으로 보여 주며 나란히 보기는 없습니다. 음영 없음은 RHWP가 흰색(#ffffff)으로 보고합니다.
- 바탕쪽, 문서 요약 정보, 쪽 테두리의 홀수/짝수·첫 쪽만·그림 채우기, 새 메모(MemoShape 미지원), 양식 개체 생성, 차트·글맵시·OLE 생성은 RHWP API가 없어 지원하지 않습니다. 스타일 추가·고치기·지우기는 RHWP wasm 래퍼에만 있어 HwpCraft가 모델을 직접 고칩니다. 줄 단위 커서 이동·선택 영역 좌표 조회는 RHWP 비공개라 캐럿 좌표로 계산합니다.
- 탭 폭 자동 기록은 본문과 1단계 표 셀만 대상입니다. 머리말·글상자·주석 안의 탭은 폭 0으로 저장됩니다.
- 여러 문단 선택은 한 구역 본문 또는 한 셀 안에서만 되며, 전체 선택은 현재 구역입니다.

### 검증 한계

- 표 선택·이동·크기 조절에는 포인터 이벤트 회귀 검사가 있습니다. 앱 전체 기능의 클릭·끌기 E2E 검증을 완료한 것은 아닙니다.
- 복잡한 실제 문서의 편집 후 왕복 저장, 대용량 문서·표 성능, 장시간 IME 입력은 충분히 검사하지 않았습니다. 중요한 문서는 원본을 유지하고 다른 이름으로 저장한 뒤 한글에서도 확인하세요.
- 문서 입력 크기는 128 MB, 한 번의 텍스트 삽입은 1 MB로 제한합니다. 파싱·렌더링이 동기식이라 복잡한 문서에서 UI가 잠시 멈출 수 있습니다.

릴리스 노트는 `docs/`에 있습니다.

## 라이선스

HwpCraft는 **MIT 라이선스**로 배포합니다. 전문은 [LICENSE](LICENSE)에 있으며, 저작권자는 HwpCraft 기여자들입니다.

**MIT 라이선스가 적용되는 범위** — 이 저장소에 들어 있는 HwpCraft의 모든 산출물입니다.

- 소스 코드: `crates/`, `apps/`의 Rust 코드와 빌드·패키징 스크립트(`HwpCraft.ps1`).
- 문서와 예제: `README.md`, `docs/`, `CONTRIBUTING.md`, `samples/`의 HWP/HWPX/PDF 예제 문서.
- HwpCraft가 직접 만든 자산: 앱 아이콘 `assets/icon.svg`, 리본 아이콘 아틀라스 `assets/toolbar-icons.png`(Lucide 아이콘을 조합한 결과물).
- 기여물: 풀 리퀘스트로 들어오는 모든 변경은 MIT로 제공되며 DCO 서명을 요구합니다([CONTRIBUTING.md](CONTRIBUTING.md)).

**제3자 구성 요소** — 각자의 라이선스를 그대로 따르며 MIT로 재라이선스하지 않습니다. 전문은 `assets/licenses/`에, 목록은 `THIRD-PARTY-NOTICES.txt`에 있습니다.

| 구성 요소 | 라이선스 | 위치 |
|---|---|---|
| [RHWP](https://github.com/edwardkim/rhwp) 문서 엔진 (Edward Kim) | MIT | 별도 저장소(`../rhwp`), 바이너리에 링크 |
| WordCraft · DeckCraft · DesignCraft에서 가져온 코드 (ArtCraft 팀) | MIT | `ribbon_layout.rs`, `file_access.rs`, `chrome.rs` 등 파일 머리에 표시 |
| [Lucide](https://lucide.dev) 아이콘 | ISC | `assets/icons/lucide/` |
| egui 기본 글꼴: Hack, Noto Emoji, Ubuntu, emoji-icon-font | MIT, OFL 1.1, UFL 1.0, MIT | 바이너리에 포함 |
| Rust 크레이트 의존성 (`Cargo.lock`) | MIT, Apache-2.0, BSD, ISC 등 허용적 라이선스 | `THIRD-PARTY-DEPENDENCIES.md` |

의존성 중 라이선스를 선택할 수 있는 크레이트는 MIT 또는 Apache-2.0으로 사용합니다. GPL이나 AGPL만으로 배포되는 코드는 없습니다.

**포함하지 않는 것** — 한글과컴퓨터의 프로그램 코드, 아이콘, 로고, 글꼴, 서식, 클립아트는 소스와 바이너리 어디에도 없습니다. "한글", "한컴", "HWP", "HWPX"는 각 권리자의 상표이며 MIT 라이선스는 상표 사용권을 부여하지 않습니다. 이 저장소에서는 파일 형식 호환성을 설명하기 위해서만 언급합니다.

바이너리를 재배포할 때는 `LICENSE`, `THIRD-PARTY-NOTICES.txt`, `assets/licenses/`의 라이선스 전문을 함께 배포하세요. 패키징 스크립트가 이를 자동으로 복사합니다.

## 감사의 글

- [RHWP](https://github.com/edwardkim/rhwp) (MIT) — HWP/HWPX 문서 엔진.
- [Lucide](https://lucide.dev) (ISC) — `assets/icons/lucide`의 리본 아이콘. `assets/toolbar-icons.png`로 조합합니다.
- [WordCraft](https://github.com/storytold/wordcraft), DeckCraft, DesignCraft (MIT, ArtCraft 팀) — 크레이트 구조, 리본 너비 계획, 파일 교체, MCP 구조, 대화상자 초안/확정 패턴. 라이선스는 `assets/licenses/`와 `THIRD-PARTY-NOTICES.txt`에 보존합니다.
- [egui](https://github.com/emilk/egui)/eframe, [resvg](https://github.com/linebender/resvg) 및 `Cargo.lock`의 크레이트들.

한글과컴퓨터, 한글(Hangul), HWP는 각 권리자의 상표이며 파일 형식 호환성을 설명하기 위해서만 언급합니다.

---

# HwpCraft (English)

[한국어](#hwpcraft) · **English**

A native Rust desktop editor for Korean HWP / HWPX word-processor documents. It takes its application structure from the ArtCraft team's [WordCraft](https://github.com/storytold/wordcraft), its Hangul editing capabilities from the [RHWP](https://github.com/edwardkim/rhwp) document engine, and its user interface from [egui](https://github.com/emilk/egui). Both WordCraft and RHWP are MIT-licensed.

Current version: **0.12**. Versions increase as `0.10 → 0.11 → 0.12 …`.

HwpCraft is an independent open-source project. It is not affiliated with, endorsed by, or derived from Hancom Inc. It ships no Hancom code, icons, logos or fonts. **It is not a 100 % replacement for Hancom Office Hangul.** Unimplemented features and known differences are listed under [Scope and limits](#scope-and-limits) rather than hidden.

License: **MIT** ([LICENSE](LICENSE)). See the [License](#license) section for scope and third-party components. Contributions: [CONTRIBUTING.md](CONTRIBUTING.md).

## Final goal

The final goal of HwpCraft is for **an AI agent to write Hangul (HWP / HWPX) documents through MCP and finish them as reports, exported and ready to deliver, without a person at the keyboard**. The desktop editor is where people review and touch up the result; the centre of automation is the MCP server provided by `hwpcraft-cli mcp`.

To get there, HwpCraft aims to complete the following as fully as possible.

- **Expose the whole writing and editing workflow as MCP commands** — new documents, text input, character and paragraph shapes, styles, tables, pictures, charts, equations, footnotes and endnotes, headers and footers, page numbers, columns, bookmarks, hyperlinks, find / replace, hanja conversion, autotext and document comparison: everything the desktop can edit, an agent must be able to edit too. Hancom Office Hangul automation action names are accepted as they are (`hwp_action`) so existing macro and script knowledge carries over.
- **Provide the building blocks of reports** — cover pages, tables of contents, heading hierarchies, numbering, table and figure captions, references, page setup and print layout, assembled by commands, with templates and styles for consistent documents.
- **Automate output and verification** — save as HWP / HWPX, export PDF, render PNG to check the result, and roll the document back when any step of a `batch` fails so that no broken output is left behind.
- **Stay compatible with Hancom Office Hangul** — save exactly what Hangul reads and writes, on top of the RHWP engine, and record unsupported features honestly under [Scope and limits](#scope-and-limits) instead of hiding them.

Progress is tracked under [What it does](#what-it-does) and in the release notes (`docs/`); remaining items live in the limits section.

## How HwpCraft is built

HwpCraft combines two open-source projects. This section states which part comes from where.

- **Application structure from ArtCraft** — HwpCraft follows the crate layering of [WordCraft](https://github.com/storytold/wordcraft) (MIT) by storytold's ArtCraft team: `doc` → `formats` → `render` → `engine` → `mcp` / `ui-egui` → `apps`. Ribbon width planning (`crates/ui-egui/src/ribbon_layout.rs`), safe file replacement (`crates/formats/src/file_access.rs`) and the MCP server's lifecycle / tools / session structure are adapted from WordCraft; the theme tokens follow DesignCraft and the dialog draft / commit pattern follows DeckCraft. The skeleton and workflow of the application belong to the ArtCraft family.
- **Hangul editing capabilities from RHWP** — HwpCraft calls [RHWP](https://github.com/edwardkim/rhwp) (MIT) directly for the HWP / HWPX parser and document model, typesetting, saving, SVG rendering, PDF export, and editing APIs such as character and paragraph shapes, tables, objects, footnotes and endnotes, headers and footers, page numbers, columns, bookmarks, hyperlinks, document passwords and privacy scanning. RHWP's document model is kept as is rather than converted into WordCraft's DOCX model. The ability to read, edit and save documents belongs to RHWP.
- **What HwpCraft implements itself** — the `Session` and command protocol that join the two (Unicode scalar offsets, table-cell addressing, selection / clipboard / caret, undo transactions), the per-area command modules that group RHWP APIs into Hangul-style commands, the Hangul-style ribbon, dialogs, shortcuts and IME handling, features RHWP does not provide (find / replace, sort, control-mark removal, character map), and the CLI and MCP surfaces.

No Hancom code or resources are used in any layer. Hancom Office Hangul is the reference for the behaviour and layout users expect, not a source of code.

## What it does

- Opens and saves **HWP** (binary) and **HWPX** (XML) documents, exports **PDF** and UTF-8 text, and renders pages to PNG.
- Keeps the RHWP document model as-is instead of converting it to another format, so original objects survive a round trip as far as the engine allows.
- Provides a Hangul-style layout: menu tabs (File | Edit View Insert Format Page Security Review Tools), contextual tabs for tables, pictures and shapes, a ribbon that collapses from the right in narrow windows, format toolbar, rulers, document tabs and a status bar with zoom (10–500 %).
- Saves atomically: the file is written to a temporary location first and then swapped in, so a failed save never destroys the existing file. Failed editing commands leave the document and undo history untouched.
- Runs fully offline. No login, server, database or Docker. Documents never leave your machine.
- Exposes the same editing engine three ways: the desktop app, a CLI, and a local stdio **MCP** (Model Context Protocol) server for AI assistants.

### Editing

- Multi-paragraph selection (Shift+click, drag, Shift+arrows), Ctrl+A, double-click word / triple-click paragraph selection, overwrite mode (Insert), Windows Korean IME input.
- Cursor movement across paragraphs, Home/End, Ctrl+←/→ by word, Ctrl+↑/↓ to the previous / next paragraph start, Ctrl+Home/End, PageUp/PageDown.
- Hangul keyboard actions (0.14): Shift+Enter line break inside a paragraph, Tab, Ctrl+T / Ctrl+Delete delete word, Ctrl+Backspace delete word back, Ctrl+Y delete line (redo is Ctrl+Shift+Z), Alt+Y delete to end of line, Ctrl+Alt+Space non-breaking space, Alt+Space fixed-width space, Ctrl+Shift+- soft hyphen. The same actions are engine commands: `caret.move` (22 motions, selection extension), `select.all/word/paragraph/line/clear/get`, `edit.copy/cut/paste` (tables and objects included, also headless), `edit.clipboard.get`, `text.delete_word/_back`, `text.delete_line`, `text.delete_to_line_end`, `insert.line_break/tab/nbsp/fixed_width_space/soft_hyphen`, `insert.auto_text` (date, time, page number, total pages, file name, user name), `edit.overwrite`, `edit.goto` (page, section, paragraph, bookmark), `hyperlink.jump/next/prev`, `edit.find_all`, `document.statistics`, `document.outline`.
- Copy, cut and paste that keep formatting, tables and objects inside the app; **HTML** to and from other programs (bold, colours and paragraphs survive), with plain text as the fallback.
- Password-protected documents: an encrypted HWP / HWPX / HWP3 asks for its password on open and is saved with the same password; the Security tab sets, changes or clears a password. The CLI takes `--password`.
- Undo/redo (40 steps, consecutive typing grouped by word).
- Find (Ctrl+F) and Replace (Ctrl+H) with case, whole-word and direction options; Replace All undoes in one step.
- Sort paragraphs, remove control marks by type, page break (Ctrl+Enter), column break (Ctrl+Shift+Enter), bookmarks, HTTP/HTTPS hyperlinks (insert, edit address / text, remove), context menu.
- Character map (Ctrl+F10) with Hangul's four tables — 사용자 (favourites), 유니코드 (160+ Unicode 15 blocks), 한글(HNC) (30 curated areas: symbols, maths, units, circled / parenthesised, box drawing, shapes, arrows, dingbats, Greek, Cyrillic, kana, Roman numerals / fractions, super- / subscripts, letterlike, jamo incl. old Hangul, IPA, full- / half-width, CJK punctuation / units, radicals, hexagrams, braille, music, currency, technical), 완성형(KS X 1001) (symbol rows, 2,350 hangul, 4,888 hanja) — plus emoji (10 areas), named special characters (spaces, dashes, marks) and a search by name, `U+` code or keyword. Multi-character input box, recent characters, a font choice for the inserted characters, Unicode / KS code display, and installed symbol / emoji fonts for glyph coverage. The same catalogue is available to the CLI / MCP (`symbol.sets`, `symbol.list`, `symbol.search`, `insert.symbol`).
- Hangul-style two-key shortcuts (Ctrl+K,H · Ctrl+K,N · Ctrl+K,O · Ctrl+N,P/H/T · Ctrl+G,… · Ctrl+Q,I) plus the standard format shortcuts and Ctrl+1…0.
- Tab stops are saved with widths computed the Hangul way (left, right, centre, decimal, leader) so they land in the same place when opened in Hangul.

### Formatting

- Character format (Alt+L: basic / extended / border & background) and paragraph format (Alt+T: basic / extended / tabs / border & background), with previews typeset by RHWP.
- Styles (F6): list, add, edit, delete, shortcut keys. Bullets and numbering (10 numbering presets, 16 bullet presets), outline numbering, promote / demote.
- Custom tabs, character and paragraph borders and shading. Increment steps match Hangul (1 % spacing/width, 1 pt size, 10 % line spacing, 1 pt indent/margin).
- Highlighter saved as real RHWP markpen marks (round-trips through HWP / HWPX); HwpCraft draws the highlight on screen itself. Format copy applies character and paragraph format in one undo step (Alt+C). Character / paragraph format inside headers, footers and footnotes through engine commands (`format.hf.*`, `format.footnote.paragraph`).

### Tables and objects

- Tables up to 200 × 64: cell blocks (F5, drag, Shift), table / cell properties, borders and backgrounds, merge / split (N×M), insert / delete rows and columns, drag grid lines, formulas and block sum / average / product, thousands separators, flip / split / join tables, text ↔ table, sort, auto-fill, captions, four table templates, cell protection.
- Cell inner margins in mm from the Table Layout tab or context menu; the horizontal ruler switches to the selected cell and its edge markers can be dragged.
- Direct manipulation: cross handle to select and move a whole table, square handles to resize (Shift keeps ratio), width / height entry.
- Table drawing and eraser tools: outer border, rows, columns, diagonals, apply line style / colour / width, erase shared borders to merge cells. Each action is one undo step.
- Pictures: PNG, JPG, BMP, GIF with size, position, wrap, rotation, flip, crop, effects, brightness, contrast, transparency and caption.
- Shapes and text boxes: line, rectangle, ellipse, arc, polygon, freeform and text box drawn by dragging; eight resize handles, rotation handle, line / fill / shadow properties, multi-paragraph text boxes.
- Equation editor with Hangul-style structure and symbol palettes and live preview, multi-paragraph footnotes and endnotes, editing of existing chart values, memo list.
- Object operations (0.13): group / ungroup (Shift+click multi-select), bring to front / send to back / forward / backward, object protection (locked objects refuse drag, delete and edits) and unlock all, horizontal / vertical flip, six connector kinds drawn shape-to-shape and re-routed when a shape moves, line end-point handles, click-here fields (누름틀) with name / value editing, and value editing of existing form objects.

### Page, view and file services

- Ctrl+wheel zoom (10–500 %) over the document, wheel-only zoom over the status-bar zoom indicator.
- Page setup (F7), page numbering (11 positions, 10 formats), headers / footers (11 templates × both / odd / even), page borders and backgrounds, restart numbering, hide current page, columns.
- Document information, print (range, copies; via PDF to the Windows print verb, macOS `open` or Linux `xdg-open`, or a PDF file), zoom presets, fit page / fit width, show or hide rulers and document tabs.
- Continuous multi-page view like Hangul (14 px page gap), drawing only visible pages and scrolling with the caret.
- 0.13: View tab paragraph marks / control codes / transparent lines; Security tab document password and *find and protect personal information* (resident numbers, cards, phones, e-mail, hidden text; mask selected values in one undo step); header / footer page-number, total-pages and file-name fields; File menu preview, recent documents, save as HML (HML-sourced documents only) and save a page range; section settings; Alt key tips for tabs and ribbon items.

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

`hwpcraft-cli mcp` is a headless stdio MCP server: create Korean documents, edit text / formatting / tables / objects, save HWP / HWPX, export PDF / TXT, render PNG for checking, and restrict read / write roots. 0.14 adds `hwp_action`, which accepts Hangul automation action names (`MoveLineEnd`, `CharShapeBold`, `DeleteWord`, about 300 of them) and runs the matching command, `batch {atomic:true}` that rolls every document change back when any command fails, and `get_selection`. It does not control a running desktop window or Hancom software. See [docs/mcp.md](docs/mcp.md) and [docs/mcp-commands.md](docs/mcp-commands.md) (Korean).

## Build

Rust 1.98.1, pinned in `rust-toolchain.toml`. Windows x64 needs MSVC and the Visual Studio C++ build tools; macOS needs the Xcode command-line tools; Linux needs a C compiler plus X11 / Wayland, OpenGL and GTK3 (file dialogs) development packages. `HwpCraft.ps1` (Windows) and `hwpcraft.sh` (macOS / Linux: `build`, `test`, `install`, `start`, `status`) wrap the same Cargo commands.

Windows installer: `scripts/build-installer.ps1` stages the same files as `HwpCraft.ps1 Install` and compiles `packaging/windows/HwpCraft.iss` with Inno Setup 6 into `output/installer/HwpCraft-<version>-windows-x64-setup.exe` (optional desktop shortcut and `.hwp` / `.hwpx` association).

Platform status (0.13): fonts come from a per-OS candidate list (Windows 맑은 고딕, macOS Apple SD Gothic Neo, Linux Noto Sans CJK KR / NanumGothic …), printing and the settings folder are OS-specific. Windows x64 is fully tested; Linux x86_64 passed `cargo check --workspace --all-targets` only; macOS could not be checked from the Windows host (the RHWP dependency `blake3` needs an Apple C cross-compiler) and must be built on a Mac. Launcher registration and portable ZIP packaging exist in the Windows script only.

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
- Security tab: distribution documents, protecting selected text, security settings, choosing a security kind (RHWP has one). Entire Review and Tools tabs: proofreading marks, document history / compare, track changes, spell check, quick correction, dictionaries, translation, Hanja conversion (no dictionary data), macros, index / table of contents / citations, preferences.
- View: page outline, grid, guides, view colour modes, split editing window, multiple document windows (RHWP has no such render modes). File: templates gallery; HML export of documents not opened from HML.
- Page: text direction, line numbers, section start position, labels, manuscript paper, header / footer date field, odd / even page-number split. Print details (printer list, N-up, watermark).
- Format: style gallery import / export, custom numbering / bullet shapes, picture bullets, drop caps. Character format inside footnotes (paragraph format only).
- Tables and objects: moving a single cell border, editing nested tables, vertical text boxes, align, the object-selection button (objects are selected on the page), picture reflection / glow / photo editing, visual equation editing and macros, new memos, creating form objects (values of existing ones can be edited), new charts, WordArt, OLE. Connector anchors are exact for paper-anchored objects and may drift for paragraph-anchored ones; line end-point editing excludes inline lines and connectors.
- Tab drop-down lists are ribbon command lists, not Hangul submenus. The status-bar column indicator is always 1.
- Direct table dragging targets top-level tables within one page; an inline table becomes floating when moved. Multi-page tables are adjusted through properties. Table drawing supports zero cell spacing only.

### RHWP typesetting and save limits (not worked around in HwpCraft)

- Paragraph border top / bottom spacing is not reflected in line layout, so paragraphs may overlap both on screen and in Hangul. Side spacing and some line styles (double lines) draw as single lines.
- Footnote separator length −1 (column width) is drawn short, and objects with outer margins are offset on screen (saved files open correctly in Hangul).
- The highlighter is saved as markpen marks but RHWP's renderer does not paint them, so HwpCraft overlays them on screen only (not in print / PDF). New numbered lists create new numbering definitions (RHWP's `set_numbering_restart` is not saved); curves are saved as dense polylines.
- Page-number / total-page auto numbers inserted into body text (`insert.auto_text`) are saved and Hangul shows the numbers, but RHWP's renderer does not draw body auto numbers, so HwpCraft's screen and PDF leave them blank. Hangul's date-code field has no RHWP counterpart; the current date is inserted as text.
- Page copy / delete work per paragraph (a paragraph continued from the previous page is left out; deleting is refused when one paragraph fills the page). Insert file lives in the File menu only (Ctrl+O stays Open). PDF print options (highlighter, drawing objects, pictures, memos), page outline off, grid, line numbers, master-page editing and document history are not provided because RHWP has no option or model for them. Facing pages only changes the page layout.
- AutoText (상용구) stores plain-text entries only; body entries with formatting, tables or objects are not saved (no groups or import / export).
- Hanja conversion picks candidates per syllable from the Unicode Unihan reading table; without a word dictionary it does not offer word-level candidates or frequency ordering like Hangul. Track changes, new memos and spell checking are not provided because RHWP has no model or dictionary for them. Document compare lists added / removed / changed paragraphs without a side-by-side view. "No shading" is reported by RHWP as white (#ffffff).
- Master pages, document summary, odd / even / first-page-only page borders, picture fill, new memos (MemoShape unsupported), form-object creation, chart / WordArt / OLE creation are unavailable because RHWP has no API for them. Style create / update / delete exist only in RHWP's wasm wrapper, so HwpCraft edits the model directly. Line-based caret motion and selection-rectangle queries are crate-private in RHWP; HwpCraft computes them from caret geometry.
- Automatic tab-width recording covers body text and first-level table cells only; tabs inside headers, text boxes and notes are saved with width 0.
- Multi-paragraph selection works within one section body or one cell; Select All covers the current section.

### Verification limits

- Pointer-event regression tests cover table selection, moving and resizing. End-to-end click / drag verification of every feature has not been completed.
- Round-trip saving of complex real-world documents, performance on very large documents and tables, and long IME sessions have not been tested thoroughly. Keep originals and save important documents under a new name, then check them in Hangul.
- Input documents are limited to 128 MB and a single text insertion to 1 MB. Parsing and rendering are synchronous, so the UI may pause on complex documents.

Release notes are in `docs/` (Korean).

## License

HwpCraft is released under the **MIT License**. The full text is in [LICENSE](LICENSE); the copyright holders are the HwpCraft contributors.

**What the MIT License covers** — everything HwpCraft itself produces in this repository.

- Source code: the Rust code in `crates/` and `apps/`, plus the build / packaging script (`HwpCraft.ps1`).
- Documentation and samples: `README.md`, `docs/`, `CONTRIBUTING.md`, and the HWP / HWPX / PDF documents in `samples/`.
- Assets made by HwpCraft: the application icon `assets/icon.svg` and the ribbon icon atlas `assets/toolbar-icons.png` (a composition of Lucide icons).
- Contributions: every change submitted by pull request is provided under MIT and requires a DCO sign-off ([CONTRIBUTING.md](CONTRIBUTING.md)).

**Third-party components** — each keeps its own license and is not relicensed under MIT. Full texts are in `assets/licenses/`; the list is in `THIRD-PARTY-NOTICES.txt`.

| Component | License | Where |
|---|---|---|
| [RHWP](https://github.com/edwardkim/rhwp) document engine (Edward Kim) | MIT | Separate repository (`../rhwp`), linked into the binaries |
| Code adapted from WordCraft, DeckCraft and DesignCraft (ArtCraft team) | MIT | Marked in the file headers of `ribbon_layout.rs`, `file_access.rs`, `chrome.rs` and others |
| [Lucide](https://lucide.dev) icons | ISC | `assets/icons/lucide/` |
| egui default fonts: Hack, Noto Emoji, Ubuntu, emoji-icon-font | MIT, OFL 1.1, UFL 1.0, MIT | Compiled into the binaries |
| Rust crate dependencies (`Cargo.lock`) | Permissive licenses: MIT, Apache-2.0, BSD, ISC and similar | `THIRD-PARTY-DEPENDENCIES.md` |

Crates offered under a choice of licenses are used under MIT or Apache-2.0. No code distributed only under GPL or AGPL is included.

**What is not included** — no Hancom application code, icons, logos, fonts, templates or clip art exist anywhere in the source or the binaries. "Hangul" (한글), "Hancom", "HWP" and "HWPX" are trademarks of their respective owners; the MIT License grants no trademark rights, and this repository uses the names only to describe file-format compatibility.

When redistributing binaries, ship `LICENSE`, `THIRD-PARTY-NOTICES.txt` and the license texts in `assets/licenses/` alongside them. The packaging script copies them automatically.

## Acknowledgements

- [RHWP](https://github.com/edwardkim/rhwp) (MIT) — HWP / HWPX document engine.
- [Lucide](https://lucide.dev) (ISC) — ribbon icons in `assets/icons/lucide`, built into `assets/toolbar-icons.png`.
- [WordCraft](https://github.com/storytold/wordcraft), DeckCraft and DesignCraft (MIT, ArtCraft team) — crate structure, ribbon width planning, file replacement, MCP structure, dialog draft / commit patterns. Licenses are kept in `assets/licenses/` and listed in `THIRD-PARTY-NOTICES.txt`.
- [egui](https://github.com/emilk/egui) / eframe, [resvg](https://github.com/linebender/resvg), and the other crates in `Cargo.lock`.

Hancom, Hangul (한글) and HWP are trademarks of their respective owners and are used here only to describe file-format compatibility.
