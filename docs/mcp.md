# HwpCraft MCP

HWP/HWPX 편집 엔진을 MCP 클라이언트에 제공하는 로컬 **헤드리스 stdio 서버**입니다.
한컴 아래한글 프로그램의 COM 자동화가 아니라 HwpCraft/RHWP 문서를 편집합니다.
데스크톱 앱과 같은 `Session::run` 명령과 문서 모델을 사용하며 DOCX 모델로 변환하지 않습니다.

## 빌드와 연결

프로젝트 폴더에서 실행합니다. 기존 runtime 실행 파일은 재빌드·설치 전에는 MCP를 포함하지 않습니다.

```powershell
cargo +1.98.1 build --locked --release -p hwpcraft-cli
../.build/hwpcraft/release/hwpcraft-cli.exe mcp --help
```

MCP 클라이언트의 서버 설정 예제입니다. 클라이언트가 프로세스를 시작하고 stdin/stdout을 연결합니다.
아래 경로는 현재 작업 공간 기준이며 다른 PC에서는 실행 파일 경로를 바꾸세요.

```json
{
  "mcpServers": {
    "hwpcraft": {
      "command": "<hwpcraft>/target/release/hwpcraft-cli.exe",
      "args": ["mcp"]
    }
  }
}
```

읽기·쓰기 폴더를 제한하려면 `args`를 다음과 같이 지정합니다. 폴더는 미리 존재해야 합니다.
문서와 그림 등의 상대 경로는 **서버의 작업 디렉터리** 기준이므로 클라이언트에서는 절대 경로가 명확합니다.

```json
["mcp",
 "--automation-read-root", "<hwpcraft>/samples",
 "--automation-write-root", "<hwpcraft>/output"]
```

두 옵션 모두 없으면 기존 CLI와 같은 파일 권한입니다. 하나만 지정하면 다른 종류의 파일 접근은 허용하지 않습니다.
예를 들어 read-root만 지정하면 문서 편집·조회·메모리 PNG 렌더링은 가능하지만 디스크 저장은 거부됩니다.
제한은 열기/저장뿐 아니라 `execute`, `batch`의 그림 읽기와 인쇄용 PDF 쓰기에도 적용합니다.
심볼릭 링크·상위 폴더 이동·접두사만 같은 형제 폴더를 검사하며 Windows 장치 파일 이름을 거부합니다.
이는 에이전트가 지정 폴더를 벗어나지 않게 하는 경로 검사입니다. 검사와 파일 열기 사이에 다른 프로세스가
링크를 바꾸는 경쟁까지 막는 OS 샌드박스는 아닙니다. 설치된 시스템 글꼴 로딩은 문서 경로 제한 대상이 아닙니다.

## 도구

| 도구 | 인자와 동작 |
|---|---|
| `list_commands` | `query?`: ID·한글 이름 검색. 명령별 인자 설명·예제·단축키 반환 |
| `execute` | `command`, `params?`: 공통 엔진 명령 실행 |
| `batch` | `commands:[{command,params?}]`, `keepGoing?`: 1–1,000개 명령 순차 실행 |
| `new_document` | `force?`: 빈 문서 생성 |
| `open_document` | `path`, `force?`: HWP/HWPX 열기 |
| `save_document` | `path?`: HWP/HWPX 저장, PDF/TXT 내보내기. 생략하면 현재 문서 경로 |
| `type_text` | `text`: 선택 영역을 대체하거나 커서에 입력. 줄바꿈은 새 문단 |
| `select_text` | `text`, `occurrence?`: 문단 안에서 대소문자를 구분해 n번째 비중첩 일치 선택 |
| `get_text` | `start?`, `limit?`: 본문과 1단계 표 셀 텍스트 조회 |
| `inspect_document` | `start?`, `limit?`: 요약·선택·문단·스타일/서식 참조 조회 |
| `render_page` | `page?`: 1부터, `scale?`: 0.25–2.0. PNG를 MCP 이미지로 반환 |
| `hwp_action` | `id?`, `params?`, `query?`: 한글 자동화 동작 이름(`MoveLineEnd`, `CharShapeBold`, `DeleteWord` …)으로 대응 명령 실행. 대응표의 기본 인자 위에 `params`를 덮어씀. `id`를 생략하면 동작 목록(`query`로 거름) |
| `get_selection` | 커서·anchor·spanAnchor·선택 텍스트 조회 |

상세 인자는 [명령 참고](mcp-commands.md)에 있습니다. 명령 검색 결과는 엔진 목록에서 생성하며 별도 편집 엔진을 만들지 않습니다.
`file.new/open/save/save_as/pdf`는 MCP 경계에서 경로 기반 작업으로 처리합니다. GUI 대화상자는 열지 않습니다.
표·그림·도형·수식·머리말/꼬리말·쪽 번호·스타일·찾아 바꾸기 등 기존 명령은 `execute`/`batch`로 사용합니다.

`new_document`와 `open_document`는 미저장 변경이 있으면 오류를 반환합니다. 저장하거나 `force:true`로 버리기를 명시해야 합니다.
읽기 실패는 기존 문서·선택·undo를 보존합니다. 저장은 임시 파일을 완성한 뒤 교체합니다.
PDF/TXT 내보내기는 현재 HWP/HWPX 저장 경로나 미저장 상태를 변경하지 않습니다.

