# HwpCraft MCP

HWP/HWPX 문서를 작성·편집·저장하고 PNG/PDF로 검증하는 로컬 헤드리스 서버입니다. 데스크톱과 같은 HwpCraft `Session` 명령·문서 모델을 사용합니다. 한컴 COM이나 실행 중인 GUI 창을 제어하지 않습니다.

현재 소스는 공식 Rust SDK `rmcp 3.7.0`을 사용하며 **MCP `2026-07-28`만 지원**합니다. 설치된 이전 실행 파일은 소스와 기능이 다를 수 있으므로, 사용할 실행 파일의 `mcp --help`와 실제 discovery를 확인하세요. 명령 등록은 아래한글 기능 전체의 완성도나 모든 옵션의 호환성 검증을 뜻하지 않습니다.

## 실행과 연결

프로젝트에서 CLI를 빌드합니다. 이 작업 공간의 공유 target 경로 예제입니다.

```powershell
cargo +1.98.1 build --locked --release -p hwpcraft-cli --target-dir ../.build/hwpcraft
../.build/hwpcraft/release/hwpcraft-cli.exe mcp --help
../.build/hwpcraft/release/hwpcraft-cli.exe mcp --transport stdio
```

기본 전송은 stdio입니다. 클라이언트가 프로세스를 시작하고 stdin/stdout을 연결합니다. 실행 파일에는 절대 경로를 지정하세요.

```json
{
  "mcpServers": {
    "hwpcraft": {
      "command": "C:/Tools/HwpCraft/hwpcraft-cli.exe",
      "args": ["mcp", "--transport", "stdio",
               "--automation-read-root", "F:/work/reports",
               "--automation-write-root", "F:/work/reports"]
    }
  }
}
```

loopback Streamable HTTP도 선택할 수 있습니다.

```powershell
../.build/hwpcraft/release/hwpcraft-cli.exe mcp --transport http --listen 127.0.0.1:7082 `
  --automation-read-root F:/work/reports --automation-write-root F:/work/reports
```

엔드포인트는 `http://127.0.0.1:7082/mcp`입니다. 기본 주소도 `127.0.0.1:7082`이며 `--listen`은 HTTP에서만 허용합니다. 비 loopback 바인딩은 거절합니다(원격 접속은 지원하지 않습니다). 모든 HTTP 요청에는 `Authorization: Bearer <토큰>`이 필요하며, 없거나 틀리면 경로와 관계없이 `401`과 `WWW-Authenticate: Bearer`를 돌려줍니다. 토큰은 HwpCraft 설정 폴더(Windows `%APPDATA%\HwpCraft`, macOS `~/Library/Application Support/HwpCraft`, Linux `~/.config/HwpCraft`)의 `mcp-http-token` 파일이나 `--auth-token-file <파일>`로 지정한 파일에서 읽습니다. 파일이 없으면 서버가 64자 무작위 토큰을 만들어 본인만 읽을 수 있게 저장하고(macOS·Linux 권한 600), 토큰 값은 출력하지 않습니다. MCP 클라이언트의 HTTP 헤더 설정에 이 파일 내용을 넣으세요. 토큰을 바꾸려면 서버를 멈추고 파일을 지운 뒤 다시 시작합니다. HTTP의 `Host`는 실제 listener 주소 또는 같은 포트의 `localhost`여야 하고, 브라우저 `Origin`은 거절합니다. 네이티브 클라이언트는 `Origin` 없이 연결합니다.

HTTP는 `POST /mcp`를 사용합니다. `Content-Type: application/json`, `Accept: application/json, text/event-stream`, `MCP-Protocol-Version: 2026-07-28`, `Mcp-Method: <본문 method>`를 보냅니다. `tools/call`·`prompts/get`에는 `Mcp-Name: <name>`, `resources/read`에는 `Mcp-Name: <uri>`도 맞춰 보내세요. HTTP 헤더와 아래 JSON 메타데이터는 모두 필요합니다. 일반 응답은 JSON이고, 진행 알림 등이 있으면 SSE 응답으로 전환될 수 있으므로 두 형식을 처리해야 합니다. 별도의 GET SSE 스트림이나 legacy HTTP 세션 초기화는 제공하지 않습니다.

## 요청마다 필요한 메타데이터

`initialize`/`notifications/initialized` 핸드셰이크를 보내지 않습니다. `server/discover`로 지원 버전을 확인하고 **모든 요청의 `params._meta`**에 다음 두 값을 넣습니다. `ping`도 현재 프로토콜에서는 제공하지 않습니다. 2025년 판 이전 클라이언트가 `initialize`를 보내면 stdio와 HTTP 모두 `-32022`(Unsupported protocol version)와 `data.supported:["2026-07-28"]`로 답합니다. HTTP에서 `MCP-Protocol-Version` 헤더가 없는 POST는 `400 Bad Request`입니다. 공식 Python SDK `mcp` 2.3.0과 TypeScript SDK `@modelcontextprotocol/client` 2.4.0은 이 서버와 동작하며, 두 SDK가 기본으로 하는 `outputSchema` 검증도 모든 도구에서 통과합니다. 예전 패키지 `@modelcontextprotocol/sdk` 1.x는 2025-11-25까지만 지원해 연결할 수 없습니다. TypeScript SDK 2.4.0은 Tasks 확장 결과(`resultType:"task"`)를 아직 해석하지 못하므로 그 클라이언트에서는 Tasks 확장을 선언하지 마세요.

