# WordCraft 검토와 HwpCraft MCP 구현

## 검토한 원본

| WordCraft 소스 | 역할 | HwpCraft 적용 |
|---|---|---|
| `crates/mcp/src/server.rs` | JSON-RPC framing, lifecycle, Tools/Resources dispatch | `crates/mcp/src/server.rs`; 요청 크기 제한, 초기화 상태·요청 형태 검사 추가 |
| `crates/mcp/src/tools.rs` | 도구 스키마, 공통 명령 실행, batch, PNG 결과 | `crates/mcp/src/tools.rs`; 편집 도구 11개, batch 오류 여부를 누적 |
| `crates/mcp/src/lib.rs` | Headless Session, 렌더링, 파일 접근 scope | `crates/mcp/src/lib.rs`; RHWP Session과 기존 Renderer 사용 |
| `crates/mcp/src/backend.rs` | TCP 제어 포트에 연결하는 Remote backend, 제어 키 인증 | 미이식. HwpCraft에는 이 제어 채널이 없어 허위 UI 도구를 노출하지 않음 |
| `crates/engine/src/file_access.rs` | 읽기/쓰기 root, canonical path, 링크 검사, thread/process scope | MIT 고지와 함께 `crates/formats/src/file_access.rs`에 적용 |
| `apps/wordcraft-cli/src/main.rs` | mcp 진입점·옵션·backend 선택 | 기존 hwpcraft-cli에 `mcp` 분기 추가 |
| `crates/mcp/src/tests.rs` | MCP 호출만으로 문서 작성·검증 | HWP/HWPX 왕복 저장, 한국어/이모지 선택, 표·서식·PNG·실제 프로세스 통신 테스트 |

## 모델과 명령의 차이

WordCraft 명령은 자체 DOCX 중심 모델, 레지스트리, 바이트 기반 위치를 사용합니다.
HwpCraft는 RHWP의 HWP 구조와 `Cursor {section,paragraph,offset,cell}`을 유지합니다.
offset은 유니코드 스칼라 단위입니다. `insert.table`도 WordCraft의 `cols`가 아닌 HwpCraft의 `columns`를 받습니다.
WordCraft의 명령 이름·매개변수를 그대로 복사하면 문서 모델과 위치 단위가 달라 잘못된 편집이 됩니다.

HwpCraft의 명령 목록은 `(id,label,shortcut)` 튜플이며 UI용 파일 명령도 포함합니다.
MCP catalog는 이를 합쳐 중복을 제거하고 인자 안내를 붙입니다. 파일 작업은 MCP 경계에서 `Session::open/save/reset`으로
연결하고 편집은 `Session::run`에 위임합니다. 일반 CLI에서도 재사용하도록 `document.text`, `document.inspect`,
`select.text`는 엔진에 추가했습니다. 기존에 실행 가능하지만 목록에 빠진 글자·문단 서식 조회,
문단 모양 통합 적용, 개요 번호 모양 설정도 목록에 포함했습니다. 현재 151개 명령을 검색할 수 있습니다.

```text
MCP client
  │ stdin/stdout JSON-RPC
  ▼
hwpcraft-cli mcp → Server → Tools → Headless
                                    ├─ catalog: engine command discovery
                                    ├─ Session::run → existing edit transactions → RHWP
                                    ├─ open/save/reset → Session → formats + file_access
                                    └─ Renderer → PNG bytes → MCP image
```

## 오류와 파일 보호

- WordCraft의 헤드리스/Remote 추상화 중 실제 구현된 헤드리스 경로를 사용합니다. Desktop 기능은 향후 별도 제어 채널 설계가 필요합니다.
- `new/open`은 미저장 변경에 `force:true`를 요구합니다. 실패한 open은 문서를 교체하지 않습니다.
- WordCraft batch의 `keepGoing` 분기는 오류가 있어도 최종 성공 결과를 낼 수 있습니다. HwpCraft는 어떤 명령이든 실패하면
  `isError:true`를 유지합니다. 이미 성공한 명령은 되돌리지 않으며 이 사실을 도구 설명에 명시합니다.
- 파일 root 검사를 MCP wrapper에만 두지 않고 formats의 읽기/쓰기와 엔진의 그림 읽기에 연결했습니다.
  `execute`/`batch`로 우회할 수 없도록 인쇄 PDF도 같은 `atomic_write` 경로로 바꿨습니다.
- 이미지 확인은 메모리에서 PNG를 생성해 반환합니다. 예측 가능한 임시 파일을 공유하지 않습니다.
- 프로토콜 개행, UTF-8, 입력 한도, stdout 오염, 응답 flush, EOF 종료는 실제 CLI 프로세스 테스트로 검증합니다.

## 파일별 책임

| 파일 | 책임 |
|---|---|
| `crates/mcp/src/server.rs` | MCP 초기화·JSON-RPC·리소스·입출력 한도 |
| `crates/mcp/src/tools.rs` | 도구 입력 스키마·검증·결과 및 이미지 형식·batch |
| `crates/mcp/src/lib.rs` | 헤드리스 세션·파일 명령·렌더링·CLI 옵션 |
| `crates/mcp/src/catalog.rs` | 기존 명령 검색 및 인자 설명/예제 |
| `crates/engine/src/automation.rs` | 문서 조회·문단 범위별 조회 결과·유니코드 검색 선택 |
| `crates/formats/src/file_access.rs` | WordCraft에서 적용한 파일 접근 범위 검사 |
| `scripts/mcp-command-docs.py` | 실제 서버 discovery에서 명령 참고 문서 생성 |

MIT 출처는 `THIRD-PARTY-NOTICES.txt`와 `assets/licenses/LICENSE-WordCraft-MIT.txt`에 보존합니다.
