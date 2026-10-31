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

표는 `insert.table`(결과의 `table`이 새 표 주소라 `table.*`·캡션 명령에 바로 넘길 수 있습니다), 그림·도형·수식·머리말/꼬리말·주석은 각 명령을 검색하고 준비 조건을 확인하세요. `get_text`의 문자열 일치만으로 개체·서식·조판이 보존됐다고 판단하지 않습니다. 모든 쪽을 PNG로 확인하고 저장·재열기 구조와 실제 PDF를 별도로 검증합니다.

## 보고서 작성 순서

표지·차례·장절·표·차트·각주·쪽 번호가 있는 보고서 한 편을 MCP만으로 만드는 순서입니다. `crates/mcp/tests/protocol.rs`의 `report_round_trip`이 이 순서를 그대로 실행하고 저장·재열기까지 확인합니다. 아래는 `execute`의 `command`와 `params`만 적었습니다(문서 ID는 생략).

1. **스타일 먼저**: `format.style.set.apply {"name":"보고서"}`. 스타일은 문단이 쓰는 모양이라 나중에 바꿔도 되지만, 먼저 정하면 쓰는 동안 쪽 나눔이 최종과 같습니다.
2. **표지**: `type_text`로 제목·작성자·`작성일: `을 쓰고 `insert.date_field {"kind":"created"}`, 이어서 `insert.page_break`.
3. **차례 자리**: `[[차례]]` 같은 자리 표시 글과 `insert.page_break`. 차례는 제목을 다 쓴 뒤에 넣습니다(6단계).
4. **장·절과 본문**: 제목 글을 쓰고 `style.apply {"id":2}`(개요 1), `{"id":3}`(개요 2)… 문단을 나눈 뒤 `style.apply {"id":0}`으로 본문 스타일로 돌아갑니다. 스타일 id는 `format.style.list`로 확인하세요.
5. **표·그림·차트·각주**:
   - 표: `insert.table {"rows","columns"}` → 결과의 `table` 주소로 `table.cells.set {"table":…,"row":0,"col":0,"cells":[["구분","금액"],["1분기","120"]]}`(셀 글을 한 번에, 실행 취소 한 단계, `null`은 그대로), `table.properties {"table":…,"props":{"hasCaption":true}}`, `table.caption_text {"table":…,"text":"1장 예산 집행"}`. 줄·칸 넣기·지우기는 `table.edit {"table":…,"row":1,"col":0,"action":"row_below"}`처럼 셀을 주소로 지정합니다(생략하면 캐럿 셀). 셀 하나에 서식을 넣어 쓰려면 셀 `cursor`를 `selection.set`으로 잡고 `type_text`를 씁니다.
   - 그림: `picture.insert {"path","width_mm","treat_as_char":true}` → 결과 `address`로 `object.properties {"props":{"hasCaption":true}}`.
   - 차트: `chart.create {"chart":{"kind":"column","categories":[…],"series":[…]}}`. 각주: `note.insert {"endnote":false,"paragraphs":[…]}`.
   - 개체를 넣은 뒤에는 캐럿이 개체 문단이나 셀에 있을 수 있으니 `hwp_action {"id":"MoveDocumentEnd"}`로 끝으로 옮기고 이어 씁니다.
6. **머리말·쪽 번호**: `page.header_footer.text {"header":true,"apply_to":"both","text":"…","fields":[{"offset":…,"kind":"page_number"}]}`, `page.number {"position":"bottom_center"}`.
7. **메모**: `select_text`로 문장을 잡고 `memo.create {"text","author"}`.
8. **차례·표 차례**: `select_text {"text":"[[차례]]"}` + `hwp_action {"id":"Delete"}`로 자리 표시를 지우고 그 자리에서 `tools.toc {"levels":2,"leader":"dot"}`, 이어서 `tools.toc {"kind":"tables"}`(그림은 `pictures`). 항목은 제목·캡션으로 가는 링크입니다.
9. **상호 참조**: 본문 끝에서 `type_text "자세한 내용은 표 "` → `insert.cross_reference {"target":{"table":1},"show":"number"}` → `type_text "을 참조하세요."`. 대상은 `tools.cross_reference.targets`로 고릅니다.
10. **새로 고침 → 확인 → 저장**: 고친 뒤에는 `tools.toc.update`(제목·표·그림 차례를 한 번에), `tools.cross_reference.update` 순서로 새로 고칩니다. `inspect_document`의 쪽 수만큼 `render_page`로 모든 쪽을 보고, `save_document`로 `.hwpx`·`.hwp`·`.pdf`·`.docx`를 씁니다(DOCX 결과의 `omitted`에 빠진 것이 적힙니다). 마지막 저장 날짜 필드를 쓴다면 저장한 뒤 `tools.date_field.update`를 부르고 다시 저장합니다.
11. **재열기 검증**: `open_document {"path","force":true}` 뒤 `document.outline`의 쪽이 차례 줄 끝 쪽 번호와 같은지, `tools.cross_reference.list`의 `expected`와 `text`가 같은지, `memo.list`가 남았는지 봅니다.

