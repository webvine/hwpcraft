# MCP 명령 사용 안내

현재 프로토콜은 **2026-07-28**입니다. 연결·요청 메타데이터·전송·문서 수명·권한은 [MCP 안내](mcp.md)를 먼저 읽으세요. 이 문서는 오래된 명령 수나 모든 명령의 예제를 정적으로 복제하지 않습니다. 실제 실행 파일의 `tools/list`, `list_commands`, `hwpcraft://commands`가 사용 가능한 명령과 인자 계약의 기준입니다.

아래 예제는 `tools/call`의 `name`과 `arguments`입니다. **각 요청을 JSON-RPC로 감싸고 `params._meta`를 넣어야 합니다.** `<doc>`는 앞서 생성·열기에서 반환한 `structuredContent.document_id`, `<generation>`·`<revision>`은 최신 조회 결과의 정수로 치환하세요. 문서 ID를 `execute.params` 안에 넣지 않습니다.

## 전체 계약 스냅샷 생성

전체 인자 예제를 별도 파일로 보관하려면 현재 CLI 실행 파일을 지정합니다. 이 스크립트는 `initialize` 없이 MCP 2026 메타데이터가 포함된 리소스 조회만 실행하고 stdout 전체의 JSONL·응답 ID·`resultType`·명령 구조를 검사합니다.

```powershell
python scripts/mcp-command-docs.py ../.build/hwpcraft/release/hwpcraft-cli.exe
python scripts/mcp-command-docs.py ../.build/hwpcraft/release/hwpcraft-cli.exe --output output/implementation/0.17/my-command-contracts.md
```

기본 출력은 gitignored `output/implementation/0.17/mcp-command-contracts.md`입니다. 이 안내 문서를 자동으로 덮어쓰지 않습니다. `--timeout`은 기본 30초이며, 검증에 실패하면 기존 출력 파일을 교체하지 않습니다. Python 입출력과 아티팩트는 UTF-8을 사용합니다. 스냅샷은 지정한 실행 파일의 계약이며 다른 설치본이나 나중의 소스를 대신하지 않습니다.

## 실행 가능한 도움말

```json
{"name":"list_commands","arguments":{"query":"context.v2"}}
{"name":"list_commands","arguments":{"query":"tools.autotext.body"}}
{"name":"hwp_action","arguments":{"query":"CharShape"}}
```

이 세 조회에는 문서 ID가 필요하지 않습니다. `list_commands` 배열은 `result.structuredContent.items`에 있으며 각 항목에 `id`, `label`, `paramsHelp`, `paramsExample`, `hwpActions` 등이 있습니다. 특정 명령의 예제를 얻은 뒤 현재 문서에 필요한 대상·선택·개체를 준비합니다.

