# HwpCraft

**한국어** · [English](#hwpcraft-english)

ArtCraft 팀의 [WordCraft](https://github.com/storytold/wordcraft) 기본 구조 위에, [RHWP](https://github.com/edwardkim/rhwp) 문서 엔진의 아래한글 편집 기능과 [egui](https://github.com/emilk/egui) 화면을 결합해 만든 Rust 네이티브 HWP/HWPX 편집기입니다. 두 프로젝트는 모두 MIT 라이선스입니다.

현재 버전은 **0.19**이며, 버전은 `0.10 → 0.11 → 0.12 …`처럼 `x.xx` 형식으로 증가합니다. 0.15·0.16은 본문 메모 생성·편집·삭제, 중첩 표의 주소 기반 편집과 생성·삭제, 본문 자동 쪽 번호의 공통 화면·PDF 표시, 공개 자동화 인자 일부를 HwpCraft 자체 계층에 추가했습니다. 아래한글 전체 기능과 동등하다고 판정한 버전은 아닙니다. 0.17은 입력 서식, 머리말·꼬리말·글상자·주석의 문서 영역 편집, MCP 2026-07-28 규격(문서별 작업자, HTTP 토큰 인증), 열기 속도·편집 지연·PDF 저장 속도·화면 정리, Windows·macOS·Linux 배포를 다룹니다. 0.18은 11종 차트 만들기, 실패한 atomic batch의 실행 취소 기록·클립보드 복원, MCP Tasks 확장·결과 형식(outputSchema)·진행 알림, 편집·열기·PDF 저장 속도, 리본·메모·대화상자 화면 정리를 다룹니다. 0.19는 아래한글 편집 기능과 그에 맞는 MCP 명령을 넓힙니다: 제목·표·그림 차례와 상호 참조(HWP/HWPX 필드, PDF에서도 눌리는 링크), 메모 댓글·표 셀 메모, 날짜 필드, 문서 비교 뒤 병합, 스타일마당, 덧말·문단 첫 글자 장식·그림 글머리표, 양식 개체 만들기, 수식 시각 편집·수식 매크로, 차트 15종(누적·혼합·영역·방사·거품형)과 그림 효과, 바탕쪽 글·구역·단 나누기·줄 번호(설정과 화면·PDF 그리기), 사용자 표 마당, 인쇄 상세(홀짝·역순·모아 찍기·워터마크)·인쇄 미리 보기, 자동 복구·문서마당, 화면 나누기·새 창·쪽 윤곽, 키 매크로, 실행 중인 데스크톱 창 MCP 연결, DOCX 내보내기, 그리고 새 문서를 HWP로 저장해 다시 열 때 쪽 나눔이 달라지던 조판 결함 수정입니다. 성능: 편람 1,445건 모두 바꾸기 81–90초 → 2.7초, 편람 PDF 48 MB → 9.2 MB(같은 시간, 픽셀 동일; 글꼴 공유와 같은 자원 합치기), 링크 있는 차례 넣기 1,500제목 18.6초 → 1.25초, 44쪽 보고서 글자 입력 프레임 75 → 32 ms(보이지 않는 글자 해석 제외), 같은 보고서에서 글자 하나 입력 처리 18.3 → 2.3 ms(줄 배치가 그대로인 문단은 구역 조판 결과를 다시 씀, RHWP 패치 0031), MCP 도구 결과 처리 약 40% 단축(JSON 재파싱 제거). MCP: `insert.table` 결과에 새 표 주소, `hyperlink.jump`로 문서 안 링크를 따라 캐럿 이동, `list_documents`와 실행 중인 데스크톱 창 연결, [보고서 작성 순서](docs/mcp-commands.md#보고서-작성-순서) 안내, 머리말·꼬리말·각주·미주·글상자·중첩 셀까지 한 번에 찾고 바꾸기(`edit.find_all`·`edit.replace_all`의 `include`, 실행 취소 한 단계), 표 셀 글 한꺼번에 쓰기(`table.cells.set`)와 셀 주소로 줄·칸 편집(`table.edit`), 편집 용지를 모든 구역에(`page.setup`의 `apply_to`), 한글 ParameterSet으로 글 넣기·편집 용지·찾기/바꾸기(`hwp_action`의 InsertText·PageSetup·AllReplace 등). 고친 결함: `edit.find_all`이 글상자·중첩 셀·수식 안의 일치를 본문·표 셀 캐럿 위치로 돌려줘 그 자리에서 고치면 엉뚱한 곳이 바뀌었습니다. 화면: 좁은 창(400 px까지)에서도 리본·탭 줄·서식 도구 줄이 잘리지 않고(» 더 보기), 접힌 묶음·더 보기에도 키팁, 회색 단추에 사유 툴팁, 저장·오류 상자의 Alt 단축키와 첫 입력 칸 포커스. 자세한 내용: [0.19 릴리스](docs/RELEASE-0.19.md), [0.18 릴리스](docs/RELEASE-0.18.md), [0.17 릴리스](docs/RELEASE-0.17.md), [0.16 릴리스](docs/RELEASE-0.16.md), [0.15 릴리스](docs/RELEASE-0.15.md), [기능 확장 순서](docs/FEATURE-ROADMAP.md).

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
- 같은 편집 엔진을 데스크톱 앱, CLI, stdio·loopback HTTP **MCP** 서버로 제공합니다.

### 편집

- 여러 문단 선택(Shift+클릭·끌기·Shift+방향키), 전체 선택(Ctrl+A), 두 번 클릭 낱말·세 번 클릭 문단 선택, 수정 모드(Insert), Windows 한글 IME.
- 커서 이동: 문단을 넘는 화살표, Home/End, 낱말(Ctrl+←/→), 앞/다음 문단 처음(Ctrl+↑/↓), 문서 처음/끝(Ctrl+Home/End), PageUp/PageDown.
- 한글 글쇠 동작(0.14): Shift+Enter 줄 나눔(문단 안), Tab, Ctrl+T·Ctrl+Delete 한 단어 지우기, Ctrl+Backspace 낱말 앞 지우기, Ctrl+Y 한 줄 지우기(다시 실행은 Ctrl+Shift+Z), Alt+Y 이후 지우기, Ctrl+Alt+Space 묶음 빈 칸, Alt+Space 고정폭 빈 칸, Ctrl+Shift+- 하이픈. 같은 동작을 엔진 명령으로도 제공합니다: `caret.move`(22가지 이동, 선택 확장), `select.all/word/paragraph/line/clear/get`, `edit.copy/cut/paste`(헤드리스에서도 표·개체 포함), `edit.clipboard.get`, `text.delete_word/_back`, `text.delete_line`, `text.delete_to_line_end`, `insert.line_break/tab/nbsp/fixed_width_space/soft_hyphen`, `insert.auto_text`(날짜·시각·쪽 번호·전체 쪽수·파일 이름·사용자 이름), `edit.overwrite`, `edit.goto`(쪽·구역·문단·책갈피), `hyperlink.jump/next/prev`, `edit.find_all`, `document.statistics`, `document.outline`.
- 복사·잘라내기·붙이기: 앱 안에서는 서식·표·개체 유지. 다른 프로그램과는 **HTML**로 주고받아 굵게·색·문단이 살아남고, HTML이 없거나 해석에 실패하면 일반 텍스트로 붙입니다.
- 암호 문서: 암호가 걸린 HWP/HWPX/HWP3를 열면 암호를 묻고, 연 문서는 같은 암호로 저장합니다. 보안 탭 **문서 암호**로 암호를 걸거나 바꾸거나 풉니다. CLI는 `--password <암호>`.
- 실행 취소/다시 실행 40단계. 이어서 친 글자는 낱말 단위로 묶습니다.
- 찾기(Ctrl+F)·찾아 바꾸기(Ctrl+H): 대소문자, 온전한 낱말, 방향 옵션. 모두 바꾸기는 한 번에 되돌립니다.
- 정렬, 조판 부호 지우기(종류별), 쪽 나누기(Ctrl+Enter), 단 나누기(Ctrl+Shift+Enter), 책갈피, HTTP/HTTPS 하이퍼링크 넣기·고치기·지우기와 Ctrl+클릭으로 따라가기(문서 안 링크는 그 자리로 옮기고, http·https·mailto 주소만 브라우저·메일 프로그램으로 엽니다. 파일·스크립트 같은 다른 주소는 열지 않습니다), 오른쪽 클릭 메뉴.
- 문자표(Ctrl+F10): 한글의 네 가지 표 — 사용자 문자표(등록 문자), 유니코드 문자표(유니코드 15 블록 160여 개), 한글(HNC) 문자표(일반 기호·수학·단위·원 문자·괄호 문자·괘선·도형·화살표·딩뱃·그리스·키릴·가나·로마 숫자/분수·첨자·영문 서체·한글 자모(옛한글 포함)·발음 기호·전각/반각·한중일 구두점/단위·한자 부수·주역 괘·점자·음악·통화·기술 기호 30개 영역), 완성형(KS X 1001) 문자표(특수 문자 행·한글 2,350자·한자 4,888자) — 에 더해 이모지 10개 영역, 이름 있는 특수 문자(공백·줄표·표식), 이름·`U+`코드·키워드 검색을 제공합니다. 여러 문자 입력 상자, 최근 사용 문자, 넣을 문자의 글꼴 선택, 유니코드·완성형 코드 표시, 설치된 기호·이모지 글꼴 활용. 같은 목록을 CLI/MCP(`symbol.sets`, `symbol.list`, `symbol.search`, `insert.symbol`)에서도 씁니다.
- 한글식 접두 단축키(Ctrl+K,H · Ctrl+K,N · Ctrl+K,O · Ctrl+N,P/H/T · Ctrl+G,… · Ctrl+Q,I)와 서식 단축키, Ctrl+1…0.
- 탭은 한글 방식으로 폭을 계산해 저장하므로(왼쪽·오른쪽·가운데·소수점 탭, 채울 모양) 한글에서 열어도 같은 자리에 놓입니다.
- 메모: 본문과 표 셀의 글 범위·캐럿 자리에 메모를 달고 고치고 지웁니다. 댓글(`memo.reply`)은 한글처럼 같은 범위에 중첩한 메모로 저장되어 HWP·HWPX를 다시 열어도 계층(`memo.list`의 replyTo·thread)이 유지되고, 첫 메모를 지우면 댓글도 함께 지워집니다. **0.18까지 표 셀 안 메모의 내용이 HWP 저장 때 사라지던 결함을 0.19에서 고쳤습니다.**
- 날짜 필드: 문서 만든 날짜·마지막 저장 날짜(`insert.date_field`, 한글의 날짜 코드·마지막 저장 날짜와 같은 문서 요약 필드). 0.19부터 HWP/HWPX로 저장할 때 문서 만든 시각(없을 때)과 마지막 저장 시각을 문서 요약 정보(HWPX content.hpf, HWP 요약 정보)에 기록합니다(`SOURCE_DATE_EPOCH`가 있으면 그 시각, 자동 복구 사본·블록 저장·PDF·DOCX는 기록하지 않음). 필드 글은 `tools.date_field.update`로 새로 고칩니다.
- 스타일마당(서식 > 스타일마당, Alt+T, `format.style.set.*`): 스타일 묶음을 골라 문서 스타일을 한 번에 고칩니다(같은 이름은 고치고 없는 스타일은 더함, 실행 취소 한 단계). HwpCraft가 만든 기본 묶음 보고서·공문·간결은 글꼴은 두고 크기·굵기·간격·정렬만 바꾸며, 지금 문서의 스타일을 사용자 묶음으로 설정 폴더 `style_sets`에 저장하고 지울 수 있습니다.

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
- 수식 편집기(구조·기호 도구와 미리 보기, 미리 보기를 눌러 그 자리에서 입력·Tab으로 빈 칸 이동, 수식 매크로 `equation.macro.*`, 조각 배치 `equation.layout`), 여러 문단 각주·미주, 기존 차트 값 편집, 메모 목록. 공통 엔진·CLI·MCP의 `chart.create`/`chart.configure`는 본문의 막대·가로 막대·선·원형·도넛·분산형·주가형 2종·3D형 3종·혼합형(막대+꺾은선, 보조 축)·영역형·방사형·거품형, 총15종 차트 생성과 자체 차트 구성을 지원합니다. 막대·가로 막대·꺾은선·영역형은 누적·100% 누적(`grouping`)을 고를 수 있습니다. `chart.types`는 종류별 제약, `chart.definition.get`은 현재 전체 구성을 제공합니다. HWP/HWPX 자체 왕복·SVG/PDF 검증과 한컴 네이티브 호환성은 별개이며, 후자는 아직 검증 중입니다.
- 표·개체(0.14): 표 안 셀 이동(Tab/Shift+Tab, 마지막 셀에서 줄 추가, `table.cell_move`), 표 마당 옵션(제목 줄·첫/마지막 칸·마지막 줄·회색조·테두리·배경), 셀 음영 진하게/연하게, 사용자 목록 자동 채우기, 개체 맞춤 11종·크기·회전·크기 고정·바깥 여백 사전 값·채우기/선/화살표 빠른 설정·그림자 이동·개체 모양 복사/붙이기·Tab 개체 순환·개체 선택 단추·연결선 종류 바꾸기, 글상자 9방향 정렬·여백·세로 글상자, 그림 파일로 저장·색조(회색조·흑백), 자동 번호(쪽·전체·그림·표·수식), 각주↔미주 변환과 각주/미주 모양(구분선·번호 모양), 주석·누름틀 지우기.
- 그림 넣기(`picture.insert`)는 `treat_as_char`를 생략하면 한글처럼 글자처럼 취급합니다(0.19부터, 이전에는 떠 있는 그림). `false`면 캐럿 문단 기준·단 왼쪽·어울림으로 넣어 글이 옆으로 흐릅니다. 그림·도형·차트 캡션 글은 `object.caption_text`, 표 캡션 글은 `table.caption_text`로 고칩니다.
- 양식 개체(`form.insert`): 선택 상자·입력 상자·콤보 상자·라디오 단추·명령 단추를 캐럿 자리(본문·표 셀)에 글자처럼 넣고 `form.set`으로 값을 고칩니다. 크기·색·속성은 한글이 새로 넣은 개체의 값을 따르고, 테두리 채우기는 쓰지 않습니다(틀만 그림). 데스크톱은 입력 탭 "양식 개체" 메뉴에서 종류를 고르면 넣기 창이 뜹니다. 글꼴·여러 줄 입력·읽기 전용 같은 세부 속성 편집은 아직 없습니다.
- 개체 조작(0.13): 묶기/풀기(Shift+클릭 다중 선택), 맨 앞/맨 뒤/앞/뒤 순서, 개체 보호(보호된 개체는 끌기·삭제·편집 거부)와 보호 모두 해제, 가로/세로 대칭, 연결선 6종(도형→도형 끌기, 도형 이동 시 재계산. 문단 기준·글자처럼 취급한 개체에도 붙고, 끝점은 화면에 그려진 자리에서 잡음. 두 개체는 같은 쪽에 있어야 하며, 본문 편집으로 문단 기준 개체가 밀려도 연결선은 그 개체를 옮기거나 고칠 때 다시 계산됨), 직선 끝점 손잡이, 누름틀 넣기와 이름·값 편집, 기존 양식 개체 값 편집.

### 쪽·보기·파일 서비스

- 문서 위 Ctrl+휠 확대/축소(10–500 %), 배율 표시 위에서는 휠만으로 확대/축소.
- 편집 용지(F7), 쪽 번호 매기기(위치 11종·모양 10종), 머리말/꼬리말(마당 11종 × 양쪽/홀수/짝수), 쪽 테두리/배경, 새 번호로 시작, 현재 쪽만 감추기, 다단.
- 문서 정보, 인쇄(범위·매수, PDF를 거쳐 Windows 인쇄 동사, macOS·Linux는 CUPS `lp`, 실패하면 기본 PDF 보기 프로그램으로 열기, 또는 PDF 저장), 보기 배율·쪽 맞춤·폭 맞춤, 눈금자·문서 탭 보이기.
- 한글처럼 여러 쪽을 세로로 이어 보는 연속 보기(쪽 사이 14 px). 보이는 쪽만 그리고 커서를 따라 스크롤합니다.
- 여러 창(0.19): 보기 → 새 창(따로 실행한 프로그램)·편집 화면 나누기(위아래·좌우), 창 전환(열린 HwpCraft 창 목록에서 골라 앞으로 가져오도록 요청함 — 요청이 그 창에 닿는 것까지만 확인했고, 실제로 앞으로 오는지는 확인하지 못함. Windows가 다른 프로그램 창을 앞으로 못 가져오게 막으면 작업 표시줄 단추만 깜박일 수 있음), 창 배열(계단식·왼쪽·오른쪽으로 나란히·위·아래로 나란히). 창끼리는 설정 폴더 `windows`의 작은 파일로 서로를 찾고 요청을 주고받습니다.
- 한자 발음 표시(0.19, 보기 탭): 화면에서만 한자 위에 한글 본음(Unihan 독음표의 교육용 음 우선)을 작게 보입니다. 글자마다 하나씩이라 낱말 단위 독음(예: 두음 법칙)은 반영하지 않고, 세로쓰기·회전한 글자에는 붙이지 않으며, 인쇄·PDF에는 나오지 않습니다. 한글 화면과 위치·크기를 대조하지 못했습니다.
- 0.14: 쪽 여백 사전 값 6종(한글 실측 수치), 문서 보기 색(기본·어둡게·고대비·옛 한글·사용자 설정)과 그림 자리 표시, 맞쪽 보기, 쪽 복사/붙이기/지우기, 끼워 넣기(`file.insert`), 블록 저장, 그림 파일로 저장(쪽 PNG), 머리말/꼬리말 편집 창 이전·다음, 문서 정보(일반·글꼴 대체·그림), 구역 이전/다음. 서식(0.14): 글자 크기·장평·줄 간격·여백 증감, 글꼴 순환, 글자 색/음영/형광펜 사전 색, 글자 테두리, 현재 위치로 들여쓰기, 문단 위/아래 이동, 위/아래 첨자 순환, 글자 스타일 해제, 현재 모양으로 스타일 갱신, 스타일 순서·JSON 가져오기/내보내기, 글자 겹치기. 검토·도구(0.14): 한자 변환(F9, Unihan 독음표), 찾기 확장(띄어쓰기 무시·아무개 문자·조건식·여러 단어·한글로 한자·서식 조건), 문서 비교(RHWP docdiff), 상용구(Ctrl+F3·Alt+I). 편집(0.14): 이동 창(Alt+G).
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

`hwpcraft-cli mcp`는 MCP `2026-07-28` 전용 헤드리스 서버입니다. 기본 stdio 외에 `--transport http --listen 127.0.0.1:7082`를 지원합니다. HTTP는 loopback 주소에서만 열리고 모든 요청에 `Authorization: Bearer <토큰>`이 필요합니다. 토큰은 설정 폴더의 `mcp-http-token`(또는 `--auth-token-file`로 지정한 파일)에 있으며, 없으면 서버가 본인만 읽을 수 있게 새로 만듭니다. 요청에는 요청마다 프로토콜 `_meta`와 문서 작업의 명시적 `document_id`를 전달합니다. 한 프로세스에서 여러 문서를 격리하고 문서 생성·텍스트/서식/표/개체 편집·HWP/HWPX 저장·PDF/TXT 출력·PNG 확인·읽기/쓰기 폴더 제한을 제공합니다. `hwp_action`은 등록된 한글 동작 별칭과 일부 공개 ParameterSet을 공통 엔진으로 연결합니다. 별칭 등록이 모든 원래 옵션의 동등성을 뜻하지는 않습니다. `batch {atomic:true}`는 문서 편집을 되돌리고 파일·설정 쓰기를 사전에 거절합니다. 모든 도구는 결과 구조(`outputSchema`)를 선언하며, `execute`·`batch`처럼 명령마다 결과가 다른 부분은 공통 봉투만 고정합니다. 명령 인자는 `list_commands`에서 조회합니다. 데스크톱 앱의 도구 탭 **MCP 연결**을 켜면 같은 서버가 그 창 안에서 `127.0.0.1:7083`으로 열리고(같은 토큰), 클라이언트는 `list_documents`로 창의 문서를 찾아 그 `document_id`로 편집합니다. 고친 내용은 창에 바로 보이고 실행 취소로 되돌릴 수 있으며, MCP로는 그 문서를 닫을 수 없습니다. 한컴 프로그램은 제어하지 않습니다. [MCP 연결과 사용법](docs/mcp.md), [명령 인자](docs/mcp-commands.md)를 참조하세요.

## 빌드

Rust 1.98.1(`rust-toolchain.toml`에 고정). Windows x64는 MSVC와 Visual Studio C++ 빌드 도구, macOS는 Xcode 명령줄 도구, Linux는 C 컴파일러와 X11/Wayland·OpenGL 개발 패키지가 필요합니다. 파일 대화상자는 xdg-desktop-portal(GNOME·KDE 기본 제공)을 쓰므로 GTK 개발 패키지는 필요 없습니다. `HwpCraft.ps1`(Windows)과 `hwpcraft.sh`(macOS·Linux: `build`, `test`, `install`, `start`, `status`)가 같은 Cargo 명령을 감쌉니다.

Windows 설치 파일: `scripts/build-installer.ps1`이 `HwpCraft.ps1 Install`과 같은 파일을 모아 Inno Setup 6으로 `packaging/windows/HwpCraft.iss`를 컴파일해 `output/installer/HwpCraft-<버전>-windows-x64-setup.exe`를 만듭니다(바탕 화면 바로 가기·`.hwp`/`.hwpx` 연결은 선택).

플랫폼 현황(0.19): Windows x64는 전체 테스트·clippy와 실제 데스크톱 화면까지 검증했습니다. macOS(arm64·x64)와 Linux x86_64는 GitHub Actions 릴리스 빌드에서 workspace 테스트를 실행하며, 실제 Mac·Linux 데스크톱에서 화면을 직접 확인하지는 않았습니다. 글꼴은 OS별 한글 글꼴 후보(Windows 맑은 고딕, macOS Apple SD Gothic Neo, Linux Noto Sans CJK KR·나눔고딕 등) 중 설치된 첫 글꼴을 씁니다. Linux에는 한글 글꼴(`fonts-noto-cjk` 또는 `fonts-nanum`)이 필요하며, 없으면 영문·숫자만 보이고 한글은 다른 설치 글꼴에 한글이 있을 때만 표시됩니다. 설정 폴더는 Windows `%APPDATA%\HwpCraft`, macOS `~/Library/Application Support/HwpCraft`(기존 `~/.config/HwpCraft`가 있으면 그대로 사용), Linux `$XDG_CONFIG_HOME/HwpCraft` 또는 `~/.config/HwpCraft`입니다. macOS 단축키는 Ctrl 대신 Cmd(⌘)이지만 메뉴·툴팁 표기는 아직 "Ctrl+…"입니다. macOS 앱은 서명·공증하지 않았으므로 처음 실행할 때 우클릭 → 열기 또는 `xattr -d com.apple.quarantine HwpCraft.app`이 필요하고, Finder에서 문서를 더블클릭해 여는 것은 지원하지 않습니다(앱 안에서 열기). Linux 배포 묶음은 압축을 풀고 `./install.sh`로 `~/.local`에 설치합니다. Launcher 등록·portable ZIP 패키징은 Windows 스크립트에만 있습니다.

HwpCraft는 RHWP를 **형제 폴더 경로 의존성**(`../rhwp`, RHWP **v0.8.7**, 커밋 `1a76570e`)으로 사용합니다. 두 저장소를 같은 상위 폴더에 받으세요.

```text
git clone https://github.com/webvine/hwpcraft.git
git clone --branch v0.8.7 https://github.com/edwardkim/rhwp.git
cd hwpcraft
cargo build --release --locked -p hwpcraft -p hwpcraft-cli
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

엔진 테스트 하나가 `../rhwp/samples`의 차트 예제를 열므로 RHWP 체크아웃에 `samples` 폴더가 있어야 합니다. HwpCraft가 RHWP에 적용하는 로컬 패치는 `patches/rhwp/`에 있습니다. 빌드 전에 `pwsh -File scripts/rhwp-patches.ps1 -Mode Apply`로 적용하세요(`scripts/sync-siblings.ps1`은 RHWP를 최신화한 뒤 자동으로 다시 적용합니다). Windows에서는 `HwpCraft.ps1`(`Build`, `Test`, `Install`, `Start`, `Status`)이 같은 명령을 감싸며 `../.build/hwpcraft`를 Cargo 빌드 폴더로 씁니다. `Launcher`·`Install` 동작은 이 저장소에 포함되지 않은 로컬 런처를 대상으로 합니다.

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
| `crates/mcp` | stdio·loopback HTTP MCP 서버, 문서 격리·헤드리스 편집·도구·리소스·프롬프트·명령 안내 |
| `crates/ui-egui` | 메인 창·연속 쪽 보기·키보드/IME, 리본·도구줄·눈금자·상태 표시줄, 영역별 대화상자 |
| `apps/hwpcraft` | eframe/glow 데스크톱 실행 파일 |
| `apps/hwpcraft-cli` | 문서 분석·변환·PNG 렌더·명령 일괄 편집 CLI, MCP 진입점 |
| `samples` | HwpCraft로 직접 생성한 HWP/HWPX/PDF 예제 |
| `docs` | 설계 노트·검증 기록·릴리스 노트 |

영역별 모듈은 `extensions.rs`를 통해 연결되어 각 영역이 자기 파일에서 명령·대화상자·단축키를 추가합니다.

## 현재 범위와 한계

### 구현하지 않음(리본·메뉴에서 비활성)

- MCP: 데스크톱 창 연결은 창에 열린 문서 하나를 편집하며, 클릭/키 입력/창 캡처, 원격 인증, 전체 선택 확장과 규격 적합성 검증은 미완료입니다. HTTP는 loopback만 허용하며 문서별 명시 ID를 사용합니다. 연결 취소가 이미 실행된 동기 편집이나 파일 쓰기의 rollback을 보장하지 않습니다. 실행 중에 취소된 저장은 파일 교체 직전에 멈춰 기존 파일을 남깁니다. 긴 작업의 진행 알림은 5초 간격 경과 시간이며 쪽 단위 진행률이 아닙니다. Tasks 확장은 열기·저장 도구에만 적용하며 상태는 tasks/get 폴링으로만 알 수 있습니다(task 알림 없음).
- 보안 탭: 배포용 문서로 저장(배포용 문서는 읽기 전용으로 열어 보기·쪽 이미지만 되고, 편집·저장·복사·인쇄/PDF·본문 글자 추출은 데스크톱·MCP·CLI 모두 거절합니다), 선택 글자 보호·개인 정보 보호 해제·보호 암호 변경·문서 보안 설정(한글의 암호화 개인 정보 필드와 설정 형식이 공개되지 않아 지원하지 않음, 다른 문서에 있는 그 필드는 그대로 보존, 대신 개인 정보 찾아서 가리기), 보안 종류 선택(RHWP는 한 가지). 검토·도구: 교정 부호, 문서 이력의 새 버전 기록·이전 버전 보기(기록된 목록은 봄), 변경 내용 추적, 여러 문단 범위의 메모(메모 해결 상태는 한글에도 없음), 맞춤법, 빠른 교정의 한글 기본 목록(사용자 목록은 지원: 데스크톱에서 친 글에만 적용하고 바로 Ctrl+Z로 되돌림), 사용자 사전, 번역, 스크립트(JavaScript) 매크로, 인용(색인은 0.19부터 됩니다: 낱말 뒤에 한글과 같은 색인 표시를 넣고 고치고 지우며(첫째·둘째 키, HWP/HWPX), 캐럿 위치에 한글 2024 색인 만들기 기본 모양으로 색인을 넣습니다 — 첫소리 묶음, '낱말<탭>쪽, 쪽', 둘째 키는 들여 씀. 한글은 새 문서에 만들고 HwpCraft는 캐럿 위치에 넣으며, 색인 표시가 든 문단은 문서 병합·서식 상용구로 가져오지 못합니다. 차례는 개요 문단이나 스타일로, 표 차례·그림 차례는 캡션 달린 표·그림으로 만들 수 있습니다. 모두 "차례 N"·"표 차례"·"그림 차례" 스타일의 일반 문단으로 넣으며, 한글의 차례 필드는 쓰지 않습니다. 새로 고침 한 번이 세 차례를 모두 고칩니다. 항목 글은 그 제목·표·그림 문단의 책갈피로 가는 하이퍼링크이며(한글 저장본의 책갈피 링크 형식, DOCX에서는 문서 안 링크), MCP·엔진은 `hyperlink.jump` 계열로, 데스크톱은 Ctrl+클릭으로 따라갑니다. PDF에서도 차례 항목과 상호 참조를 누르면 그 쪽으로 갑니다(0.19, RHWP 패치 0039; 모아 찍기 인쇄본과 PDF에 없는 쪽으로 가는 링크는 빠짐), 환경 설정. 상호 참조는 개요·캡션 달린 표와 그림·책갈피가 있는 쪽 번호, 표의 캡션 번호(한글처럼 숫자만)와 캡션 전체를 넣고 새로 고칩니다(한글과 같은 상호 참조 필드로 HWP/HWPX에 저장). 한글이 HWPX 저장본의 표 번호·캡션 참조 필드를 그렇게 읽는지는 아직 확인하지 못했습니다. 그림·개요의 번호·제목 참조는 아직 없고, 표 셀 안이나 표·그림·책갈피·머리말·쪽 번호 같은 개체가 든 문단에는 넣지 못합니다. 키 매크로는 편집 명령·키 입력·캐럿/셀 이동을 기록해 이름으로 저장·실행하며(마우스로 옮긴 캐럿은 기록하지 않음), 실행 중 실패하면 실행 전으로 되돌립니다. 문서 비교·한자 변환·상용구는 아래에 적은 제한 범위에서 지원합니다.
- 보기: 안내선. 창 배열은 주 모니터 기준이고(작업 표시줄 자리는 아래 48 px로 어림), 데스크톱 MCP 연결은 한 번에 한 창만 켤 수 있습니다. 문서 보기 색은 화면 후처리로 지원합니다. 파일: 한컴 기본 서식(문서마당은 사용자가 저장한 서식만 제공), HML 원본이 아닌 문서의 HML 저장. 자동 복구는 바뀐 문서를 쉴 때 5분마다 설정 폴더 `recovery`에 사본으로 저장하고, 비정상 종료 뒤 다음 시작 때 복구를 제안합니다(원본 파일은 건드리지 않음).
- 쪽: 본문 세로쓰기(표 셀·글상자 세로쓰기는 지원), 구역 시작 위치, 원고지, 머리말/꼬리말 날짜 필드(쪽 번호 바깥쪽·안쪽 위치는 지원). 인쇄: 프린터 목록(종이 인쇄는 운영체제 기본 프린터 창 사용). 라벨(A4 규격 표)·모아 찍기(1·2·4·8쪽)·워터마크(인쇄물에만)·홀짝·역순·부수는 지원합니다.
- 서식: 사용자 정의 번호/글머리표 모양, 각주 안 글자 모양(문단 모양만 가능). 스타일 가져오기/내보내기와 스타일마당은 HwpCraft JSON 형식이며, 한글의 스타일마당 파일(.sty)과 한글 기본 스타일마당 목록은 쓰지 않습니다.
- 표·개체: 블록만큼 가로 경계선 옮기기, 중첩 표의 캔버스 셀 블록 선택(중첩 표 격자선 끌기는 `context.table.move_border`로 지원), 사진 편집, 글맵시·일반 OLE 생성은 미지원입니다. 새 차트는 위의 15종·본문 명령 범위입니다. 입체 꺾은선(line_3d)·표면형(surface)은 미지원이고, 거품 크기는 `chart.configure`로만 바꿉니다(`chart.data`는 X·Y 값). 데스크톱 차트 창은 혼합형·영역형·방사형·거품형을 아직 만들 수 없습니다(MCP·CLI로 작성). 중첩/그룹 도넛 표시와 한컴 네이티브 호환성은 미완료입니다. 수식 시각 편집은 미리 보기 위 커서·입력·빈 칸 이동까지이며 끌어서 범위 선택은 없습니다. 세로 글상자·개체 맞춤·개체 선택 단추는 기본 경로를 지원합니다. 연결선 부착 좌표는 종이 기준 개체에 정확하고 문단 기준 개체는 어긋날 수 있으며, 직선 끝점 편집은 글자처럼 취급한 직선·연결선을 제외합니다.
- HwpCraft에서 새로 만든 문서(또는 HwpCraft가 저장한 HWPX를 연 문서)는 HWPX로 저장할 때 줄 배치 정보(linesegarray) 없이 저장되어, 한글과 HwpCraft가 열 때 다시 조판합니다. 한글에서 연 문서는 원본 줄 배치를 보존하지만, 많이 고친 문단에는 RHWP가 계산한 낡은 줄 배치가 남을 수 있습니다.
- 탭 ∨ 목록은 한글 하위 메뉴가 아닌 리본 명령 목록입니다. 상태 표시줄 '단'은 항상 1입니다.
- 표 직접 이동은 한 쪽 안의 최상위 표가 대상입니다. 글자처럼 취급한 표는 한글처럼 놓은 본문 글자 자리로 옮겨지고(`object.move_inline`), 떠 있는 표는 위치가 옮겨집니다. 여러 쪽에 걸친 표는 속성으로 조절하세요. 셀 간격이 있는 표도 표 그리기·지우기가 되며, 좌표는 셀 간격을 뺀 셀 격자(`table.info`)입니다. RHWP는 셀 간격을 앞 셀 안에 넣어 그려 셀 사이 틈이 보이지 않습니다.

- 표 안의 표(중첩 표) 셀과 글상자는 0.19부터 쪽 화면에서 바로 편집합니다: 그곳을 누르면 캐럿이 놓이고 글 입력(한글 입력기 포함)·Backspace·Delete·Enter(문단 나누기)·방향키·Home/End·Shift 선택·Ctrl+Z·Ctrl+B/I/U·복사·붙여넣기가 되며, 편집마다 실행 취소 한 단계입니다(RHWP 패치 0046). Home/End는 줄의 처음·끝, 더블/트리플 클릭은 낱말·문단 선택, 중첩 표에서 Tab/Shift+Tab은 다음·앞 셀(글상자에서는 탭 문자)이고, 리본·서식 도구 줄의 글자·문단 서식 단추도 그 캐럿에 적용됩니다. Esc나 본문 클릭으로 본문 캐럿에 돌아갑니다. 글상자는 한글처럼 글 위를 누르면 캐럿이, 테두리 근처(4 px 안)를 누르면 개체 선택이 되고, 선택한 글상자를 끌면 옮겨집니다. 선택은 한 문단 안에서만 되고, 글자 모양·문단 모양 대화상자는 아직 본문 캐럿 기준입니다. 묶음 안 글상자와 아래 영역은 다음 메뉴로 고칩니다.
- 중첩 표와 머리말·꼬리말·글상자·주석은 편집 탭의 **문서 영역 편집** 메뉴에서 문자/문단 서식·나누기/합치기·표 생성/삭제를 수행합니다. 글자처럼 취급한 개체는 끌어서 다른 글자 자리로 옮기고(`object.move_inline`, 메모·누름틀 시작에서는 안쪽·끝에서는 바깥쪽), 여러 개 선택한 떠 있는 개체는 함께 옮깁니다(`object.move_by`). 세로 글상자도 같은 경로로 고칠 수 있고(`context.v2.*`), 세로 머리말·꼬리말과 세로쓰기 구역은 아직 읽기 전용입니다. 공개 인자 `CharShape`·`ParagraphShape`·`TableCreate`는 `hwp.parameters.schema`로 지원 필드를 조회할 수 있습니다.

### 남아 있는 조판·저장 한계

- 메모 해결 상태는 한글에 없는 기능이라 두지 않습니다. 여러 문단에 걸친 메모는 아직 만들 수 없습니다(열어 둔 것은 보존). 댓글이 있는 메모를 HWPX로 쓸 때 끝 표식(fieldEnd) 순서가 한글과 다릅니다(HwpCraft는 바깥 메모부터 닫고 한글은 안쪽부터 닫음, 짝은 beginIDRef로 맞음). 한글이 저장할 때 마지막 저장 날짜 필드의 글을 바꾸는지는 확인하지 못했습니다. HwpCraft는 저장 때 요약 정보의 시각만 갱신하고 필드 글은 새로 고침 명령으로 바꿉니다. 날짜 필드는 한글 표본처럼 월·일·시에 0을 채우지 않고, HWPX에는 확인된 Command 매개 변수만 씁니다.

- 큰 표가 많은 일부 HWP 문서는 쪽 나눔이 한글과 1–3쪽 다를 수 있습니다. 예를 들어 행정업무운영 편람은 .hwp가 381쪽, 같은 문서의 .hwpx와 한글은 383–384쪽입니다(한글 쪽 수도 설치된 글꼴에 따라 다름). RHWP의 HWP5 조판 경로에서 생기는 편차로, RHWP에서 추적 중인 알려진 문제(#6842)입니다.
- 그림 효과(그림자·네온·부드러운 가장자리·반사)는 `picture.effects`로 넣을 수 있고, HWPX에서 저장·다시 열기·그리기를 지원합니다. HWP 저장은 아직 효과를 쓰지 못해 효과가 있는 문서(HWPX에서 읽은 효과 포함)의 .hwp 저장은 기본으로 거절하며, 명시하면(MCP `dropPictureEffects`, CLI `--drop-picture-effects`, 데스크톱 확인 창) 효과를 빼고 저장합니다. .hwp에서 연 효과는 원본 그대로 다시 씁니다. 네온·부드러운 가장자리·반사 모양은 아직 한글과 대조하지 않았습니다.
- DOCX 내보내기는 글자·문단 모양, 표(병합·테두리·배경), 그림, 수식(그림으로), 머리말·꼬리말과 쪽 번호, 각주·미주, 다단을 씁니다. 본문을 밀어내는 도형·글상자·차트는 쪽 화면에서 잘라 그림으로 넣고, 글 뒤·글 앞 도형과 표·머리말 안 도형은 넣지 않습니다. 떠 있는 그림·표는 제자리(글자처럼)에 놓이며, 결과에 담지 못한 것 목록을 돌려줍니다(MCP `omitted`, CLI 오류 출력, 데스크톱 메시지 줄). DOCX 가져오기와 ODT는 아직 지원하지 않습니다.
- 줄 번호: 구역마다 켜고 시작 번호·번호 간격·본문과의 거리·번호 매기기(쪽마다·구역마다·이어서)를 정하며(`page.line_numbers`, 쪽 → 줄 번호, 구역 설정), 화면·PDF의 본문 줄에 번호를 그립니다(RHWP 패치 0020·0037). 설정은 HWP·HWPX 어느 쪽으로 저장해도 남습니다. 표·글상자·각주·머리말/꼬리말 안의 줄은 세지 않고, 번호의 위치·글꼴은 한글 화면과 대조하지 못했습니다. 번호 매기기 값(`restartType`)의 뜻은 공개 문서에 없어 한글 자동화 표(`LineNumberRestart`)를 따랐습니다(미확인). PDF에서는 번호가 글자로 들어가 글자 추출 결과에도 나옵니다(쪽 글 뒤에). 문단의 "줄 번호 건너뜀"(쪽 → 줄 번호 → 현재 문단 건너뜀, `page.line_numbers.skip`)은 HWPX에서 읽고 저장하고 그릴 때 지킵니다(RHWP 패치 0042). HWP 파일에서의 자리는 공개된 HWP 5.0 문서에 없어 HWP는 읽지도 쓰지도 않으므로, HWPX 원본을 HWP로 저장하면 건너뜀이 사라집니다.
- 문단 테두리 위·아래 간격이 줄 배치에 반영되지 않아 화면과 한글에서 연 저장본 모두 문단이 겹칠 수 있습니다. 좌우 간격과 일부 선 종류(이중선 등)는 단선으로 그려집니다.
- 바깥 여백이 있는 개체는 여백만큼 다른 자리에 그립니다(저장본은 한글에서 정상).
- 바탕쪽 그림·도형은 바탕쪽 창과 엔진·MCP(`page.master.object.*`)로 종이 위 자리(mm)에 넣고 지웁니다. 크기·자리를 바꾸려면 지우고 다시 넣어야 하고, 표가 든 바탕쪽은 글을 고칠 수 없습니다.
- 한글에서 만든 HWPX를 편집해 HWPX로 다시 저장하면, 편집한 문단의 줄 배치는 한글이 다시 조판하도록 비워 둡니다. 그래서 HwpCraft로 다시 열면 쪽 나눔이 편집할 때와 달라질 수 있습니다(HWP로 저장하면 그대로입니다).
- HwpCraft에서 새로 만든 문서를 HWP로 저장하면 줄 배치를 구현 행(HWP 줄 정보 bit31 '구현상 편의 속성')으로, HWPX로 다시 읽을 때와 같은 새 조판 값으로 저장합니다. 한글이 이런 HWP를 여는 모습은 아직 실측하지 못했습니다(RHWP 샘플 HWP 2개에 같은 표시가 있음).
- 형광펜은 markpen으로 저장하고 HwpCraft가 공통 SVG 경로에 합성해 화면·PNG·PDF/인쇄에 표시합니다. 좌표 지원 범위는 본문과 1단계 표 셀이며, 중첩 표·머리말·글상자·주석 내부는 지원하지 않습니다. 새 번호 목록은 새 번호 정의로 저장합니다(RHWP `set_numbering_restart`는 저장되지 않음). 곡선은 촘촘한 직선 구간으로 저장합니다.
- 새로 만들거나 편집한 중첩 표는 조상 셀의 필요한 높이를 보정합니다. 가져온 문서의 짧은 셀 프레임은 화면 표시용으로만 보정하며(원본·저장본은 그대로), 열기가 멈추지 않도록 문서마다 정해진 작업량 안에서만 보정합니다. 한도를 넘는 문서와 128쪽을 넘는 문서는 일부 또는 전체 표를 원본 배치 그대로 표시하므로 깊은 중첩 표가 잘려 보일 수 있습니다.
- 행·열 수가 실제 셀보다 작게 기록된 손상 표는 열 때 실제 셀 범위로 보정합니다.
- 본문 쪽 번호·전체 쪽수는 글자 모양과 여러 자리 폭을 반영해 화면·PNG·PDF에 동적으로 표시합니다. 표/글상자/주석 안의 동적 번호와 복잡한 조판은 아직 별도 검증·구현 대상입니다. 번호로 쪽 경계가 달라지면 HWP/HWPX/TXT 쪽 추출은 거절하며 PDF 쪽 저장은 지원합니다. 쪽 번호 필드가 있는 문서는 본문 글자 입력·삭제만 해당 문단을 다시 배치하고(381쪽 문서 약 40 ms), 문단 나누기 등 다른 편집은 표시용 문서 전체를 다시 조판합니다(약 0.3초). 날짜 코드는 현재 날짜 글로 넣습니다.
- 표: 셀 음영 진하게/연하게는 한글처럼 ±1단계에서 멈추지 않고 누를 때마다 적용됩니다. 셀 블록 안 세로 경계선을 끌면 한글처럼 블록의 줄에서만 옮겨지지만(`table.move_border`의 `range`), 가로 경계선은 블록만큼 옮기지 못합니다. 이렇게 어긋난 표에 줄·칸을 넣으면 새 줄도 표 격자에 맞춰집니다(RHWP 패치 0027). 표 마당의 글자 모양 적용은 없습니다. 사용자 표 마당(`table.user_style.*`, 설정 폴더 table_styles)은 테두리·배경·대각선만 셀 위치(첫/가운데 홀짝/마지막 줄·칸)별로 저장하고 글자 모양은 저장하지 않습니다. 표 만들기 창에서는 표 마당을 고를 수 없습니다(표를 만든 뒤 표 디자인 탭에서 적용). 개체: 그림 그림자·네온·옅은 테두리·반사(RHWP는 보존만), 그리기 마당·안내선·격자·서식 저장은 없습니다. 표 셀 안에서 넣은 그림은 그 셀 왼쪽 위에 떠 있는 그림(어울림)으로 들어갑니다. 셀 안에 글자처럼 넣기는 아직 없습니다. 자유선은 끄는 대로, 곡선은 한글처럼 점을 찍어 그리며, 둘 다 짧은 선분 곡선으로 저장합니다. 바깥 여백·글상자 여백 사전 값과 그림자 이동 단위는 한글 수치를 확인하지 못한 가정값입니다. 각주 번호 '쪽마다 새로'는 화면·PDF에서 각주가 있는 쪽마다 시작 번호부터 다시 매기고(본문 각주 표시와 각주 영역 모두, RHWP 패치 0036), 파일에는 이어지는 번호를 저장합니다. 각주·미주 구분선 길이는 한글 사전 값 5 cm(−1)와 미주의 단 너비(−4), 직접 지정 0–115 mm(HWP 16비트 한도)만 받습니다.
- 쪽 복사/지우기는 그 쪽에 놓인 글 범위대로 합니다(여러 쪽에 걸친 문단은 그 쪽 부분만). 끼워 넣기는 파일 메뉴에서만(Ctrl+O는 불러오기). PDF 인쇄 옵션(형광펜·그리기 개체·그림·메모 포함)은 RHWP에 옵션이 없어 제공하지 않습니다. 문서 이력 관리(검토 탭, `review.history.list`)는 한글이 HWP에 기록한 버전(날짜·작성자·설명)을 보여 주기만 하고, 마지막 이력 버전을 HML로 따로 저장할 수 있습니다(`review.history.save_last`). 새 버전 기록, 이전 버전 보기·되돌리기(버전 사이 차이 적용)는 하지 않으며, 그 HML은 한글이 HWPML 2.5로 기록해 HwpCraft가 열지 못합니다. 기록은 HWP로 저장해도 그대로 남고, HWPX의 이력은 읽지 않습니다. 쪽 윤곽(Ctrl+G,L)을 끄면 쪽마다 본문만(위·아래 여백, 머리말·꼬리말·쪽 번호 없이) 이어 보이고 쪽 경계는 점선입니다. 화면에서만 바뀌고 왼쪽·오른쪽 여백은 그대로입니다. 격자는 화면에만 그리는 5 mm 선입니다. 맞쪽 보기는 쪽 배치만 바꾸며 쪽 번호 안/바깥 구분은 없습니다.
- 글 상용구 외에 서식·표·내장 그림·기본 도형을 포함한 본문 상용구를 별도 저장소로 지원합니다. 본문 캐럿 자리에 넣습니다(0.19부터 문단 중간도: 표 없는 한 문단은 글 사이에 이어지고 여러 문단은 나눈 문단 사이에 들어감). 표 셀 안 넣기, 그룹·차트/OLE 조각과 범용 외부 교환은 미지원입니다.
- 한자 변환은 Unicode Unihan 독음표로 음절마다 후보를 고르는 방식이며 낱말 사전이 없어 아래한글처럼 낱말 단위 후보·빈도순 정렬은 하지 않습니다. 변경 내용 추적·맞춤법 검사는 미지원이며(변경 추적 기록이 있는 HWP/HWPX 문서는 지금 보이는 결과본으로 열립니다. 변경 추적 기록은 보존하지 못하므로 HWP/HWPX로 저장하면 기본으로 거절하고, 사용자가 허락하면(데스크톱 확인 창, MCP `acceptTrackChanges`, CLI `--accept-track-changes`) 변경 내용을 결과본으로 합치고 변경 기록을 지운 뒤 저장합니다. PDF·TXT 내보내기는 그대로 되며, 자동 복구 사본은 결과본 기준입니다), 메모는 본문·표 셀 범위의 CRUD·작성자·댓글까지 지원합니다(해결 상태는 한글에도 없고, 여러 문단 범위 메모는 만들 수 없음). 문서 비교는 추가·삭제·변경 문단 목록과 현재 글·비교 파일 글을 나란히 놓은 목록을 보여 주고, 고른 차이를 비교 파일 쪽으로 병합합니다(0.19, `review.merge.plan`·`review.merge`, 서식·표·그림도 가져오고 한 번에 되돌림). 두 문서를 화면에 나란히 놓고 함께 넘겨 보는 보기와 변경 추적으로 병합하기는 없고, 구역 수가 다르면 같은 번호의 구역끼리만 맞대며, 개체가 있는 구역 첫 문단은 병합으로 지우지 않습니다. 음영 없음은 RHWP가 흰색(#ffffff)으로 보고합니다.
- 구역 나누기(`page.section_break`)는 캐럿 자리부터 새 구역을 만들고 쪽 설정·단 설정을 이어받습니다. 머리말·꼬리말은 한글처럼 다음 구역으로 이어집니다. 한글은 새 구역에 바탕쪽을 두지 않고 앞 구역 것을 이어 쓰지만 RHWP는 구역을 넘어 바탕쪽을 그리지 않아, HwpCraft는 바탕쪽을 새 구역에 복사합니다(한글에서도 같게 보이나, 이후 한 구역의 바탕쪽을 고쳐도 다른 구역은 따라 바뀌지 않습니다). 같은 이유로 한글에서 나눈 문서의 뒤 구역에는 앞 구역 바탕쪽이 보이지 않습니다. 구역 첫 문단 처음에서 Backspace, 앞 구역 끝에서 Delete를 누르면 두 구역이 합쳐지고 앞 구역의 설정·바탕쪽을 따릅니다(문단은 그대로 두고, 한 번 더 누르면 문단이 합쳐집니다).
- 바탕쪽은 양쪽·홀수 쪽·짝수 쪽 바탕쪽의 글만 만들고 고치고 지울 수 있습니다(`page.master.get/set/delete`의 `applyTo`). 한글처럼 홀수·짝수 바탕쪽은 그 쪽에서 양쪽 바탕쪽 대신 쓰이고, 저장 순서와 구역 플래그도 한글과 같습니다. 마지막 쪽·임의 쪽·그림이나 도형이 든 바탕쪽은 보기와 저장만 됩니다. 홀수 쪽이나 짝수 쪽 바탕쪽 하나만 있는 구역은 한글이 그 쪽에만 그리지만 RHWP는 모든 쪽에 그립니다. 문서 요약 정보, 쪽 테두리의 홀수/짝수·첫 쪽만·그림 채우기, 양식 개체 생성, 글맵시·일반 OLE 생성은 미지원입니다. 차트는 위의 자체 11종 생성·구성 명령 범위와 한계를 따릅니다. 스타일 추가·고치기·지우기는 RHWP wasm 래퍼에만 있어 HwpCraft가 모델을 직접 고칩니다. 줄 단위 커서 이동·선택 영역 좌표 조회는 RHWP 비공개라 캐럿 좌표로 계산합니다.
- 탭 폭 자동 기록은 본문과 1단계 표 셀만 대상입니다. 머리말·글상자·주석 안의 탭은 폭 0으로 저장됩니다.
- 여러 문단 선택은 한 구역 본문 또는 한 셀 안에서만 되며, 전체 선택은 현재 구역입니다. 블록 저장(`file.save_block`)도 같은 범위(한 구역 본문)만 씁니다.

### 검증 한계

- 인쇄는 HwpCraft가 쪽 범위·홀짝·역순·여러 벌 모아 찍기·용지 방향·2/4/8쪽 모아 찍기·워터마크를 적용한 PDF를 만들어 운영체제에 넘깁니다. 그 PDF는 0.19에서 44쪽 보고서로 옵션마다 장 수와 각 장의 쪽을 HwpCraft PDF 저장본과 대조했습니다(가로 원본의 2·8쪽 모아 찍기는 세로 장에 위아래로). 그 뒤 드라이버까지는 확인하지 못했습니다. 이 검증 PC는 기본 프린터가 종이 프린터이고, Windows 11 기본 PDF 앱(Edge)에 인쇄 동사가 없어 HwpCraft는 PDF를 열어 주기만 합니다(부수는 그 창에서 다시 정함). Microsoft Print to PDF는 파일 이름 창이 떠서 자동으로 보내지 못했습니다.
- 표 선택·이동·크기 조절에는 포인터 이벤트 회귀 검사가 있습니다. 앱 전체 기능의 클릭·끌기 E2E 검증을 완료한 것은 아닙니다.
- 복잡한 실제 문서의 편집 후 왕복 저장, 대용량 문서·표 성능은 충분히 검사하지 않았습니다. 장시간 입력은 0.19에서 데스크톱 앱(release)에 IME 조합·확정 이벤트로 1만 자씩 넣어 보았습니다(`--view bench-ime`). 44쪽 보고서와 381쪽 편람 모두 메모리는 실행 취소 40단계 상한 덕분에 평탄했고(약 370 MB / 830 MB), 저장·재열기한 보고서는 49쪽이 모두 같았습니다. 실제 Windows IME로 치는 시험은 대화형 화면이 없는 검증 환경이라 하지 못했습니다. 중요한 문서는 원본을 유지하고 다른 이름으로 저장한 뒤 한글에서도 확인하세요.
- 문서 입력 크기는 128 MB, 한 번의 텍스트 삽입은 1 MB로 제한합니다. 파싱·렌더링이 동기식이라 복잡한 문서에서 UI가 잠시 멈출 수 있습니다. 큰 문서의 PDF 저장은 시간이 걸립니다(381쪽 문서 약 17초, 쪽을 최대 8개 스레드에서 나눠 변환하며 이때 메모리를 약 1.9 GB까지 씁니다. `RHWP_PDF_THREADS`로 스레드 수를 조절합니다).

릴리스 노트는 `docs/`에 있습니다.

## 라이선스

HwpCraft는 **MIT 라이선스**로 배포합니다. 전문은 [LICENSE](LICENSE)에 있으며, 저작권자는 HwpCraft 기여자들입니다.

**MIT 라이선스가 적용되는 범위** — 이 저장소에 들어 있는 HwpCraft의 모든 산출물입니다.

- 소스 코드: `crates/`, `apps/`의 Rust 코드와 빌드·패키징 스크립트(`HwpCraft.ps1`, `hwpcraft.sh`).
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

Current version: **0.19**. Versions increase as `0.10 → 0.11 → 0.12 …`. 0.15 and 0.16 added body memo creation/editing/deletion, address-based nested table editing and creation/deletion, dynamic body page fields in shared screen/PDF output, and a subset of public automation parameters. Full Hangul feature parity has not been established. 0.17 covers pending input formatting, document-region editing of headers, footers, text boxes and notes, the MCP 2026-07-28 service (per-document workers, HTTP token authentication), opening speed, edit latency, PDF export speed and UI cleanup, and Windows / macOS / Linux packages. 0.18 adds creation of eleven chart kinds, atomic batches that also restore the undo history and clipboards on failure, the MCP Tasks extension, output schemas and progress notifications, faster editing, opening and PDF export, and a cleaner ribbon, memo panel and dialogs. 0.19 widens Hangul editing and the MCP commands that match it: tables of contents for headings, tables and pictures and cross references (Hangul's own HWP/HWPX fields, links that also work in PDF), memo replies and memos in table cells, date fields, merging after document compare, style sets, ruby text, drop caps and picture bullets, form object creation, visual equation editing and equation macros, 15 chart kinds (stacked, combo, area, radar, bubble) and picture effects, master-page text, section and column breaks, line numbers (settings, drawn on screen and in PDF), user table styles, print options (odd / even, reverse, n-up, watermark) and print preview, auto-recovery and templates, split view, new windows and page outline off, keyboard macros, an MCP connection to the running desktop window, DOCX export, and a layout fix for new documents whose pages moved when saved as HWP and reopened. Performance: replace-all of 1,445 matches in the handbook 81–90 s → 2.7 s, the handbook PDF 48 MB → 9.2 MB (same time, identical pixels; shared fonts and merged duplicate resources), inserting a linked table of contents for 1,500 headings 18.6 s → 1.25 s, typing in a 44-page report 75 → 32 ms per frame (text outside the view is no longer parsed) and 18.3 → 2.3 ms to take one typed character (a section's typesetting is reused when the edited paragraph keeps its lines, RHWP patch 0031), and MCP tool results about 40% faster (no second JSON parse). MCP: `insert.table` returns the new table's address, `hyperlink.jump` follows links inside the document, `list_documents` and a connection to the running desktop window, a [report-writing recipe](docs/mcp-commands.md#보고서-작성-순서), find and replace across headers, footers, footnotes, endnotes, text boxes and nested cells in one go (`include` on `edit.find_all` / `edit.replace_all`, one undo step), writing many table cells at once (`table.cells.set`) and row/column edits by cell address (`table.edit`), page setup for every section (`apply_to` on `page.setup`), and Hangul ParameterSets for inserting text, page setup and find/replace (`hwp_action` InsertText, PageSetup, AllReplace and others). Fixed: `edit.find_all` gave matches inside text boxes, nested cells and equation scripts as body or table-cell caret positions, so editing there changed the wrong text. Interface: the ribbon, tab row and formatting toolbar no longer cut off in narrow windows down to 400 px (» more), key tips also on collapsed groups and the more menu, greyed buttons explain why in a tooltip, and save / error boxes take Alt shortcuts and focus their first input. Details: [0.19 release](docs/RELEASE-0.19.md), [0.18 release](docs/RELEASE-0.18.md), [0.17 release](docs/RELEASE-0.17.md), [0.16 release](docs/RELEASE-0.16.md), [0.15 release](docs/RELEASE-0.15.md), [feature roadmap](docs/FEATURE-ROADMAP.md).

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
- Sort paragraphs, remove control marks by type, page break (Ctrl+Enter), column break (Ctrl+Shift+Enter), bookmarks, HTTP/HTTPS hyperlinks (insert, edit address / text, remove) and following a link with Ctrl+click (an in-document link moves the caret there; only http, https and mailto addresses go to the browser or mail program, never files, scripts or other schemes), context menu.
- Character map (Ctrl+F10) with Hangul's four tables — 사용자 (favourites), 유니코드 (160+ Unicode 15 blocks), 한글(HNC) (30 curated areas: symbols, maths, units, circled / parenthesised, box drawing, shapes, arrows, dingbats, Greek, Cyrillic, kana, Roman numerals / fractions, super- / subscripts, letterlike, jamo incl. old Hangul, IPA, full- / half-width, CJK punctuation / units, radicals, hexagrams, braille, music, currency, technical), 완성형(KS X 1001) (symbol rows, 2,350 hangul, 4,888 hanja) — plus emoji (10 areas), named special characters (spaces, dashes, marks) and a search by name, `U+` code or keyword. Multi-character input box, recent characters, a font choice for the inserted characters, Unicode / KS code display, and installed symbol / emoji fonts for glyph coverage. The same catalogue is available to the CLI / MCP (`symbol.sets`, `symbol.list`, `symbol.search`, `insert.symbol`).
- Hangul-style two-key shortcuts (Ctrl+K,H · Ctrl+K,N · Ctrl+K,O · Ctrl+N,P/H/T · Ctrl+G,… · Ctrl+Q,I) plus the standard format shortcuts and Ctrl+1…0.
- Tab stops are saved with widths computed the Hangul way (left, right, centre, decimal, leader) so they land in the same place when opened in Hangul.
- Memos on a text range or at the caret in the body and in table cells: add, edit and remove. Replies (`memo.reply`, Hangul's 댓글) are stored as Hangul stores them, as memos nested on the same text, and keep their thread (`replyTo`, `thread` in `memo.list`) through HWP and HWPX reopening; removing a thread's first memo removes its replies. **A memo in a table cell lost its content when saved as HWP up to 0.18; fixed in 0.19.**
- Date fields for the document's creation and last-save dates (`insert.date_field`, the same document-summary field as Hangul's 날짜 코드 and 마지막 저장 날짜). Since 0.19 an HWP/HWPX save records the creation time (when the document has none) and the save time in the document summary (HWPX content.hpf, HWP summary information), using `SOURCE_DATE_EPOCH` when set; recovery copies, block saves, PDF and DOCX are not stamped. `tools.date_field.update` refreshes the field text.
- Style sets (서식 > 스타일마당, Alt+T, `format.style.set.*`): pick a set to restyle the document in one undo step (styles of the same name are updated, missing ones added). The built-in sets 보고서, 공문 and 간결 are HwpCraft's own and change sizes, weight, spacing and alignment while keeping the fonts; the current document's styles can be kept as a user set in the settings folder's `style_sets` and removed again.

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
- Equation editor with Hangul-style structure and symbol palettes and live preview (click the preview to type in place, Tab between empty slots, equation macros `equation.macro.*`, piece layout `equation.layout`), multi-paragraph footnotes and endnotes, editing of existing chart values, memo list. The shared engine, CLI and MCP `chart.create`/`chart.configure` author 15 body chart kinds: column, bar, line, pie, doughnut, scatter, two stock kinds, three 3D kinds, combo (columns + lines, secondary axis), area, radar and bubble; column, bar, line and area take stacked / percent-stacked `grouping`. `chart.types` lists each kind's limits and `chart.definition.get` returns the current configuration. HWP/HWPX round trips and SVG/PDF checks are HwpCraft's own; compatibility with Hancom's native charts is still being verified.
- Tables and objects (0.14): in-table cell navigation (Tab / Shift+Tab, append a row from the last cell, `table.cell_move`), table-style options (header row, first / last column, last row, grayscale, borders, background), darker / lighter cell shading, user-list auto fill, eleven object alignments, size, rotation, size lock, outer-margin presets, quick fill / outline / arrow styles, shadow nudging, object format copy / paste, Tab cycling through objects, the object-selection button, connector type change, nine-way text-box alignment, text-box margins and vertical text boxes, save pictures to files and recolour (grayscale, black-and-white), auto numbers (page, total, picture, table, equation), footnote ↔ endnote conversion with note shape (separator, number format), note and field deletion.
- Picture insertion (`picture.insert`) treats the picture as a character when `treat_as_char` is omitted, like Hangul (since 0.19; a floating picture before). With `false` it is anchored at the caret paragraph, at the column's left edge, with square wrapping so text flows beside it. Caption text is set with `object.caption_text` for pictures, shapes and charts and with `table.caption_text` for tables.
- Form objects (`form.insert`): check boxes, input boxes, combo boxes, radio buttons and push buttons go in at the caret (body or table cell) as characters and `form.set` edits their values. Sizes, colours and properties follow the values Hangul writes for a new object, without a border fill (frame only). On the desktop the Insert tab's "양식 개체" menu opens an insert dialog for each kind. Detailed properties (font, multi-line input, read-only) cannot be edited yet.
- Object operations (0.13): group / ungroup (Shift+click multi-select), bring to front / send to back / forward / backward, object protection (locked objects refuse drag, delete and edits) and unlock all, horizontal / vertical flip, six connector kinds drawn shape-to-shape and re-routed when a shape moves (also to paragraph-relative objects and objects treated as characters, from where the page draws them; both objects must be on one page, and a connector re-routes when its object is moved or edited, not when body editing pushes a paragraph-relative object along), line end-point handles, click-here fields (누름틀) with name / value editing, and value editing of existing form objects.

### Page, view and file services

- Ctrl+wheel zoom (10–500 %) over the document, wheel-only zoom over the status-bar zoom indicator.
- Page setup (F7), page numbering (11 positions, 10 formats), headers / footers (11 templates × both / odd / even), page borders and backgrounds, restart numbering, hide current page, columns.
- Document information, print (range, copies; via PDF to the Windows print verb or CUPS `lp` on macOS / Linux, falling back to the default PDF viewer, or a PDF file), zoom presets, fit page / fit width, show or hide rulers and document tabs.
- Continuous multi-page view like Hangul (14 px page gap), drawing only visible pages and scrolling with the caret.
- Several windows (0.19): View → new window (a separate process) and split view (top / bottom, left / right), switch window (pick another open HwpCraft window and ask it to come to the front — only the request reaching that window has been checked, not that it actually comes forward; where Windows refuses to bring another program's window forward, only its taskbar button may flash), and arrange windows (cascade, side by side, stacked). Windows find each other and pass requests through small files in the settings folder's `windows` directory.
- Hanja readings (0.19, View tab): on screen only, the Hangul reading of each hanja (its dictionary reading from the Unihan table, the education reading first) is drawn small above it. Readings go character by character, so word-level readings (such as the initial-sound rule) are not applied; rotated or vertical characters get none, and printing / PDF are unchanged. Position and size have not been compared with Hangul's screen.
- 0.14: six page-margin presets (values measured in Hangul), document view colours (default, dark, high contrast, classic, custom) and picture placeholders, facing pages, page copy / paste / delete, insert file (`file.insert`), save block, save pages as PNG, previous / next in the header-footer editor, document info (general, font substitution, pictures), previous / next section. Formatting (0.14): font size / width / line spacing / margin steps, font cycling, preset text / shade / highlight colours, character borders, indent at caret, move paragraph up / down, super- / subscript cycle, clear character style, update style from caret, style ordering and JSON import / export, character overlap. Review and tools (0.14): Hanja conversion (F9, Unihan readings), extended find (ignore spaces, wildcards, regex, any word, Hanja by Hangul reading, format conditions), document compare (RHWP docdiff), AutoText (Ctrl+F3, Alt+I). Editing (0.14): Go To dialog (Alt+G).
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

`hwpcraft-cli mcp` is a headless MCP server (stdio by default; `--transport http --listen 127.0.0.1:7082` serves loopback HTTP, where every request needs `Authorization: Bearer <token>` with the token read from `mcp-http-token` in the settings folder or `--auth-token-file`, created readable only by you if missing): create Korean documents, edit text / formatting / tables / objects, save HWP / HWPX, export PDF / TXT, render PNG for checking, and restrict read / write roots. 0.14 adds `hwp_action`, which accepts Hangul automation action names (`MoveLineEnd`, `CharShapeBold`, `DeleteWord`, about 300 of them) and runs the matching command, `batch {atomic:true}` that rolls every document change back when any command fails, and `get_selection`. Every tool declares an `outputSchema` for its structured result; for `execute` and `batch`, whose result depends on the command, only the common envelope is fixed. With **MCP 연결** (Tools tab) the desktop app serves the same protocol from its window on `127.0.0.1:7083` (same token); clients find the window's document with `list_documents` and edit it by that `document_id`, the window shows each edit at once and undo reverts it, and MCP cannot close it. It does not control Hancom software. See [docs/mcp.md](docs/mcp.md) and [docs/mcp-commands.md](docs/mcp-commands.md) (Korean).

## Build

Rust 1.98.1, pinned in `rust-toolchain.toml`. Windows x64 needs MSVC and the Visual Studio C++ build tools; macOS needs the Xcode command-line tools; Linux needs a C compiler plus X11 / Wayland and OpenGL development packages; file dialogs use xdg-desktop-portal (shipped by GNOME and KDE), so no GTK development packages are needed. `HwpCraft.ps1` (Windows) and `hwpcraft.sh` (macOS / Linux: `build`, `test`, `install`, `start`, `status`) wrap the same Cargo commands.

Windows installer: `scripts/build-installer.ps1` stages the same files as `HwpCraft.ps1 Install` and compiles `packaging/windows/HwpCraft.iss` with Inno Setup 6 into `output/installer/HwpCraft-<version>-windows-x64-setup.exe` (optional desktop shortcut and `.hwp` / `.hwpx` association).

Platform status (0.19): Windows x64 is fully tested, including clippy and the real desktop window. macOS (arm64 and x64) and Linux x86_64 run the workspace tests in the GitHub Actions release build; the desktop window has not been checked by hand on a Mac or a Linux desktop. Fonts come from a per-OS candidate list (Windows 맑은 고딕, macOS Apple SD Gothic Neo, Linux Noto Sans CJK KR / NanumGothic …), first installed wins. Linux needs a Korean font (`fonts-noto-cjk` or `fonts-nanum`); without one only Latin text and digits show, and Hangul appears only if another installed font covers it. The settings folder is `%APPDATA%\HwpCraft` on Windows, `~/Library/Application Support/HwpCraft` on macOS (an existing `~/.config/HwpCraft` keeps being used) and `$XDG_CONFIG_HOME/HwpCraft` or `~/.config/HwpCraft` on Linux. On macOS shortcuts use Cmd (⌘) instead of Ctrl, although menus and tooltips still read "Ctrl+…". The macOS app is not signed or notarized: on first launch right-click → Open, or run `xattr -d com.apple.quarantine HwpCraft.app`; opening documents by double-clicking in Finder is not supported (open them from the app). On Linux, unpack the bundle and run `./install.sh` to install into `~/.local`. Launcher registration and portable ZIP packaging exist in the Windows script only.

HwpCraft depends on RHWP as a **sibling path dependency** (`../rhwp`, pinned to RHWP **v0.8.7**, commit `1a76570e`). Clone both repositories next to each other:

```text
git clone https://github.com/webvine/hwpcraft.git
git clone --branch v0.8.7 https://github.com/edwardkim/rhwp.git
cd hwpcraft
cargo build --release --locked -p hwpcraft -p hwpcraft-cli
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

One engine test opens a chart sample from `../rhwp/samples`, so the RHWP checkout must include its `samples` folder. HwpCraft's local RHWP patches live in `patches/rhwp/`; apply them before building with `pwsh -File scripts/rhwp-patches.ps1 -Mode Apply` (`scripts/sync-siblings.ps1` re-applies them after updating RHWP). On Windows, `HwpCraft.ps1` (`Build`, `Test`, `Install`, `Start`, `Status`) wraps the same commands and uses `../.build/hwpcraft` as the Cargo target directory; the `Launcher` and `Install` actions target a local launcher that is not part of this repository.

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

- MCP: the desktop connection edits the one document open in the window; click / key / screen capture, remote authentication, select-all extension and a conformance check are not done. HTTP accepts loopback only (no standalone GET SSE stream) and uses an explicit ID per document; each MCP process is an independent headless session. Cancelling a request does not roll back a synchronous edit or file write that already ran; a save cancelled while running stops just before the file is replaced and leaves the existing file. Progress notifications for long operations report elapsed time every 5 seconds, not per-page progress. The Tasks extension applies to the open and save tools only, and task state is known only by polling tasks/get (no task notifications).
- Security: saving distribution documents (a distribution document opens read-only: viewing and page images only; editing, saving, copying, printing / PDF and text extraction are refused in the desktop, MCP and CLI), selected-text protection, removing private-information protection, changing its password and document security settings (Hangul's encrypted private-information field and these settings are not publicly specified, so they are not supported; such fields in existing documents are kept, and finding and masking private information is offered instead). Review/tools: proofreading marks, recording new document-history versions and viewing older ones (the recorded list is shown), track changes, memos across several paragraphs (memos have no resolved state in Hangul either), spelling, Hangul's built-in quick-correction list (a user list works for text typed in the desktop only, undone by one Ctrl+Z), user dictionaries, translation, script (JavaScript) macros, citations (an index works since 0.19: index marks after a word as Hangul stores them, added, edited and removed with first and second keys in HWP/HWPX, and an index at the caret in Hangul 2024's default 색인 만들기 layout — first-sound groups, 'word<tab>page, page', second keys indented; Hangul makes it in a new document, HwpCraft at the caret, and paragraphs holding index marks cannot be brought in by document merge or formatted auto text. A table of contents can be made from outline paragraphs or styles, and a list of tables or figures from captioned tables or pictures; all are written as ordinary paragraphs in "차례 N" / "표 차례" / "그림 차례" styles, not as Hangul's table-of-contents field, and one refresh updates all three; each entry links to a bookmark at what it lists, in the bookmark-link form Hangul-saved documents use and as an in-document link in DOCX, and MCP / the engine follow them with the `hyperlink.jump` family, the desktop with Ctrl+click; in PDF, table of contents entries and cross references link to their page too (0.19, RHWP patch 0039; dropped on n-up sheets and for pages outside the PDF)) and preferences. Cross references insert and refresh the page of an outline heading, a captioned table or picture, or a bookmark, and a table's caption number (the bare number, as Hangul shows it) or whole caption (stored as Hangul's own cross-reference field in HWP/HWPX). Whether Hangul reads the HWPX form of a table number or caption reference as such has not been confirmed yet. Number and heading-text references to pictures and headings are not available yet, and a cross reference cannot go inside a table cell or into a paragraph that holds an object such as a table, picture, bookmark, header or page number. Key macros record editing commands, typing and caret/cell moves (not caret moves made with the mouse), are saved by name and run with one rollback if a step fails. Document comparison, Hanja conversion and AutoText have the limited implementations described below.
- View: guides. Window arranging uses the main monitor (the task bar is assumed to take the bottom 48 px), and only one window at a time can turn on the desktop MCP connection. View colours use screen post-processing. File: Hancom's built-in templates (the template gallery offers only templates the user saved); HML export of documents not opened from HML. Auto-recovery saves a copy of a changed document every 5 minutes while idle in the `recovery` folder of the settings folder and offers it after an abnormal exit; the original file is never touched.
- Page: vertical body text (vertical text in table cells and text boxes works), section start position, manuscript paper, header / footer date field (outside / inside page-number positions are supported). Print: printer list (paper printing uses the system print dialog). Labels (A4 label tables), N-up (1, 2, 4, 8 pages per sheet), print-only watermarks, odd / even pages, reverse order and copies are supported.
- Format: custom numbering/bullet shapes. Character format inside footnotes (paragraph format only). Style import/export and style sets use HwpCraft JSON; Hangul's style-set files (.sty) and its built-in set list are not used.
- Tables and objects: moving a horizontal border for a cell block only, canvas cell-block selection inside nested tables (dragging a nested table's grid line works, `context.table.move_border`), photo editing, 3D line and surface charts, verified Hancom-native chart compatibility, WordArt and OLE. Bubble sizes change only through `chart.configure` (`chart.data` edits X/Y values); the desktop chart dialog cannot yet create combo, area, radar or bubble charts (author them through MCP or the CLI). Visual equation editing covers the caret, typing and slot navigation in the preview, without drag selection. Vertical text boxes, object alignment/selection and existing form values have basic support. Connector anchors are exact for paper-anchored objects and may drift for paragraph-anchored ones; line end-point editing excludes inline lines and connectors.
- Documents created in HwpCraft (or HWPX files HwpCraft wrote) are saved to HWPX without line layout (linesegarray), so Hangul and HwpCraft lay them out afresh when opening. Documents opened from Hangul keep their stored line layout, but heavily edited paragraphs may still carry stale layout computed by RHWP.
- Tab drop-down lists are ribbon command lists, not Hangul submenus. The status-bar column indicator is always 1.
- Direct table dragging targets top-level tables within one page. A table treated as a character goes to the body text position it is dropped on, as in Hangul (`object.move_inline`); a floating table moves by position. Multi-page tables are adjusted through properties. Table drawing and erasing work on tables with cell spacing too, on the cell grid of `table.info` (spacing not counted); RHWP draws the spacing inside the preceding cell, so no gap shows between cells.
- Cells of a table inside a table and text boxes are edited on the page since 0.19: a click there places the caret, and typing (including the Korean IME), Backspace, Delete, Enter (paragraph split), arrows, Home / End, Shift selection, Ctrl+Z, Ctrl+B / I / U, copy and paste work, each edit one undo step (RHWP patch 0046). Home / End go to the start / end of the line, a double / triple click selects the word / paragraph, Tab / Shift+Tab move to the next / previous cell of a nested table (a tab character in a text box), and the character and paragraph buttons of the ribbon and the formatting toolbar apply there too. Esc or a click in the body returns to the body caret. As in Hangul, a click on a text box's text places the caret there, a click near its frame (within 4 px) selects the box, and dragging a selected box moves it. A selection stays within one paragraph, and the character and paragraph format dialogs still follow the body caret. Text boxes inside groups and the regions below use the menu that follows.
- Nested tables, headers / footers, text boxes and notes are edited from the Edit tab's **문서 영역 편집** (edit document regions) menu: character / paragraph format, split / merge, table create / delete. An object treated as a character is dragged to another text position (`object.move_inline`; at a memo or form-field start it goes inside, at its end outside), and several selected floating objects move together (`object.move_by`). Vertical text boxes are edited the same way (`context.v2.*`); vertical headers / footers and vertical-text sections are still read-only. The public parameter sets `CharShape`, `ParagraphShape` and `TableCreate` list their supported fields through `hwp.parameters.schema`.

### Remaining typesetting and save limits

- Memos have no resolved state, as Hangul has none. A memo across several paragraphs cannot be created yet (existing ones are kept). For a memo with replies, the HWPX end markers (fieldEnd) close the outer memo first while Hangul closes the inner one first; they pair by beginIDRef. Whether Hangul rewrites a last-save date field's text when it saves is not confirmed; HwpCraft updates only the summary times on save and the field text on refresh. Date fields follow Hangul's samples (no zero padding for month, day or hour) and write only the confirmed Command parameter to HWPX.

- Some HWP documents with many large tables break pages 1–3 pages differently from Hangul: the 행정업무운영 편람 handbook is 381 pages as .hwp, while its .hwpx and Hangul give 383–384 (Hangul's own count depends on the installed fonts). The difference comes from RHWP's HWP5 layout path and is a known RHWP issue (#6842).
- Picture effects (shadow, glow, soft edge, reflection) can be set with `picture.effects` and are saved, reopened and drawn in HWPX. HWP saving cannot write them yet: saving a document that has them (including effects read from HWPX) as .hwp is refused unless asked to drop them (MCP `dropPictureEffects`, CLI `--drop-picture-effects`, a desktop confirmation); effects in a document opened from .hwp are written back as they were. Glow, soft-edge and reflection drawing has not been compared with Hangul yet.
- DOCX export writes character and paragraph formatting, tables (merges, borders, fills), pictures, equations (as pictures), headers and footers with page numbers, footnotes and endnotes, and columns. Shapes, text boxes and charts that push body text aside are cut from the page image and placed as pictures; shapes behind or in front of text and shapes inside tables or headers are left out. Floating pictures and tables are placed inline, and the result lists what could not be written (MCP `omitted`, CLI stderr, the desktop message bar). DOCX import and ODT are not supported yet.
- Line numbers: turned on per section with start, interval, distance from the body and numbering (per page, per section, continuing) (`page.line_numbers`, Page → 줄 번호, section settings), and drawn on body lines on screen and in PDF (RHWP patches 0020 and 0037). The settings are kept when saved as HWP or HWPX. Lines in tables, text boxes, notes, headers and footers are not counted, and the numbers' position and font have not been compared with Hangul's screen. The meaning of the numbering value (`restartType`) is not in the public documents; Hangul's automation table (`LineNumberRestart`) is followed (unconfirmed). In PDF the numbers are text, so text extraction returns them too (after the page's text). A paragraph's "skip line numbering" (Page → 줄 번호 → 현재 문단 건너뜀, `page.line_numbers.skip`) is read, saved and honoured in HWPX (RHWP patch 0042). Its place in HWP files is not in the public HWP 5.0 document, so HWP neither reads nor writes it: an HWPX original saved as HWP loses it.
- Paragraph border top / bottom spacing is not reflected in line layout, so paragraphs may overlap both on screen and in Hangul. Side spacing and some line styles (double lines) draw as single lines.
- Objects with outer margins are offset on screen (saved files open correctly in Hangul).
- Master-page pictures and shapes are placed on the paper (in mm) and removed from the master-page dialog and through the engine / MCP (`page.master.object.*`); to resize or move one, delete it and insert it again. A master page holding a table cannot have its text edited.
- When a Hangul-made HWPX is edited and saved as HWPX again, the edited paragraphs are written without line layout so that Hangul lays them out afresh; reopened in HwpCraft, its page breaks may differ from the edited document (an HWP save keeps them).
- A document made in HwpCraft and saved as HWP stores its line layout as implementation rows (HWP line-segment flag bit 31) with the fresh layout its HWPX would get when read. How Hangul opens such an HWP is not measured yet (two RHWP sample HWP files carry the same flag).
- Highlights are saved as markpen marks and composited by HwpCraft's shared SVG path for screen, PNG and PDF/print. Geometry covers body text and first-level cells; nested tables, headers, text boxes and notes are excluded. New numbered lists create new numbering definitions (RHWP's `set_numbering_restart` is not saved); curves are saved as dense polylines.
- Newly created/edited nested tables grow the necessary ancestor frames. Short frames in imported documents are normalized for display only (source and saved file unchanged) and within a fixed per-document work budget so opening never stalls. Documents over the budget or over 128 pages keep some or all tables in their original layout, so deeply nested content may still look clipped.
- Damaged tables whose stored row/column counts are smaller than their cells are repaired to the addressed extent on open.
- Body page/total fields reserve measured width and render dynamically in screen, PNG and PDF output. Dynamic fields inside tables/textboxes/notes and complex layout remain incomplete. HWP/HWPX/TXT page extraction refuses field-induced boundary differences; PDF page export is supported. In documents with page fields, body typing and deletion re-lay out only the edited paragraph (about 40 ms on a 381-page document); other edits such as paragraph splits re-lay out the whole display copy (about 0.3 s). Date codes currently insert literal text.
- Tables: darker / lighter cell shading applies on every press instead of stopping after one step like Hangul; dragging a vertical border inside a cell block moves it for the block's rows only (`range` of `table.move_border`), as in Hangul, but horizontal borders cannot move for a block; rows and columns inserted into such a ragged table follow its grid (RHWP patch 0027); table-style font application is missing; user table styles (`table.user_style.*`, settings folder table_styles) keep borders, fills and diagonal lines by cell role (first, odd/even body, last row/column) but not character shapes; and the table-creation dialog cannot pick a table style (apply one from the Table Design tab afterwards). An object treated as a character is dragged to another text position (`object.move_inline`; inside a memo or form field at its start, outside at its end), and several selected floating objects move together (`object.move_by`). Vertical text boxes are edited through the typed context commands (`context.v2.*`) like horizontal ones; vertical headers / footers and vertical sections stay read-only there. Objects: picture shadow / glow / soft edge / reflection (RHWP only preserves them), drawing gallery, guides, grid and saved formats are missing; a picture inserted in a table cell floats at that cell's top-left corner (square wrap); a picture treated as a character inside a cell is not supported yet; a free-form line follows the drag and a curve passes smoothly through clicked points as in Hangul, both stored as short line segments; outer-margin / text-box-margin presets and the shadow nudge unit are assumptions not verified against Hangul. Footnote "restart per page" renumbers from the start number on every page that has footnotes, on screen and in PDF (body marks and the footnote area, RHWP patch 0036); files keep running numbers. Note separator lengths take Hangul's presets 5 cm (−1) and, for endnotes, column width (−4), or 0–115 mm (the HWP 16-bit limit).
- Page copy / delete work on the text laid out on that page (a paragraph running over a page break contributes only its part on that page). Insert file lives in the File menu only (Ctrl+O stays Open). PDF print options (highlighter, drawing objects, pictures, memos) are not provided because RHWP has no option for them. Document history (Review tab, `review.history.list`) only lists the versions Hangul recorded in an HWP file (date, writer, description), and the last recorded version can be saved as HML (`review.history.save_last`). New versions are not recorded and older versions are not shown or restored (the differences between versions are not applied); that HML is HWPML 2.5, which HwpCraft cannot open. The record is kept when saved as HWP; HWPX history is not read. With the page outline turned off (Ctrl+G,L) each page shows only its body (no top / bottom margins, header, footer or page number) and pages run on with a dotted line between them; it changes the screen only and keeps the left and right margins. The grid is a screen-only 5 mm overlay. Facing pages only changes the page layout.
- Besides plain AutoText (상용구), body AutoText with formatting, tables, embedded pictures and basic shapes is kept in its own store. It goes in at the body caret (since 0.19 also mid-paragraph: a one-paragraph snippet without a table joins the text, a longer one goes between the halves of the split paragraph); insertion in a cell, groups, chart/OLE fragments and general external exchange are not supported.
- Hanja conversion picks candidates per syllable from the Unicode Unihan reading table; without a word dictionary it does not offer word-level candidates or frequency ordering like Hangul. Track changes and spell checking remain unavailable. Documents with Hangul change tracking open as the shown (final) text. Tracking cannot be preserved, so saving them as HWP/HWPX is refused by default; when the user accepts (desktop confirmation, MCP `acceptTrackChanges`, CLI `--accept-track-changes`) the changes are merged into the shown text and the tracking records are removed before saving. PDF/TXT export is unaffected, and auto-recovery copies hold the shown text. Memos cover body and table-cell ranges with replies; there is no resolved state (Hangul has none) and memos across several paragraphs cannot be created. Document compare lists added / removed / changed paragraphs and a side-by-side list of the current and the file's text, and merges chosen differences toward the file (0.19, `review.merge.plan` / `review.merge`, formatting, tables and pictures included, one undo step). Showing both documents next to each other with synchronised scrolling and merging into tracked changes are not available; with a different section count only sections of the same number are compared, and a section's first paragraph holding objects is not removed by a merge. "No shading" is reported by RHWP as white (#ffffff).
- Section break (`page.section_break`) starts a new section at the caret with the same page setup and columns; headers and footers carry on as in Hangul. Hangul leaves a new section without master pages and keeps using the previous section's, but RHWP does not draw master pages across sections, so HwpCraft copies them into the new section (Hangul shows the result the same way, but editing one section's master page later does not change the other). For the same reason, later sections of a document split in Hangul show no master page. Backspace at a section's start or Delete at the end of the previous one joins the two sections, which then follow the first section's settings and master pages (the paragraphs stay apart; pressing again joins them).
- Master pages: only the text of both-pages, odd-page and even-page master pages can be created, edited and deleted (`applyTo` of `page.master.get/set/delete`). As in Hangul, an odd or even master page takes the place of the both-pages one on its pages, and the storage order and section flags match Hangul's. Last-page / arbitrary-page masters and masters holding pictures or shapes are shown and saved only. When a section has only an odd or only an even master page, Hangul draws it on those pages only, but RHWP draws it on every page. Document summary, odd / even / first-page-only page borders, picture fill, form-object creation, chart / WordArt / OLE creation are unavailable because RHWP has no API for them. Style create / update / delete exist only in RHWP's wasm wrapper, so HwpCraft edits the model directly. Line-based caret motion and selection-rectangle queries are crate-private in RHWP; HwpCraft computes them from caret geometry.
- Automatic tab-width recording covers body text and first-level table cells only; tabs inside headers, text boxes and notes are saved with width 0.
- Multi-paragraph selection works within one section body or one cell; Select All covers the current section. Block save (`file.save_block`) covers the same range (one section body).

### Verification limits

- Printing makes a PDF with the page range, odd / even, reverse order, collated copies, paper orientation, 2 / 4 / 8 pages per sheet and watermark applied, and hands it to the OS. In 0.19 that PDF was checked option by option on a 44-page report (sheet count and the pages on each sheet against HwpCraft's own PDF; 2 and 8 pages per sheet from landscape originals stack on a portrait sheet). The step after it, to the printer driver, was not checked: the test PC's default printer is a paper printer, and Windows 11's default PDF app (Edge) has no print verb, so HwpCraft only opens the PDF there (copies are set again in that dialog). Microsoft Print to PDF asks for a file name, so it could not be driven automatically.
- Pointer-event regression tests cover table selection, moving and resizing. End-to-end click / drag verification of every feature has not been completed.
- Round-trip saving of complex real-world documents and performance on very large documents and tables have not been tested thoroughly. Long typing was tried in 0.19 by feeding 10,000 characters as IME composition and commit events into the release desktop app (`--view bench-ime`): on the 44-page report and the 381-page handbook memory stayed flat thanks to the 40-step undo limit (about 370 MB / 830 MB), and the report saved and reopened with all 49 pages identical. Typing through the real Windows IME was not possible in the test environment, which had no interactive desktop. Keep originals and save important documents under a new name, then check them in Hangul.
- Input documents are limited to 128 MB and a single text insertion to 1 MB. Parsing and rendering are synchronous, so the UI may pause on complex documents. PDF export of large documents takes time (about 17 s for a 381-page document; pages are converted on up to 8 threads, using up to about 1.9 GB of memory, adjustable with `RHWP_PDF_THREADS`).

Release notes are in `docs/` (Korean).

## License

HwpCraft is released under the **MIT License**. The full text is in [LICENSE](LICENSE); the copyright holders are the HwpCraft contributors.

**What the MIT License covers** — everything HwpCraft itself produces in this repository.

- Source code: the Rust code in `crates/` and `apps/`, plus the build / packaging scripts (`HwpCraft.ps1`, `hwpcraft.sh`).
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