```json
{"jsonrpc":"2.0","id":"discover-1","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

메타데이터가 빠지면 거절하며, `2025-06-18` 등 다른 버전으로 협상하여 실행하지 않습니다. 요청 ID는 문자열 또는 정수이며 응답은 ID로 연결합니다. 알림은 응답과 분리해 처리하세요. stdio는 UTF-8 JSON 객체 하나마다 줄바꿈으로 끝내는 JSONL입니다. stdout에는 서버 프로토콜 메시지만 기록하고 진단은 stderr로 보냅니다.

## 명시적 문서 핸들

서버에 연결별 암묵적 현재 문서는 없습니다. `new_document`, `open_document`, `open_protected_document`에서 `document_id`를 생략하면 새 독립 문서를 만들고 결과의 `structuredContent.document_id`로 UUID 핸들을 반환합니다.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"new_document","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

이후 각 문서 도구의 **`arguments.document_id`**에 반환값을 전달합니다. `_meta`는 도구 인자가 아니라 `params` 아래에 둡니다.

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"type_text","arguments":{"document_id":"<반환된 UUID>","text":"주간 보고서\n이번 주 작업 내용을 정리합니다."},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

생성·열기 도구에 기존 `document_id`를 지정하면 그 문서만 교체합니다. 미저장 변경이 있으면 `force:true`가 필요합니다. 열기 실패나 잘못된 암호는 기존 문서·선택·이력을 보존합니다. 신규 핸들 없는 열기가 실패하면 사용할 새 핸들은 생성되지 않습니다. 암호 도구의 `password`는 결과로 반환하지 않습니다.

`close_document`도 핸들을 요구하며 미저장 문서는 `force:true` 없이 닫지 않습니다. 닫힌 핸들은 재사용할 수 없습니다. 핸들은 서버 수명 동안만 유효하며 다른 클라이언트에 자동 열거하지 않습니다. 한 서버에서 최대 16개 문서를 유지하고 각 문서의 대기 큐는 64개로 제한합니다. 16개가 모두 열린 상태에서 새 문서를 만들면, 30분 넘게 요청이 없고 저장하지 않은 변경이 없는 문서 가운데 가장 오래 쓰지 않은 하나를 닫고 그 자리를 씁니다. 그런 문서가 없으면 지금처럼 한도 오류입니다. 수정된 문서는 자동으로 닫지 않습니다. 이렇게 닫힌 핸들은 이후 "문서 ID가 없거나 문서가 닫혔습니다" 오류를 냅니다. 같은 문서의 작업은 직렬 실행하며 다른 문서는 별도의 엔진 스레드에서 처리합니다. 프로토콜의 요청별 메타데이터와 서버가 보관하는 문서 수명은 별개입니다.

`list_commands`와 `id` 없는 `hwp_action` 목록 조회에는 문서 핸들이 필요하지 않습니다. 나머지 도구에는 현재 discovery의 스키마에 따라 핸들을 명시하세요.

## 데스크톱 창 연결

실행 중인 HwpCraft 데스크톱 창의 문서를 MCP로 고칠 수 있습니다. 도구 탭의 **MCP 연결**(`tools.mcp_connect`)을 켜면 데스크톱 프로세스 안에서 같은 MCP 서버가 HTTP로 뜹니다.

- 주소: `http://127.0.0.1:7083/mcp` (CLI HTTP 기본값 7082와 겹치지 않습니다). 루프백 전용입니다.
- 인증: CLI HTTP와 같은 `Authorization: Bearer <token>`이며, 토큰은 설정 폴더의 `mcp-http-token`에 있습니다.
- 연결 정보 창에 주소, 토큰 파일 위치, 연결된 문서 ID, 클라이언트 설정 예가 나옵니다.

클라이언트는 `list_documents`로 창이 공유한 문서를 찾고(`source:"desktop"`), 그 `document_id`를 다른 문서 도구에 넘깁니다.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_documents","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

- MCP로 고친 내용은 창에 바로 보이고 창의 실행 취소 기록에 들어갑니다.
- 창 문서에 대한 `close_document`는 거절합니다(창을 닫지 않습니다). `new_document`·`open_document`는 창과 별개인 MCP 문서를 만듭니다.
- 요청은 창의 화면 그리기 사이에 실행되며 한 프레임에 최대 30 ms를 씁니다. 편집 요청 하나가 실측 48 ms였습니다.
- `list_documents`는 데스크톱이 공유한 문서만 돌려줍니다. MCP로 만든 문서의 핸들은 만든 클라이언트만 압니다.
- 한계: 창의 문서 하나만 연결되며, 창을 닫거나 연결을 끄면 그 `document_id`는 더 쓸 수 없습니다.