주의할 점:

- **캐럿 위치**: `type_text`·`insert.*`는 캐럿 자리에 들어갑니다. 표·그림·차트·차례를 넣은 뒤 캐럿이 어디 있는지 `get_selection`이나 `inspect_document`로 확인하고, 본문을 이어 쓸 때는 `MoveDocumentEnd`로 옮기세요. 차례는 표 셀 안에 넣을 수 없습니다.
- **주소 재사용**: `insert.table`의 `table`, `picture.insert`의 `address`는 그 앞에 문단을 넣거나 지우면 바뀝니다. 개체를 만든 바로 다음에 캡션·속성을 정하고, 나중에 다시 다룰 때는 `inspect_document`나 `tools.cross_reference.targets`로 새 주소를 받으세요.
- **새로 고침 순서**: 차례·상호 참조·날짜 필드의 글은 자동으로 따라오지 않습니다. 본문을 고친 뒤 차례 → 상호 참조 순서로 새로 고치세요(차례가 쪽을 밀면 참조 쪽 번호가 바뀔 수 있음). 저장은 그다음입니다.
- **알려진 한계**: 차례는 한글의 차례 필드가 아니라 차례 스타일의 일반 문단입니다. 색인은 있고(아래 색인 절), 인용·참고 문헌은 아직 없습니다. PDF에서는 차례 항목과 상호 참조가 대상 쪽으로 가는 링크입니다(쪽 일부만 내보내면 PDF에 있는 쪽만, 모아 찍기 인쇄본에는 없음).

## 차트 만들기·구성 변경

`chart.create`는 본문에서 `column`·`bar`·`line`·`pie`·`doughnut`·`scatter`·`stock_hlc`·`stock_ohlc`·`column_3d`·`bar_3d`·`pie_3d`를 지원합니다. `chart.types`에서 각 종류의 실제 제약을 먼저 조회하고, 기존 자체 차트는 `chart.definition.get {index}`의 최신 구성과 `generation`으로 편집하세요. 다음은 본문 커서가 준비된 문서의 새 차트 예제입니다. 선택이 있다면 기존 내용을 대체할 수 있으므로 삽입 대상을 먼저 확인하세요.

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

## 그림 효과

- `picture.effects` {address, shadow|glow|soft_edge|reflection: 객체=설정·null=해제·생략=유지, clear} → effects
- `picture.effects.get` {address}

단위는 mm·투명도 0–100 %·방향 0–360°·#RRGGBB입니다. 효과가 있는 문서를 .hwp로 저장하는 규칙은 `save_document`의 `dropPictureEffects`를 보세요.

## 그림 넣기·개체 캡션

- `picture.insert` {path, width_mm?, treat_as_char?}: treat_as_char를 생략하면 글자처럼 취급(0.19부터, 이전에는 떠 있는 그림). false면 캐럿 문단 기준·단 왼쪽·어울림으로 넣고 글이 옆으로 흐른다.
- `object.caption_text` {address, text} — address(그림·도형·묶음·차트·OLE), text(캡션 번호 뒤 문자열, 1,000자 이하). 캡션은 object.properties hasCaption으로 먼저 넣는다
- `object.caption_text.get` {address}
- 표 캡션은 `table.caption_text` {table, text}

캡션 글은 표 차례·그림 차례와 상호 참조(`show:"caption"`)가 그대로 씁니다. 캡션을 고친 뒤 `tools.toc.update`를 부르면 차례와 참조가 함께 따라옵니다.

## 양식 개체