실행 형식은 다음과 같습니다.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"format.character.get","params":{}}}
{"name":"batch","arguments":{"document_id":"<doc>","atomic":true,"commands":[{"command":"text.insert","params":{"text":"내용"}},{"command":"paragraph.split","params":{}}]}}
```

`batch.commands`에 도구 이름이 아니라 **엔진 명령 ID**를 넣습니다. 파일 작업은 경로 기반이며 GUI 대화상자를 열지 않습니다. 공개 한글 동작 이름은 `hwp_action` 또는 discovery의 `hwpActions`로 찾으세요. 대응표가 있더라도 공개 Action/ParameterSet의 모든 옵션을 구현했다는 뜻은 아닙니다.

## 문서 생성·열기·보호 문서·닫기

```json
{"name":"new_document","arguments":{}}
{"name":"open_document","arguments":{"path":"F:/work/source.hwpx"}}
{"name":"open_protected_document","arguments":{"path":"F:/work/protected.hwp","password":"<알고 있는 암호>"}}
```

각 요청은 독립 문서 핸들을 생성합니다. 아래는 기존 문서 한 개만 교체하는 요청입니다. 다른 핸들의 문서를 바꾸지 않습니다.

```json
{"name":"open_document","arguments":{"document_id":"<doc>","path":"F:/work/source.hwpx","force":true}}
{"name":"close_document","arguments":{"document_id":"<doc>","force":true}}
```

`force`는 해당 문서의 미저장 변경을 버릴 때만 명시합니다. 미저장 변경이 없으면 생략할 수 있습니다. 잘못된 암호·실패한 열기는 기존 문서를 보존합니다. 암호는 결과로 반환하지 않지만 요청 데이터이므로 클라이언트 로그에도 남기지 않도록 다루세요. 생성 실패의 결과에는 사용할 새 핸들이 없습니다.

## AI 보고서 작성의 기본 경로

다음 요청을 같은 `<doc>`로 순서대로 실행합니다.

```json
{"name":"type_text","arguments":{"document_id":"<doc>","text":"주간 보고서\n이번 주 작업 내용을 정리합니다."}}
{"name":"select_text","arguments":{"document_id":"<doc>","text":"주간 보고서"}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"format.character","params":{"bold":true,"fontSize":1800}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"format.paragraph","params":{"alignment":"center"}}}
{"name":"inspect_document","arguments":{"document_id":"<doc>","limit":100}}
{"name":"render_page","arguments":{"document_id":"<doc>","page":1,"scale":1}}
{"name":"save_document","arguments":{"document_id":"<doc>","path":"F:/work/report.hwpx"}}
{"name":"save_document","arguments":{"document_id":"<doc>","path":"F:/work/report.pdf"}}
{"name":"open_document","arguments":{"document_id":"<doc>","path":"F:/work/report.hwpx"}}
{"name":"get_text","arguments":{"document_id":"<doc>"}}
```

`fontSize`는 1/100pt이므로 1800은 18pt입니다. 기존 글자 서식은 범위를 선택한 뒤 적용합니다. **선택 없는 `format.character`는 이후 입력의 대기 서식**이며 앞서 입력한 문단 전체에 적용되지 않습니다. `format.character.get`, `format.paragraph.get`으로 실제 속성을 조회하세요.

표는 `insert.table`, 그림·도형·수식·머리말/꼬리말·주석은 각 명령을 검색하고 준비 조건을 확인하세요. `get_text`의 문자열 일치만으로 개체·서식·조판이 보존됐다고 판단하지 않습니다. 모든 쪽을 PNG로 확인하고 저장·재열기 구조와 실제 PDF를 별도로 검증합니다.

## 차트 만들기·구성 변경

`chart.create`는 본문에서 `column`·`bar`·`line`·`pie`·`doughnut`을 지원합니다. 다음은 본문 커서가 준비된 문서의 새 차트 예제입니다. 선택이 있다면 기존 내용을 대체할 수 있으므로 삽입 대상을 먼저 확인하세요.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"chart.create","params":{"chart":{"kind":"column","title":"분기 실적","categories":["전반기","후반기"],"series":[{"name":"매출","values":[4,6],"color":"#4472C4"}],"width_mm":120,"height_mm":75,"legend":false}}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"chart.list","params":{}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"caret.get","params":{}}}
```

생성 결과의 `index`를 사용하고 최신 `generation`으로 자체 차트를 재구성합니다. 아래 `index:0`·세대 42는 예시이며 실제 반환값으로 바꿉니다. `chart`는 부분 patch가 아니라 완전한 교체 정의입니다.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"chart.configure","params":{"index":0,"expected_generation":42,"chart":{"kind":"doughnut","title":"매출 구성","categories":["전반기","후반기"],"series":[{"name":"매출","values":[4,6],"color":"#4472C4"}],"width_mm":120,"height_mm":75,"legend":false}}}}
```

`chart.configure`는 본문에 직접 있는 HwpCraft 자체 생성 차트만 받습니다. 다른 지원 차트의 값만 바꾸는 작업은 기존 `chart.data`/`chart.data.get` 계약을 검색하세요. 범주 1–1,000개, 계열 1–64개, 전체 값 16,000개 이내이며 각 계열의 값 수는 범주 수와 같아야 합니다. 원형·도넛은 한 계열·음수 없는 값·양수 합계를 요구합니다. 다른 종류·중첩/그룹 도넛 표시를 지원한다고 해석하지 마세요.

새 차트는 실제 내장 OOXML/OLE 데이터를 사용하고 자체 HWP/HWPX 저장·재열기와 PNG/PDF 표시를 검사했습니다. 실제 CLI 종단 검사에는 생성→도넛 구성→값 변경→실패 rollback→왕복·시각 확인이 포함됩니다. 그러나 생성 결과의 **`nativeInterop:"unverified"`**는 그대로 유지합니다. 한컴 아래한글에서 새 차트를 네이티브로 열고 편집하는 호환성 검증은 아직 완료되지 않았습니다.

## 선택·배치 좌표·문맥 편집

`select_text`는 본문/1단계 셀의 **한 문단 안**에서 n번째 비중첩 일치를 선택합니다. 문단 사이의 범위를 정확히 지정하려면 먼저 `caret.get`으로 `generation`을 읽고 `selection.range`를 사용합니다.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"caret.get","params":{}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"selection.range","params":{"start":{"section":0,"paragraph":0,"offset":0},"end":{"section":0,"paragraph":1,"offset":2},"expected_generation":42}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"page.layout.get","params":{"page":1,"includeControls":true}}}
```

