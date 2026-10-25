# MCP 명령 인자 참고

`scripts/mcp-command-docs.py <hwpcraft-cli.exe>`로 실제 MCP discovery에서 생성합니다.
인자 예제는 형식 안내입니다. 현재 선택·기존 개체·파일 등 실행 전제 조건을 먼저 준비하세요.
`execute`에 `command`와 아래 객체를 `params`로 전달합니다. 추가 선택 인자는 설명을 참고하세요.
이 문서는 모든 세부 서식 필드의 JSON Schema를 대체하지 않습니다. `*.get` 조회와 엔진 검증을 함께 사용하세요.

명령 수: **285**

## `document.text` — 본문·표 셀 텍스트 조회

Paginated body and first-level cells; follow nextStart. start >= 0, limit 1–1000.

```json
{
  "limit": 100,
  "start": 0
}
```

## `document.inspect` — 문서 구조 조회

Paginated structure with style/shape references and selection; start >= 0, limit 1–1000.

```json
{
  "limit": 100,
  "start": 0
}
```

## `select.text` — 문자열 선택 (n번째 일치)

Case-sensitive search within a paragraph. occurrence starts at 1.

```json
{
  "occurrence": 1,
  "text": "한글"
}
```

## `format.character.get` — 글자 모양 조회

No parameters; properties at the caret (or selection start).

```json
{}
```

## `format.paragraph.get` — 문단 모양 조회

No parameters; paragraph properties at the caret.

```json
{}
```

## `format.paragraph.apply` — 문단 모양·머리 모양 적용

format?: paragraph shape patch (format.paragraph keys), head?: {kind: none|outline|number|bullet, level?: 0–6}; one undo step.

```json
{
  "format": {
    "alignment": "center"
  },
  "head": {
    "kind": "outline",
    "level": 0
  }
}
```

## `format.outline.shape` — 개요 번호 모양 설정

levelFormats, numberFormats, startNumber?: the section's 개요 번호 모양.

```json
{
  "levelFormats": [
    "^1.",
    "^2.",
    "^3)",
    "^4)",
    "(^5)",
    "(^6)",
    "^7"
  ],
  "numberFormats": [
    0,
    8,
    0,
    8,
    0,
    8,
    1
  ]
}
```

## `insert.equation` — 수식 넣기

Hancom equation script, size_pt 4–100.

```json
{
  "script": "a over b",
  "size_pt": 12
}
```

## `equation.set` — 수식 고치기

ObjectAddress and options:{script,size_pt}.

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "options": {
    "script": "x^2",
    "size_pt": 12
  }
}
```

## `insert.footnote` — 각주 넣기

text of the new footnote at the caret.

```json
{
  "text": "각주 내용"
}
```

## `insert.endnote` — 미주 넣기

text of the new endnote at the caret.

```json
{
  "text": "미주 내용"
}
```

## `note.set` — 각주·미주 텍스트 고치기

ObjectAddress and replacement text.

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "text": "수정한 내용"
}
```

## `insert.shape` — 도형·글상자 넣기

kind rectangle/ellipse/line/textbox/arc/polygon/curve, dimensions mm, text required.

```json
{
  "height_mm": 20,
  "kind": "rectangle",
  "text": "",
  "width_mm": 40
}
```

## `shape.resize` — 도형·글상자 크기

ObjectAddress and dimensions in mm.

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "height_mm": 20,
  "width_mm": 40
}
```

## `textbox.set` — 글상자 텍스트 고치기

ObjectAddress and replacement text.

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "text": "수정한 내용"
}
```

## `object.list` — 개체 목록

No parameters.

```json
{}
```

## `object.delete` — 개체 지우기

ObjectAddress from object.list, passed directly (no address wrapper).

```json
{
  "control": 0,
  "paragraph": 0,
  "section": 0
}
```

## `style.list` — 스타일 목록

No parameters.

```json
{}
```

## `style.apply` — 기존 스타일 적용

Existing numeric style ID from style.list.

```json
{
  "id": 0
}
```

## `file.new` — 새 문서

Blank document; force:true explicitly discards unsaved changes.

```json
{
  "force": false
}
```

## `file.open` — 열기

HWP/HWPX/HWP3/HML input path; force:true explicitly discards unsaved changes.

```json
{
  "force": false,
  "path": "input.hwpx"
}
```

## `file.save` — 저장

HWP/HWPX save or PDF/TXT export. Only file.save permits omitting path.

```json
{
  "path": "output.hwpx"
}
```

## `file.save_as` — 다른 이름으로 저장

HWP/HWPX save or PDF/TXT export. Only file.save permits omitting path.

```json
{
  "path": "output.hwpx"
}
```

## `file.pdf` — PDF 내보내기

Export to a .pdf path.

```json
{
  "path": "output.pdf"
}
```

## `edit.undo` — 실행 취소

No parameters.

```json
{}
```

## `edit.redo` — 다시 실행

No parameters.

```json
{}
```

## `edit.find` — 찾기 / 바꾸기

query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.

```json
{
  "options": {
    "case_sensitive": true,
    "direction": "all"
  },
  "query": "기존",
  "replacement": "변경"
}
```

## `edit.replace` — 바꾸기 (선택한 찾은 내용)

query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.

```json
{
  "options": {
    "case_sensitive": true,
    "direction": "all"
  },
  "query": "기존",
  "replacement": "변경"
}
```

## `edit.replace_all` — 모두 바꾸기

query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.

```json
{
  "options": {
    "case_sensitive": true,
    "direction": "all"
  },
  "query": "기존",
  "replacement": "변경"
}
```

## `edit.sort` — 정렬 (선택한 문단)

key text/number, descending and case_sensitive booleans. Selected body paragraphs or whole section.

```json
{
  "case_sensitive": false,
  "descending": false,
  "key": "text"
}
```

## `edit.delete_ctrl_marks` — 조판 부호 지우기

kinds array: table,picture,shape,equation,footnote,endnote,bookmark,hyperlink,hidden_comment,field.

```json
{
  "kinds": [
    "bookmark"
  ]
}
```

## `page.setup` — 편집 용지

Partial page setup; dimensions/margins in mm, landscape bool.

```json
{
  "height_mm": 297,
  "landscape": false,
  "width_mm": 210
}
```

## `insert.page_break` — 쪽 나누기

No parameters; body only.

```json
{}
```

## `format.character` — 글자 모양

Partial character properties. fontSize is 1/100 pt, colors #RRGGBB. Use format.character.get to inspect.

```json
{
  "bold": true,
  "fontSize": 1200
}
```

## `format.paragraph` — 문단 모양

Partial paragraph properties. Use format.paragraph.get to inspect.

```json
{
  "alignment": "center",
  "lineSpacing": 160
}
```

## `insert.table` — 표 만들기

rows 1–30, columns 1–12 at the caret (body only).

```json
{
  "columns": 3,
  "rows": 2
}
```

## `text.insert` — 텍스트 입력

text string; newlines split paragraphs. Maximum 1 MB per call.

```json
{
  "text": "안녕하세요"
}
```

## `text.backspace` — 앞 글자 지우기

No parameters.

```json
{}
```

## `text.delete` — 뒤 글자 지우기

No parameters.

```json
{}
```

## `paragraph.split` — 문단 나누기

No parameters.

```json
{}
```

## `selection.set` — 문단 / 표 셀 선택