- `form.insert` {kind check_box|edit|combo_box|radio_button|push_button, caption?, text?, checked?, items?[](콤보), group?(라디오), name?, width_mm?, height_mm?} → at. 캐럿 자리(본문·표 셀)에 글자처럼 넣고, 값은 form.set으로 고친다. 기본 크기·색·속성은 한글이 새로 넣은 개체의 값(표본 form-01.hwpx).
- `form.list` {} · `form.get` · `form.set` — 문서의 양식 개체 목록과 값 조회·변경

## 메모·댓글

- `memo.create` {text, author?} — 본문이나 표 셀(한 단계)의 선택 범위 또는 캐럿 자리에 메모
- `memo.reply` {id, text, author?} — 댓글(한글처럼 같은 범위에 중첩한 메모). 캐럿 자리 메모에는 달 수 없음
- `memo.list` {} → 각 메모의 id, at(본문·셀 주소), anchor, text, author, replyTo(스레드 첫 메모 id, 첫 메모는 null), thread(바깥에서 안쪽 순서 0, 1, 2…)
- `memo.update` {id, text?, author?} · `memo.delete` {id} — 첫 메모를 지우면 댓글도 함께 지우고 removedIds를 돌려줌(실행 취소 한 단계)

메모 해결 상태는 한글에 없는 기능이라 두지 않습니다. 여러 문단에 걸친 메모는 아직 만들 수 없습니다.

## 날짜 필드

- `insert.date_field` {kind: created|modified} — 문서 만든 날짜(한글 날짜 코드 `$createtime`)·마지막 저장 날짜(`$modifiedtime`) 필드. 보이는 글은 지역 시간 "2026년 10월 30일 금요일 오후 11:27:37"
- `tools.date_field.update` {} — 모든 날짜 필드 글을 지금 문서 요약 정보 시각으로(실행 취소 한 단계)

HWP/HWPX로 저장할 때 문서 만든 시각(없을 때)과 마지막 저장 시각이 문서 요약 정보에 기록됩니다. 저장한 적 없는 새 문서의 만든 날짜는 지금이고 첫 저장 때 기록되며, 마지막 저장 날짜 필드는 저장한 뒤에 넣을 수 있습니다. 저장해도 필드 글은 그대로이니 저장한 뒤 `tools.date_field.update`를 부르세요.

## 문서 비교·병합

- `review.compare` {path, ignore_whitespace?, password?} → findings[](종류 label·위치·detail(RHWP 원문, A=현재·B=파일)·text(사용자 문구, 0.19))
- `review.merge.plan` {path, …} → changes[{id, kind: changed|added|removed, section, paragraph, other_paragraph, current_text, other_text}] — 문단마다 현재 글과 비교 파일 글을 나란히
- `review.merge` {path, …, ids?} → applied, changes — 고른 차이(생략하면 전부)를 비교 파일 쪽으로 맞춤. 바뀐 문단은 파일 문단으로 바꾸고, 파일에만 있는 문단은 넣고, 현재에만 있는 문단은 지움. 서식·표·그림도 가져오며 실행 취소 한 단계

## 문서 이력

- `review.history.list` {} → file, entries[{index, date, writer, description}], lastVersion — 한글의 문서 이력 관리로 HWP 파일에 기록된 버전(읽기만, 이력은 HWP로 저장해도 남음)
- `review.history.save_last` {path(.hml)} → path, bytes — 마지막 이력 버전을 UTF-16 HML(HWPML 2.5)로 저장. 이전 버전 되돌리기·새 버전 기록은 없음

## 스타일마당

- `format.style.set.list` {} → sets[{name, builtin, description, styles(바꾸는 스타일 이름)}] — HwpCraft 기본 묶음(보고서·공문·간결) 다음에 사용자 묶음
- `format.style.set.apply` {name} — 묶음의 스타일을 이름으로 찾아 고치고 없으면 더함(`format.style.import`와 같음, 실행 취소 한 단계)
- `format.style.set.save` {name} · `format.style.set.delete` {name} — 지금 문서의 스타일 전체를 설정 폴더 `style_sets/<name>.json`에 사용자 묶음으로 저장·삭제(기본 묶음 이름은 쓸 수 없음, atomic batch 안에서는 쓸 수 없음)

