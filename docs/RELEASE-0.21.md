# HwpCraft 0.21 — AI 편집 패널·대화 기록·명령줄 도구 현행화

0.21 릴리스. 아래한글 전체 기능의 100% 호환을 뜻하지 않는다. 한글 2024 공개 도움말 명령 325행 대비 기능 완성도는 0.20과 같은 64%다. 0.21의 새 기능(AI 편집 패널)은 한글에 없는 기능이라 그 판정에 넣지 않고 기능 원장의 'HwpCraft 고유 기능' 절에 따로 적었다.

## AI 편집 패널

- 창 오른쪽 패널에서 이 PC에 설치·로그인된 Claude Code 또는 Codex에게 말로 요청하면, 열린 문서를 MCP로 바로 고친다. 보기 → AI 편집 패널(메뉴 줄의 ✦ AI)로 열고 닫으며, 열림과 폭(260–560 px)을 기억한다.
- 선택한 곳이나 커서가 있는 문단(표 셀 포함)이 맥락으로 함께 가서 '이 부분'이 그곳을 뜻한다. 맥락 칩의 ✕로 맥락 없이 보내면 AI는 문서 전체를 두고 판단하고, '이 부분'이 무엇인지 분명하지 않으면 편집하지 않고 되묻는다.
- 질문 하나의 편집은 Ctrl+Z 한 번으로 되돌리고 Ctrl+Y로 다시 한다. 질문 전의 사용자 실행 취소 기록은 AI가 많이 고쳐도 밀려나지 않는다. 편집이 없었으면 다시 실행 기록도 그대로다.
- 실행한 명령은 한 줄로 접어 보여 주고, 응답 중에는 중지할 수 있다(CLI 자식 프로세스까지 끝냄). AI가 편집하는 동안 문서 입력·클릭·편집 명령·열기·새 문서는 잠기고(스크롤·확대·보기 명령은 됨), 상태 표시줄에 표시된다.
- 모델과 추론 수준을 고를 수 있다. 기본은 가장 최신 모델이다(Claude Code는 Fable·Opus·Sonnet·Haiku 계열의 최신, Codex는 Codex가 알려 주는 모델 목록의 첫 모델). 선택은 앱을 실행하는 동안 엔진별로 기억한다.
- 대화 잇기와 새 대화, 엔진 바꾸기. 실행 시간 상한(10분)을 넘으면 묻지 않고 중지한다.
- 대화 기록: 대화를 문서별로 남겨, 저장한 문서를 다시 열면(그 뒤 바뀌지 않았으면) 마지막 대화가 패널에 다시 열리고, 다른 대화는 '이 문서의 대화 기록'에서 불러와 이어 간다(CLI에 그 대화가 남아 있으면 그대로 잇고, 없으면 앞 대화의 요약을 다음 질문에 함께 보낸다). 문서는 문서 파일에 아무것도 넣지 않고 경로와 본문 글의 지문으로 찾아서, 옮기거나 이름을 바꾼 파일은 '다른 위치의 같은 문서일 수 있음', 다른 프로그램에서 고친 파일은 '그 뒤 문서가 바뀜'으로 보인다. 다른 이름으로 저장하면 대화가 새 파일을 따라간다. 기록은 설정 폴더(`ai-history`)에 이 사용자만 읽게 저장하고 MCP 토큰·문서 id는 남기지 않는다. 화면 캡처·성능 측정 실행은 기록을 남기지 않는다. 끄기, Markdown으로 내보내기, 한 대화·이 문서·전부 지우기(Claude Code·Codex 쪽 대화 파일까지)를 할 수 있다.
- 바뀐 곳과 되돌리기: 답마다 바꾼 문단이 보이고 누르면 문서의 그곳으로 간다. '되돌리기'는 그 질문의 편집만 되돌린다. 바로 뒤면 실행 취소 한 번이고, 그 뒤 문서가 바뀌었으면 글만 고친 질문에 한해 바뀐 글을 모두 찾은 뒤 한 실행 취소 단계로 되돌린다(못 찾으면 아무것도 바꾸지 않음).
- 변경 추적으로 남기기(선택): 켜 두면 글만 고친 질문을 변경 추적(작성자 'AI · Claude Code'/'AI · Codex')으로 다시 적용해 검토 탭에서 하나씩 수락·거부한다. 실제로 바뀐 글자만 표시되고, 적용 전에 모두 수락한 결과가 AI의 결과와 같은지 미리 확인하며, 다르면 AI 결과를 그대로 둔다. 질문은 여전히 Ctrl+Z 한 번으로 되돌린다.
- 안전 장치: 질문마다 새로 만드는 일회용 토큰으로만 연결하고, 끝나면 그 토큰은 바로 무효다. AI는 이 창의 문서만 고칠 수 있고, 저장·열기·새 문서·닫기, 파일 쓰기·내보내기(PDF 포함), 로컬 파일 읽어 넣기, 사용자 설정 폴더 쓰기(상용구·매크로·스타일마당·표 마당·수식 매크로), 암호, 실행 취소·다시 실행은 거절된다(단일 명령·묶음·한글 동작·매크로 모든 경로). 그동안 다른 MCP 클라이언트는 이 문서를 읽기만 할 수 있다. Claude Code는 내장 도구와 사용자 설정을 끄고 HwpCraft MCP 도구만 쓰게, Codex는 셸·브라우저·플러그인을 끈 전용 설정으로 실행한다(셸을 끌 수 없는 판이면 쓰지 않음).
- 실제 Claude Code·Codex로 시나리오 26개(빈 문서·HWPX, 맥락, 잇기, 중지, 비정상 종료, Ctrl+Z 한 번, 제한 모드, 셸·파일 읽기 차단, 다른 클라이언트, 잠금, 배포용 문서, 연결 수명, 한글 입력)를 두 엔진 모두 통과했다.
- 대화 기록 시나리오 11개(기록, 다시 열어 이어 묻기, 바뀜·이동·복사본 감지, 다른 편집 뒤 되돌리기, CLI 사본까지 지우기, 변경 추적으로 남기기·수락·실행 취소, 서식 질문 제외)를 두 엔진 모두 통과했다.