Zero-based Unicode scalar offset. Optional cell:{control,index,paragraph}; anchor is a scalar offset in the same paragraph.

```json
{
  "anchor": 0,
  "cursor": {
    "cell": null,
    "offset": 2,
    "paragraph": 0,
    "section": 0
  }
}
```

## `table.edit` — 표 줄·칸 편집 / 본문 폭에 맞춤

action: row_above,row_below,column_left,column_right,delete_row,delete_column,fit_page,merge_right,split_merged. Select a cell first.

```json
{
  "action": "row_below"
}
```

## `insert.hyperlink` — 하이퍼링크

HTTP/HTTPS uri and optional displayed text.

```json
{
  "text": "참고",
  "uri": "https://example.com"
}
```

## `insert.picture` — PNG 그림 삽입

Image path, subject to read root.

```json
{
  "path": "picture.png"
}
```

## `bookmark.add` — 책갈피 추가

Bookmark name at the caret.

```json
{
  "name": "요약"
}
```

## `bookmark.list` — 책갈피 목록

No parameters.

```json
{}
```

## `bookmark.goto` — 책갈피로 이동

Bookmark name to move to.

```json
{
  "name": "요약"
}
```

## `bookmark.delete` — 책갈피 삭제

Bookmark name to remove.

```json
{
  "name": "요약"
}
```

## `page.columns` — 다단 설정

Column count and spacing in mm for the caret section.

```json
{
  "count": 2,
  "spacing_mm": 8
}
```

## `insert.column_break` — 단 나누기

No parameters; body only.

```json
{}
```

## `page.header_footer` — 머리말 / 꼬리말 텍스트

Header or footer plain text.

```json
{
  "is_header": true,
  "text": "보고서"
}
```

## `insert.page_number` — 꼬리말에 가운데 쪽 번호

No parameters; centred page number in a new footer of the caret section.

```json
{}
```

## `hyperlink.list` — 현재 문단의 하이퍼링크 목록

No parameters; links of the caret paragraph with fieldId/start/end/text/uri.

```json
{}
```

## `hyperlink.update` — 하이퍼링크 주소·표시 문자열 고치기

field_id from hyperlink.list; new uri (http/https) and/or one-line text. Equal values are a no-op.

```json
{
  "field_id": 1,
  "text": "새 표시 글",
  "uri": "https://example.org"
}
```

## `hyperlink.remove` — 하이퍼링크 지우기 (글자 유지)

field_id from hyperlink.list; restore_format puts back the colour/underline the link covered.

```json
{
  "field_id": 1,
  "restore_format": true
}
```

## `edit.paste_html` — HTML 서식 붙이기

HTML fragment (max 4 MB) inserted at the caret with its formatting; the selection is replaced.

```json
{
  "html": "<p><b>굵게</b> 보통</p>"
}
```

## `file.password` — 문서 암호 설정·해제 (저장 시 적용)

password string sets the password used by later HWP/HWPX saves; null clears it. Never logged.

```json
{
  "password": "비밀번호"
}
```

## `insert.symbol` — 문자표: 문자 넣기 (글꼴 지정 가능)

문자표: text (1–1000 characters, no line breaks) inserted at the caret; optional font applied to them only.

```json
{
  "font": "Segoe UI Symbol",
  "text": "★"
}
```

## `symbol.sets` — 문자표 영역 목록 (HNC·KS·유니코드·이모지·특수 문자)

No parameters.

```json
{}
```

## `symbol.list` — 문자표 영역의 문자 목록

set id from symbol.sets (hnc.*, ks.*, u.<hex>, emoji.*, special); start offset, limit 1–4096 (default 256).

```json
{
  "limit": 64,
  "set": "hnc.circled",
  "start": 0
}
```

## `symbol.search` — 문자표 검색 (코드·이름·키워드)

query: U+XXXX / hex / #decimal code point, a typed character, a special-character name, or set keywords (Korean or English).

```json
{
  "query": "화살표"
}
```

## `symbol.special` — 이름 있는 특수 문자 목록

No parameters.

```json
{}
```

## `caret.move` — 캐럿 이동 (글자·낱말·줄·문단·쪽·리스트·문서, 선택 확장)

motion: left|right|word_left|word_right|word_start|word_end|line_start|line_end|up|down|page_up|page_down|page_start|page_end|paragraph_start|paragraph_end|prev_paragraph_start|next_paragraph_start|list_start|list_end|document_start|document_end; extend keeps the anchor (Shift). Covers Hangul Move*/MoveSel*.

```json
{
  "extend": false,
  "motion": "word_right"
}
```

## `select.all` — 모두 선택

No parameters; whole cell or the caret section's body.

```json
{}
```

## `select.word` — 낱말 선택

No parameters; the word (or run of spaces/punctuation) at the caret.

```json
{}
```

## `select.paragraph` — 문단 선택

No parameters.

```json
{}
```

## `select.line` — 줄 선택

No parameters; the laid-out line under the caret.

```json
{}
```

## `select.clear` — 선택 해제

No parameters.

```json
{}
```

## `select.get` — 선택 영역 조회

No parameters; cursor, anchor, spanAnchor, hasSelection, text.

```json
{}
```

## `edit.copy` — 복사하기

No parameters; copies the selection to the internal clipboard (formats, tables, objects) and returns its text.

```json
{}
```

## `edit.cut` — 오려 두기

No parameters; copy plus one undoable deletion.

```json
{}
```

## `edit.paste` — 붙이기 (내부 클립보드·글·HTML)

Without arguments pastes the internal clipboard. text pastes plain text (or, when it equals the last copy, the rich internal copy); html pastes with formatting; mode text_only forces plain text.

```json
{
  "text": "붙일 글"
}
```

## `edit.clipboard.get` — 내부 클립보드 조회

No parameters; text, hasControl (table/picture/shape), internal.

```json
{}
```

## `text.delete_word` — 한 단어 지우기 (캐럿부터 낱말 끝까지)

No parameters; deletes from the caret to the next word start (Hangul Ctrl+T).

```json
{}
```

## `text.delete_word_back` — 낱말 앞 지우기

No parameters; deletes back to the word start (Ctrl+Backspace).

```json
{}
```

## `text.delete_line` — 한 줄 지우기

No parameters; deletes the whole caret line, the paragraph when it is one line (Ctrl+Y).

```json
{}
```

## `text.delete_to_line_end` — 이후 지우기 (줄 끝까지)

No parameters; deletes from the caret to the end of its line (Alt+Y).

```json
{}
```

## `insert.line_break` — 줄 나눔 (문단 안)

No parameters; a line break inside the paragraph (saved as HWPX lineBreak).

```json
{}
```

## `insert.tab` — 탭

No parameters.

```json
{}
```

## `insert.nbsp` — 묶음 빈 칸

No parameters; U+00A0 묶음 빈 칸.

```json
{}
```

## `insert.fixed_width_space` — 고정폭 빈 칸

No parameters; U+2007 고정폭 빈 칸.

```json
{}
```

## `insert.soft_hyphen` — 하이픈

No parameters; U+00AD.

```json
{}
```

## `insert.auto_text` — 상용구: 날짜·시각·쪽 번호·전체 쪽수·파일 이름·사용자 이름

kind: date|time|page_number|total_pages|page_of_total|file_name|user_name. date/time accept an optional strftime format (default 2026년 10월 25일 토요일 / 오후 3:04). Page fields are live auto-number controls in body text.