위 42는 예시 세대입니다. 실제 요청에는 직전 조회에서 반환한 정수를 넣습니다. 커서 offset은 Unicode 스칼라이며 같은 구역의 본문 또는 같은 1단계 셀 안의 범위만 지원합니다. 셀·본문 간, 다른 셀 간, 표현할 수 없는 깊은 경로는 이 API에서 거절합니다. `selection.range`는 선택 방향도 보존하며 잘못된 주소·세대는 기존 선택을 유지합니다.

`page.layout.get.page`는 1부터이며 `pageInfo`, 선택적으로 `controlLayout`을 반환합니다. `caret.get`의 geometry에는 0부터의 `pageIndex`가 있습니다. 좌표 단위는 배율 적용 전 `page_px`입니다. 시각 좌표 조회와 문서 구조 주소를 혼동하지 마세요.

기본 `get_text`/`inspect_document`는 본문과 1단계 표 셀만 페이지 조회합니다. 머리말·꼬리말·각주·미주·글상자·중첩 표는 종류가 지정된 주소로 탐색합니다.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"context.v2.list","params":{"limit":100}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"context.v2.get","params":{"address":{"version":2,"section":0,"paragraph":0,"path":[]}}}}
```

목록에서 반환한 `address`를 그대로 `context.v2.get`에 전달하세요. `path`에는 `table_cell`, `header`, `footer`, `textbox`, `footnote`, `endnote` 등의 종류와 인덱스가 있습니다. 부모 개체의 인덱스를 추측하여 만들지 않습니다. 변경 명령은 `context.v2.replace/split/merge/character/paragraph`이며 최신 `expected_generation`을 요구합니다. 교체는 한 문단 안의 Unicode 범위, 병합은 같은 컨테이너 안에서만 가능합니다. 보호된 조상·인라인 제어 손실·지원하지 않는 구조는 명시적으로 거절합니다. 기존 `context.*`, 개체별 조회와 v2의 주소 계약을 서로 바꾸어 사용하지 마세요.

## 서식 있는 본문 상용구

기존 글상용구 `tools.autotext.*`와 별도로 `tools.autotext.body.*`를 사용합니다. 설정 파일도 분리되어 기존 글상용구 내용·100KB 글자 데이터 한도를 변경하지 않습니다.

| 명령 | 실제 인자·결과 |
|---|---|
| `.source` | 인자 없음. 고정할 `source:{section,start,end}`, 현재 `generation`, 요약·문단 수 |
| `.list` | 인자 없음. store `revision`과 메타데이터; binary payload는 반환하지 않음 |
| `.add` | `name`, 필수 `expected_generation`, 선택 `source`, `description`, `overwrite`, `expected_store_revision` |
| `.get` | `id`, 선택 `expected_store_revision`; 메타데이터 조회 |
| `.preview` | `id`, 필수 `expected_generation`, 선택 `expected_store_revision`; 첫 쪽 SVG·strict import 검사 |
| `.insert` | `.preview`와 같은 인자; 문서에 삽입하고 한 단계 undo 지원 |
| `.delete` | `id`, 선택 `expected_store_revision`; 설정 항목 삭제 |

`.source`는 완전한 본문 문단 선택을 쓰고, 선택이 없으면 현재 본문 문단 전체를 사용합니다. `source.start/end`는 0부터의 문단 범위 **[start,end)** 입니다. 선택 일부·셀에서의 캡처는 거절합니다. 등록 UI나 에이전트가 대기하는 동안 문서가 바뀔 수 있으므로 조회한 `source`와 `generation`을 함께 고정하세요.

지원 프로필은 본문 문단의 글자/문단 서식, 표, 내장 그림, 지원하는 기본 도형·글상자입니다. 외부 그림 링크, 미지원 제어·동적 필드·그룹·메모 등을 묵시적으로 평문으로 바꾸지 않습니다. 삽입은 선택이 없는 본문 문단의 시작/끝 경계에서만 가능하며 문단 중간·셀·남은 선택은 거절합니다. 대상의 기존 빈 문단과 용지/여백을 보존하며 실패 시 문서와 설정을 유지합니다. 지원 프로필은 실제 fragment 검사에 따라 거절될 수 있으므로 `.preview`를 먼저 사용하세요.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"tools.autotext.body.source","params":{}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"tools.autotext.body.list","params":{}}}
```