## 도구와 결과 읽기

| 도구 | 주요 인자·동작 (`document_id`는 위 규칙 적용) |
|---|---|
| `list_commands` | `query?`: 명령 ID·한글 이름 검색, 실제 인자 도움말·예제·단축키 |
| `execute` | `command`, `params?`: 공통 엔진 명령 |
| `batch` | `commands:[{command,params?}]`, `keepGoing?`, `atomic?`: 1–1,000개 순차 실행 |
| `new_document` | `force?`: 새 독립 문서 또는 지정 문서 초기화 |
| `open_document` | `path`, `force?`: 파일 열기 |
| `open_protected_document` | `path`, `password`, `force?`: 암호 파일 열기 |
| `save_document` | `path?`: HWP/HWPX 저장, PDF/TXT/DOCX 내보내기(DOCX 결과의 `omitted`에 담지 못한 것 목록: 그리기 개체·수식·문단 번호 글자 등). 생략하면 현재 저장 경로. `acceptTrackChanges?`(기본 false): 한글 변경 추적 기록이 있는 문서는 HWP/HWPX 저장을 거절하며, true면 변경 내용을 지금 보이는 결과본으로 합치고 변경 기록을 지운 뒤 저장(`execute document.track_changes`로 상태 확인) `dropPictureEffects?`(기본 false): 그림 효과(그림자·네온·부드러운 가장자리·반사)는 아직 .hwp로 쓰지 못해, 모델에 효과가 있는 문서(`picture.effects`로 넣은 효과, HWPX에서 읽은 효과 포함)의 .hwp 저장은 거절됩니다. true면 효과를 뺀 뒤 저장하고(실행 취소 한 단계, 저장 실패 시 되돌림) 결과의 `droppedPictureEffects`에 뺀 수를 돌려줍니다. .hwp에서 연 효과는 원본 바이트로 왕복하므로 거절하지 않고, HWPX·PDF는 영향이 없습니다. |
| `close_document` | `force?`: 지정 문서 닫기 |
| `list_documents` | 입력 없음: 데스크톱 HwpCraft 사용자가 MCP에 공유한 문서의 `document_id`·제목·경로·미저장 여부. MCP로 만든 문서는 나열하지 않음 |
| `type_text` | `text`: 선택 대체 또는 커서 입력, 줄바꿈은 새 문단 |
| `select_text` | `text`, `occurrence?`: 문단 내 n번째 대소문자 구분 일치 선택 |
| `get_text`, `inspect_document` | `start?`, `limit?`: 본문·1단계 표 셀 조회와 구조 조회 |
| `get_selection` | 현재 커서·선택 조회 |
| `render_page` | `page?`: 1부터, `scale?`: 0.25–2.0. 메모리 PNG |
| `hwp_action` | `id?`, `params?`, `parameterSet?`, `query?`: 공개 한글 동작 이름의 대응 명령·인자 실행 또는 목록 조회 |

`tools/list`의 inputSchema는 JSON Schema 2020-12입니다. 예제는 입력 형식을 보여주며 현재 선택·개체·파일 전제 조건까지 보장하지 않습니다. 모든 명령을 이 문서에 복제하지 않고 [명령 사용 안내](mcp-commands.md)와 실행 중 `list_commands`를 사용합니다.

도구 응답의 `result`에는 `resultType:"complete"`, `isError`, `structuredContent`, `content`가 있습니다. 엔진 결과가 객체이면 그 필드를 유지하고, 배열이면 `structuredContent.items`, 원시 값이면 `structuredContent.value`로 감쌉니다. 문서 결과에는 `document_id`를 덧붙입니다. 텍스트 content는 같은 구조의 JSON 문자열이며, PNG 도구에는 별도의 `type:"image"` content가 함께 있습니다. `content[0]`이 항상 텍스트라고 가정하지 마세요.

모든 도구는 `outputSchema`(JSON Schema 2020-12)를 선언합니다. 최상위는 `anyOf`로 성공 모양과 실행 오류 모양을 함께 허용하며, `isError:true` 결과에는 항상 문자열 `message`가 있습니다. 성공 모양에는 늘 들어가는 필드만 `required`로 적고 객체는 열어 두므로, 이후 필드가 늘어도 기존 클라이언트 검증은 깨지지 않습니다.