```json
{
  "format": "%Y-%m-%d",
  "kind": "date"
}
```

## `edit.overwrite` — 삽입/수정 모드

Optional value boolean; omit to toggle. Returns overwrite.

```json
{
  "value": true
}
```

## `edit.goto` — 이동 (쪽·구역·문단·책갈피)

Exactly one of page (1-based), section (0-based), paragraph (0-based body paragraph of the caret section), bookmark (name).

```json
{
  "page": 2
}
```

## `hyperlink.jump` — 캐럿의 하이퍼링크 주소

No parameters; uri/text/fieldId of the link under the caret, for the caller to open.

```json
{}
```

## `hyperlink.next` — 하이퍼링크 다음

No parameters; selects the next link after the caret (no wrap). found false when none.

```json
{}
```

## `hyperlink.prev` — 하이퍼링크 이전

No parameters; selects the previous link before the caret.

```json
{}
```

## `edit.find_all` — 모두 찾기 (일치 위치 목록)

query (one line), case_sensitive (default false), include_cells (default true), limit 1–10000 (default 1000). Returns matches with cursor/length; nested cells carry cellPath only.

```json
{
  "case_sensitive": false,
  "query": "한글"
}
```

## `document.statistics` — 문서 통계 (글자·낱말·줄·문단·쪽·개체 수)

No parameters; characters, charactersNoSpaces, words, lines, paragraphs, pages, sections, tables, pictures, shapes, equations, footnotes, endnotes, memos.

```json
{}
```

## `document.outline` — 개요 탐색 목록

No parameters; outline paragraphs with level, number, title, page, section, paragraph.

```json
{}
```

## `format.bold` — 진하게

value?: bool; omitted toggles the shape under the caret. Selection or whole caret paragraph.

```json
{
  "value": true
}
```

## `format.italic` — 기울임

value?: bool; omitted toggles.

```json
{}
```

## `format.underline` — 밑줄

value?: bool; omitted toggles (underlineType Bottom/None).

```json
{}
```

## `format.strikethrough` — 취소선

value?: bool; omitted toggles.

```json
{}
```

## `format.superscript` — 위 첨자

value?: bool; omitted toggles. Clears subscript.

```json
{}
```

## `format.subscript` — 아래 첨자

value?: bool; omitted toggles. Clears superscript.

```json
{}
```

## `format.normal` — 보통 모양

No parameters; clears bold, italic, underline, strikethrough, scripts, outline, shadow, emboss, engrave.

```json
{}
```

## `format.highlight` — 형광펜

color?: #rrggbb or name?: black|red|blue|green|yellow|violet|bluish|white (Hangul presets); neither removes the 형광펜. Selection or whole caret paragraph.

```json
{
  "color": "#ffff00"
}
```

## `format.highlight.get` — 형광펜 범위 조회

No parameters; highlight under the caret and the caret paragraph's ranges.

```json
{}
```

## `format.font_size.increase` — 글자 크기 크게

No parameters; +1 pt (100 units) like Hangul, up to 4096 pt.

```json
{}
```

## `format.font_size.decrease` — 글자 크기 작게

No parameters; −1 pt, down to 1 pt.

```json
{}
```

## `format.ratio.increase` — 장평 넓게

No parameters; 장평 +1 % for all languages, up to 200 %.

```json
{}
```

## `format.ratio.decrease` — 장평 좁게

No parameters; 장평 −1 %, down to 50 %.

```json
{}
```

## `format.spacing.increase` — 자간 넓게

No parameters; 자간 +1 %, up to 50 %.

```json
{}
```

## `format.spacing.decrease` — 자간 좁게

No parameters; 자간 −1 %, down to −50 %.

```json
{}
```

## `format.align.justify` — 양쪽 정렬

No parameters; selected paragraphs.

```json
{}
```

## `format.align.left` — 왼쪽 정렬

No parameters.

```json
{}
```

## `format.align.center` — 가운데 정렬

No parameters.

```json
{}
```

## `format.align.right` — 오른쪽 정렬

No parameters.

```json
{}
```

## `format.align.distribute` — 배분 정렬

No parameters; 배분 정렬.

```json
{}
```

## `format.align.split` — 나눔 정렬

No parameters; 나눔 정렬.

```json
{}
```

## `format.line_spacing.increase` — 줄 간격 넓게

No parameters; +10 % (percent mode) or +1 pt, like Hangul.

```json
{}
```

## `format.line_spacing.decrease` — 줄 간격 좁게

No parameters; −10 % or −1 pt, down to 0.

```json
{}
```

## `format.indent.positive` — 첫 줄 들여쓰기

No parameters; 첫 줄 들여쓰기 +1 pt (200 raw units).

```json
{}
```

## `format.indent.negative` — 첫 줄 내어쓰기

No parameters; 첫 줄 −1 pt (negative = 내어쓰기).

```json
{}
```

## `format.margin_left.increase` — 왼쪽 여백 늘리기

No parameters; 왼쪽 여백 +1 pt; margins may go negative like Hangul.

```json
{}
```

## `format.margin_left.decrease` — 왼쪽 여백 줄이기

No parameters; 왼쩍 여백 −1 pt.

```json
{}
```

## `format.margin_right.increase` — 오른쪽 여백 늘리기

No parameters; 오른쪽 여백 +1 pt.

```json
{}
```

## `format.margin_right.decrease` — 오른쪽 여백 줄이기

No parameters; 오른쪽 여백 −1 pt.

```json
{}
```

## `format.margin.increase` — 양쪽 여백 늘리기

No parameters; both margins +1 pt.

```json
{}
```

## `format.margin.decrease` — 양쪽 여백 줄이기

No parameters; both margins −1 pt.

```json
{}
```

## `format.style.list` — 스타일 목록

No parameters; id, name, englishName, type, nextStyleId, shortcut, current.

```json
{}
```

## `format.style.detail` — 스타일 정보

id: style index; returns its character and paragraph shapes.

```json
{
  "id": 0
}
```

## `format.style.create` — 스타일 추가하기

name, englishName?, type?: paragraph|character, nextStyleId?, character?/paragraph?: shape patches; starts from the caret's shapes.

```json
{
  "character": {
    "bold": true
  },
  "name": "강조",
  "type": "paragraph"
}
```

## `format.style.update` — 스타일 편집하기

id, name?, englishName?, nextStyleId?, character?/paragraph?: patches; paragraphs using the style follow.

```json
{
  "character": {
    "italic": true
  },
  "id": 1
}
```

## `format.style.delete` — 스타일 지우기

id (not 0), replacement?: style for its paragraphs (default 바탕글).

```json
{
  "id": 3,
  "replacement": 0
}
```

## `format.style.shortcut` — 스타일 단축키 적용

number: 1–9 and 0 or 10 (Ctrl+1 … Ctrl+0), 11–20 for StyleShortcut11–20; applies that style.

```json
{
  "number": 2
}
```

## `format.numbering.list` — 문단 번호 / 글머리표 모양 목록

No parameters; numberings, bullets, the section's outline id and preceding lists.

```json
{}
```

## `format.numbering.create` — 문단 번호 모양 만들기

levelFormats: 7 strings (^1. …), numberFormats: 7 codes 0–16, startNumber?.