위 조회 후 `.add`에는 실제 정수 `expected_generation`과 `expected_store_revision`, 고정한 `source`를 전달합니다. `.add` 결과의 `entry.id`를 보관하고 대상 문서의 최신 세대에서 `.preview`/`.insert`를 호출합니다. 문서 세대와 설정 store revision은 별도입니다. 등록·삭제는 설정 변경이므로 문서 undo나 atomic batch로 되돌리지 않습니다.

새 저장소는 설정 폴더의 `autotext-body-v1.json`과 별도 lock 파일을 사용합니다. payload는 독립 HWPX이며 항목당 8 MiB, 압축 해제 32 MiB·ZIP 1,024개·프로필 깊이 32/노드 16,000 제한을 적용합니다. 설정 파일 전체는 32 MiB, 항목은 최대 1,000개이고 SVG 미리보기는 4 MiB까지입니다. 권한·동시 갱신·잘못된 파일은 오류로 알리며 자동 덮어쓰지 않습니다.

## 공개 한글 동작과 ParameterSet

```json
{"name":"hwp_action","arguments":{"document_id":"<doc>","id":"CharShape","parameterSet":{"Height":1800,"Bold":1,"TextColor":5649426}}}
```

이 예제도 기존 글자를 고치려면 먼저 선택합니다. 알려진 공개 항목만 변환하며 미지원 항목을 무시해 성공으로 반환하지 않습니다. `parameterSet` 지원 범위는 `hwp_action` 목록 항목의 `parameterSet` 설명을 읽으세요. 일부 대응은 `complete:false`입니다. 직접 `params`는 대응표 기본값 위에 적용되며 이름만으로 한컴 COM 동작 전체를 호출하는 기능은 아닙니다.

## 프롬프트·완성·리소스 예제

아래는 `tools/call`이 아닌 JSON-RPC method와 params입니다. 각각 `_meta`를 추가합니다.

```json
{"method":"prompts/get","params":{"name":"report-authoring","arguments":{"title":"주간 개발 보고서","audience":"개발팀","language":"ko","format":"hwpx"}}}
{"method":"completion/complete","params":{"ref":{"type":"ref/prompt","name":"report-authoring"},"argument":{"name":"format","value":"hw"}}}
{"method":"resources/read","params":{"uri":"hwpcraft://documents/<doc>"}}
```

프롬프트는 작성 지침과 작업 데이터를 반환합니다. 클라이언트가 도구 호출·저장·검증을 수행해야 하며, `requestState`/`inputResponses`를 이용한 MRTR 후속 흐름은 받지 않습니다. 문서 리소스는 지정 핸들의 첫 100개 문단이며 이후 내용은 `inspect_document`의 `nextStart`를 따라 조회하세요. 문서 핸들 완성은 다른 문서 ID를 노출하지 않고 빈 후보를 반환합니다.

## 문서 변경 구독의 수명

구독은 도구가 아니라 `subscriptions/listen` JSON-RPC 요청입니다. `capabilities.resources.subscribe:true`인 서버에서 **`params.notifications.resourceSubscriptions`**를 사용합니다. 별도의 `params.filter`나 legacy `resources/subscribe`를 보내지 않습니다.