기본 묶음은 글꼴을 바꾸지 않고 크기·굵기·문단 간격·줄 간격·정렬만 고칩니다. 한글의 스타일마당 파일(.sty)은 읽지 않습니다. 한글 공개 액션 `StyleTemplate`은 `format.style.set.apply`에 대응합니다.

## 차례·표 차례·그림 차례

- `tools.toc` {levels, title, leader: dot|dash|solid|none, page_numbers, numbers, styles} → entries, section, first, paragraphs — 개요 문단(또는 `styles`로 지정한 제목 스타일)의 차례
- `tools.toc` {kind: "tables"|"pictures", title, leader, page_numbers} → kind, entries, … — 캡션 달린 표·그림의 캡션 글(예: "표 1 예산 집행")과 쪽
- `tools.toc.update` {} → 문서의 제목 차례·표 차례·그림 차례를 모두 새로 고치고 `lists[{kind, entries, …}]`를 더합니다(실행 취소 한 단계). `kind`를 주면 그 차례만 고칩니다.
- 항목 글은 기본으로(`links`, 기본 true) 그 제목·표·그림 문단의 책갈피로 가는 하이퍼링크입니다(한글 저장본의 책갈피 링크 형식 `?이름;0;0;0;`). 대상 문단에 책갈피가 없으면 `hwpcraft-ref-…`를 달고, 있으면 그대로 씁니다.
- 링크를 따라가려면 캐럿을 항목 글에 두고 `hyperlink.jump` {}를 실행합니다. 문서 안 링크면 대상으로 캐럿을 옮기고 `jumped: true`와 새 `cursor`를 돌려주며, 웹 주소면 `jumped: false`와 `uri`만 돌려줍니다.

차례는 캐럿 위치에 일반 문단으로 들어갑니다. 스타일은 "차례 제목"·"차례 N", "표 차례 제목"·"표 차례", "그림 차례 제목"·"그림 차례"이며, 문서에 없으면 만듭니다. 한글의 차례 필드로는 쓰지 않습니다. 새로 고침은 이 스타일로 차례를 찾습니다. 보고서에서는 제목·표·캡션을 모두 넣은 뒤 `tools.toc`를 부르고, 편집을 마친 뒤 `tools.toc.update`를 한 번 부르면 됩니다.

## 색인(찾아보기)

- `index.mark.insert` {first?, second?} — 선택한 낱말 뒤(선택이 없으면 캐럿, 본문)에 한글과 같은 색인 표시. first를 생략하면 선택한 글이 첫째 키
- `index.mark.list` {} → marks[{section, paragraph, control, first, second, page}] · `index.mark.update` {section, paragraph, control, first?, second?} · `index.mark.delete` {section, paragraph, control}
- `tools.index` {} — 캐럿 위치(본문)에 색인: 첫소리 묶음 머리 `(ᄀ)`…, `낱말<탭>1, 3`, 둘째 키는 그 아래 `   둘째 키<탭>2`(한글 2024 색인 만들기 기본 모양, 바탕글). 결과 entries·groups·first·paragraphs

한글은 색인을 새 문서에 만들고 HwpCraft는 캐럿 위치에 넣습니다. 다시 부르면 하나 더 넣으므로, 고친 뒤에는 옛 색인 문단을 지우고 다시 만드세요. 쪽 번호는 문서의 실제 쪽 순서입니다. 공개 액션 `IndexMark`·`IndexMarkModify`·`MakeIndex`에 대응합니다.

## 상호 참조(쪽 번호·표 캡션 번호)

- `insert.cross_reference` {target: {bookmark: 이름} | {heading: 개요 제목} | {table: n} | {picture: n}, show: "page"|"number"|"caption"} → field_id, text, (page) bookmark·page, (number·caption) table
- `tools.cross_reference.update` {} → references, updated, missing, skipped
- `tools.cross_reference.list` {} → items[{field_id, show, text, section, paragraph, bookmark·page | table·expected}]
- `tools.cross_reference.targets` {} → headings[{title, level, page}], tables·pictures[{n, caption, page}], bookmarks[{name, page}]