```json
{
  "levelFormats": [
    "^1.",
    "^2)",
    "(^3)",
    "(^4)",
    "^5",
    "^6)",
    "^7)"
  ],
  "numberFormats": [
    0,
    8,
    0,
    8,
    1,
    10,
    5
  ]
}
```

## `format.numbering.apply` — 문단 번호 / 글머리표 / 개요 적용

kind: number|bullet|outline|none, id?, char? (bullet), level?: 0–6, levelFormats?/numberFormats?/startNumber? (inline numbering), restart?: {mode, start?}.

```json
{
  "kind": "number",
  "level": 0
}
```

## `format.numbering.toggle` — 문단 번호 적용 / 해제

No parameters; 문단 번호 on/off by the caret paragraph.

```json
{}
```

## `format.bullet.toggle` — 글머리표 적용 / 해제

No parameters; 글머리표 on/off.

```json
{}
```

## `format.outline.toggle` — 개요 적용 / 해제

No parameters; 개요 on/off.

```json
{}
```

## `format.level.increase` — 한 수준 증가

No parameters; one level deeper for numbered paragraphs.

```json
{}
```

## `format.level.decrease` — 한 수준 감소

No parameters; one level up.

```json
{}
```

## `format.numbering.restart` — 새 번호로 시작

mode: continue|previous|new, start?: 1–65535 with new.

```json
{
  "mode": "new",
  "start": 1
}
```

## `format.copy_shape.apply` — 모양 복사 적용 (글자 + 문단)

character?/paragraph?: copied shape patches applied in one undo step (모양 복사 Alt+C).

```json
{
  "character": {
    "bold": true
  },
  "paragraph": {
    "alignment": "center"
  }
}
```

## `format.hf.character` — 머리말 / 꼬리말 글자 모양

header: bool, applyTo?: both|odd|even, paragraph?: index, start?/end?: offsets, format: character patch.

```json
{
  "format": {
    "bold": true
  },
  "header": true
}
```

## `format.hf.character.get` — 머리말 / 꼬리말 글자 모양 조회

header, applyTo?, paragraph?, offset?.

```json
{
  "header": true
}
```

## `format.hf.paragraph` — 머리말 / 꼬리말 문단 모양

header, applyTo?, paragraph?, format: paragraph patch.

```json
{
  "format": {
    "alignment": "right"
  },
  "header": false
}
```

## `format.hf.paragraph.get` — 머리말 / 꼬리말 문단 모양 조회

header, applyTo?, paragraph?.

```json
{
  "header": false
}
```

## `format.footnote.paragraph` — 각주 / 미주 문단 모양

paragraph?: host paragraph (default caret), control: index of the footnote/endnote, footnoteParagraph?, format: paragraph patch.

```json
{
  "control": 0,
  "format": {
    "alignment": "justify"
  }
}
```

## `format.footnote.paragraph.get` — 각주 / 미주 문단 모양 조회

paragraph?, control, footnoteParagraph?.

```json
{
  "control": 0
}
```

## `format.superscript.cycle` — 위 / 아래 첨자 순환 (보통 → 위 → 아래)

No parameters; 보통 → 위 첨자 → 아래 첨자 → 보통 by the shape under the caret (Hangul CharShapeSuperSubscript).

```json
{}
```

## `format.indent.at_caret` — 현재 위치로 내어쓰기

No parameters; following lines start at the caret's x (negative indent), Hangul ParaShapeIndentAtCaret.

```json
{}
```

## `format.font.next` — 다음 글꼴

fonts?: [names] in Hangul's list order (the UI passes installed fonts); omitted cycles the document's fonts. All seven languages take the font.

```json
{
  "fonts": [
    "함초롬바탕",
    "함초롬돋움"
  ]
}
```

## `format.font.prev` — 이전 글꼴

Same as format.font.next, backwards.

```json
{}
```

## `format.text_color` — 글자 색 (사전 지정 색 포함)

color: #rrggbb or name: black|red|blue|green|yellow|violet|bluish|white (Hangul TextColor* presets).

```json
{
  "name": "red"
}
```

## `format.shade_color` — 글자 음영 색

color?: #rrggbb or name? (same presets); neither removes the 음영 (white).

```json
{
  "name": "yellow"
}
```

## `format.char_border` — 글자 테두리

value?: bool, type?: 0–17 (HWP line type), width?: 0–15, color?: #rrggbb; all four sides. No parameters toggles a thin solid border.

```json
{
  "color": "#000000",
  "type": 1,
  "width": 0
}
```

## `format.paragraph.up` — 문단 위로 이동

No parameters; swaps the caret paragraph with the previous one in its container (body or cell). 구역 정의 stays first.

```json
{}
```

## `format.paragraph.down` — 문단 아래로 이동

No parameters; swaps with the next paragraph.

```json
{}
```

## `format.style.clear_char` — 글자 스타일 해제

No parameters; the selection (or caret paragraph) returns to its paragraph style's character shape (StyleClearCharStyle).

```json
{}
```

## `format.style.update_from_caret` — 현재 모양으로 스타일 바꾸기