```json
{"jsonrpc":"2.0","id":7,"method":"subscriptions/listen","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}},"notifications":{"toolsListChanged":true,"resourcesListChanged":true,"promptsListChanged":true,"resourceSubscriptions":["hwpcraft://documents/<doc>"]}}}
```

서버는 먼저 `notifications/subscriptions/acknowledged`로 실제 수락한 필터를 보냅니다. 위 세 목록 변경 종류는 현재 수락하지 않고 명시한 live 문서 URI만 수락합니다. 확인 알림 이후 URI마다 한 번의 초기 resync가 오므로 현재 리소스를 다시 읽습니다. **초기 resync를 실제 편집의 증거로 세지 않습니다.** 이후 변경도 `notifications/resources/updated`의 URI-only 무효화이며 빠른 변경은 합쳐질 수 있습니다.

모든 알림의 `params._meta["io.modelcontextprotocol/subscriptionId"]`는 원래 listen 요청의 ID입니다. 같은 연결의 숫자 `7`과 문자열 `"7"`을 구별하고, 독립 클라이언트의 같은 ID도 서로 다른 수명으로 관리합니다. 변경 요청을 보낸 클라이언트가 아니라 해당 URI를 구독한 연결에 알립니다. 최대 서로 다른 문서 URI 16개, 서버 전체 동시 listen 64개이며 본문·암호·저장 경로·다른 핸들을 알림에 담지 않습니다.

```json
{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7,"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}
```

stdio 취소는 위 ID 없는 알림으로 끝내며 취소 처리가 완료된 구독에는 이후 변경의 새 업데이트를 보내지 않습니다. stdio EOF도 연결 전용 token으로 그 연결의 listen만 종료하고 다른 클라이언트 구독과 일반 응답 drain은 유지합니다. 문서 close는 마지막 URI 업데이트→리소스 not-found 순서로 관찰하며, 마지막으로 구독한 URI까지 사라지면 원래 listen의 final complete가 옵니다. 여러 URI 중 하나만 닫히면 나머지 구독은 계속됩니다. 상세 ACK·업데이트 형태는 [문서 URI 변경 구독](mcp.md#문서-uri-변경-구독)에 있습니다.

구독 검사는 기본 3개와 실제 SDK stdio 경로 4개로 총 7/7 통과했습니다. 공유 서비스의 두 peer를 포함해 수명·필터·ID·취소·EOF·닫힘·비노출을 확인했습니다. 실제 CLI HTTP 제품도 기존 문서 흐름 1개와 새 SSE 구독 흐름 1개로 **2/2 통과**했습니다. 새 구독 검사는 독립 TCP 두 개의 동일 listen ID·URI 격리, ACK/초기 resync/변경, close 뒤 final complete·read -32602, 네트워크 disconnect 뒤 서비스 유지·재구독을 확인했습니다(`logs/0.17-http-subscriptions-product.log`). **해당 제품 시나리오의 통과이며 전체 normative 적합성 증명은 아닙니다.** 공식 문서의 서버 주도 종료 문구 충돌(N-01)은 미해결로 남습니다. HTTP PNG·실행 중 동기 엔진 취소·원격 인증은 이 검사로 증명하지 않습니다.

## 오류 뒤 재시도

JSON-RPC `error`와 `result.isError`를 먼저 검사합니다. `expected_generation` 또는 `expected_store_revision` 오류를 숫자만 바꾸어 맹목적으로 재시도하지 마세요. 새 조회로 대상 범위·주소·상용구 내용을 다시 확인한 후 요청을 구성합니다. 기본 batch의 앞선 성공은 남을 수 있고 atomic은 문서 변경만 복구합니다. 취소도 완료된 편집이나 파일 쓰기를 되돌리는 트랜잭션은 아닙니다.

MCP 전송과 실제 CLI 검사 범위는 [MCP 안내의 검증 범위](mcp.md#진행취소한도와-검증-범위)를 참고하세요. 자동 검사를 통과한 대표 경로와 아래한글의 모든 기능·조합 검증은 구별합니다.
