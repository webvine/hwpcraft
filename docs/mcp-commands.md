# MCP 명령 인자 참고

`scripts/mcp-command-docs.py <hwpcraft-cli.exe>`로 실제 MCP discovery에서 생성합니다.
인자 예제는 형식 안내입니다. 현재 선택·기존 개체·파일 등 실행 전제 조건을 먼저 준비하세요.
`execute`에 `command`와 아래 객체를 `params`로 전달합니다. 추가 선택 인자는 설명을 참고하세요.
이 문서는 모든 세부 서식 필드의 JSON Schema를 대체하지 않습니다. `*.get` 조회와 엔진 검증을 함께 사용하세요.

명령 수: **154**

## `document.text` — 본문·표 셀 텍스트 조회

Paginated body and first-level cells; follow nextStart. start >= 0, limit 1–1000.

```json
{
  "limit": 100,
  "start": 0
}
```

## `document.inspect` — 문서 구조 조회

Paginated body and first-level cells; follow nextStart. start >= 0, limit 1–1000.

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

No parameters.

```json
{}
```

## `format.paragraph.get` — 문단 모양 조회

No parameters.

```json
{}
```

## `format.paragraph.apply` — 문단 모양·머리 모양 적용

format: partial paragraph properties, optional head:{kind,level}.

```json
{
  "format": {
    "alignment": "left"
  }
}
```

## `format.outline.shape` — 개요 번호 모양 설정

levelFormats and numberFormats arrays (7 levels), optional startNumber. Use format.numbering.list for examples.