## 명령줄 도구

- 새 `run <문서|--new> <명령|한글 동작> [params] [-o 출력]`: 명령 하나를 실행하고 결과를 JSON으로 낸다. 조회는 파일을 쓰지 않고, `-o`가 있을 때만 성공한 뒤 원자 저장한다.
- `commands [검색어] [--json]`(MCP `list_commands`와 같은 도움말·예시), 새 `actions [검색어]`(한글 동작 이름과 대응 명령).
- `edit`: 계획을 표준 입력(`-`)으로 받고, `hwpAction` 단계와 `--keep-going`을 쓸 수 있다.
- `render [쪽|all] --scale`, `--new-password`·`--remove-password`, `info`의 형식·암호·배포용·변경 추적 필드.
- 도움말을 다시 썼다(HTTP MCP 옵션, `-h`·`help`·`-V`). 종료 코드는 사용법 오류 2, 실패 1이고, 오류에 경로와 할 일을 붙였다.
- 같은 22개 기준으로 MCP·데스크톱과 비교한 CLI 완성도: 64% → 100%(실제 실행 63건, 새 CLI 시험 6개).

## MCP

- AI 편집 패널을 위한 데스크톱 연결 실행: 질문마다 일회용 토큰을 만들고 실행이 끝나면 무효로 한다. 그동안 AI는 이 창의 문서만 고친다.
  - 문서 열기·만들기·저장·닫기 도구, 그리고 파일 쓰기·읽기·설정 폴더 쓰기·암호·실행 취소 명령 41개는 `execute`·`batch`·`hwp_action`·`macro.run`·창 PDF 저장 어디로 보내도 거절한다.
  - 다른 클라이언트는 그 문서를 읽기만 할 수 있다.
  - 질문 하나의 편집은 실행 취소 한 단계로 묶이고, 묶음 안에서도 atomic batch 되돌리기와 `keepGoing`이 그대로 동작한다.
  - 실행을 끝낼 때 대기 중이던 요청은 같은 규칙으로 마저 처리한다(최대 3초).