id?: style (default: the caret paragraph's); it takes the caret's character shape and paragraph shape (ChangeToCurrentStyle).

```json
{
  "id": 1
}
```

## `format.style.move` — 스타일 목록 위 / 아래로 이동

id, direction: up|down; swaps list positions, 바탕글 stays first (StyleWSListUp/Down).

```json
{
  "direction": "up",
  "id": 2
}
```

## `format.style.export` — 스타일 내보내기 (JSON)

path?: write JSON there; otherwise returns {format, version, styles:[{name, englishName, type, nextStyle, character, paragraph}]}.

```json
{
  "path": "styles.json"
}
```

## `format.style.import` — 스타일 가져오기 (JSON)

path? or styles?: exported JSON; styles are matched by name (update) or added, nextStyle by name. One undo step.

```json
{
  "styles": [
    {
      "character": {
        "bold": true
      },
      "name": "강조"
    }
  ]
}
```

## `format.char_overlap` — 글자 겹치기 넣기

chars: 1–9 characters drawn on top of each other at the caret (Hangul 글자 겹치기, HWP tcps); border?: 0 none|1 circle|2 reversed circle|3 square|4 reversed square|5 triangle|6 reversed triangle; size?: inner size % (1–127, default 100); expansion?: 1 overlap (default) | 0 spread. Body paragraphs only.

```json
{
  "border": 1,
  "chars": "주의"
}
```

## `format.char_overlap.get` — 글자 겹치기 조회

No parameters; the caret paragraph's 글자 겹치기 controls (control index, position, chars, border, size, expansion).

```json
{}
```

## `format.char_overlap.update` — 글자 겹치기 고치기

control: index from format.char_overlap.get; chars?/border?/size?/expansion? to change.

```json
{
  "border": 3,
  "control": 0
}
```

## `format.char_overlap.delete` — 글자 겹치기 지우기

control: index of the 글자 겹치기 control in the caret paragraph.

```json
{
  "control": 0
}
```

## `table.create` — 표 만들기

rows·columns(1–200줄·1–64칸·4,096셀), treat_as_char(글자처럼 취급), width_mm(없으면 단 폭), height_mm(없으면 자동), column_widths_mm[]

```json
{
  "columns": 4,
  "rows": 3,
  "width_mm": 120.0
}
```

## `table.info` — 표 정보

table(개체 주소, 없으면 커서 표), range(셀 블록) → 줄·칸 수, 칸 너비, 셀 목록

```json
{
  "range": {
    "end_col": 1,
    "end_row": 1,
    "start_col": 0,
    "start_row": 0
  }
}
```

## `table.properties.get` — 표 속성 조회

table(없으면 커서 표) → 표 속성 JSON

```json
{}
```

## `table.properties` — 표/셀 속성

table, props{cellSpacing, padding*, pageBreak 0–2, repeatHeader, treatAsChar, textWrap, vertRelTo, horzAlign, *Offset, outerMargin*, hasCaption, caption*…} (HWPUNIT)

```json
{
  "props": {
    "captionDirection": 3,
    "hasCaption": true,
    "repeatHeader": true
  }
}
```

## `table.cell_properties.get` — 셀 속성 조회

table, range(없으면 커서 셀) → 셀 속성 JSON

```json
{}
```

## `table.cell_properties` — 셀 속성 / 셀 테두리·배경

table, range, props{width, height, padding*, applyInnerMargin, verticalAlign 0–2, textDirection, isHeader, cellProtect, fieldName, fillType none|solid, fillColor #rrggbb, diagonal*…}, borders{left|right|top|bottom|inner_vertical|inner_horizontal: {type,width,color}}

```json
{
  "borders": {
    "bottom": {
      "color": "#4f81bd",
      "type": 1,
      "width": 4
    }
  },
  "props": {
    "fillColor": "#dbe5f1",
    "fillType": "solid"
  }
}
```

## `table.merge` — 셀 합치기

table, range(2셀 이상; 합친 셀이 걸치면 블록이 넓어짐)

```json
{
  "range": {
    "end_col": 1,
    "end_row": 1,
    "start_col": 0,
    "start_row": 0
  }
}
```

## `table.split` — 셀 나누기

table, range, rows·columns(나눌 수), equal_height, merge_first(합친 후 나누기)

```json
{
  "columns": 2,
  "rows": 2
}
```

## `table.equal_height` — 셀 높이를 같게

table, range → 블록 줄 높이를 같게

```json
{}
```

## `table.equal_width` — 셀 너비를 같게

table, range → 블록 칸 너비를 같게

```json
{}
```

## `table.resize` — 셀 크기 조절

table, range, vertical(true: 줄 높이), delta_mm(±; Ctrl+방향키는 1 mm)

```json
{
  "delta_mm": 1.0,
  "vertical": false
}
```

## `table.formula` — 계산식

table, formula(SUM·AVG·PRODUCT·MIN·MAX·COUNT·ABS·SQRT·INT·ROUND·MOD·IF, A1:B3·LEFT·ABOVE), row·col(없으면 커서 셀)

```json
{
  "formula": "=SUM(LEFT)"
}
```

## `table.block_calc` — 블록 합계 / 평균 / 곱

table, range, op sum|average|product → 블록 끝 빈 줄/칸에 결과

```json
{
  "op": "sum"
}
```

## `table.delete` — 표 지우기

table(없으면 커서 표)

```json
{}
```

## `table.transpose` — 표 뒤집기 (줄/칸 바꾸기)

table → 줄/칸 바꾸기 (합친 셀 없는 표)

```json
{}
```

## `table.split_table` — 표 나누기

table, range(없으면 커서 줄) → 그 줄 위에서 표 나누기

```json
{}
```

## `table.merge_table` — 표 붙이기

table → 다음 문단의 표와 붙이기

```json
{}
```

## `table.thousands` — 1,000 단위 구분 쉼표

table, range, remove(true: 쉼표 빼기)

```json
{
  "remove": false
}
```

## `table.move_border` — 셀 경계선 옮기기 (마우스 끌기)

table, vertical(true: 세로 경계선), boundary(경계선 번호), delta_mm

```json
{
  "boundary": 1,
  "delta_mm": 5.0,
  "vertical": true
}
```

## `table.from_text` — 문자열을 표로

delimiter(탭·쉼표 등 구분 문자열); 선택 문단을 표로

```json
{
  "delimiter": "\t"
}
```

## `table.to_text` — 표를 문자열로

table, delimiter → 표를 문자열로

```json
{
  "delimiter": "\t"
}
```

## `table.sort` — 표 정렬 (줄 단위)

table, range(정렬할 줄), column(기준 칸), descending

```json
{
  "column": 0,
  "descending": false
}
```

## `table.auto_fill` — 자동 채우기

table, range(2셀 이상), values[](사용자 목록; 없으면 숫자·요일·월·분기·간지 등 자동 인식)

```json
{
  "values": [
    "상",
    "중",
    "하"
  ]
}
```

## `table.caption_text.get` — 캡션 내용 조회

table → 캡션의 번호 뒤 문자열

```json
{}
```

## `table.caption_text` — 캡션 내용 고치기

table, text(캡션 번호 뒤 문자열)

```json
{
  "text": "분기별 판매 현황"
}
```

## `table.style` — 표 마당 (기본 모양 + 제목 줄·첫/마지막 칸·마지막 줄 옵션)

table, preset basic|blue_header|gray_grid|striped, header_row(기본 true), first_column, last_column, last_row, grayscale, borders(기본 true), background(기본 true)

```json
{
  "first_column": true,
  "preset": "blue_header"
}
```

## `table.cell_shade` — 셀 음영 어둡게 / 밝게

table, range(없으면 커서 셀), darker(true: 어둡게, false: 밝게) → 바뀐 셀 수

```json
{
  "darker": true
}
```

## `table.cell_move` — 표 안 셀 이동 (left·right·up·down·row_begin·row_end·col_top·col_bottom·next·prev)

table, motion left|right|up|down|row_begin|row_end|col_top|col_bottom|next|prev → moved, row, col, appended(next가 마지막 셀에서 줄 추가)

```json
{
  "motion": "next"
}
```

## `table.draw_create` — 마우스로 표 외곽 그리기

Draw a floating single-cell table. Dimensions and paper-relative origin are millimetres; no active selection or nested table.

```json
{
  "height_mm": 50,
  "width_mm": 100,
  "x_mm": 30,
  "y_mm": 40
}
```

## `table.pen` — 표 구분선 그리기 / 지우기

Start/end are [x,y] millimetres from the table origin. erase merges a shared rectangular edge or hides an outer edge. line uses HWP type/width indices.

```json
{
  "end": [
    30,
    50
  ],
  "erase": false,
  "line": {
    "color": "#000000",
    "type": 1,
    "width": 0
  },
  "start": [
    30,
    0
  ],
  "table": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `table.move` — 표 위치 이동

Move a top-level table on its current page. page is zero-based; dx_mm/dy_mm are millimetre deltas. Inline tables become floating; split tables are rejected.

```json
{
  "dx_mm": 10,
  "dy_mm": 5,
  "page": 0,
  "table": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `picture.insert` — 그림 넣기 (PNG·JPG·BMP·GIF)

path(PNG·JPG·BMP·GIF, 20 MB·8192 px 이하), width_mm(없으면 원본 비율), treat_as_char

```json
{
  "path": "C:/img/logo.png",
  "width_mm": 60.0
}
```

## `object.properties.get` — 개체 속성 조회

address{section,paragraph,control} → kind, props

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `object.properties` — 개체 속성

address, props{width, height, treatAsChar, textWrap, horz/vertRelTo, horz/vertAlign, horz/vertOffset, rotationAngle, sizeProtect, outerMargin*, caption*, 그림: crop*, brightness, contrast, effect, transparency; 도형: borderColor(BGR), borderWidth, lineType 0–17, arrowStart/End, fillType, fillBgColor, shadowType 0–8, shadowOffsetX/Y(%), tbMargin*, tbVerticalAlign}

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "props": {
    "rotationAngle": 90
  }
}
```

## `shape.insert` — 도형 넣기 (직선·사각형·타원·호·다각형·곡선·글상자)

kind rectangle|ellipse|line|textbox|arc|polygon|curve|connector_*, width_mm, height_mm, treat_as_char, text_wrap, points_mm[[x,y]], paragraphs[], x_mm·y_mm(종이 기준), flip_x/flip_y, start/end{address,point}, direction horizontal|vertical|vertical_all(글상자)

```json
{
  "height_mm": 20.0,
  "kind": "rectangle",
  "width_mm": 40.0
}
```

## `textbox.paragraphs.get` — 글상자 문단 조회

address → paragraphs[]

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `textbox.paragraphs` — 글상자 여러 문단 고치기

address, paragraphs[](1–200문단)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "paragraphs": [
    "첫 문단",
    "둘째 문단"
  ]
}
```

## `note.paragraphs.get` — 각주·미주 문단 조회

address → paragraphs[] (첫 문단은 번호 뒤 글자)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `note.paragraphs` — 각주·미주 여러 문단 고치기

address, paragraphs[]

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "paragraphs": [
    "주석 내용"
  ]
}
```

## `note.insert` — 각주·미주 넣기 (여러 문단)

endnote(false: 각주), paragraphs[] → address

```json
{
  "endnote": false,
  "paragraphs": [
    "각주 내용"
  ]
}
```

## `equation.insert` — 수식 넣기 (색·기준선·글꼴)

script(수식 스크립트), size_pt, color(BGR), baseline, font_name → address

```json
{
  "script": "a^2+b^2=c^2",
  "size_pt": 10.0
}
```

## `chart.list` — 차트 목록

(없음) → 차트 목록

```json
{}
```

## `chart.data.get` — 차트 데이터 조회

index → 계열·값

```json
{
  "index": 0
}
```

## `chart.data` — 차트 데이터 고치기 (값·계열 이름)

index, series[{name, values[]}]

```json
{
  "index": 0,
  "series": [
    {
      "name": "1분기",
      "values": [
        1,
        2,
        3
      ]
    }
  ]
}
```

## `memo.list` — 메모 목록 (읽기)

(없음) → 메모 목록(읽기)

```json
{}
```

## `object.group` — 개체 묶기 (도형·그림 2개 이상)

targets[address…](같은 구역의 떠 있는 도형·그림 2–256개) → address

```json
{
  "targets": [
    {
      "control": 0,
      "paragraph": 0,
      "section": 0
    },
    {
      "control": 1,
      "paragraph": 0,
      "section": 0
    }
  ]
}
```

## `object.ungroup` — 개체 풀기

address(묶음) → 한 단계 풀기

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `object.z_order` — 개체 앞뒤 순서 (front·back·forward·backward)

address, mode front|back|forward|backward → moved

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "mode": "front"
}
```

## `object.lock` — 개체 보호 (잠금 켜기/끄기)

address, locked(없으면 토글) → locked

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `object.unlock_all` — 모든 개체 보호 해제

(없음) → unlocked 개수

```json
{}
```

## `object.flip` — 개체 대칭 (horizontal·vertical)

address, axis horizontal|vertical

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "axis": "horizontal"
}
```

## `object.line_endpoint` — 직선 끝점 옮기기 (mm)

address(직선), start_mm[x,y], end_mm[x,y] (종이 기준)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "end_mm": [
    80.0,
    30.0
  ],
  "start_mm": [
    20.0,
    30.0
  ]
}
```