커서 자리에 대상의 쪽 번호를 한글과 같은 상호 참조 필드(`%xrf`, 책갈피 대상·쪽 번호)로 넣습니다. 표·그림 번호는 본문 순서로 1부터 셉니다. 개요·표·그림 대상에는 그 문단에 `hwpcraft-ref-…` 책갈피를 달아 가리킵니다. 표·그림·책갈피·머리말·쪽 번호 같은 개체가 든 문단과 표 셀 안에는 넣지 않습니다. 넣을 대상과 그 번호(`n`)는 `tools.cross_reference.targets`로 먼저 확인합니다. 쪽이 바뀌는 편집 뒤에는 `tools.cross_reference.update`로 모든 참조를 지금 쪽으로 고칩니다(한 번 실행 취소). HWP·HWPX 저장 후 다시 열어도 필드와 대상이 유지됩니다. 표는 캡션 번호(`show:"number"`, 한글처럼 숫자만 보입니다)와 캡션 전체(`show:"caption"`, 예: "표 1 예산 집행")도 참조합니다. 보고서의 "표 1 참조"는 "표 "를 직접 입력한 뒤 번호 참조를 넣습니다.

```json
{"name":"type_text","arguments":{"document_id":"<doc>","text":"자세한 내용은 표 "}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"insert.cross_reference","params":{"target":{"table":1},"show":"number"}}}
{"name":"type_text","arguments":{"document_id":"<doc>","text":"을 참조하세요."}}
```

번호·캡션 참조는 표 자체를 개체 id로 가리킵니다(id가 없거나 겹치면 새로 줍니다). 캡션을 고친 뒤 `tools.cross_reference.update`를 실행하면 새 캡션으로 바뀝니다. HWP 저장본의 명령 문자열과 보이는 글은 한글 2024와 같습니다. 다만 한글이 HWPX 저장본의 이 필드를 번호·캡션 참조로 읽는지는 아직 확인하지 못했습니다. 그림·개요의 번호·제목 참조는 아직 없습니다.

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

### 문서 전체 찾기·바꾸기

`edit.find_all`과 `edit.replace_all`의 기본 범위는 본문과 1단계 표 셀입니다(이전과 같음). `include`에 `context.v2.list`의 종류 배열을 주면 그 문맥을 찾고 바꿉니다. 빈 배열 `[]`은 모든 종류(본문·모든 깊이의 표 셀·머리말·꼬리말·글상자·각주·미주)입니다. 보고서의 용어를 한 번에 바꿀 때 쓰세요.

```json
{"name":"execute","arguments":{"document_id":"<doc>","command":"edit.find_all","params":{"query":"초안","include":[]}}}
{"name":"execute","arguments":{"document_id":"<doc>","command":"edit.replace_all","params":{"query":"초안","replacement":"최종","include":[]}}}
```

- `include`가 있는 `edit.find_all`은 일치마다 버전 2 `address`·`kind`·`offset`·`length`·`readonly`를 줍니다. 본문·1단계 셀 일치에는 캐럿용 `cursor`도 붙습니다. 이 주소는 `context.v2.get`·`context.v2.replace`·`context.v2.character`에 그대로 넘길 수 있습니다.
- `include`가 있는 `edit.replace_all`은 모든 교체를 실행 취소 한 단계로 묶습니다. 보호된 셀·잠긴 개체·검증하지 않은 세로쓰기 머리말 같은 읽기 전용 문맥의 일치는 바꾸지 않고 `skippedProtected`로 셉니다. `options`(대소문자·온전한 낱말·아무개 문자·조건식·서식 찾기)는 기본 범위와 같습니다. `options`가 없으면 대소문자를 가리는 그대로 찾기입니다. 381쪽 문서에서 1,470건(글상자·각주 포함)을 바꾸는 데 약 4초 걸렸습니다.
- `include` 없는 `edit.find_all`도 이제 일치마다 `address`·`kind`를 줍니다. 글상자·중첩 셀·수식 스크립트 안의 일치에는 `cursor`를 주지 않고 `nested:true`로 표시합니다(수식은 `equationControl`). 0.18까지는 이런 일치를 본문이나 표 셀 캐럿처럼 돌려줘, 그 위치에서 선택·입력하면 엉뚱한 곳이 바뀌었습니다.
- `page.setup`은 `apply_to:"document"`로 모든 구역에 적용합니다(구역마다 주지 않은 항목은 그대로, 실행 취소 한 단계). 기본은 캐럿이 있는 구역입니다.

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

## 쪽·구역·단·바탕쪽·줄 번호

