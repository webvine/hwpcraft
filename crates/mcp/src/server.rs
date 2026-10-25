//! MCP lifecycle and bounded newline-delimited JSON-RPC 2.0 framing.
use crate::{Headless, catalog, tools};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const VERSIONS: &[&str] = &[PROTOCOL_VERSION];
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

pub struct Server {
    backend: Headless,
    negotiated: bool,
    initialized: bool,
}
fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message.into()}})
}
impl Server {
    pub fn new(backend: Headless) -> Self {
        Self {
            backend,
            negotiated: false,
            initialized: false,
        }
    }

    pub fn serve(
        &mut self,
        mut input: impl BufRead,
        mut output: impl Write,
    ) -> std::io::Result<()> {
        loop {
            let mut bytes = Vec::new();
            let count = input
                .by_ref()
                .take((MAX_REQUEST_BYTES + 1) as u64)
                .read_until(b'\n', &mut bytes)?;
            if count == 0 {
                return Ok(());
            }
            let oversized = bytes.len() > MAX_REQUEST_BYTES;
            let reply = if oversized {
                Some(error(Value::Null, -32600, "request exceeds 4 MiB; closing input").to_string())
            } else {
                match std::str::from_utf8(&bytes) {
                    Ok(line) => self.handle_line(line),
                    Err(_) => Some(error(Value::Null, -32700, "invalid UTF-8").to_string()),
                }
            };
            if let Some(reply) = reply {
                output.write_all(reply.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
            if oversized {
                return Ok(());
            }
        }
    }

    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        if line.trim().is_empty() {
            return None;
        }
        let reply = if line.len() > MAX_REQUEST_BYTES {
            Some(error(Value::Null, -32600, "request exceeds 4 MiB"))
        } else {
            match serde_json::from_str::<Value>(line) {
                Ok(value) => self.handle(value),
                Err(e) => Some(error(Value::Null, -32700, format!("parse error: {e}"))),
            }
        };
        reply.map(|reply| {
            let encoded = reply.to_string();
            if encoded.len() > MAX_RESPONSE_BYTES {
                error(
                    reply["id"].clone(),
                    -32603,
                    "response exceeds 32 MiB; use a smaller document window or render scale",
                )
                .to_string()
            } else {
                encoded
            }
        })
    }

    fn handle(&mut self, value: Value) -> Option<Value> {
        let Some(o) = value.as_object() else {
            return Some(error(
                Value::Null,
                -32600,
                "expected a JSON-RPC object; protocol batches are not supported",
            ));
        };
        let id = o.get("id").cloned();
        let valid_id = id
            .as_ref()
            .filter(|v| v.is_string() || v.as_i64().is_some() || v.as_u64().is_some())
            .cloned();
        if o.get("jsonrpc") != Some(&json!("2.0")) || (id.is_some() && valid_id.is_none()) {
            return Some(error(
                valid_id.unwrap_or(Value::Null),
                -32600,
                "invalid JSON-RPC version or id",
            ));
        }
        let Some(method) = o.get("method").and_then(Value::as_str) else {
            // Responses to server requests are not used by this server.
            if o.contains_key("result") || o.contains_key("error") {
                return None;
            }
            return Some(error(
                valid_id.unwrap_or(Value::Null),
                -32600,
                "missing method",
            ));
        };
        let params = o.get("params").cloned().unwrap_or(json!({}));
        let Some(id) = valid_id else {
            if params.is_object() && method == "notifications/initialized" && self.negotiated {
                self.initialized = true;
            }
            return None;
        };
        if !params.is_object() {
            return Some(error(id, -32602, "params must be an object"));
        }
        Some(match self.request(method, params) {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err((code, message)) => error(id, code, message),
        })
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, (i64, String)> {
        if method == "initialize" {
            if self.negotiated {
                return Err((-32600, "already initialized".into()));
            }
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .ok_or((-32602, "protocolVersion is required".into()))?;
            let version = if VERSIONS.contains(&asked) {
                asked
            } else {
                PROTOCOL_VERSION
            };
            self.negotiated = true;
            return Ok(
                json!({"protocolVersion":version,"capabilities":{"tools":{},"resources":{}},
                "serverInfo":{"name":"hwpcraft","title":"HwpCraft","version":hwpcraft_engine::VERSION},
                "instructions":"HWP/HWPX editor using RHWP, headless. Use list_commands to discover IDs and parameter examples, execute or batch to edit. Cursor section/paragraph/offset/cell indexes are zero-based; offset counts Unicode scalar values, not UTF-8 bytes or UTF-16 units. render_page page is one-based. type_text splits newlines into paragraphs. get_text/inspect_document cover body and first-level table cells, paginated by start/limit; follow nextStart. Read objects via object.list and specialized get commands. New/open refuse unsaved changes unless force:true. Save explicitly to HWP/HWPX; PDF/TXT are exports. Batch is sequential and not atomic. No desktop UI, remote bridge, tracking, or Hancom COM control."}),
            );
        }
        if method == "ping" {
            return Ok(json!({}));
        }
        if !self.initialized {
            return Err((
                -32000,
                "initialize and notifications/initialized are required".into(),
            ));
        }
        match method {
            "tools/list" => Ok(json!({"tools":tools::tool_definitions()})),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or((-32602, "missing tool name".into()))?;
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !args.is_object() {
                    return Err((-32602, "arguments must be an object".into()));
                }
                if !tools::known_tool(name) {
                    return Err((-32602, format!("unknown tool: {name}")));
                }
                Ok(tools::call(&mut self.backend, name, args))
            }
            "resources/list" => Ok(json!({"resources":[
                {"uri":"hwpcraft://document","name":"document","description":"First 100 body/table-cell paragraphs; use inspect_document for subsequent windows","mimeType":"application/json"},
                {"uri":"hwpcraft://commands","name":"commands","description":"Command IDs, labels, shortcuts and parameter examples","mimeType":"application/json"}
            ]})),
            "resources/templates/list" => Ok(json!({"resourceTemplates":[]})),
            "resources/read" => {
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or((-32602, "missing uri".into()))?;
                let data = match uri {
                    "hwpcraft://document" => self
                        .backend
                        .execute("document.inspect", json!({}))
                        .map_err(|e| (-32603, e.to_string()))?,
                    "hwpcraft://commands" => catalog::commands(""),
                    _ => return Err((-32002, format!("resource not found: {uri}"))),
                };
                Ok(
                    json!({"contents":[{"uri":uri,"mimeType":"application/json","text":data.to_string()}]}),
                )
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        }
    }
}