```json
{
  "levelFormats": [
    "^1.",
    "^2.",
    "^3.",
    "^4.",
    "^5.",
    "^6.",
    "^7."
  ],
  "numberFormats": [
    0,
    0,
    0,
    0,
    0,
    0,
    0
  ],
  "startNumber": 1
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

text string; text.insert splits newlines into paragraphs.

```json
{
  "text": "안녕하세요"
}
```

## `insert.endnote` — 미주 넣기

text string; text.insert splits newlines into paragraphs.

```json
{
  "text": "안녕하세요"
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

HWP/HWPX input path; force:true explicitly discards unsaved changes.

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

No parameters.

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

rows/columns. table.create also accepts width_mm,height_mm,treat_as_char,column_widths_mm.

```json
{
  "columns": 3,
  "rows": 2
}
```

## `text.insert` — 텍스트 입력

text string; text.insert splits newlines into paragraphs.

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

Bookmark name.

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

Bookmark name.

```json
{
  "name": "요약"
}
```

## `bookmark.delete` — 책갈피 삭제

Bookmark name.

```json
{
  "name": "요약"
}
```

## `page.columns` — 다단 설정

Column count and spacing in mm.

```json
{
  "count": 2,
  "spacing_mm": 8
}
```

## `insert.column_break` — 단 나누기

No parameters.

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

No parameters.

```json
{}
```

## `format.bold` — 진하게

Optional value boolean; omit to toggle.

```json
{
  "value": true
}
```

## `format.italic` — 기울임

Optional value boolean; omit to toggle.

```json
{
  "value": true
}
```

## `format.underline` — 밑줄

Optional value boolean; omit to toggle.

```json
{
  "value": true
}
```

## `format.strikethrough` — 취소선

Optional value boolean; omit to toggle.

```json
{
  "value": true
}
```

## `format.superscript` — 위 첨자

Optional value boolean; omit to toggle.

```json
{
  "value": true
}
```

## `format.subscript` — 아래 첨자

Optional value boolean; omit to toggle.

```json
{
  "value": true
}
```

## `format.normal` — 보통 모양

No parameters.

```json
{}
```

## `format.highlight` — 형광펜(음영 색)

color #RRGGBB or null to clear; implemented as shading.

```json
{
  "color": "#ffff00"
}
```

## `format.font_size.increase` — 글자 크기 크게

No parameters.

```json
{}
```

## `format.font_size.decrease` — 글자 크기 작게

No parameters.

```json
{}
```

## `format.ratio.increase` — 장평 넓게

No parameters.

```json
{}
```

## `format.ratio.decrease` — 장평 좁게

No parameters.

```json
{}
```

## `format.spacing.increase` — 자간 넓게

No parameters.

```json
{}
```

## `format.spacing.decrease` — 자간 좁게

No parameters.

```json
{}
```

## `format.align.justify` — 양쪽 정렬

No parameters.

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

No parameters.

```json
{}
```

## `format.align.split` — 나눔 정렬

No parameters.

```json
{}
```

## `format.line_spacing.increase` — 줄 간격 넓게

No parameters.

```json
{}
```

## `format.line_spacing.decrease` — 줄 간격 좁게

No parameters.

```json
{}
```

## `format.indent.positive` — 첫 줄 들여쓰기

No parameters.

```json
{}
```

## `format.indent.negative` — 첫 줄 내어쓰기

No parameters.

```json
{}
```

## `format.margin_left.increase` — 왼쪽 여백 늘리기

No parameters.

```json
{}
```

## `format.margin_left.decrease` — 왼쪽 여백 줄이기

No parameters.

```json
{}
```

## `format.margin_right.increase` — 오른쪽 여백 늘리기

No parameters.

```json
{}
```

## `format.margin_right.decrease` — 오른쪽 여백 줄이기

No parameters.

```json
{}
```

## `format.margin.increase` — 양쪽 여백 늘리기

No parameters.

```json
{}
```

## `format.margin.decrease` — 양쪽 여백 줄이기

No parameters.

```json
{}
```

## `format.style.list` — 스타일 목록

No parameters.

```json
{}
```

## `format.style.detail` — 스타일 정보

Numeric id from format.style.list.

```json
{
  "id": 0
}
```

## `format.style.create` — 스타일 추가하기

name required; optional englishName,type,nextStyleId,character,paragraph.

```json
{
  "character": {
    "bold": true,
    "fontSize": 1800
  },
  "name": "보고서 제목"
}
```

## `format.style.update` — 스타일 편집하기

id required; optional name,englishName,nextStyleId,character,paragraph.

```json
{
  "character": {
    "fontSize": 1200
  },
  "id": 0
}
```

## `format.style.delete` — 스타일 지우기

id; optional replacement style ID.

```json
{
  "id": 1,
  "replacement": 0
}
```

## `format.style.shortcut` — 스타일 단축키 적용

number 0–9, matching Ctrl+0…Ctrl+9.

```json
{
  "number": 1
}
```

## `format.numbering.list` — 문단 번호 / 글머리표 모양 목록

No parameters.

```json
{}
```

## `format.numbering.create` — 문단 번호 모양 만들기

levelFormats and numberFormats arrays (7 levels), optional startNumber. Use format.numbering.list for examples.

```json
{
  "levelFormats": [
    "^1.",
    "^2.",
    "^3.",
    "^4.",
    "^5.",
    "^6.",
    "^7."
  ],
  "numberFormats": [
    0,
    0,
    0,
    0,
    0,
    0,
    0
  ],
  "startNumber": 1
}
```

## `format.numbering.apply` — 문단 번호 / 글머리표 / 개요 적용

kind none/number/bullet/outline; optional id,char,level,levelFormats,numberFormats,startNumber,restart:{mode,start}.

```json
{
  "kind": "number",
  "level": 0
}
```

## `format.numbering.toggle` — 문단 번호 적용 / 해제

No parameters.

```json
{}
```

## `format.bullet.toggle` — 글머리표 적용 / 해제

No parameters.

```json
{}
```

## `format.outline.toggle` — 개요 적용 / 해제

No parameters.

```json
{}
```

## `format.level.increase` — 한 수준 증가

No parameters.

```json
{}
```

## `format.level.decrease` — 한 수준 감소

No parameters.

```json
{}
```

## `format.numbering.restart` — 새 번호로 시작

mode continue/previous/new, start required for new.

```json
{
  "mode": "new",
  "start": 1
}
```

## `table.create` — 표 만들기

rows/columns. table.create also accepts width_mm,height_mm,treat_as_char,column_widths_mm.

```json
{
  "columns": 3,
  "rows": 2
}
```

## `table.info` — 표 정보

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.properties.get` — 표 속성 조회

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.properties` — 표/셀 속성

Optional table:ObjectAddress; props uses HWPUNIT lengths and native enum names. Use table.properties.get first.

```json
{
  "props": {
    "repeatHeader": true
  }
}
```

## `table.cell_properties.get` — 셀 속성 조회

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.cell_properties` — 셀 속성 / 셀 테두리·배경

Optional table:ObjectAddress, range:CellRange, props object, borders object. Read matching .get command first; use only fields to change.

```json
{
  "props": {
    "width": 14400
  }
}
```

## `table.merge` — 셀 합치기

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.split` — 셀 나누기

rows,columns; optional table,range,equal_height,merge_first.

```json
{
  "columns": 2,
  "rows": 2
}
```

## `table.equal_height` — 셀 높이를 같게

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.equal_width` — 셀 너비를 같게

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.resize` — 셀 크기 조절

vertical bool,delta_mm; optional table,range.

```json
{
  "delta_mm": 2,
  "vertical": true
}
```

## `table.formula` — 계산식

formula string, optional table,row,col; otherwise current cell.

```json
{
  "formula": "=SUM(A1:A2)"
}
```

## `table.block_calc` — 블록 합계 / 평균 / 곱

op sum/average/product; optional table,range.

```json
{
  "op": "sum"
}
```

## `table.delete` — 표 지우기

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.transpose` — 표 뒤집기 (줄/칸 바꾸기)

Optional table:ObjectAddress (otherwise current table), range:{start_row,start_col,end_row,end_col} where applicable; zero-based indices.

```json
{}
```

## `table.split_table` — 표 나누기

Optional table:ObjectAddress,range:CellRange; normally operates on current cell/block.

```json
{}
```

## `table.merge_table` — 표 붙이기

Optional table:ObjectAddress,range:CellRange; normally operates on current cell/block.

```json
{}
```

## `table.thousands` — 1,000 단위 구분 쉼표

Optional table,range,remove boolean.

```json
{
  "remove": false
}
```

## `table.move_border` — 셀 경계선 옮기기 (마우스 끌기)

vertical bool,boundary zero-based,delta_mm; optional table.

```json
{
  "boundary": 1,
  "delta_mm": 2,
  "vertical": true
}
```

## `table.from_text` — 문자열을 표로

delimiter string; table.to_text accepts optional table:ObjectAddress.

```json
{
  "delimiter": "\t"
}
```

## `table.to_text` — 표를 문자열로

delimiter string; table.to_text accepts optional table:ObjectAddress.

```json
{
  "delimiter": "\t"
}
```

## `table.sort` — 표 정렬 (줄 단위)

column (zero-based), optional descending,table,range.

```json
{
  "column": 0,
  "descending": false
}
```

## `table.auto_fill` — 자동 채우기

Optional table:ObjectAddress,range:CellRange; normally operates on current cell/block.

```json
{}
```

## `table.caption_text.get` — 캡션 내용 조회

Optional table:ObjectAddress,range:CellRange; normally operates on current cell/block.

```json
{}
```

## `table.caption_text` — 캡션 내용 고치기

text string, optional table:ObjectAddress. Requires an existing caption: table.properties {props:{hasCaption:true}} first.

```json
{
  "text": "표 설명"
}
```

## `table.style` — 표 마당 (기본 모양)

preset string, optional table:ObjectAddress. Presets: basic,blue_header,gray_grid,striped.

```json
{
  "preset": "basic"
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

Image path, optional width_mm (1–600), treat_as_char boolean.

```json
{
  "path": "picture.png",
  "treat_as_char": true,
  "width_mm": 50
}
```

## `object.properties.get` — 개체 속성 조회

ObjectAddress from object.list.

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

ObjectAddress and partial props; use object.properties.get first. Sizes HWPUNIT, colors BGR integers.

```json
{
  "address": {
    "control": 0,
    "paragraph": 0,
    "section": 0
  },
  "props": {
    "width": 14400
  }
}
```

## `shape.insert` — 도형 넣기 (직선·사각형·타원·호·다각형·곡선·글상자)

kind, width_mm,height_mm; optional treat_as_char,text_wrap,points_mm:[[x,y]],paragraphs:[text],x_mm,y_mm.

```json
{
  "height_mm": 20,
  "kind": "textbox",
  "paragraphs": [
    "글상자"
  ],
  "width_mm": 40
}
```

## `textbox.paragraphs.get` — 글상자 문단 조회

ObjectAddress from object.list.

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

ObjectAddress and array of paragraph strings.

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

ObjectAddress from object.list.

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

ObjectAddress and array of paragraph strings.

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

## `note.insert` — 각주·미주 넣기 (여러 문단)

endnote boolean, paragraphs array.

```json
{
  "endnote": false,
  "paragraphs": [
    "각주 내용"
  ]
}
```

## `equation.insert` — 수식 넣기 (색·기준선·글꼴)

script,size_pt; optional color (BGR integer), baseline, font_name.

```json
{
  "script": "a over b",
  "size_pt": 12
}
```

## `chart.list` — 차트 목록

No parameters.

```json
{}
```

## `chart.data.get` — 차트 데이터 조회

Zero-based chart index from chart.list.

```json
{
  "index": 0
}
```

## `chart.data` — 차트 데이터 고치기 (값·계열 이름)

Existing chart index and series:[{name?:string,values:[string]}]; no new chart creation.

```json
{
  "index": 0,
  "series": [
    {
      "name": "매출",
      "values": [
        "10",
        "20"
      ]
    }
  ]
}
```

## `memo.list` — 메모 목록 (읽기)

No parameters.

```json
{}
```

## `page.border_fill.get` — 쪽 테두리/배경 조회

No parameters; current section/body paragraph.

```json
{}
```

## `page.border_fill` — 쪽 테두리/배경

camelCase keys; borders in left/right/top/bottom order, type 0–17,width 0–15,color #RRGGBB; optional fill,fillArea,wholeDocument.

```json
{
  "applyPage": "all",
  "basis": "paper",
  "borders": [
    {
      "color": "#000000",
      "type": 0,
      "width": 0
    },
    {
      "color": "#000000",
      "type": 0,
      "width": 0
    },
    {
      "color": "#000000",
      "type": 0,
      "width": 0
    },
    {
      "color": "#000000",
      "type": 0,
      "width": 0
    }
  ],
  "footerInside": false,
  "headerInside": false,
  "spacingMm": [
    0,
    0,
    0,
    0
  ]
}
```

## `page.header` — 머리말 (마당에서 만들기)

apply_to both/odd/even, template 0–10.

```json
{
  "apply_to": "both",
  "template": 2
}
```

## `page.footer` — 꼬리말 (마당에서 만들기)

apply_to both/odd/even, template 0–10.

```json
{
  "apply_to": "both",
  "template": 2
}
```

## `page.header_footer.list` — 머리말/꼬리말 목록

No parameters; current section/body paragraph.

```json
{}
```

## `page.header_footer.delete` — 머리말/꼬리말 지우기

header bool, apply_to both/odd/even.

```json
{
  "apply_to": "both",
  "header": true
}
```

## `page.number.get` — 쪽 번호 매기기 조회

No parameters; current section/body paragraph.

```json
{}
```

## `page.number` — 쪽 번호 매기기

position none/top_left/top_center/top_right/bottom_left/bottom_center/bottom_right/outside_top/outside_bottom/inside_top/inside_bottom; format 0–5 or 8/10/12/13, dash bool, start optional.

```json
{
  "dash": false,
  "format": 0,
  "position": "bottom_center"
}
```

## `page.new_number` — 새 번호로 시작

start 1–65535.

```json
{
  "start": 1
}
```

## `page.hide.get` — 현재 쪽만 감추기 조회

No parameters; current section/body paragraph.

```json
{}
```

## `page.hide` — 현재 쪽만 감추기

All six booleans required (snake_case).

```json
{
  "border": false,
  "fill": false,
  "footer": false,
  "header": false,
  "master_page": false,
  "page_number": true
}
```

## `page.master.list` — 바탕쪽 목록

No parameters; current section/body paragraph.

```json
{}
```

## `file.doc_info` — 문서 정보

No parameters.

```json
{}
```

## `file.print_pdf` — 인쇄용 PDF 만들기 (쪽 범위)

PDF export: pages all/current/to_current/from_current or 1,3,5-7; current is zero-based.

```json
{
  "current": 0,
  "pages": "all",
  "path": "print.pdf"
}
```