- `page.section_break` {} — 캐럿에서 새 구역(용지·바탕쪽·단 설정 유지, 쪽 번호 이어짐, 실행 취소 한 단계)
- `page.column_def_break` {count 1–6, spacing_mm?} — 캐럿 문단을 나누고 거기서부터 새 단 정의(예: 제목은 1단, 본문은 2단)
- `page.master.get` · `page.master.set` · `page.master.delete` {applyTo: both|odd|even, text?} — 현재 구역 바탕쪽을 글로 만들거나 바꾸고 지움(줄마다 한 문단, 10 KB까지). 바탕쪽에 그림·도형이 있거나 마지막 쪽·임의 쪽 바탕쪽이 있는 구역은 거절
- `page.line_numbers` {show, start?, count_by?, distance_mm?, restart?, all_sections?} · `page.line_numbers.get` {} — 줄 번호 설정(HWP/HWPX 저장). `page.line_numbers.skip` {skip} · `page.line_numbers.skip.get` {} — 선택 문단(없으면 캐럿 문단)의 줄 번호 건너뜀(HWPX만 저장, HWP로 저장하면 사라짐). 화면·PDF의 본문 줄에 번호를 그리며(표·글상자·각주·머리말 안 줄은 세지 않음) PDF 글자 추출에도 번호가 나옵니다. 꺼져 있던 줄 번호를 distance_mm 없이 켜면 저장된 거리가 0일 때 5 mm로 둡니다. restart 0–3의 뜻은 한글 자동화 표 기준이며 한글 파일로는 확인하지 못했습니다
- `page.label` {rows, columns, labelWidthMm, labelHeightMm, gapXMm?, topMm?, leftMm?, force?} — 새 A4 라벨 문서(테두리 없는 표 한 개, 셀마다 입력)

## 덧말·첫 글자 장식·그림 글머리표

- `format.ruby` {text, position?: top|bottom, align?, size?} — 본문 한 문단 안 선택 글자에 덧말. `format.ruby.list` {} → 캐럿 문단의 덧말(control 번호), `format.ruby.update` {control, …바꿀 것}, `format.ruby.delete` {control}(기준 글은 본문 글자로 되돌림)
- `format.drop_cap` {style: double|triple|margin|none, font?, gap_mm?} — 캐럿 문단 첫 글자 장식(첫 글자를 테두리 없는 글상자로 옮김, none은 되돌림)
- `format.bullet.picture` {path, level?} — PNG·JPG·BMP·GIF(1 MB·512픽셀 이하)를 캐럿·선택 문단의 그림 글머리표로

## 개체 옮기기·표 경계선

- `object.move_by` {addresses[1–64], dx_mm, dy_mm} — 떠 있는 개체 여러 개를 같은 만큼 옮김(붙은 연결선 다시 계산, 한 단계). 글자처럼 취급한 개체·보호 개체·표는 거절
- `object.move_inline` {address, to: {section, paragraph, offset}} — 글자처럼 취급한 본문 개체(그림·도형·글상자·수식·표)를 본문 캐럿 자리 앞으로. 결과는 새 주소
- `context.table.move_border` {address, control, vertical, boundary, delta_mm, range?} — 셀 안 중첩 표의 경계선 옮기기(본문 표는 `table.move_border`)
- `caret.move_cell` {backward} — 표 안에서 다음(Tab)·이전(Shift+Tab) 셀로

## 사용자 표 마당

- `table.user_style.save` {name, table?, force?} — 표의 테두리·채우기·대각선을 셀 역할(첫 줄·홀짝 본문·마지막 줄/칸)별로 설정 폴더 `table_styles/`에 저장(글자 모양은 저장하지 않음)
- `table.user_style.apply` {name, table?} · `table.user_style.list` {} · `table.user_style.delete` {name} — 다른 크기의 표에도 역할별로 적용(한 단계)

## 키 매크로

- `macro.record.start` {} → 편집을 기록(입력·명령·캐럿·셀 이동, 마우스로 옮긴 캐럿은 기록 안 함), `macro.record.status` {}, `macro.record.stop` {name?} → 단계 목록(이름을 주면 저장)
- `macro.save` {name, steps[{command, params}]} · `macro.list` {} · `macro.get` {name} · `macro.delete` {name} — 설정 폴더에 64개까지
- `macro.run` {name 또는 steps, repeat?} — 단계마다 실행 취소 한 단계, 하나라도 실패하면 실행 전체를 되돌림