`batch`는 기본적으로 전체 트랜잭션이 아닙니다. 기본값은 첫 실패에서 중단하며 앞서 성공한 편집·파일 쓰기는 남습니다.
`keepGoing:true`이면 나머지를 실행하지만 하나라도 실패하면 `isError:true`와 명령별 결과를 반환합니다.
`atomic:true`이면 시작 전 스냅샷을 잡고, 하나라도 실패하면 그때까지의 문서 변경을 모두 되돌려 `rolledBack:true`로 알립니다(파일 명령은 넣을 수 없고 `keepGoing`은 무시, 되돌린 뒤 다시 실행 기록은 비워집니다).
전체 batch의 JSON 명령 구조는 실행 전에 검증합니다. 문서 편집의 undo/redo는 기존 엔진 정책을 따릅니다.

## 예제: 한글 보고서

다음은 각 `tools/call`에 전달할 `name`과 `arguments`입니다.

```json
{"name":"new_document","arguments":{}}
{"name":"type_text","arguments":{"text":"주간 보고서\n이번 주 작업 내용을 정리합니다."}}
{"name":"select_text","arguments":{"text":"주간 보고서"}}
{"name":"execute","arguments":{"command":"format.character","params":{"bold":true,"fontSize":1800}}}
{"name":"execute","arguments":{"command":"format.paragraph","params":{"alignment":"center"}}}
{"name":"inspect_document","arguments":{}}
{"name":"render_page","arguments":{"page":1,"scale":1}}
{"name":"save_document","arguments":{"path":"<hwpcraft>/output/주간보고서.hwpx"}}
{"name":"save_document","arguments":{"path":"<hwpcraft>/output/주간보고서.pdf"}}
```

커서·문단·셀 인덱스는 0부터, `occurrence`와 렌더링 쪽 번호는 1부터입니다.
커서 `offset`과 선택 `anchor`는 유니코드 스칼라 개수입니다. `가😀한글`에서 `한글`의 시작은 2입니다.
UTF-8 바이트나 UTF-16 코드 유닛 수로 계산하지 마세요. `inspect_document.charShapes`는 RHWP의 원래 서식 참조이므로
그 위치를 곧바로 Cursor offset으로 쓰지 마세요. 현재 선택의 실제 속성은 `format.character.get`과 `format.paragraph.get`으로 조회합니다.

`get_text`/`inspect_document`는 기본 100개, 최대 1,000개 문단을 반환합니다. `nextStart`가 null이 될 때까지
그 값을 다음 `start`로 넘기세요. `total`에는 본문과 1단계 표 셀이 포함됩니다.
표 셀은 각 block의 `cursor.cell` 주소를 `selection.set`에 넘겨 선택합니다.
머리말·글상자·주석·중첩 표를 본문에 합쳐 반환하지 않습니다. 개체는 `object.list`, `textbox.paragraphs.get`,
`note.paragraphs.get`, `page.header_footer.list`, `chart.list` 등 전용 명령을 이용하세요.

## 프로토콜과 리소스

- 구현 프로토콜: `2025-06-18`. 다른 버전 요청에는 이 버전을 제안하며 클라이언트가 호환성을 판단합니다.
- `initialize` → `notifications/initialized` 이후 도구와 리소스를 사용할 수 있습니다. `ping`은 초기화 전에도 가능합니다.
- `hwpcraft://document`: 첫 100개 문단의 구조. 이후는 `inspect_document`로 조회합니다.
- `hwpcraft://commands`: 전체 명령 목록·인자 설명·예제. 각 명령에 대응하는 한글 동작 이름(`hwpActions`)도 포함합니다.
- `hwpcraft://hwp-actions`: 한글 자동화 동작 이름 → HwpCraft 명령·기본 인자 대응표(`mapping`: mapped/partial).
- stdout에는 JSON-RPC 응답만 기록하고 오류 진단은 stderr로 보냅니다. EOF 또는 출력 파이프 종료로 세션을 끝냅니다.
- 잘못된 JSON/프로토콜은 JSON-RPC error, 유효한 도구 실행 실패는 `isError:true`로 반환합니다.
- 요청 한 줄은 최대 4 MiB, 응답은 최대 32 MiB입니다. 과대 입력은 오류 응답 후 연결을 끝냅니다.
  큰 응답은 limit 또는 렌더링 scale을 줄이세요. 문서 128 MB, 텍스트 한 번 입력 1 MB, 렌더링 2,400만 화소 제한도 적용합니다.

구현 기준: MCP 공식 [stdio 전송](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports),
[초기화](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle),
[도구](https://modelcontextprotocol.io/specification/2025-06-18/server/tools).

## 지원하지 않는 범위

- `--connect`, 실행 중인 HwpCraft 창 공유, 클릭·키 입력·창 캡처. MCP마다 별도의 문서 세션을 가집니다.
- HTTP/SSE 서버, Prompts, Sampling, 리소스 구독·변경 알림, 진행 알림, 실행 중 요청 취소, JSON-RPC 배열 batch.
  `batch` 도구와 JSON-RPC 배열은 별개입니다.
- 한컴 앱 제어, DOCX 편집, 암호 문서 편집, 변경 내용 추적, 새 메모·새 차트 생성 등 기존 엔진의 미구현 기능.
- 아래한글과의 완전한 조판·저장 호환성. 기존 [README의 범위와 한계](../README.md)를 그대로 따릅니다.

검증은 `crates/mcp/tests/protocol.rs`, `apps/hwpcraft-cli/tests/mcp.rs`, `crates/formats/tests/file_access.rs`에 있습니다.
실제 한컴 아래한글에서 새 출력물을 열어 비교하는 검증은 이번 MCP 테스트에 포함되지 않습니다.