## `insert.field` — 누름틀 넣기 (안내문·이름)

guide(안내문), name, memo(도움말), editable

```json
{
  "guide": "이름을 입력하세요",
  "name": "이름"
}
```

## `field.list` — 누름틀 목록

(없음) → 누름틀 목록

```json
{}
```

## `field.set` — 누름틀 값 고치기

field_id 또는 name+occurrence, value

```json
{
  "name": "이름",
  "value": "홍길동"
}
```

## `form.list` — 양식 개체 목록

(없음) → 양식 개체 목록

```json
{}
```

## `form.get` — 양식 개체 정보 조회

at{section,paragraph,control,cell?} → 종류·값

```json
{
  "at": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `form.set` — 양식 개체 값 고치기 (value·text·caption)

at, value|text|caption (종류별)

```json
{
  "at": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "value": true
}
```

## `object.align` — 개체 맞춤 (왼쪽·가운데·오른쪽·위·중간·아래, 같은 너비·높이·크기, 가로·세로 간격 동일)

targets[address…](없으면 1개는 종이 기준), mode left|center|right|top|middle|bottom|same_width|same_height|same_size|space_h|space_v → moved 개수

```json
{
  "mode": "left",
  "targets": [
    {
      "control": 0,
      "paragraph": 0,
      "section": 0
    },
    {
      "control": 1,
      "paragraph": 0,
      "section": 0
    }
  ]
}
```

## `object.size` — 개체 너비·높이 (mm)

address, width_mm, height_mm(둘 중 하나 이상), keep_ratio

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "keep_ratio": true,
  "width_mm": 50.0
}
```

## `object.rotate` — 개체 회전 (각도·증감)

address, angle(절대 0–359) 또는 by(증감 ±) → angle

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "by": 90
}
```

## `object.protect_size` — 개체 크기 고정

address, protected(없으면 토글) → protected

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `object.outer_margin` — 개체 바깥 여백 (사전 값·mm)

address, preset none|narrow|normal|wide 또는 mm(0–50)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "preset": "normal"
}
```

## `object.style` — 개체 빠른 스타일 (채우기·선 색, 선 종류·굵기, 화살표)

address, fill "#rrggbb"|"none", outline "#rrggbb"|"none", line_type 0–17, line_width_mm 0–5, arrow_start/arrow_end 0–8, arrow_size 0–8

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "fill": "#ffe08a",
  "line_width_mm": 0.5,
  "outline": "#000000"
}
```

## `object.shadow` — 개체 그림자 (종류·이동·없음)

address, mode none|on|left|right|up|down|reset, kind 1–8(on), step(%, 기본 1)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "kind": 4,
  "mode": "on"
}
```

## `object.format.get` — 개체 모양 복사 (선·채우기·그림자 읽기)

address → format{선·채우기·그림자·회전 속성}

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `object.format` — 개체 모양 붙이기

address, format(object.format.get 결과)

```json
{
  "address": {
    "control": 1,
    "paragraph": 0,
    "section": 0
  },
  "format": {
    "borderColor": 255,
    "borderWidth": 28
  }
}
```

## `object.neighbor` — 다음·이전 개체 찾기

address(없으면 커서 문단), direction next|prev, wrap(기본 true) → address, kind, page

```json
{
  "direction": "next"
}
```

## `object.connector_type` — 연결선 종류 바꾸기 (직선·꺾인·곡선)

address(연결선), kind straight|stroke|arc

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "kind": "stroke"
}
```

## `textbox.align` — 글상자 글자 정렬 (가로 3 × 세로 3)

address, horizontal left|center|right|justify, vertical top|center|bottom (둘 중 하나 이상)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "horizontal": "center",
  "vertical": "center"
}
```

