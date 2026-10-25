use crate::{Headless, catalog};
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn tool(name: &str, description: &str, schema: Value, readonly: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":schema,
        "annotations":{"readOnlyHint":readonly,"destructiveHint":!readonly,"openWorldHint":false}})
}
pub fn tool_definitions() -> Value {
    let text = json!({"type":"string"});
    let boolean = json!({"type":"boolean"});
    let window = object(
        json!({"start":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":1000,"default":100}}),
        &[],
    );
    let command = object(
        json!({"command":text,"params":{"type":"object","default":{}}}),
        &["command"],
    );
    json!([
        tool(
            "list_commands",
            "Search command IDs and Korean labels; returns parameter examples and help. Query is case-insensitive.",
            object(json!({"query":text}), &[]),
            true
        ),
        tool(
            "execute",
            "Run a shared engine command, e.g. format.character {bold:true} or insert.table {rows:2,columns:3}. File commands use paths, never dialogs.",
            command.clone(),
            false
        ),
        tool(
            "batch",
            "Run 1–1000 commands sequentially. Prior successes remain on failure. Stops at first failure unless keepGoing=true; any failure sets isError=true. Not a transaction.",
            object(
                json!({"commands":{"type":"array","minItems":1,"maxItems":1000,"items":command},"keepGoing":boolean}),
                &["commands"]
            ),
            false
        ),
        tool(
            "new_document",
            "Create a blank HWP document; refuses unsaved changes unless force=true.",
            object(json!({"force":boolean}), &[]),
            false
        ),
        tool(
            "open_document",
            "Open an HWP/HWPX file. Failed load preserves the current document. Unsaved changes require force=true.",
            object(json!({"path":text,"force":boolean}), &["path"]),
            false
        ),
        tool(
            "save_document",
            "Save HWP/HWPX or export PDF/TXT, by path extension. Omit path to save to the current HWP/HWPX path. Exports do not clear dirty state.",
            object(json!({"path":text}), &[]),
            false
        ),
        tool(
            "type_text",
            "Insert text at the caret, replacing selection. Newlines create paragraphs. Maximum 1,000,000 UTF-8 bytes per edit.",
            object(json!({"text":text}), &["text"]),
            false
        ),
        tool(
            "select_text",
            "Select the nth non-overlapping, case-sensitive match within a body/table-cell paragraph; occurrence is 1-based. Unicode scalar offsets.",
            object(
                json!({"text":text,"occurrence":{"type":"integer","minimum":1,"default":1}}),
                &["text"]
            ),
            false
        ),
        tool(
            "get_text",
            "Read body and first-level table-cell text. Follow nextStart until null; notes/textboxes use specialized commands.",
            window.clone(),
            true
        ),
        tool(
            "inspect_document",
            "Read summary, selection and paginated paragraphs with style/shape references. charShapes retain RHWP native positions; Cursor offsets use Unicode scalars.",
            window,
            true
        ),
        tool(
            "render_page",
            "Return a page as inline PNG, with no temporary file. Page is 1-based. Scale is pixels per SVG unit, 0.25–2.0.",
            object(
                json!({"page":{"type":"integer","minimum":1,"default":1},"scale":{"type":"number","minimum":0.25,"maximum":2.0,"default":1.0}}),
                &[]
            ),
            true
        )
    ])
}
pub(crate) fn known_tool(name: &str) -> bool {
    matches!(
        name,
        "list_commands"
            | "execute"
            | "batch"
            | "new_document"
            | "open_document"
            | "save_document"
            | "type_text"
            | "select_text"
            | "get_text"
            | "inspect_document"
            | "render_page"
    )
}
pub(crate) fn result(value: Value, is_error: bool) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"isError":is_error})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Execute {
    command: String,
    #[serde(default = "empty")]
    params: Value,
}
fn empty() -> Value {
    json!({})
}

pub(crate) fn call(backend: &mut Headless, name: &str, args: Value) -> Value {
    match answer(backend, name, args) {
        Ok(value) => value,
        Err(e) => json!({"content":[{"type":"text","text":format!("{e:#}")}],"isError":true}),
    }
}
fn answer(b: &mut Headless, name: &str, args: Value) -> Result<Value> {
    let value = match name {
        "list_commands" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Query {
                #[serde(default)]
                query: String,
            }
            catalog::commands(&serde_json::from_value::<Query>(args)?.query)
        }
        "execute" => {
            let p: Execute = serde_json::from_value(args)?;
            b.execute(&p.command, p.params)?
        }
        "batch" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Batch {
                commands: Vec<Execute>,
                #[serde(default, rename = "keepGoing")]
                keep_going: bool,
            }
            let p: Batch = serde_json::from_value(args)?;
            if p.commands.is_empty() || p.commands.len() > 1000 {
                bail!("batch는 1–1000개 명령이어야 합니다.");
            }
            if p.commands
                .iter()
                .any(|c| !c.params.is_object() || c.command.is_empty())
            {
                bail!("각 명령에는 command와 객체 params가 필요합니다.");
            }
            let mut results = Vec::new();
            let mut failed = false;
            for c in p.commands {
                match b.execute(&c.command, c.params) {
                    Ok(value) => {
                        results.push(json!({"command":c.command,"ok":true,"result":value}))
                    }
                    Err(e) => {
                        failed = true;
                        results
                            .push(json!({"command":c.command,"ok":false,"error":format!("{e:#}")}));
                        if !p.keep_going {
                            break;
                        }
                    }
                }
            }
            return Ok(result(json!({"results":results}), failed));
        }
        "new_document" => b.execute("file.new", args)?,
        "open_document" => b.execute("file.open", args)?,
        "save_document" => b.execute("file.save", args)?,
        "type_text" => b.execute("text.insert", args)?,
        "select_text" => b.execute("select.text", args)?,
        "get_text" => b.execute("document.text", args)?,
        "inspect_document" => b.execute("document.inspect", args)?,
        "render_page" => {
            #[derive(Deserialize)]
            #[serde(default, deny_unknown_fields)]
            struct Render {
                page: u32,
                scale: f32,
            }
            impl Default for Render {
                fn default() -> Self {
                    Self {
                        page: 1,
                        scale: 1.0,
                    }
                }
            }
            let p: Render = serde_json::from_value(args)?;
            return b.render(p.page, p.scale);
        }
        _ => bail!("알 수 없는 도구: {name}"),
    };
    Ok(result(value, false))
}