## 수식 시각 편집·수식 매크로

- `equation.layout` {script, size_pt} → 그려지는 조각마다 상자와 그 조각을 만든 스크립트 범위(start·end, 맞추지 못하면 null), 빈 `{}` 칸(slots). 미리 보기 위에서 커서를 놓는 데 씁니다
- `equation.macro.list` · `equation.macro.get` · `equation.macro.save` · `equation.macro.delete` {name?, script?} — 설정 폴더 `equation_macros.json`(이름 1–40자, 200개까지). 키 매크로와 따로 저장

## 문서마당·변경 추적 문서

- `file.template.save` {name} — 지금 문서를 사용자 서식(설정 폴더 `templates/`, HWPX·암호 문서는 HWP)으로 저장. `file.template.list` {} · `file.template.new` {name, force?}(사본을 새 문서로 엶) · `file.template.delete` {name}
- `document.track_changes` {} → tracked, authors·changes·ranges(HWP), marks(HWPX). 변경 추적 기록이 있으면 HWP/HWPX 저장은 기본 거절이고 `save_document`의 `acceptTrackChanges:true`로만(결과본으로 합침) 저장합니다

## 공개 한글 동작과 ParameterSet

```json
{"name":"hwp_action","arguments":{"document_id":"<doc>","id":"CharShape","parameterSet":{"Height":1800,"Bold":1,"TextColor":5649426}}}
{"name":"hwp_action","arguments":{"document_id":"<doc>","id":"InsertText","parameterSet":{"Text":"한글 자동화"}}}
{"name":"hwp_action","arguments":{"document_id":"<doc>","id":"PageSetup","parameterSet":{"ApplyTo":3,"PageDef":{"TopMargin":5668,"BottomMargin":5668,"LeftMargin":8504,"RightMargin":8504}}}}
{"name":"hwp_action","arguments":{"document_id":"<doc>","id":"AllReplace","parameterSet":{"FindString":"초안","ReplaceString":"최종","Direction":2,"MatchCase":1,"IgnoreMessage":1}}}
```

`parameterSet`을 받는 동작은 `CharShape`·`ParagraphShape`·`TableCreate`·`InsertText`·`PageSetup`·`PageMarginSetup`과 찾기·바꾸기(`AllReplace`·`ExecReplace`·`RepeatFind`·`FindDlg`·`FindAll`)입니다. 필드 이름·단위·값은 한컴 공개 ParameterSet 표를 따릅니다. `PageSetup`의 `PageDef` 길이는 HWPUNIT(1 mm ≈ 283.465)이고, `Landscape`는 0 좁게·1 넓게, `GutterType`은 0 한쪽·1 맞쪽·2 위로 넘기기, `ApplyTo`는 0·2 현재 구역·3 문서 전체입니다(1 선택된 문자열·4 새 구역은 거절). 찾기·바꾸기의 `Direction`은 0 아래쪽·1 위쪽·2 문서 전체이고, `MatchCase`·`WholeWordOnly`·`UseWildCards`·`SeveralWords`·`FindRegExp`·`HanjaFromHangul`과 `FindCharShape`의 `Bold`·`Italic`·`UnderlineType`·`Height`(100 = 1 pt 단위)를 씁니다. `IgnoreMessage`·`ReplaceMode`·`FindType`은 받기만 합니다(HwpCraft에는 메시지 창이 없습니다). `AllWordForms`·`AutoSpell`·`FindJaso`·`FindStyle`·`ReplaceCharShape` 같은 아직 없는 기능은 꺼진 값(0, 빈 값)일 때만 받습니다. 그래서 GetDefault로 채운 묶음을 그대로 보내도 됩니다. 이 예제도 기존 글자를 고치려면 먼저 선택합니다. 알려진 공개 항목만 변환하며 미지원 항목을 무시해 성공으로 반환하지 않습니다. `parameterSet` 지원 범위는 `hwp_action` 목록 항목의 `parameterSet` 설명을 읽으세요. 일부 대응은 `complete:false`입니다. 직접 `params`는 대응표 기본값 위에 적용되며 이름만으로 한컴 COM 동작 전체를 호출하는 기능은 아닙니다.

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