## `textbox.margin` — 글상자 안 여백 (사전 값·mm)

address, preset none|narrow|normal|wide 또는 mm(0–50)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "preset": "normal"
}
```

## `textbox.direction` — 글상자 글자 방향 (가로·세로·영문 세움)

address, direction horizontal|vertical(세로 글상자)|vertical_all(영문 세움)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "direction": "vertical"
}
```

## `picture.save` — 그림 파일로 저장

address, path(확장자는 원본대로 바뀜), overwrite → path

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "path": "C:/out/picture1.png"
}
```

## `picture.save_all` — 모든 그림 파일로 저장

dir, prefix(기본 picture), overwrite → files[]

```json
{
  "dir": "C:/out"
}
```

## `picture.recolor` — 그림 색 조정 (원래·회색조·흑백·워터마크)

address, mode original|grayscale|black_white|watermark

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "mode": "grayscale"
}
```

## `insert.auto_number` — 번호 넣기 (쪽·전체 쪽·그림·표·수식 번호)

kind page|total|picture|table|equation (본문 커서 위치)

```json
{
  "kind": "picture"
}
```

## `note.convert` — 각주 ↔ 미주 변환

address(각주 또는 미주) → address(변환된 주석)

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  }
}
```

## `note.shape.get` — 각주·미주 모양 조회 (번호·구분선)

kind footnote|endnote, section(없으면 커서 구역) → 번호·구분선 속성

```json
{
  "kind": "footnote"
}
```

## `note.shape` — 각주·미주 모양 (번호 모양·구분선 길이·종류·색)

kind, section, props{numberFormat, prefixChar, suffixChar, startNumber, separatorEnabled, separatorLength(HWPUNIT; -1 단 폭), separatorLineType 0–17, separatorLineWidth, separatorColor "#rrggbb", numberCodeSuperscript, numbering continue|section|page}

```json
{
  "kind": "footnote",
  "props": {
    "separatorLength": 5669,
    "startNumber": 1
  }
}
```

## `note.delete` — 각주·미주 지우기

address(없으면 커서 문단의 첫 주석)

```json
{}
```

## `field.delete` — 누름틀 지우기 (글자 유지)

field_id(없으면 커서 위치의 누름틀) → removed

```json
{}
```

## `view.marks.get` — 문단 부호·조판 부호·투명 선 보기 조회

No parameters; paragraphMarks, controlCodes, transparentBorders (screen only).

```json
{}
```

## `view.marks` — 문단 부호·조판 부호·투명 선 보기 (화면 전용, 저장 안 함)

paragraphMarks, controlCodes, transparentBorders: bool (default false). Screen only: not saved and not part of undo.

```json
{
  "controlCodes": true,
  "paragraphMarks": true,
  "transparentBorders": false
}
```

## `page.header_footer.text.get` — 머리말/꼬리말 내용·필드 조회

header: bool; apply_to: both|odd|even (default both). Returns exists, text and fields [{offset, kind}].

```json
{
  "apply_to": "both",
  "header": true
}
```

## `page.header_footer.text` — 머리말/꼬리말 내용·필드(쪽 번호·전체 쪽수·파일 이름)

header: bool; apply_to: both|odd|even; text: one line; fields: [{offset, kind: page_number|total_pages|file_name}] inserted at text offsets.

```json
{
  "apply_to": "both",
  "fields": [
    {
      "kind": "page_number",
      "offset": 4
    }
  ],
  "header": true,
  "text": "보고서 "
}
```

## `page.section.get` — 구역 설정 조회

No parameters; current section: pageNum, pageNumType, pictureNum, tableNum, equationNum (0 = continue), defaultTabSpacingMm, hideHeader, hideFooter, hideMasterPage, hideBorder, hideFill, hideEmptyLine.

```json
{}
```

## `page.section` — 구역 설정

Optional pageNum, pageNumType, pictureNum, tableNum, equationNum (0 = continue from the previous section), defaultTabSpacingMm, hideHeader, hideFooter, hideMasterPage, hideBorder, hideFill, hideEmptyLine; wholeDocument applies to every section.

```json
{
  "defaultTabSpacingMm": 40,
  "hideHeader": false,
  "pageNum": 1
}
```

## `security.privacy.scan` — 개인 정보·숨은 글자 찾기

kinds: subset of ssn|card|phone|email (default all); mask: mask character(s); hiddenText: also list hidden text (default true); offPage: include text off the page.

```json
{
  "kinds": [
    "ssn",
    "phone"
  ],
  "mask": "*"
}
```

## `security.privacy.mask` — 개인 정보 가리기

items: [{raw, masked}] with equal character counts, as returned by the scan; every occurrence is replaced in one undo step.

```json
{
  "items": [
    {
      "masked": "010-****-****",
      "raw": "010-1234-5678"
    }
  ]
}
```

## `file.save_hml` — HML로 저장 (HML 원본 문서만)

path ending in .hml; only documents opened from HML can be written back.

```json
{
  "path": "out.hml"
}
```

## `file.save_pages` — 쪽 범위만 저장 (현재 쪽만)

path (.hwp/.hwpx/.pdf/.txt); from, to: 1-based inclusive page range. The open document is unchanged.

```json
{
  "from": 2,
  "path": "2-3쪽.hwpx",
  "to": 3
}
```

## `page.border_fill.get` — 쪽 테두리/배경 조회

No parameters; current section border/fill with spacingMm [left, right, top, bottom].

```json
{}
```

## `page.border_fill` — 쪽 테두리/배경

basis: paper|page; spacingMm: [left, right, top, bottom]; headerInside, footerInside: bool; applyPage: all|exceptFirst; borders: four {type, width, color #rrggbb} for left, right, top, bottom; fill: #rrggbb or null; fillArea: paper|page|border; wholeDocument: bool.

```json
{
  "applyPage": "all",
  "basis": "paper",
  "borders": [
    {
      "color": "#000000",
      "type": 1,
      "width": 2
    },
    {
      "color": "#000000",
      "type": 1,
      "width": 2
    },
    {
      "color": "#000000",
      "type": 1,
      "width": 2
    },
    {
      "color": "#000000",
      "type": 1,
      "width": 2
    }
  ],
  "fill": null,
  "footerInside": false,
  "headerInside": false,
  "spacingMm": [
    5,
    5,
    5,
    5
  ]
}
```

## `page.header` — 머리말 (마당에서 만들기)

apply_to: both|odd|even (default both); template: 0-10 (0 none, 1-3 page number left/centre/right, 4-5 with file name, 6-10 the same bold and underlined).

```json
{
  "apply_to": "both",
  "template": 2
}
```

## `page.footer` — 꼬리말 (마당에서 만들기)

Same parameters as page.header.

```json
{
  "apply_to": "both",
  "template": 2
}
```

## `page.header_footer.list` — 머리말/꼬리말 목록

No parameters; headers and footers of the current section.

```json
{}
```

## `page.header_footer.delete` — 머리말/꼬리말 지우기

header: bool; apply_to: both|odd|even (default both).

```json
{
  "apply_to": "both",
  "header": true
}
```

## `page.number.get` — 쪽 번호 매기기 조회

No parameters; exists, position, format, dash, start.

```json
{}
```

## `page.number` — 쪽 번호 매기기

position: none|top_left|top_center|top_right|bottom_left|bottom_center|bottom_right|outside_top|outside_bottom|inside_top|inside_bottom; format: 0 1,2,3 / 1 circled / 2 I,II,III / 3 i,ii,iii / 4 A,B,C / 5 a,b,c / 8 가,나,다 / 10 ㄱ,ㄴ,ㄷ / 12 일,이,삼 / 13 一,二,三; dash: bool; start: optional first number.

```json
{
  "dash": true,
  "format": 0,
  "position": "bottom_center",
  "start": 1
}
```

## `page.new_number` — 새 번호로 시작

start: page number (1 or more) that begins at the caret page.

```json
{
  "start": 5
}
```

## `page.hide.get` — 현재 쪽만 감추기 조회

No parameters; hide flags of the caret page.

```json
{}
```

## `page.hide` — 현재 쪽만 감추기

header, footer, master_page, border, fill, page_number: bool, for the caret page only.

```json
{
  "border": false,
  "fill": false,
  "footer": false,
  "header": true,
  "master_page": false,
  "page_number": true
}
```

## `page.master.list` — 바탕쪽 목록

No parameters; master pages of the current section (read only).

```json
{}
```

## `file.doc_info` — 문서 정보

No parameters; file {path, format, bytes, modified, created}, pages, sections, paragraphs, lines, characters, words, tables, pictures, shapes, equations, notes, fonts, fontSubstitutions, binData, pictureData.

```json
{}
```

## `file.print_pdf` — 인쇄용 PDF 만들기 (쪽 범위)

path (.pdf); pages: all|current|to_current|from_current or a list like 1,3,5-7; current: 0-based caret page used by the current* ranges.

```json
{
  "current": 0,
  "pages": "1,3,5-7",
  "path": "print.pdf"
}
```

## `page.margins.presets` — 쪽 여백 사전 값 목록 (기본·좁게1·좁게2·넓게(머리말/꼬리말 포함)·좁게·넓게)

No parameters; the six Hangul 쪽 여백 presets with their millimetre values.

```json
{}
```

## `page.margins.preset` — 쪽 여백 사전 값 적용 (현재 구역)

preset: default|narrow1_hf|narrow2_hf|wide_hf|narrow|wide, the Hangul gallery in order (기본, 좁게1/좁게2/넓게 with header and footer margins, 좁게, 넓게). Applies to the caret section; paper size and gutter stay.

```json
{
  "preset": "narrow"
}
```

## `page.section.next` — 이후 구역으로

No parameters; caret to the first paragraph of the next section.

```json
{}
```

## `page.section.prev` — 이전 구역으로

No parameters; caret to the first paragraph of the previous section.

```json
{}
```

## `page.copy` — 쪽 복사하기 (쪽 클립보드)

page: optional 1-based page (default: caret page). Copies the paragraphs that start on that page to the process-wide page clipboard.

```json
{
  "page": 2
}
```

## `page.paste` — 쪽 붙이기 (현재 쪽 뒤에 새 쪽으로)

page: optional 1-based page (default: caret page). Inserts the copied page as a new page after it; the caret ends after the pasted text.

```json
{}
```

## `page.delete` — 쪽 지우기

page: optional 1-based page (default: caret page). Removes the paragraphs of that page so later pages move up; refused for single-page documents.

```json
{}
```

## `file.insert` — 문서 끼워 넣기 (HWP/HWPX를 커서 위치에)

path of an HWP/HWPX/HWP3 document; password when it is protected. Its body is merged at the caret with fonts, styles and pictures remapped; one undo step.

```json
{
  "path": "부록.hwpx"
}
```

## `file.save_block` — 블록 저장 (선택 영역만 새 파일로)

path (.hwp/.hwpx/.pdf/.txt); writes only the selected body text, including whole middle paragraphs of a multi-paragraph selection.

```json
{
  "path": "블록.hwpx"
}
```

## `tools.hanja.candidates` — 한자 후보 조회 (한글 음절별 한자, 한자의 음과 동음 한자)

text(선택, 64자 이하): 생략하면 선택 영역 또는 커서 위치의 낱말. 결과 items[]에 음절별 후보(hanja, ks_x_1001, education, personal_name)와 hangul(한자를 한글로 읽은 값)을 돌려줍니다.

```json
{
  "text": "한자"
}
```

## `tools.hanja.convert` — 한자로 바꾸기 (선택·낱말, 입력 형식: 한자 / 한자(한글) / 한글(한자) / 한자[한글])

hanja(필수): 바꿀 한자 문자열(고르지 않은 음절은 한글 그대로 가능). format: hanja | hanja_hangul | hangul_hanja | hanja_bracket (기본 hanja). hangul(선택): 괄호 안 한글 표기, 기본값은 바뀌는 글자의 독음. 선택 영역 또는 커서 위치의 낱말을 바꿉니다.

```json
{
  "format": "hanja_hangul",
  "hanja": "漢字"
}
```

## `tools.hanja.to_hangul` — 한자를 한글로 바꾸기 (선택·낱말·문서 전체, 두음 법칙 적용)

scope: auto(선택 영역 또는 낱말, 기본) | selection | word | document. 한자를 독음으로 바꾸며 낱말 첫 음절에 두음 법칙을 적용합니다. document는 본문과 표 셀 전체를 한 번의 실행 취소로 바꿉니다.

```json
{
  "scope": "document"
}
```

## `review.compare` — 문서 비교 (현재 문서와 파일: 추가·삭제·변경 문단, 표·개체·스타일 차이)

path(필수): 비교할 HWP/HWPX/HML 파일. ignore_whitespace(선택): 띄어쓰기 차이 무시. password(선택): 암호 문서. 현재 문서를 수정본, 파일을 원본으로 보고 findings[]에 종류(label)·위치(section, paragraph, cursor)·내용(detail)을 돌려줍니다. 글상자·각주·머리말 안은 비교하지 않습니다.

```json
{
  "ignore_whitespace": true,
  "path": "original.hwpx"
}
```

## `tools.autotext.list` — 상용구 목록

No parameters; entries[] with name, abbreviation, text and the store path.

```json
{}
```

## `tools.autotext.add` — 상용구 등록 (이름·준말·내용 또는 선택 영역)

name(필수, 64자 이하), abbreviation(선택, 준말: 공백 없는 32자 이하), text(선택: 생략하면 현재 선택 영역의 텍스트), overwrite(선택: 같은 이름을 덮어씀). 글 상용구만 저장하며 서식·표·개체는 저장하지 않습니다.

```json
{
  "abbreviation": "인사",
  "name": "인사말",
  "text": "안녕하십니까. 귀사의 무궁한 발전을 기원합니다."
}
```

## `tools.autotext.delete` — 상용구 지우기

name(필수).

```json
{
  "name": "인사말"
}
```

## `tools.autotext.insert` — 상용구 넣기 (이름)

name(필수): 상용구 내용을 커서 위치에 넣습니다(선택 영역은 바뀜). 줄 바꿈은 문단 나누기가 됩니다.

```json
{
  "name": "인사말"
}
```

## `tools.autotext.expand` — 상용구 바로 넣기 (커서 앞 준말·이름을 내용으로 바꿈)

No parameters; 커서 바로 앞 낱말과 같은 준말 또는 이름의 상용구로 바꿉니다.

```json
{}
```