| 도구 | 성공 시 필수 필드 | 비고 |
|---|---|---|
| `new_document`, `open_document`, `open_protected_document`, `save_document`, `type_text` | `pages`, `layoutError`, `sections`, `paragraphs`, `dirty`, `path`, `document_id` | 문서 요약. `pages`·`layoutError`·`path`는 null일 수 있음 |
| `get_text` | `total`, `start`, `nextStart`, `scope`, `offsetUnit`, `text`, `document_id` | `nextStart`가 null이면 끝 |
| `inspect_document` | 위 쪽 나눔 필드와 `document`, `blocks`, `selection`, `canUndo`, `canRedo`, `document_id` | `blocks[].charShapes` 항목은 RHWP 원본 모양이라 항목 스키마를 고정하지 않음 |
| `select_text` | `cursor`, `anchor`, `document_id` | |
| `get_selection` | `cursor`, `anchor`, `spanAnchor`, `hasSelection`, `text`, `document_id` | |
| `render_page` | `page`, `pages`, `width`, `height`, `document_id` | PNG는 별도 image content |
| `close_document` | `closed`(true), `document_id` | |
| `list_documents` | `documents[]`: `document_id`, `title`, `path`(null 가능), `source`(`desktop`), `dirty` | 공유한 데스크톱 문서가 없으면 빈 배열 |
| `list_commands` | `items[]`: `id`, `label`, `shortcut`, `paramsHelp`, `paramsExample`, `hwpActions` | `paramsExample`은 명령마다 다름 |
| `batch` | `results[]`: `command`, `ok`, (`result` 또는 `error`), `rolledBack`, `document_id` | 실패해도 같은 모양에 `message` 추가 |
| `hwp_action` | 목록 `items[]` 또는 실행 `action`, `command`, `mapping`, `result`, `document_id` | `result`는 대응 명령의 결과 |
| `execute` | `document_id` | 본문은 명령마다 다르므로 공통 봉투만 선언. 명령별 결과는 `list_commands` 도움말 참고 |

스키마·프로토콜 실패는 JSON-RPC `error`, 유효한 도구의 실행 실패는 `isError:true`입니다. 먼저 둘을 확인한 뒤 결과를 읽으세요. SDK 디코딩 단계에서 잘못된 요청 구조가 거절되는 경우도 있으므로 모든 실패를 하나의 고정 오류 코드로 해석하지 않습니다. HwpCraft 고유 오류 코드는 2026-07-28 규격에 따라 JSON-RPC 예약 범위(`-32768`~`-32000`) 밖의 `-31429`(completion 빈도 제한)와 `-31503`(구독 한도)만 씁니다.

## 보고서 작성·검증 순서

1. `server/discover`, `tools/list`, `hwpcraft://capabilities`를 조회하고 필요한 명령을 검색합니다.
2. 문서를 만들고 핸들을 저장합니다. `type_text`로 본문을 입력하고 `select_text` 또는 `selection.range`로 편집 대상을 명시합니다.
3. `execute`/`batch`로 서식·표·그림·쪽 설정을 적용합니다. 선택이 없는 `format.character`는 이후 입력의 대기 서식이므로 기존 글자를 고칠 때는 먼저 범위를 선택합니다.
4. `inspect_document`, 문맥 조회와 `page.layout.get`으로 내용을 점검하고 모든 출력 쪽을 `render_page`로 확인합니다.
5. 편집 가능한 HWP/HWPX를 저장하고 재열어 본문·표·서식을 다시 조회합니다. PDF를 내보내고 실제 페이지의 글자·표·페이지 나눔도 검증합니다. PNG 성공만으로 PDF 조판 일치를 판단하지 않습니다.
6. 작업이 끝나면 `close_document`로 닫습니다.

커서·문단·셀 인덱스는 0부터, `occurrence`와 `render_page`/`page.layout.get`의 쪽은 1부터입니다. 커서 offset은 **Unicode 스칼라** 개수입니다. `가😀한글`에서 `한글`은 offset 2에 시작합니다. UTF-8 바이트나 UTF-16 단위가 아닙니다. `inspect_document.charShapes`의 native 위치를 커서 offset으로 그대로 사용하지 마세요.

`get_text`/`inspect_document`는 기본 100개, 최대 1,000개 문단을 반환합니다. `nextStart`가 null일 때까지 이어서 읽으세요. 머리말·글상자·주석·중첩 표를 모두 평탄화하지 않습니다. 해당 내용은 `context.v2.list/get`과 개체별 조회 명령으로 탐색합니다. 편집은 반환된 주소와 최신 `generation`을 사용합니다. 본문 상용구도 공통 `tools.autotext.body.*` 명령으로 등록·미리보기·삽입할 수 있으며 지원 문맥과 거절 조건은 [명령 안내](mcp-commands.md)에 있습니다.

