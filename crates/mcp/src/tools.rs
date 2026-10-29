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
            "Run 1–1000 commands sequentially. Stops at first failure unless keepGoing=true; any failure sets isError=true. atomic=true rolls every document change back when any command fails (keepGoing is then ignored); otherwise prior successes remain.",
            object(
                json!({"commands":{"type":"array","minItems":1,"maxItems":1000,"items":command},"keepGoing":boolean,"atomic":boolean}),
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
            "open_protected_document",
            "Open a password-protected HWP/HWPX/HWP3 file without a dialog. Wrong passwords and failed loads preserve the current document. Unsaved changes require force=true. The password is not returned.",
            object(
                json!({"path":text,"password":{"type":"string","minLength":1},"force":boolean}),
                &["path", "password"]
            ),
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
            "hwp_action",
            "Run a Hangul automation action by its public name (e.g. MoveLineEnd, CharShapeBold, DeleteWord). Default parameters from the mapping table are merged under params. Omit id to list actions (query filters).",
            object(
                json!({"id":text,"params":{"type":"object","default":{}},"parameterSet":{"type":"object"},"query":text}),
                &[]
            ),
            false
        ),
        tool(
            "get_selection",
            "Read the caret, anchor and selected text (cursor section/paragraph/offset/cell, Unicode scalar offsets).",
            object(json!({}), &[]),
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
    if !backend.usable {
        return result(
            json!({"error":"복원에 실패하여 닫힌 문서입니다. 새 문서를 열어 작업하세요."}),
            true,
        );
    }
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
                #[serde(default)]
                atomic: bool,
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
            if p.atomic
                && p.commands.iter().any(|c| {
                    matches!(
                        c.command.as_str(),
                        "file.new"
                            | "file.open"
                            | "file.open_protected"
                            | "file.save"
                            | "file.save_as"
                            | "file.pdf"
                            | "file.save_hml"
                            | "file.save_pages"
                            | "file.print_pdf"
                            | "file.save_block"
                            | "picture.save"
                            | "picture.save_all"
                            | "tools.autotext.add"
                            | "tools.autotext.delete"
                            | "tools.autotext.body.add"
                            | "tools.autotext.body.delete"
                    ) || (c.command == "format.style.export"
                        && c.params.get("path").is_some_and(|path| !path.is_null()))
                })
            {
                bail!("atomic batch에는 파일 또는 설정 쓰기 명령을 넣을 수 없습니다.");
            }
            let checkpoint = p.atomic.then(|| b.session.full_checkpoint()).transpose()?;
            // Beyond the list above, any write a command attempts is refused while atomic.
            let _no_writes = p.atomic.then(hwpcraft_formats::file_access::forbid_writes);
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
                        if !p.keep_going || p.atomic {
                            break;
                        }
                    }
                }
            }
            let mut rolled_back = false;
            if let Some(checkpoint) = checkpoint {
                if failed {
                    crate::atomic_batch::rollback(b, checkpoint)?;
                    rolled_back = true;
                } else {
                    crate::atomic_batch::release(b, checkpoint)?;
                }
            }
            return Ok(result(
                json!({"results":results,"rolledBack":rolled_back}),
                failed,
            ));
        }
        "hwp_action" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Action {
                #[serde(default)]
                id: Option<String>,
                #[serde(default = "empty")]
                params: Value,
                #[serde(default)]
                query: String,
                #[serde(default, rename = "parameterSet")]
                parameter_set: Option<Value>,
            }
            let p: Action = serde_json::from_value(args)?;
            let Some(id) = p.id else {
                return Ok(result(catalog::actions(&p.query), false));
            };
            let prepared = b
                .session
                .prepare_hwp_action(&id, p.params, p.parameter_set)?;
            let value = b.execute(prepared.command, prepared.params)?;
            json!({"action":id,"command":prepared.command,"mapping":prepared.mapping,"result":value})
        }
        "get_selection" => b.execute("select.get", args)?,
        "new_document" => b.execute("file.new", args)?,
        "open_document" => b.execute("file.open", args)?,
        "open_protected_document" => b.execute("file.open_protected", args)?,
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