- 실제 HTTP 브리지 종단 시험으로 41개 명령이 네 경로에서 거절되는지, 파일·설정 폴더에 아무것도 쓰지 않는지, 서식·batch·롤백이 한 번에 되돌아가는지 확인했다. 이 시험이 창 PDF 저장 작업의 제한 누락(.pdf로 저장하는 `file.pdf`·`file.save`·`file.save_as`)을 찾아 고쳤다. 사용법은 [MCP 연결과 사용법](mcp.md#ai-편집-패널의-실행-021).

## 성능

- 서식 도구 줄이 위젯마다 다시 읽던 글자·문단 모양과 스타일 목록을 한 화면 그리기에 한 번만 읽는다. 서식 도구 줄 비용(중앙값) 연속 입력 0.9 → 0.55 ms, 쪽 이동 1.2 → 0.7 ms, 찾기 중 입력 0.9 → 0.5 ms.
- AI 실행 묶음의 추가 비용은 381쪽 편람에서 질문당 시작 5–9 ms·끝 약 2 ms이고, 묶음 안의 편집은 평소 편집과 같다.
- 화면 속도: 캐럿 위치를 문서·커서가 바뀔 때만 다시 계산하고, 상태 표시줄이 변경 추적 상태를 캐시에서 읽는다. 381쪽 편람(1500×916, release, 중앙값)에서 휠 스크롤 프레임 5.7–6.0 → 1.2–1.5 ms, 끌기 13–14 → 7.6 ms, 확대/축소 7.4 → 1.4 ms, 창 크기 바꾸기 5.5 → 1.2 ms, 한 프레임 CPU 6.65 → 1.9–2.4 ms. AI 패널 열기·닫기는 각각 3프레임, 최대 6.5 ms이고, 시작은 0.20과 같은 140–151 ms다.

## 고친 결함

- MCP: 0.20에서 2026-07-28 판이 없앤 메서드(`initialize`, `logging/setLevel`, `resources/subscribe`·`unsubscribe`, `tasks/result`, `tasks/list`)를 2026 요청으로 보내면 규격의 404·-32601 대신 200·-32602로 답하던 회귀(공식 conformance 2026 요구 시나리오 103 → 109 통과로 복구). 2025 세션과 `tools/call` 형식 오류의 -32602는 그대로다.
- DOCX 내보내기: 쪽 나누기를 말없이 버리던 것을 Word의 "앞에서 쪽 나누기"로 쓰고, 담을 수 없는 단 나누기는 담지 못한 것 목록(CLI 오류 출력·MCP `omitted`)에 올린다.
- 문단 번호·글머리표를 없애도 HwpCraft가 번호를 켤 때 준 왼쪽 여백이 남던 것(한글처럼 되돌림, 문단 자신의 여백은 그대로).
- CLI: `run`·`edit` 단계의 `file.save`·`file.save_as`·`file.pdf`(한글 동작 FileSave 등 포함)가 CLI 저장 규칙으로 실제 저장하고, `file.new`·`file.open`은 `--new`·문서 경로·`--password`를 쓰라고 안내한다.
- 한글 동작: 기본 인자가 비어 실행되지 않던 동작 29개를 정리했다. 이름이 뜻을 정하는 15개(표 캡션 위치 8, 셀 둘로 나누기 2, 줄 더하기, 셀 크기 조절 4)는 그 기본 인자로 실행되고, 호출자 값이 필요한 14개(캡션 위치 8, 줄 크기 조절 4, 쪽 번호 양쪽, 첫 줄에서 결과가 다른 `TableSubtractRow`)는 부분 지원으로 표시한다. 뜻은 한글 실측으로 확인했다.
- 표: 새 표의 줄(282 HU)을 1mm 늘렸다가 원래대로 줄이면 크기 조절 하한(283 HU)에 걸려 거절되던 것.
- RHWP 0103: 0101의 머리말·꼬리말 캐시가 구역 번호와 구역 수 변화를 키로 삼는다(구역을 넣거나 지운 뒤 다른 구역의 캐시를 쓰지 않게).
- AI 편집 패널은 작업 창처럼 상태 표시줄까지 닿고, 좁은 창에서는 창 폭의 절반까지만 차지한다(기억한 폭은 넓은 창에서 다시 씀). AI가 편집하는 동안 서식 도구 줄 전체가 흐려지고 상태 표시줄에 'AI 편집 중'이 나온다. AI 실행을 끝낼 때는 요청이 실패했거나 연결이 먼저 끊겨도 제한을 풀고 실행 취소 단계를 닫는다.
- 문자표 창이 창 높이만큼 커지고 제목 줄이 짧던 것(0.20).
- macOS: 단축키 시험이 ⌘ 표기를 기대하고, 다른 브라우저 출처의 요청을 거절할 때 본문을 먼저 읽어 macOS가 연결을 끊지 않고 403을 돌려준다(0.20 릴리스 빌드에서 찾음, v0.20에도 들어감).
- 시험이 사용자 설정 폴더(최근 문서·보기 설정)에 쓰지 않고, 시험용 설정 폴더는 한 시간 뒤 다음 시험이 지운다. 화면 캡처·성능 측정 실행도 설정을 저장하지 않는다.

## 검증과 한계

- CI와 같은 방식(깨끗한 RHWP v0.8.7에 패치 87개를 이름순으로 적용)으로 Windows x64 `cargo test --release --locked --workspace` 1,155 통과·0 실패(무시 17), `cargo clippy --workspace --all-targets -D warnings` 0. Linux(WSL) 같은 시험 1,155 통과·0 실패(무시 17). macOS(arm64)는 개발 중 교차 clippy 0이었고, 릴리스 빌드에서 GitHub Actions가 Windows·macOS·Linux workspace 시험을 다시 돌린다.
- RHWP 로컬 패치는 `patches/rhwp/`에 MIT 라이선스 그대로 기록한다(0001–0103 중 87개). 0.21에서 새 외부 Rust 크레이트는 없다.

### 알려진 한계

- AI 편집 패널은 Claude Code 또는 Codex CLI가 설치·로그인되어 있어야 한다(API 키를 직접 부르지 않음). AI는 파일에서 그림을 넣을 수 없다. 표 안의 표·글상자 안 캐럿에서는 맥락을 보내지 않는다. 각 CLI는 대화를 잇기 위해 자기 폴더에 대화 기록(맥락으로 보낸 문서 글 포함)을 남긴다. 실행 중 Codex 로그인 토큰이 갱신되면 사용자의 `~/.codex/auth.json`에 반영한다(원본이 그동안 바뀌지 않았을 때만). 질문을 보내기 직전에 다른 MCP 클라이언트가 보낸 편집은 그 질문의 실행 취소 단계에 섞일 수 있다.
- AI 대화 기록의 바뀐 곳은 본문·표 셀 문단의 글만 기록한다. 서식·그림·표 구조의 변화는 '바뀐 곳'에 나오지 않고, 그런 질문과 개체가 든 문단을 바꾼 질문은 그 뒤 문서가 바뀌면 패널에서 되돌리거나 변경 추적으로 남기지 않는다(Ctrl+Z는 됨). 이미 변경 추적이 있는 문서에는 AI 변경을 변경 추적으로 섞지 않는다. 문서는 본문 글의 지문으로도 찾아서 글이 같은 다른 문서가 같은 문서로 보일 수 있고, 같은 글의 문단이 여러 곳에 있으면 '되돌리기'가 자리를 정하지 못해 아무것도 바꾸지 않는다.
- 글자처럼 취급한 개체 때문에 높아진 줄에서 ↓를 누르면 바로 아래 줄을 건너뛸 수 있고, 그런 문단을 마우스로 누르면 캐럿이 한 글자 앞에 놓일 수 있다(RHWP 위치 계산, 0.19부터 있었음, 다음 버전에서 고침).
- 글자처럼 취급하지 않은 표(위아래 배치) 바로 뒤 문단은 저장한 문서를 다시 열면 표 바깥 여백(기본 1mm)만큼 표 마지막 줄에 겹쳐 그려지고, 떠 있는 표에 캡션을 달면 바로 다음 문단이 캡션 위에 겹쳐 그려진다. 글자처럼 취급한 표는 정상이다(RHWP 배치 한계, 0.19부터 있었음, 다음 버전에서 고침).
- AI 편집 패널은 리본의 파일 탭(문서 정보·최근 문서 화면)에서는 숨고, 다른 탭으로 돌아오면 다시 보인다(✦ AI 단추는 켜진 채로 남음).
- AI 편집 패널은 Windows에서 실제 창과 실제 Claude Code·Codex로 확인했다. macOS·Linux에서는 시험(Unix 프로세스 그룹 중지 포함)만 돌렸고 데스크톱 화면은 직접 확인하지 않았다.

---

# HwpCraft 0.21 — AI edit panel, conversation history, command-line tool brought level

0.21 release. This is not 100% compatibility with every Hangul feature. Against the 325 commands of Hangul 2024's public help HwpCraft scores 64%, as in 0.20. The new AI edit panel is not a Hangul feature, so it is not part of that score; the feature ledger lists it separately under "features of HwpCraft's own".

## AI edit panel

- Ask the locally installed and signed-in Claude Code or Codex in the panel on the right of the window, and it edits the open document at once through MCP. View → AI edit panel (✦ AI on the menu bar) shows or hides it; whether it is open and its width (260–560 px) are remembered.
- The selection, or the paragraph holding the caret (table cells included), is sent along, so "this part" means that place. Sent without it (✕ on the context chip), the AI judges by the whole document and, when "this part" is unclear, asks instead of editing.
- All edits from one question undo with a single Ctrl+Z and redo with Ctrl+Y. The user's undo history from before the question is not pushed out however much the AI edits, and redo stays when nothing changed.
- The commands it ran are folded into one line, and a running answer can be stopped (the CLI's child processes included). While the AI edits, typing, clicks, editing commands, opening and new documents wait (scrolling, zoom and view commands still work), and the status bar says so.
- The model and reasoning level can be chosen; the newest model is the default (for Claude Code the latest Fable, Opus, Sonnet or Haiku model, for Codex the first model in the list Codex provides). The choice is kept per engine while the app runs.
- Continuing a conversation, new conversation, switching engines. A run longer than the time limit (10 minutes) is stopped without asking.
- Conversation history: conversations are kept per document. Opening a saved document again (unchanged since) brings its last conversation back, and others can be picked under "Conversations about this document" and continued (through the CLI's own conversation while it still has it, otherwise a summary of the earlier turns goes with the next question). Nothing is written into the document file: a document is recognized by its path and a fingerprint of its body text, so a moved or renamed file shows as "may be the same document at another location" and a file edited in another program as "the document has changed since". Save As takes the conversation along to the new file. The history is stored in `ai-history` in the settings folder, readable only by the current user, without MCP tokens or document ids; screen-capture and benchmark runs keep none. It can be turned off, exported as Markdown, and deleted per conversation, per document or all at once (Claude Code's and Codex's conversation files included).
- Changed places and Revert: each answer lists the paragraphs it changed, and clicking one moves there. Revert takes back only that question: one undo right after it; once the document has changed since, a question that only changed text is taken back as one undo step after all of its changed text has been found (nothing changes if any is missing).
- Recording as tracked changes (optional): when turned on, a question that only changed text is applied again as tracked changes (author "AI · Claude Code" or "AI · Codex") to be accepted or rejected one by one on the Review tab. Only the characters that actually changed are marked; a rehearsal first checks that accepting everything gives exactly the AI's result, and otherwise the AI's result is left as it was. One Ctrl+Z still takes the question back.
- Safety: the CLI connects only with a token made for that question, which stops working when it ends. The AI edits only this window's document; saving, opening, new documents and closing, writing or exporting files (PDF included), reading local files into the document, writing the user's settings folder (autotext, macros, style sets, table styles, equation macros), passwords and undo / redo are refused on every path (single commands, batches, Hangul actions, macros). Other MCP clients can only read the document meanwhile. Claude Code runs with its built-in tools and user settings off and only HwpCraft's MCP tools allowed; Codex runs with its own settings with the shell, browser and plugins off (a Codex version whose shell cannot be turned off is not used).
- 26 scenarios (empty document and HWPX, context, continuing, stopping, abnormal exit, one Ctrl+Z, restrictions, shell and file reading blocked, other clients, locking, distribution documents, connection lifetime, Korean input) and 11 conversation-history scenarios passed with both engines, run with the real Claude Code and Codex.

## Command-line tool

- New `run <doc|--new> <command|Hangul action> [params] [-o out]`: runs one command and prints its result as JSON. Queries write nothing; only `-o` saves, atomically and only on success.
- `commands [query] [--json]` (the same help and examples as MCP `list_commands`) and new `actions [query]` (Hangul action names and their commands).
- `edit` reads its plan from standard input (`-`) and takes `hwpAction` steps and `--keep-going`.
- `render [page|all] --scale`, `--new-password` / `--remove-password`, and format, password, distribution and track-change fields in `info`.
- Rewritten help (HTTP MCP options, `-h` / `help` / `-V`). Exit code 2 for usage errors and 1 for failures; errors name the path and what to do.
- CLI completeness against MCP and the desktop on the same 22 items: 64% → 100% (63 real runs, 6 new CLI tests).

## MCP

- Desktop connection runs for the AI edit panel: each question gets a one-time token, revoked when the run ends, and the AI edits only this window's document.
  - Opening, creating, saving and closing documents are refused, and so are 41 commands that write or read files, write the settings folder, set a password or step undo / redo, whether sent through `execute`, `batch`, `hwp_action`, `macro.run` or the window's PDF export.
  - Other clients may read that document but not edit it.
  - One question's edits become one undo step; atomic batch rollback and `keepGoing` still work inside it.
  - When a run ends, requests already past the token check are still served under the same rules.
- Verified end to end over the real HTTP bridge: the 41 commands are refused on all four paths, nothing is written to files or the settings folder, and formatting, batches and rollbacks undo in one step. The test found and closed a gap in the window's PDF export job.
- The MCP specification `2026-07-28` is still the latest. Python SDK 2.3.0 52/53, TypeScript client 2.4.0 41/43, official conformance for the 2026 requirements 109 passed. rmcp stays at 3.7.0 (3.8.0 changes nothing on the server side).

## Performance

- Screen: the caret position is computed again only when the document or the cursor changes, and the status bar reads the track-change state from a cache. In the 381-page handbook (1500×916, release, median): wheel-scroll frame 5.7–6.0 → 1.2–1.5 ms, dragging 13–14 → 7.6 ms, zoom 7.4 → 1.4 ms, window resize 5.5 → 1.2 ms, CPU per frame 6.65 → 1.9–2.4 ms. Opening or closing the AI panel takes 3 frames each, 6.5 ms at most; start-up stays at 140–151 ms.
- The formatting toolbar reads the character and paragraph shapes and the style list once per frame instead of once per widget: continuous typing 0.9 → 0.55 ms, page moves 1.2 → 0.7 ms, typing while finding 0.9 → 0.5 ms (toolbar cost, median).
- An AI run costs 5–9 ms to open and about 2 ms to close per question in the 381-page handbook; edits inside it cost the same as usual.

## Fixes

- MCP: since 0.20, methods removed in `2026-07-28` (`initialize`, `logging/setLevel`, `resources/subscribe` / `unsubscribe`, `tasks/result`, `tasks/list`) sent as 2026 requests got 200 and -32602 instead of the specified 404 and -32601 (official conformance for the 2026 requirements back from 103 to 109 passed). 2025 sessions and malformed `tools/call` still get -32602.
- DOCX export: page breaks were dropped silently; they are now written as Word "page break before", and column breaks, which cannot be carried, are listed as left out (CLI error output, MCP `omitted`).
- Removing paragraph numbers or bullets left the left margin HwpCraft added when turning them on; it is now taken back as in Hangul (the paragraph's own margin stays).
- CLI: `file.save`, `file.save_as` and `file.pdf` steps in `run` and `edit` (Hangul actions such as FileSave included) save through the CLI's save rules, and `file.new` / `file.open` point to `--new`, the document path and `--password`.
- Hangul actions: 29 actions with empty default parameters did nothing. The 15 whose name fixes the meaning (table caption position 8, split cell in two 2, append row, cell resize 4) run with those defaults; the 14 that need a caller's value (caption position 8, line resize 4, page number on both sides, and `TableSubtractRow`, which behaves differently on the first row) are marked as partly supported. The meanings were checked in Hangul.
- Tables: a new table's 282 HU row grown by 1 mm could not be shrunk back (resize lower limit 283 HU).
- RHWP 0103: the header / footer cache of 0101 is keyed by section number and dropped when the section count changes.
- AI edit panel: the panel reaches the status bar like a task pane and takes at most half of a narrow window (the remembered width comes back in a wide one); while the AI edits, the whole formatting toolbar greys out and the status bar says "AI 편집 중"; ending a run always lifts its restrictions and closes its undo step, even after a failed request or a lost connection.
- The character map grew to the window height with a short title bar (0.20).
- macOS: shortcut tests expect ⌘, and a request from a foreign browser origin has its small body read before it is refused, so macOS answers 403 instead of resetting the connection (found by the 0.20 release build, also in v0.20).
- Tests no longer write the user's settings folder (recent documents, view settings); test settings folders are removed by a later test run after an hour, and screen-capture and benchmark runs save no settings.

## Verification and limits

- Built as CI does (a clean RHWP v0.8.7 with the 87 patches applied in name order): Windows x64 `cargo test --release --locked --workspace` 1,155 passed, 0 failed (17 ignored), `cargo clippy --workspace --all-targets -D warnings` clean; the same tests on Linux (WSL) 1,155 passed, 0 failed (17 ignored); macOS (arm64) cross clippy was clean during development, and the release build runs the workspace tests again on Windows, macOS and Linux in GitHub Actions.
- Local RHWP patches are kept in `patches/rhwp/` under RHWP's MIT license (87 of 0001–0103). No new third-party Rust crates in 0.21.

### Known limits

- The AI edit panel needs the Claude Code or Codex CLI installed and signed in; it does not call an API key directly. The AI cannot insert pictures from files. No context is sent while the caret is in a table inside a table or in a text box. Each CLI keeps its conversation record (including the document text sent as context) in its own folder to continue conversations. If Codex refreshes its sign-in token during a run, the new token is written back to the user's `~/.codex/auth.json` (only when the original has not changed meanwhile). An edit another MCP client sent just before a question may end up in that question's undo step.
- The AI conversation history records changes only as the text of body and table-cell paragraphs. Formatting, pictures and table structure do not appear under the changed places; once the document has changed since, the panel neither reverts nor records as tracked changes a question that changed them or a paragraph holding an object (Ctrl+Z still works). AI changes are not mixed into a document that already has tracked changes. Because a document is also recognized by a fingerprint of its body text, another document with the same text may look like the same one, and when the same paragraph text appears in more than one place, Revert cannot tell which one and changes nothing.
- In a line made tall by an object treated as a character, ↓ may skip the next line, and clicking such a paragraph may put the caret one character early (RHWP position calculation, present since 0.19, to be fixed in the next version).
- A paragraph right after a table that is not treated as a character (top-and-bottom wrap) is drawn over the table's last row by the table's outer margin (1 mm by default) once the saved document is opened again, and the paragraph after a floating table with a caption is drawn over the caption. Tables treated as characters are fine (an RHWP layout limit, present since 0.19, to be fixed in the next version).
- The AI edit panel hides on the ribbon's File tab (document info and recent documents) and comes back on the other tabs (the ✦ AI button stays on).
- The AI edit panel was checked on Windows with the real window and the real Claude Code and Codex; on macOS and Linux only its tests ran (stopping a Unix process group included), and the desktop screen was not checked by hand.
