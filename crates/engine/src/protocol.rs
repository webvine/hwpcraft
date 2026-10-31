use crate::{Command, Cursor, Session};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    rows: u16,
    columns: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replace {
    query: String,
    #[serde(default)]
    replacement: String,
    #[serde(default)]
    options: Option<crate::FindOptions>,
}

/// (command id, parameter help, JSON example) for the core commands registered
/// in `COMMANDS`; shown by `list_commands` and docs/mcp-commands.md.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "edit.delete_ctrl_marks",
        "kinds 배열: table,picture,shape,equation,footnote,endnote,bookmark,hyperlink,hidden_comment,field.",
        r##"{"kinds":["bookmark"]}"##,
    ),
    (
        "edit.find",
        "query, replacement(선택), options:{case_sensitive,whole_word,direction:down/up/all}.",
        r##"{"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}"##,
    ),
    (
        "edit.paste_html",
        "캐럿에 서식과 함께 넣을 HTML 조각(4 MB까지). 선택은 바뀝니다.",
        r##"{"html":"<p><b>굵게</b> 보통</p>"}"##,
    ),
    (
        "edit.replace",
        "query, replacement(선택), options:{case_sensitive,whole_word,direction:down/up/all}.",
        r##"{"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}"##,
    ),
    (
        "edit.replace_all",
        "query, replacement(선택), options:{case_sensitive,whole_word,direction:down/up/all}. 기본 범위는 본문과 1단계 셀입니다. include(context.v2.list의 종류 배열, []이면 전부)를 주면 머리말·꼬리말·각주·미주·글상자·모든 깊이의 셀도 한 번에(실행 취소 한 단계) 바꾸고, 보호·읽기 전용 문맥의 일치는 건너뛰어 skippedProtected로 셉니다. options가 없으면 대소문자를 가리는 그대로 찾기입니다.",
        r##"{"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}"##,
    ),
    (
        "file.new",
        "빈 문서. force:true면 저장하지 않은 변경을 버립니다.",
        r##"{"force":false}"##,
    ),
    (
        "file.save",
        "HWP/HWPX 저장 또는 PDF/TXT 내보내기. path를 생략할 수 있는 것은 file.save뿐입니다.",
        r##"{"path":"output.hwpx"}"##,
    ),
    (
        "file.save_as",
        "HWP/HWPX 저장 또는 PDF/TXT 내보내기. path를 생략할 수 있는 것은 file.save뿐입니다.",
        r##"{"path":"output.hwpx"}"##,
    ),
    (
        "format.paragraph",
        "문단 속성 일부. format.paragraph.get으로 확인하세요.",
        r##"{"alignment":"center","lineSpacing":160}"##,
    ),
    (
        "hyperlink.update",
        "field_id: hyperlink.list의 값. 새 uri(http/https)와/또는 한 줄 text. 같은 값이면 아무것도 하지 않습니다.",
        r##"{"field_id":1,"uri":"https://example.org","text":"새 표시 글"}"##,
    ),
    (
        "insert.equation",
        "한컴 수식 스크립트, size_pt 4–100.",
        r##"{"script":"a over b","size_pt":12}"##,
    ),
    (
        "insert.hyperlink",
        "HTTP/HTTPS uri와 표시할 text(선택).",
        r##"{"uri":"https://example.com","text":"참고"}"##,
    ),
    (
        "insert.picture",
        "그림 파일 경로(읽기 허용 폴더 안).",
        r##"{"path":"picture.png"}"##,
    ),
    (
        "note.set",
        "ObjectAddress와 바꿀 글.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"text":"수정한 내용"}"##,
    ),
    (
        "page.header_footer",
        "머리말 또는 꼬리말 일반 글.",
        r##"{"is_header":true,"text":"보고서"}"##,
    ),
    (
        "page.setup",
        "용지 설정 일부. 크기·여백은 mm, landscape는 bool. apply_to: section(기본, 캐럿 구역)|document(모든 구역, 구역마다 주지 않은 항목은 그대로).",
        r##"{"landscape":false,"width_mm":210,"height_mm":297}"##,
    ),
    (
        "select.text",
        "한 문단 안에서 대소문자를 가려 찾습니다. occurrence는 1부터.",
        r##"{"text":"한글","occurrence":1}"##,
    ),
    (
        "selection.set",
        "0부터 센 유니코드 스칼라 오프셋. cell:{control,index,paragraph}(선택), anchor는 같은 문단 안의 스칼라 오프셋.",
        r##"{"cursor":{"section":0,"paragraph":0,"offset":2,"cell":null},"anchor":0}"##,
    ),
    (
        "symbol.search",
        "query: U+XXXX / 16진수 / #10진수 코드 포인트, 입력한 글자, 특수 문자 이름, 또는 묶음 낱말(한국어·영어).",
        r##"{"query":"화살표"}"##,
    ),
    ("symbol.sets", "No parameters.", r##"{}"##),
    ("symbol.special", "No parameters.", r##"{}"##),
    (
        "textbox.set",
        "ObjectAddress와 바꿀 글.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"text":"수정한 내용"}"##,
    ),
    (
        "file.open",
        "HWP/HWPX/HWP3/HML 파일 경로. force:true면 저장하지 않은 변경을 버립니다.",
        r##"{"path":"input.hwpx","force":false}"##,
    ),
    (
        "file.pdf",
        ".pdf 경로로 내보냅니다.",
        r##"{"path":"output.pdf"}"##,
    ),
    (
        "file.password",
        "password: 이후 HWP/HWPX 저장에 쓸 암호 문자열, null이면 지웁니다. 기록에 남기지 않습니다.",
        r##"{"password":"비밀번호"}"##,
    ),
    (
        "text.insert",
        "text 문자열. 줄 바꿈으로 문단을 나눕니다. 한 번에 1 MB까지.",
        r##"{"text":"안녕하세요"}"##,
    ),
    (
        "insert.footnote",
        "text: 캐럿에 넣을 새 각주의 글.",
        r##"{"text":"각주 내용"}"##,
    ),
    (
        "insert.endnote",
        "text: 캐럿에 넣을 새 미주의 글.",
        r##"{"text":"미주 내용"}"##,
    ),
    (
        "document.text",
        "본문과 1단계 셀을 나눠 읽습니다(nextStart를 따라감). start ≥ 0, limit 1–1000.",
        r##"{"start":0,"limit":100}"##,
    ),
    (
        "document.inspect",
        "스타일·모양 참조와 선택을 포함한 구조를 나눠 읽습니다. start ≥ 0, limit 1–1000.",
        r##"{"start":0,"limit":100}"##,
    ),
    (
        "format.character",
        "글자 속성 일부. fontSize는 1/100 pt, 색은 #RRGGBB. format.character.get으로 확인하세요.",
        r##"{"bold":true,"fontSize":1200}"##,
    ),
    (
        "format.character.get",
        "매개 변수 없음. 캐럿(또는 선택 시작)의 글자 속성.",
        r##"{}"##,
    ),
    (
        "format.paragraph.get",
        "매개 변수 없음. 캐럿의 문단 속성.",
        r##"{}"##,
    ),
    (
        "insert.table",
        "rows 1–30, columns 1–12, 캐럿에(본문만). 결과의 `table`은 새 표 주소로 table.*·캡션 명령에 바로 넘길 수 있습니다.",
        r##"{"rows":2,"columns":3}"##,
    ),
    (
        "bookmark.add",
        "캐럿에 넣을 책갈피 이름.",
        r##"{"name":"요약"}"##,
    ),
    (
        "bookmark.goto",
        "옮겨 갈 책갈피 이름.",
        r##"{"name":"요약"}"##,
    ),
    (
        "bookmark.delete",
        "지울 책갈피 이름.",
        r##"{"name":"요약"}"##,
    ),
    ("bookmark.list", "No parameters.", r##"{}"##),
    ("style.apply", "style.list의 스타일 번호.", r##"{"id":0}"##),
    ("style.list", "No parameters.", r##"{}"##),
    (
        "page.columns",
        "캐럿 구역의 단 수와 간격(mm).",
        r##"{"count":2,"spacing_mm":8}"##,
    ),
    (
        "object.delete",
        "object.list의 ObjectAddress를 그대로 넘깁니다(address로 감싸지 않음).",
        r##"{"section":0,"paragraph":0,"control":0}"##,
    ),
    ("object.list", "No parameters.", r##"{}"##),
    (
        "shape.resize",
        "ObjectAddress와 mm 크기.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"width_mm":40,"height_mm":20}"##,
    ),
    (
        "equation.set",
        "ObjectAddress와 options:{script,size_pt}.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"options":{"script":"x^2","size_pt":12}}"##,
    ),
    (
        "insert.shape",
        "kind rectangle/ellipse/line/textbox/arc/polygon/curve, 크기 mm, text 필수.",
        r##"{"kind":"rectangle","width_mm":40,"height_mm":20,"text":""}"##,
    ),
    (
        "edit.sort",
        "key text/number, descending·case_sensitive bool. 선택한 본문 문단 또는 구역 전체.",
        r##"{"key":"text","descending":false,"case_sensitive":false}"##,
    ),
    (
        "table.edit",
        "action: row_above,row_below,column_left,column_right,delete_row,delete_column,fit_page,merge_right,split_merged; table?{section,paragraph,control}+row,col(0부터, 그 셀로 캐럿을 옮겨 편집) — 없으면 캐럿이 있는 셀",
        r##"{"action":"row_below","table":{"section":0,"paragraph":0,"control":0},"row":0,"col":0}"##,
    ),
    (
        "insert.symbol",
        "문자표: text(1–1000자, 줄 바꿈 없음)를 캐럿에 넣고, font(선택)는 그 글자에만 적용합니다.",
        r##"{"text":"★","font":"Segoe UI Symbol"}"##,
    ),
    (
        "symbol.list",
        "set: symbol.sets의 id(hnc.*, ks.*, u.<hex>, emoji.*, special), start 오프셋, limit 1–4096(기본 256).",
        r##"{"set":"hnc.circled","start":0,"limit":64}"##,
    ),
    (
        "hyperlink.list",
        "매개 변수 없음. 캐럿 문단의 링크: fieldId/start/end/text/uri.",
        r##"{}"##,
    ),
    (
        "hyperlink.remove",
        "field_id: hyperlink.list의 값. restore_format이면 링크가 덮었던 색·밑줄을 되돌립니다.",
        r##"{"field_id":1,"restore_format":true}"##,
    ),
    ("edit.undo", "No parameters.", r##"{}"##),
    ("edit.redo", "No parameters.", r##"{}"##),
    ("text.backspace", "No parameters.", r##"{}"##),
    ("text.delete", "No parameters.", r##"{}"##),
    ("paragraph.split", "No parameters.", r##"{}"##),
    ("insert.page_break", "매개 변수 없음. 본문만.", r##"{}"##),
    ("insert.column_break", "매개 변수 없음. 본문만.", r##"{}"##),
    (
        "insert.page_number",
        "매개 변수 없음. 캐럿 구역에 새 꼬리말을 만들고 가운데 쪽 번호를 넣습니다.",
        r##"{}"##,
    ),
];

impl Session {
    /// The CLI and automation share the exact desktop editing transactions.
    /// File destinations stay at the application boundary, never in a document command.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value> {
        // 배포용 문서: read-only commands only, and no text in their results.
        self.guard_distribution_command(id)?;
        let step = self.record_run(id, &params);
        let value = self.recorded(|| step, |s| s.run_unrecorded(id, params))?;
        Ok(self.scrub_distribution(value))
    }
    fn run_unrecorded(&mut self, id: &str, params: Value) -> Result<Value> {
        let object = params
            .as_object()
            .context("명령 params는 JSON 객체여야 합니다.")?;
        let command = match id {
            "document.text" | "document.inspect" | "select.text" => {
                return self.run_automation(id, params);
            }
            "format.character.get" | "format.paragraph.get" => {
                if !object.is_empty() {
                    bail!("서식 조회에는 매개 변수가 없습니다.");
                }
                return if id == "format.character.get" {
                    self.character_properties()
                } else {
                    self.paragraph_format()
                };
            }
            "object.list" | "style.list" => {
                if !object.is_empty() {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                return Ok(if id == "object.list" {
                    self.objects()
                } else {
                    self.styles()
                });
            }
            "style.apply" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Style {
                    id: usize,
                }
                self.apply_style(serde_json::from_value::<Style>(params)?.id)?;
                return Ok(self.info());
            }
            "object.delete" => {
                self.delete_object(serde_json::from_value(params)?)?;
                return Ok(self.info());
            }
            "insert.equation" => {
                self.set_equation(None, serde_json::from_value(params)?)?;
                return Ok(self.info());
            }
            "equation.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Equation {
                    address: crate::ObjectAddress,
                    options: crate::EquationOptions,
                }
                let p: Equation = serde_json::from_value(params)?;
                self.set_equation(Some(p.address), p.options)?;
                return Ok(self.info());
            }
            "insert.footnote" | "insert.endnote" => {
                let p: Text = serde_json::from_value(params)?;
                self.insert_note(id == "insert.endnote", &p.text)?;
                return Ok(self.info());
            }
            "note.set" | "textbox.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Edit {
                    address: crate::ObjectAddress,
                    text: String,
                }
                let p: Edit = serde_json::from_value(params)?;
                if id == "note.set" {
                    self.set_note_text(p.address, &p.text)?;
                } else {
                    self.set_textbox_text(p.address, &p.text)?;
                }
                return Ok(self.info());
            }
            "insert.shape" => {
                self.insert_shape(serde_json::from_value(params)?)?;
                return Ok(self.info());
            }
            "shape.resize" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Size {
                    address: crate::ObjectAddress,
                    width_mm: f64,
                    height_mm: f64,
                }
                let p: Size = serde_json::from_value(params)?;
                self.resize_shape(p.address, p.width_mm, p.height_mm)?;
                return Ok(self.info());
            }
            "page.header_footer" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct HeaderFooter {
                    is_header: bool,
                    text: String,
                }
                let p: HeaderFooter = serde_json::from_value(params)?;
                self.set_header_footer(p.is_header, &p.text)?;
                return Ok(self.info());
            }
            "insert.page_number" => {
                if !object.is_empty() {
                    bail!("쪽 번호는 매개 변수가 없습니다.");
                }
                self.insert_page_number()?;
                return Ok(self.info());
            }
            "page.columns" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Columns {
                    count: u16,
                    spacing_mm: f64,
                }
                let p: Columns = serde_json::from_value(params)?;
                self.set_columns(p.count, p.spacing_mm)?;
                return Ok(self.info());
            }
            "insert.column_break" => {
                if !object.is_empty() {
                    bail!("단 나누기는 매개 변수가 없습니다.");
                }
                self.column_break()?;
                return Ok(self.info());
            }
            "insert.picture" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Picture {
                    path: std::path::PathBuf,
                }
                let p: Picture = serde_json::from_value(params)?;
                self.insert_png(&p.path)?;
                return Ok(self.info());
            }
            "table.edit" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Edit {
                    action: crate::TableEdit,
                    #[serde(default)]
                    table: Option<crate::ObjectAddress>,
                    #[serde(default)]
                    row: Option<u16>,
                    #[serde(default)]
                    col: Option<u16>,
                }
                let p: Edit = serde_json::from_value(params)?;
                let before = self.cursor;
                match (p.row, p.col) {
                    (Some(row), Some(col)) => self.caret_to_table_cell(p.table, row, col)?,
                    (None, None) if p.table.is_none() => {}
                    _ => bail!("표 셀은 row와 col을 함께 지정하세요."),
                }
                if let Err(e) = self.edit_table(p.action) {
                    // A refused edit leaves the caret where the caller had it.
                    let _ = self.select(before, false);
                    return Err(e);
                }
                return Ok(self.info());
            }
            "insert.hyperlink" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Link {
                    uri: String,
                    text: Option<String>,
                }
                let p: Link = serde_json::from_value(params)?;
                self.insert_hyperlink(&p.uri, p.text.as_deref())?;
                return Ok(self.info());
            }
            "bookmark.add" | "bookmark.delete" | "bookmark.goto" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Bookmark {
                    name: String,
                }
                let p: Bookmark = serde_json::from_value(params)?;
                match id {
                    "bookmark.add" => self.add_bookmark(&p.name)?,
                    "bookmark.delete" => self.delete_bookmark(&p.name)?,
                    _ => self.goto_bookmark(&p.name)?,
                };
                return Ok(self.info());
            }
            "hyperlink.list" => {
                if !object.is_empty() {
                    bail!("hyperlink.list 명령에는 매개 변수가 없습니다.");
                }
                return Ok(serde_json::to_value(self.hyperlinks()?)?);
            }
            "hyperlink.update" => {
                self.update_hyperlink(serde_json::from_value(params)?)?;
                return Ok(self.info());
            }
            "hyperlink.remove" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Remove {
                    field_id: u32,
                    #[serde(default)]
                    restore_format: bool,
                }
                let p: Remove = serde_json::from_value(params)?;
                self.remove_hyperlink(p.field_id, p.restore_format)?;
                return Ok(self.info());
            }
            "edit.paste_html" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Html {
                    html: String,
                }
                let p: Html = serde_json::from_value(params)?;
                self.paste_html(&p.html)?;
                return Ok(self.info());
            }
            "file.password" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Password {
                    #[serde(default)]
                    password: Option<String>,
                }
                let p: Password = serde_json::from_value(params)?;
                self.set_password(p.password)?;
                return Ok(json!({ "protected": self.has_password() }));
            }
            "insert.symbol" => {
                let p: crate::SymbolInsert = serde_json::from_value(params)?;
                let range = self.insert_symbol(&p)?;
                return Ok(json!({"start": range.start, "end": range.end, "cursor": self.cursor}));
            }
            "symbol.sets" | "symbol.special" => {
                if !object.is_empty() {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                return Ok(if id == "symbol.sets" {
                    serde_json::to_value(crate::symbol_sets())?
                } else {
                    serde_json::to_value(crate::special_chars())?
                });
            }
            "symbol.list" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct List {
                    set: String,
                    #[serde(default)]
                    start: usize,
                    #[serde(default = "default_limit")]
                    limit: usize,
                }
                fn default_limit() -> usize {
                    256
                }
                let p: List = serde_json::from_value(params)?;
                if p.limit == 0 || p.limit > 4096 {
                    bail!("limit은 1–4096입니다.");
                }
                let chars = crate::symbol_chars(&p.set)
                    .ok_or_else(|| anyhow::anyhow!("알 수 없는 문자표 영역: {}", p.set))?;
                let total = chars.len();
                let page: Vec<Value> = chars
                    .iter()
                    .skip(p.start)
                    .take(p.limit)
                    .map(|c| json!({"ch": c.to_string(), "code": format!("U+{:04X}", *c as u32)}))
                    .collect();
                return Ok(json!({"set": p.set, "total": total, "start": p.start, "chars": page}));
            }
            "symbol.search" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Query {
                    query: String,
                }
                let p: Query = serde_json::from_value(params)?;
                if p.query.chars().count() > 100 {
                    bail!("검색어는 100자 이하입니다.");
                }
                return Ok(serde_json::to_value(crate::symbol_search(&p.query))?);
            }
            "bookmark.list" => {
                if !object.is_empty() {
                    bail!("bookmark.list 명령에는 매개 변수가 없습니다.");
                }
                return self.bookmarks();
            }
            "text.insert" => Command::Insert(serde_json::from_value::<Text>(params)?.text),
            "text.backspace" | "text.delete" | "paragraph.split" | "insert.page_break"
            | "edit.undo" | "edit.redo" => {
                if !object.is_empty() {
                    bail!("{id} 명령에는 매개 변수가 없습니다.");
                }
                match id {
                    "text.backspace" => Command::Backspace,
                    "text.delete" => Command::Delete,
                    "paragraph.split" => Command::Split,
                    "insert.page_break" => Command::PageBreak,
                    "edit.undo" => Command::Undo,
                    _ => Command::Redo,
                }
            }
            "format.character" => Command::Character(params),
            "format.paragraph" => Command::Paragraph(params),
            "insert.table" => {
                let t: Table = serde_json::from_value(params)?;
                Command::Table {
                    rows: t.rows,
                    columns: t.columns,
                }
            }
            "page.setup" => {
                let mut object = object.clone();
                // 적용 범위: the caret's section (default) or every section, each
                // keeping the items not given (Hangul's ApplyTo 현재 구역 / 문서 전체).
                let everywhere = match object.remove("apply_to") {
                    None => false,
                    Some(v) => match v.as_str() {
                        Some("section") => false,
                        Some("document") => true,
                        _ => bail!("apply_to는 section 또는 document입니다."),
                    },
                };
                let merged =
                    |page: &hwpcraft_doc::model::page::PageDef| -> Result<crate::PageSetup> {
                        let mut current = serde_json::to_value(crate::PageSetup::from_page(page))?;
                        let target = current
                            .as_object_mut()
                            .context("편집 용지 정보를 읽지 못했습니다.")?;
                        for (key, value) in &object {
                            if !target.contains_key(key) {
                                bail!("알 수 없는 편집 용지 항목: {key}");
                            }
                            target.insert(key.clone(), value.clone());
                        }
                        Ok(serde_json::from_value(current)?)
                    };
                if everywhere {
                    let setups = self
                        .core
                        .document()
                        .sections
                        .iter()
                        .map(|s| merged(&s.section_def.page_def))
                        .collect::<Result<Vec<_>>>()?;
                    for setup in &setups {
                        setup.validate()?;
                    }
                    self.guard_protected()?;
                    self.transaction(|s| {
                        s.deferred_layout(|s| {
                            for (section, setup) in setups.iter().enumerate() {
                                s.core
                                    .set_page_def_native(section, &setup.properties().to_string())
                                    .map_err(crate::err)?;
                            }
                            Ok(())
                        })
                    })?;
                    return Ok(self.info());
                }
                let section = self.cursor.section;
                let page = &self
                    .core
                    .document()
                    .sections
                    .get(section)
                    .context("편집 용지 정보를 읽지 못했습니다.")?
                    .section_def
                    .page_def;
                Command::PageSetup(merged(page)?)
            }
            // Without `options` the legacy exact (case-sensitive) replacement is kept.
            "edit.replace_all" => {
                // `include` (context kinds as in `context.v2.list`, empty: all) is
                // this command's own; the rest is shared with find and replace.
                let mut object = object.clone();
                let include = match object.remove("include") {
                    Some(v) => Some(serde_json::from_value::<Vec<String>>(v)?),
                    None => None,
                };
                let r: Replace = serde_json::from_value(Value::Object(object))?;
                if let Some(include) = include {
                    crate::find::check_context_kinds(&include)?;
                    // Without `options`, the exact (case-sensitive) match, as below.
                    let o = r.options.unwrap_or(crate::FindOptions {
                        case_sensitive: true,
                        ..Default::default()
                    });
                    let (replaced, skipped, truncated) =
                        self.replace_all_in_contexts(&r.query, &r.replacement, o, &include)?;
                    return Ok(json!({"replaced": replaced, "skippedProtected": skipped,
                        "truncated": truncated}));
                }
                let replaced = match r.options {
                    Some(o) => self.replace_all_with(&r.query, &r.replacement, o)?,
                    None => self.replace_all(&r.query, &r.replacement)?,
                };
                return Ok(json!({ "replaced": replaced }));
            }
            "edit.sort" => {
                self.sort_paragraphs(serde_json::from_value(params)?)?;
                return Ok(self.info());
            }
            "edit.delete_ctrl_marks" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Marks {
                    kinds: Vec<crate::MarkKind>,
                }
                let p: Marks = serde_json::from_value(params)?;
                return Ok(json!({ "deleted": self.delete_ctrl_marks(&p.kinds)? }));
            }
            "edit.find" | "edit.replace" => {
                let r: Replace = serde_json::from_value(params)?;
                let o = r.options.unwrap_or_default();
                let found = if id == "edit.find" {
                    self.find(&r.query, o)?
                } else {
                    self.replace_next(&r.query, &r.replacement, o)?
                };
                return Ok(json!({"found":found,"cursor":self.cursor,"anchor":self.anchor}));
            }
            "selection.set" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Selection {
                    cursor: Cursor,
                    #[serde(default)]
                    anchor: Option<usize>,
                }
                let s: Selection = serde_json::from_value(params)?;
                let length = self.paragraph(s.cursor)?.text.chars().count();
                if s.cursor.offset > length || s.anchor.is_some_and(|a| a > length) {
                    bail!("선택 범위가 문단을 벗어났습니다.");
                }
                self.select(s.cursor, false)?;
                self.anchor = s.anchor;
                return Ok(json!({"cursor":self.cursor,"anchor":self.anchor}));
            }
            _ => return self.run_extension(id, params),
        };
        self.created_table = None;
        self.execute(command)?;
        let mut info = self.info();
        // Hand a new table's address back so an automation client can caption or
        // format it without searching the document.
        if let Some(table) = self.created_table.take() {
            info["table"] = serde_json::to_value(table)?;
        }
        Ok(info)
    }
}
