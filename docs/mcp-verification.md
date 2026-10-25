# MCP 검증 기록

검증일: 2026-10-24. Windows x64, Rust 1.98.1, 작업 공간의 RHWP 경로 의존성을 사용했습니다.

## 자동 검사

| 검사 | 결과 | 기록 |
|---|---|---|
| `cargo +1.98.1 test --locked --workspace` | 110 통과, 0 실패, 기존 수동용 테스트 2개 ignored | `logs/mcp-workspace-tests.log` |
| `cargo +1.98.1 clippy --locked --workspace --all-targets -- -D warnings` | 통과 | `logs/mcp-clippy.log` |
| `cargo +1.98.1 fmt` (HwpCraft 패키지 이름을 명시) | 적용 | RHWP/WordCraft 소스를 포맷하지 않음 |
| `cargo +1.98.1 fmt --check` (HwpCraft 8개 패키지 명시) | 통과 | RHWP 경로 의존성 제외 |
| `cargo +1.98.1 build --locked --release -p hwpcraft-cli` | 통과 | `logs/mcp-release-build.log` |

최종 실행 파일: `../.build/hwpcraft/release/hwpcraft-cli.exe`.
별도 임시 폴더를 읽기/쓰기 root로 지정한 배포용 프로세스에서도 초기화·151개 명령 조회·한글/이모지 입력·선택·굵게 적용·
HWPX/HWP/PDF 저장·PNG 반환을 확인했습니다. JSON-RPC 응답 9개가 모두 성공했고,
HWP 13,312바이트, HWPX 7,464바이트, PDF 17,345바이트, PNG 2,995바이트를 생성했습니다.
이 크기는 스모크 테스트 문서의 결과이며 일반 문서 크기 제한을 뜻하지 않습니다.

## 새 테스트 12개

- MCP 프로토콜 7개: 초기화·오류·리소스·입력 크기 제한, 명령 검색 및 인자 예제 누락 검사,
  공개 예제로 서식/쪽/표 편집, 한글/이모지 선택과 서식/표 편집 후 HWP/HWPX 왕복 저장·PDF/TXT 출력·PNG 디코딩,
  실패 시 문서/선택/undo 보존과 batch 부분 성공, 폴더 제한의 도구/명령/batch/그림/PDF 적용,
  실패한 저장의 기존 파일 보존 및 내보내기의 미저장 상태 보존.
- CLI 프로세스 3개: 실제 stdio로 한글 문서 편집·저장·읽기와 stdout 순도,
  잘못된 옵션의 stdin 대기 전 거부, stdin이 열린 동안 응답 flush.
- 파일 접근 2개: 경로 구성요소·상위 이동·권한 분리·scope 복원·Windows 장치 이름,
  루트 밖을 향한 심볼릭 링크. 심볼릭 링크 생성 권한이 없으면 해당 검사 내부에서 사유를 출력하고 건너뜁니다.

명령 참고 문서는 실제 빌드한 서버의 `hwpcraft://commands` 응답에서 생성했습니다.
151개 명령 모두 인자 설명과 객체 형태의 예제가 있으며 동일 명령 ID는 중복하지 않습니다.
일부 명령 예제에는 현재 셀·기존 개체·캡션 등 사전 조건이 필요합니다.

## 검증 범위

테스트는 HwpCraft 엔진과 RHWP를 사용합니다. 새 MCP 출력물을 실제 한컴 아래한글에 열어 비교하는 검증,
GUI 자동 조작, 실제 외부 MCP 클라이언트 앱에 서버를 등록하는 작업은 포함하지 않았습니다.
실행 중인 데스크톱 앱 제어와 HTTP 전송은 구현 범위에 없습니다.
