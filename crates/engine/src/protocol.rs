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
        "kinds array: table,picture,shape,equation,footnote,endnote,bookmark,hyperlink,hidden_comment,field.",
        r##"{"kinds":["bookmark"]}"##,
    ),
    (
        "edit.find",
        "query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.",
        r##"{"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}"##,
    ),
    (
        "edit.paste_html",
        "HTML fragment (max 4 MB) inserted at the caret with its formatting; the selection is replaced.",
        r##"{"html":"<p><b>굵게</b> 보통</p>"}"##,
    ),
    (
        "edit.replace",
        "query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.",
        r##"{"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}"##,
    ),
    (
        "edit.replace_all",
        "query, optional replacement and options:{case_sensitive,whole_word,direction:down/up/all}.",
        r##"{"query":"기존","replacement":"변경","options":{"case_sensitive":true,"direction":"all"}}"##,
    ),
    (
        "file.new",
        "Blank document; force:true explicitly discards unsaved changes.",
        r##"{"force":false}"##,
    ),
    (
        "file.save",
        "HWP/HWPX save or PDF/TXT export. Only file.save permits omitting path.",
        r##"{"path":"output.hwpx"}"##,
    ),
    (
        "file.save_as",
        "HWP/HWPX save or PDF/TXT export. Only file.save permits omitting path.",
        r##"{"path":"output.hwpx"}"##,
    ),
    (
        "format.paragraph",
        "Partial paragraph properties. Use format.paragraph.get to inspect.",
        r##"{"alignment":"center","lineSpacing":160}"##,
    ),
    (
        "hyperlink.update",
        "field_id from hyperlink.list; new uri (http/https) and/or one-line text. Equal values are a no-op.",
        r##"{"field_id":1,"uri":"https://example.org","text":"새 표시 글"}"##,
    ),
    (
        "insert.equation",
        "Hancom equation script, size_pt 4–100.",
        r##"{"script":"a over b","size_pt":12}"##,
    ),
    (
        "insert.hyperlink",
        "HTTP/HTTPS uri and optional displayed text.",
        r##"{"uri":"https://example.com","text":"참고"}"##,
    ),
    (
        "insert.picture",
        "Image path, subject to read root.",
        r##"{"path":"picture.png"}"##,
    ),
    (
        "note.set",
        "ObjectAddress and replacement text.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"text":"수정한 내용"}"##,
    ),
    (
        "page.header_footer",
        "Header or footer plain text.",
        r##"{"is_header":true,"text":"보고서"}"##,
    ),
    (
        "page.setup",
        "Partial page setup; dimensions/margins in mm, landscape bool.",
        r##"{"landscape":false,"width_mm":210,"height_mm":297}"##,
    ),
    (
        "select.text",
        "Case-sensitive search within a paragraph. occurrence starts at 1.",
        r##"{"text":"한글","occurrence":1}"##,
    ),
    (
        "selection.set",
        "Zero-based Unicode scalar offset. Optional cell:{control,index,paragraph}; anchor is a scalar offset in the same paragraph.",
        r##"{"cursor":{"section":0,"paragraph":0,"offset":2,"cell":null},"anchor":0}"##,
    ),
    (
        "symbol.search",
        "query: U+XXXX / hex / #decimal code point, a typed character, a special-character name, or set keywords (Korean or English).",
        r##"{"query":"화살표"}"##,
    ),
    ("symbol.sets", "No parameters.", r##"{}"##),
    ("symbol.special", "No parameters.", r##"{}"##),
    (
        "textbox.set",
        "ObjectAddress and replacement text.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"text":"수정한 내용"}"##,
    ),
    (
        "file.open",
        "HWP/HWPX/HWP3/HML input path; force:true explicitly discards unsaved changes.",
        r##"{"path":"input.hwpx","force":false}"##,
    ),
    (
        "file.pdf",
        "Export to a .pdf path.",
        r##"{"path":"output.pdf"}"##,
    ),
    (
        "file.password",
        "password string sets the password used by later HWP/HWPX saves; null clears it. Never logged.",
        r##"{"password":"비밀번호"}"##,
    ),
    (
        "text.insert",
        "text string; newlines split paragraphs. Maximum 1 MB per call.",
        r##"{"text":"안녕하세요"}"##,
    ),
    (
        "insert.footnote",
        "text of the new footnote at the caret.",
        r##"{"text":"각주 내용"}"##,
    ),
    (
        "insert.endnote",
        "text of the new endnote at the caret.",
        r##"{"text":"미주 내용"}"##,
    ),
    (
        "document.text",
        "Paginated body and first-level cells; follow nextStart. start >= 0, limit 1–1000.",
        r##"{"start":0,"limit":100}"##,
    ),
    (
        "document.inspect",
        "Paginated structure with style/shape references and selection; start >= 0, limit 1–1000.",
        r##"{"start":0,"limit":100}"##,
    ),
    (
        "format.character",
        "Partial character properties. fontSize is 1/100 pt, colors #RRGGBB. Use format.character.get to inspect.",
        r##"{"bold":true,"fontSize":1200}"##,
    ),
    (
        "format.character.get",
        "No parameters; properties at the caret (or selection start).",
        r##"{}"##,
    ),
    (
        "format.paragraph.get",
        "No parameters; paragraph properties at the caret.",
        r##"{}"##,
    ),
    (
        "insert.table",
        "rows 1–30, columns 1–12 at the caret (body only).",
        r##"{"rows":2,"columns":3}"##,
    ),
    (
        "bookmark.add",
        "Bookmark name at the caret.",
        r##"{"name":"요약"}"##,
    ),
    (
        "bookmark.goto",
        "Bookmark name to move to.",
        r##"{"name":"요약"}"##,
    ),
    (
        "bookmark.delete",
        "Bookmark name to remove.",
        r##"{"name":"요약"}"##,
    ),
    ("bookmark.list", "No parameters.", r##"{}"##),
    (
        "style.apply",
        "Existing numeric style ID from style.list.",
        r##"{"id":0}"##,
    ),
    ("style.list", "No parameters.", r##"{}"##),
    (
        "page.columns",
        "Column count and spacing in mm for the caret section.",
        r##"{"count":2,"spacing_mm":8}"##,
    ),
    (
        "object.delete",
        "ObjectAddress from object.list, passed directly (no address wrapper).",
        r##"{"section":0,"paragraph":0,"control":0}"##,
    ),
    ("object.list", "No parameters.", r##"{}"##),
    (
        "shape.resize",
        "ObjectAddress and dimensions in mm.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"width_mm":40,"height_mm":20}"##,
    ),
    (
        "equation.set",
        "ObjectAddress and options:{script,size_pt}.",
        r##"{"address":{"section":0,"paragraph":0,"control":0},"options":{"script":"x^2","size_pt":12}}"##,
    ),
    (
        "insert.shape",
        "kind rectangle/ellipse/line/textbox/arc/polygon/curve, dimensions mm, text required.",
        r##"{"kind":"rectangle","width_mm":40,"height_mm":20,"text":""}"##,
    ),
    (
        "edit.sort",
        "key text/number, descending and case_sensitive booleans. Selected body paragraphs or whole section.",
        r##"{"key":"text","descending":false,"case_sensitive":false}"##,
    ),
    (
        "table.edit",
        "action: row_above,row_below,column_left,column_right,delete_row,delete_column,fit_page,merge_right,split_merged. Select a cell first.",
        r##"{"action":"row_below"}"##,
    ),
    (
        "insert.symbol",
        "문자표: text (1–1000 characters, no line breaks) inserted at the caret; optional font applied to them only.",
        r##"{"text":"★","font":"Segoe UI Symbol"}"##,
    ),
    (
        "symbol.list",
        "set id from symbol.sets (hnc.*, ks.*, u.<hex>, emoji.*, special); start offset, limit 1–4096 (default 256).",
        r##"{"set":"hnc.circled","start":0,"limit":64}"##,
    ),
    (
        "hyperlink.list",
        "No parameters; links of the caret paragraph with fieldId/start/end/text/uri.",
        r##"{}"##,
    ),
    (
        "hyperlink.remove",
        "field_id from hyperlink.list; restore_format puts back the colour/underline the link covered.",
        r##"{"field_id":1,"restore_format":true}"##,
    ),
    ("edit.undo", "No parameters.", r##"{}"##),
    ("edit.redo", "No parameters.", r##"{}"##),
    ("text.backspace", "No parameters.", r##"{}"##),
    ("text.delete", "No parameters.", r##"{}"##),
    ("paragraph.split", "No parameters.", r##"{}"##),
    ("insert.page_break", "No parameters; body only.", r##"{}"##),
    (
        "insert.column_break",
        "No parameters; body only.",
        r##"{}"##,
    ),
    (
        "insert.page_number",
        "No parameters; centred page number in a new footer of the caret section.",
        r##"{}"##,
    ),
];

impl Session {
    /// The CLI and automation share the exact desktop editing transactions.
    /// File destinations stay at the application boundary, never in a document command.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value> {
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
                }
                let p: Edit = serde_json::from_value(params)?;
                self.edit_table(p.action)?;
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
                let mut current = serde_json::to_value(self.page_setup()?)?;
                let target = current
                    .as_object_mut()
                    .context("편집 용지 정보를 읽지 못했습니다.")?;
                for (key, value) in object {
                    if !target.contains_key(key) {
                        bail!("알 수 없는 편집 용지 항목: {key}");
                    }
                    target.insert(key.clone(), value.clone());
                }
                Command::PageSetup(serde_json::from_value(current)?)
            }
            // Without `options` the legacy exact (case-sensitive) replacement is kept.
            "edit.replace_all" => {
                let r: Replace = serde_json::from_value(params)?;
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
        self.execute(command)?;
        Ok(self.info())
    }
}