새 차트는 본문의 `chart.create`로 11종을 작성합니다. 기본 5종(`column`, `bar`, `line`, `pie`, `doughnut`)에 분산형(`scatter`), 주가형(`stock_hlc`, `stock_ohlc`), 3D형(`column_3d`, `bar_3d`, `pie_3d`)이 추가되었습니다. 먼저 `chart.types`로 종류별 제약을 조회하세요. `chart.definition.get {index}`로 자체 차트의 현재 XML 기반 전체 구성을 읽고 `chart.configure`로 변경합니다. 분산형은 계열별 공통 `x_values`와 같은 점 개수가 필요하며, 주가형은 고정된 계열 순서와 가격 범위를 검사합니다. 다른 기존 차트의 지원되는 값 편집은 `chart.data` 경로입니다. 생성 결과의 `nativeInterop:"unverified"`는 한컴에서 새 출력물을 열고 고치는 호환성을 아직 확인하지 않았다는 뜻입니다. HwpCraft의 HWP/HWPX 자체 왕복·PNG·PDF 검사와 별개이며, area/radar/bubble/surface/combo 등 다른 차트 종류나 중첩·그룹 도넛 표시까지 지원하지 않습니다. [차트 인자 예제](mcp-commands.md#차트-만들기구성-변경)를 참고하세요.

## 리소스·프롬프트·완성

| 요청 | 현재 제공하는 내용 |
|---|---|
| `resources/list` | `hwpcraft://commands`, `hwpcraft://hwp-actions`, `hwpcraft://capabilities` |
| `resources/templates/list` | `hwpcraft://documents/{document_id}` |
| `resources/read` | 위 정적 리소스 또는 명시한 문서 핸들의 첫 100개 문단 구조 |
| `prompts/list`, `prompts/get` | `report-authoring`: 필수 `title`, 선택 `audience`, `language`(ko/en/ja/zh), `format`(hwpx/hwp/pdf) |
| `completion/complete` | report-authoring의 language·format 후보, 문서 템플릿의 document_id 후보는 빈 목록 |
| `subscriptions/listen` | 명시한 live 문서 URI의 초기 resync·변경 무효화 알림. tools/prompts/resources 목록 변경은 수락하지 않음 |

정적 목록·정적 리소스는 `ttlMs:60000`, `cacheScope:"public"`이며 현재 문서의 내용·핸들·실제 파일 경로·암호·사용자 설정을 포함하지 않습니다. 명령의 경로 인자 예제는 포함할 수 있습니다. 문서 리소스는 `ttlMs:0`, `cacheScope:"private"`입니다. 목록은 완전한 한 번의 결과이며 pagination cursor를 받지 않습니다. completion은 서비스당 초당 30회로 제한하며, 넘으면 오류 코드 `-31429`와 `data.retryAfterMs:1000`을 돌려줍니다(0.17까지는 `-32901`). 문서 ID를 자동 완성하거나 열거하지 않습니다.

프롬프트는 작성 절차와 별도 작업 데이터를 반환하며 문서를 자동으로 생성하지 않습니다. 제목 등 사용자 입력을 실행 명령이나 기존 문서 폐기 권한으로 취급하지 않습니다. 완성 요청과 `prompts/get` 예제는 [명령 안내](mcp-commands.md)에 있습니다.

## 문서 URI 변경 구독

`server/discover`의 `capabilities.resources.subscribe:true`는 **`subscriptions/listen`**으로 명시한 문서 URI 업데이트를 구독할 수 있다는 뜻입니다. 구형 `resources/subscribe`/`resources/unsubscribe`를 호출하지 않습니다. 필터의 실제 wire 위치는 `params.notifications.resourceSubscriptions`이며 `params.filter`가 아닙니다.

다음 요청은 구독 검사와 같은 형태입니다. 목록 변경 종류도 요청하지만 현재 지원하지 않는 종류는 확인 알림에서 제외됩니다. 실제 사용에서는 필요한 문서 URI만 요청해도 됩니다.

```json
{"jsonrpc":"2.0","id":"document-watch-1","method":"subscriptions/listen","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}},"notifications":{"toolsListChanged":true,"resourcesListChanged":true,"promptsListChanged":true,"resourceSubscriptions":["hwpcraft://documents/<doc>"]}}}
```

서버가 먼저 보내는 확인 알림의 필수 필드는 다음과 같습니다. `<doc>`는 이미 얻은 실제 문서 핸들로 바꿉니다. 정적 리소스·존재하지 않는 문서·닫힌 문서는 수락하지 않으며 문서 핸들을 열거하지 않습니다.

```json
{"jsonrpc":"2.0","method":"notifications/subscriptions/acknowledged","params":{"_meta":{"io.modelcontextprotocol/subscriptionId":"document-watch-1"},"notifications":{"resourceSubscriptions":["hwpcraft://documents/<doc>"]}}}
```

그다음 수락한 URI마다 **초기 resync** 알림을 한 번 보내 구독 등록 경계에서 현재 상태를 다시 읽도록 합니다. 이것은 실제 편집이 발생했다는 증거가 아닙니다. 이후 해당 문서의 내용·서식·선택·dirty/저장 경로·이력 상태 등 문서 리소스 상태가 바뀌면 같은 형태로 무효화합니다.

```json
{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"_meta":{"io.modelcontextprotocol/subscriptionId":"document-watch-1"},"uri":"hwpcraft://documents/<doc>"}}
```

알림에는 URI와 구독 ID만 있고 본문·파일 경로·암호·다른 문서 목록은 없습니다. 클라이언트는 `resources/read`나 문서 도구로 최신 내용을 조회합니다. 빠른 연속 변경은 **coalescing**될 수 있어 알림 개수와 편집 횟수가 같지 않으며, 변경 로그나 빠짐없는 이벤트 재생을 제공하지 않습니다. 세대가 바뀌는 rollback도 보수적인 재조회 알림을 낼 수 있습니다.

한 구독에서 서로 다른 URI 최대 16개, 서버 전체에서 동시 listen 최대 64개를 유지합니다. 한도를 넘는 listen은 오류 코드 `-31503`으로 거절합니다(0.17까지는 `-32901`). URI 중복은 별도 문서로 세지 않습니다. 여러 클라이언트의 같은 요청 ID는 각 연결에 속하고, 한 연결에서도 정수 `7`과 문자열 `"7"`은 다른 ID입니다. 모든 알림을 `_meta`의 `io.modelcontextprotocol/subscriptionId`로 구분하세요. 변경 요청의 발신자와 관계없이 그 URI를 구독한 클라이언트에만 전달합니다. 이 격리는 URI 필터·연결·요청 수명에 대한 것이며 원격 사용자 인증을 추가하는 기능은 아닙니다.

문서가 닫히면 마지막 `updated` 뒤 `resources/read`는 not-found가 됩니다. 표준 알림에 별도 `deleted` 필드를 붙이지 않습니다. 다른 URI도 구독 중이면 계속 유지하고, 마지막 URI가 닫히면 원래 listen 요청에 `resultType:"complete"`와 해당 subscriptionId를 담은 최종 응답으로 종료합니다. 미저장 변경 때문에 닫기를 거절한 경우에는 삭제 알림을 내지 않습니다.

stdio 구독은 다음 **ID 없는 알림**으로 취소합니다. 취소 알림 자체에는 응답이 없습니다.

```json
{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"document-watch-1","_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

종료한 구독에는 이후 변경을 보내지 않습니다. stdio EOF는 연결 전용 token으로 해당 연결의 구독만 종료하고 일반 요청의 응답 drain과 다른 클라이언트의 구독을 보존합니다. 재연결하면 구독을 다시 요청해야 합니다. HTTP에는 SDK의 SSE 요청 취소 경로를 사용합니다. 실제 CLI와 독립 TCP 스트림 두 개로 같은 listen ID의 URI 격리, ACK·초기 resync·변경·문서 close의 final complete·read not-found, 네트워크 disconnect 뒤 서비스 유지·재구독을 검증했습니다. **해당 제품 시나리오의 통과이며 전체 normative 적합성 증명은 아닙니다.** 서버 주도 종료의 공식 문구 충돌은 로컬 규격 감사의 N-01에 미해결로 기록했습니다.

## 파일 권한과 atomic

`--automation-read-root`/`--automation-write-root` 폴더는 시작 전에 존재해야 합니다. 두 옵션이 없으면 일반 CLI 파일 권한을 사용하고, 한쪽만 지정하면 다른 종류의 파일 접근은 거절합니다. 상대 경로는 서버 작업 디렉터리 기준입니다.

제한은 직접 도구뿐 아니라 `execute`·`batch`의 읽기/쓰기, 연결된 그림을 읽는 렌더링, 스타일 가져오기/내보내기, 그림 저장과 PDF 출력에도 적용합니다. 글상용구와 본문 상용구 설정 파일도 같은 제약을 받습니다. 조회·삽입에는 필요한 설정 읽기 권한, 등록·삭제에는 읽기와 쓰기 권한이 필요합니다. 별도 자동화 설정을 쓰려면 서버 시작 전 `HWPCRAFT_CONFIG_DIR`을 지정하고 해당 폴더를 필요한 root 안에 두세요. 기존 시스템 글꼴 로딩 예외는 유지합니다.

경로 검사는 심볼릭 링크·상위 폴더 이동·접두사만 같은 형제 폴더·Windows 장치 이름을 검사합니다. 파일 열기 사이의 외부 링크 교체까지 막는 OS 샌드박스는 아닙니다.

기본 `batch`는 첫 실패에서 멈추고 앞선 성공은 남깁니다. `keepGoing:true`는 이후 명령도 실행하지만 하나라도 실패하면 도구 전체의 `isError`가 true입니다. `atomic:true`는 전체 이력 checkpoint를 사용해 실패 시 문서·선택·입력 대기 서식·typing 묶음·기존 undo/redo 및 암호 저장 상태를 복구하고 `rolledBack:true`를 반환하며 `keepGoing`을 무시합니다. 복구 뒤에는 generation을 새로 부여하므로 주소와 세대를 다시 조회합니다. native 복원 자체가 실패하면 `rolledBack:true`로 보고하지 않고 해당 문서 핸들을 닫으며 다른 문서는 유지합니다. 이 동작은 단일 배치 범위이며 임의 중첩 checkpoint나 엔진 panic 복구를 보장하지 않습니다. 본문·셀의 내부 rich 클립보드는 원래 복사 문서와 정확한 복사 범위로 재생성하며 서식·표·그림·도형과 문서 layout을 함께 복원합니다. 쪽 클립보드도 문서별로 소유하고 rollback에 포함합니다. 다른 문서 핸들에서는 원본 핸들 없이 그 복사 내용을 붙일 수 없습니다. 직접 native API로 추적 밖에서 바꾼 클립보드, native 개체 복사의 cascade 상태, OS 시스템 클립보드까지 복구하는 API는 아닙니다.

atomic은 외부 파일·설정 트랜잭션이 아닙니다. 문서 교체, 저장·내보내기·인쇄용 PDF·블록/그림 저장, 글상용구/본문 상용구 등록·삭제, 경로를 지정한 스타일 내보내기는 **첫 편집 전에** 거절합니다. 추가 파일 쓰기도 atomic 실행 중 차단합니다. 경로 없는 스타일 JSON 조회, 읽기와 문서 편집만 하는 스타일 가져오기·상용구 삽입은 허용할 수 있으며 파일 읽기 권한은 그대로 적용합니다. 전체 JSON 명령 구조는 실행 전에 검증합니다.

HWP/HWPX 저장은 임시 파일 완성 후 교체합니다. PDF/TXT 내보내기는 편집 문서의 저장 경로나 dirty 상태를 바꾸지 않습니다. 지원하지 않는 저장 확장자는 오류를 반환하며 기존 파일을 자르지 않습니다.

## 긴 파일 작업과 Tasks 확장

서버는 `server/discover`의 `capabilities.extensions`에 `io.modelcontextprotocol/tasks`를 광고합니다. 요청의 `_meta`에 있는 `io.modelcontextprotocol/clientCapabilities.extensions`에 같은 이름을 넣은 요청만 task를 받습니다. 선언하지 않은 클라이언트는 지금과 같은 직접 결과를 받습니다.

```json
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"save_document","arguments":{"document_id":"<doc>","path":"report.pdf"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{"extensions":{"io.modelcontextprotocol/tasks":{}}}}}}
```

- **task가 되는 도구**: `open_document`, `open_protected_document`, `save_document`, 그리고 `execute`의 `file.open`·`file.open_protected`·`file.save`·`file.save_as`·`file.save_hml`·`file.save_pages`·`file.save_block`·`file.pdf`·`file.print_pdf`입니다. 다른 도구와 `batch`는 선언과 관계없이 직접 결과입니다.
- **결과**: `resultType:"task"`, `taskId`, `status:"working"`, `ttlMs:600000`, `pollIntervalMs:1000`을 바로 돌려줍니다. `tasks/get`을 `pollIntervalMs` 간격으로 불러 `completed`가 되면 `result`에 직접 호출과 같은 도구 결과가 들어 있습니다. 도구 실패도 `completed`이며 `result.isError:true`입니다. 실행 중에는 `statusMessage`가 5초마다 경과 시간을 알려 줍니다.
- **알림 없음**: task 상태 알림(`notifications/tasks`)은 보내지 않습니다. 사용하는 SDK가 아직 `subscriptions/listen`으로 전달하지 못하므로 폴링만 지원합니다.
- **취소**: `tasks/cancel`은 협조적입니다. 실행 중인 엔진 작업은 끝까지 가지만, 파일 교체 전에 취소되면 기존 파일을 그대로 두고 상태가 `cancelled`가 됩니다. 이미 교체가 끝난 뒤 도착한 취소는 저장을 되돌리지 않습니다.
- **수명과 범위**: task 상태는 서버 프로세스 메모리에만 있어 서버를 다시 시작하면 사라집니다. `taskId`는 UUID이며 같은 서버(HTTP는 같은 Bearer 토큰)에 접근할 수 있는 클라이언트라면 조회할 수 있습니다. `tasks/*` 요청에도 task 확장 선언이 필요하며 없으면 `-32021`, 없는 `taskId`는 `-32602`입니다. 같은 문서의 요청은 task이든 아니든 순서대로 실행됩니다.

## 진행·취소·한도와 검증 범위

`params._meta.progressToken`을 주면 도구 시작(0)과 끝, 그리고 실행 중 5초마다 해당 token의 진행 알림을 보냅니다. 값은 호출 시작부터 지난 초이며 `total` 없이 항상 증가합니다. 쪽 수 같은 엔진 내부 단계의 진행률은 아닙니다. 진행 알림으로 요청 제한 시간을 늘리는 클라이언트는 긴 저장·PDF 내보내기 동안 연결을 유지할 수 있습니다. SDK 취소 token을 문서 큐와 작업 경계에 전달하며 실행 전 취소·큐 취소·연결 종료를 처리합니다. 이미 실행 중인 동기 엔진 작업은 끝까지 진행하지만, 실행 중 취소된 요청은 파일을 덮어쓰지 않습니다. 원자적 저장이 마지막 교체 직전에 취소를 확인해 임시 파일을 지우고 기존 파일을 그대로 둡니다. 취소된 요청에는 응답을 보내지 않습니다. 취소는 이미 끝난 편집이나 이미 교체된 파일의 rollback 보장이 아닙니다. 같은 문서의 요청은 순서대로 실행하므로 긴 저장 뒤의 같은 문서 요청은 그 저장이 끝날 때까지 기다립니다. 다른 문서는 영향을 받지 않습니다. 실제 CLI에서 장시간 엔진 실행 도중 취소되는 모든 경우를 검증한 것으로 집계하지 않습니다.

stdio 입력 한 줄은 최대 4 MiB, 출력 한 메시지는 최대 32 MiB입니다. 잘못된 JSON·UTF-8·과대 줄은 오류를 내고 다음 완전한 줄에서 복구합니다. EOF의 미완성 줄도 오류입니다. HTTP 요청 본문도 4 MiB로 제한합니다. 큰 결과는 조회 limit 또는 렌더링 scale을 줄이세요. 텍스트 입력 한 번은 UTF-8 1,000,000바이트 이내입니다. JSON-RPC 배열 batch는 제공하지 않으며 `batch` 도구와 별개입니다.

검증은 아래 계층을 구분합니다. 2026-10-26 문서 갱신 시점의 최종 전체 workspace 검사는 진행 중이며, 아래 집중 검사 통과를 전체 workspace 완료로 집계하지 않습니다.

- `crates/mcp/src` 단위 검사: worker 격리·큐·취소 경계·실패한 생성 정리, SDK 스키마/카탈로그, strict JSONL, loopback HTTP 전송·Origin/Host·진행/SSE·자체 대기 handler 연결 해제. `document_subscriptions`는 기본 검사 3개와 실제 공통 서비스의 SDK stdio 수명 검사 4개로 총 7/7 통과했습니다. 공유 서비스의 두 peer, ACK·초기 resync·실제 변경·ID 구분·취소·닫힘·EOF/late-write 방지·URI 외 정보 비노출을 포함합니다. 아래 실제 HTTP 구독 제품 검사는 별도 증거입니다.
- `crates/mcp/tests/protocol.rs`: 실제 `serve_stream`+duplex와 격리된 stdio 설정 fixture를 통한 기존 행동 18개. 문서·권한·atomic·메모·중첩표·지역·HWP/HWPX 왕복·PNG를 포함합니다.
- `apps/hwpcraft-cli/tests/mcp_modern.rs`: 실제 CLI stdio subprocess 9/9 통과(`logs/0.17-cli-full-history-and-charts.log`; 이후 rich/page clipboard 추가 제품 검사는 재실행 대기). discovery·명시 핸들·작성·보호 문서·출력·오류·알림에 더해 새 차트 생성/구성·실패 상태 보존·HWP/HWPX 재열기·PNG/PDF·도넛의 실제 색/구멍 표시를 확인했습니다. 기존 redo와 암호 6조건, 1,000명령 실패 후 기존40단계 undo/redo 복원도 실제 CLI에서 통과했습니다. stdout 앞부분을 건너뛰어 통과시키지 않습니다. 실행 중 동기 작업의 실제 중단이나 한컴 차트 네이티브 호환성까지 검증한 것은 아닙니다.
- `apps/hwpcraft-cli/tests/mcp_http_modern.rs`: 실제 CLI HTTP 제품 **2/2** 통과(`logs/0.17-http-subscriptions-product.log`). 기존 문서 생성·분리·편집·저장/재열기·PDF·리소스 흐름 1개와 새 SSE 문서 구독 흐름 1개입니다. 새 흐름은 독립 TCP 두 개의 같은 listen ID·URI 격리, ACK/초기 resync/변경, close의 final complete·read -32602, disconnect 뒤 서비스 유지·재구독을 확인했습니다. HTTP 제품 경로의 PNG·실행 중 동기 엔진 취소·원격 인증까지 검증한 것은 아니며 전체 규범 적합성 판정도 아닙니다. 리소스의 발급되지 않은 requestState 거절과 정상 캐시 조회 보존은 별도 실제 stdio CLI 집중 검사 1개가 통과했습니다(`logs/0.17-mcp-resource-continuation.log`).
- `scripts/mcp-command-docs.py`: 같은 최신 CLI에 리소스 조회만 보내 정상 EOF 종료와 320개 계약 추출을 확인했습니다. 기본 UTF-8 스냅샷은 `output/implementation/0.17/mcp-command-contracts.md`이며 두 안내 문서의 hash가 유지되는 것도 확인했습니다. 이 수치는 해당 실행 파일의 registry 스냅샷이며 기능 전체 완성률이 아닙니다.

현재 광고하는 기능은 tools/resources/prompts/completions, 문서 URI의 resource-update 구독, 그리고 열기·저장 도구의 Tasks 확장(폴링 전용)입니다. 원격 인증, task 상태 알림, MRTR 후속 입력, sampling, tools/prompts/resources 목록 변경 알림은 제공하지 않습니다. `--connect`나 기존 GUI 세션 공유, 화면 클릭/키 입력, 한컴 앱 제어, DOCX 편집, OS 인쇄 대화상자 제어도 이 서버 범위 밖입니다. 새 메모의 기본 CRUD·암호 HWP/HWPX 열기·위 11종의 본문 차트 생성은 지원하지만 변경 내용 추적·모든 중첩/동적 필드 조합·아래한글과의 완전한 조판 호환을 뜻하지 않습니다. 기능별 지원 범위는 [README](../README.md)와 실행 중 discovery를 함께 확인하세요.
