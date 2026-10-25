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
