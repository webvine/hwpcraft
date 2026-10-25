//! Actual CLI wire checks. No libtest stdout prefix is skipped and no legacy
//! initialize handshake is sent. Every child and all its files are test-owned.
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const VERSION: &str = "2026-07-28";
const DEADLINE: Duration = Duration::from_secs(30);
const MAX_LINE: u64 = 32 * 1024 * 1024;

enum Wire {
    Message(Value, String),
    Invalid(String),
    Eof,
}

struct Pending {
    token: Option<Value>,
    last_progress: Option<f64>,
    canceled: bool,
}

struct Cli {
    process: Child,
    input: Option<ChildStdin>,
    events: mpsc::Receiver<Wire>,
    readers: Vec<JoinHandle<()>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    pending: HashMap<String, Pending>,
    responses: HashMap<String, Value>,
    anonymous_errors: Vec<Value>,
    allow_anonymous_errors: bool,
    transcript: Vec<Value>,
    client_wire: Vec<String>,
    server_wire: Vec<String>,
    root: std::path::PathBuf,
    next_id: u64,
    deadline: Instant,
    finished: bool,
}

fn meta() -> Value {
    json!({"io.modelcontextprotocol/protocolVersion":VERSION,"io.modelcontextprotocol/clientCapabilities":{}})
}

fn key(id: &Value) -> Result<String> {
    if !(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()) {
        bail!("invalid response ID: {id}");
    }
    Ok(id.to_string()) // preserves number 1 versus string "1"
}

impl Cli {
    fn start(root: &Path, confined: bool) -> Result<Self> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"));
        command.arg("mcp");
        if confined {
            command.args(["--automation-read-root"]).arg(root);
            command.args(["--automation-write-root"]).arg(root);
        }
        let mut process = command
            .env("HWPCRAFT_CONFIG_DIR", root.join("config"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("launch actual hwpcraft-cli mcp binary")?;
        let input = process.stdin.take().context("child stdin")?;
        let stdout = process.stdout.take().context("child stdout")?;
        let stderr_pipe = process.stderr.take().context("child stderr")?;
        let (send, events) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            for _ in 0..4096 {
                let mut bytes = Vec::new();
                match reader
                    .by_ref()
                    .take(MAX_LINE + 1)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) => {
                        let _ = send.send(Wire::Eof);
                        return;
                    }
                    Ok(_) if bytes.len() as u64 > MAX_LINE => {
                        let _ = send.send(Wire::Invalid("stdout JSON line exceeds 32 MiB".into()));
                        return;
                    }
                    Ok(_) => match serde_json::from_slice::<Value>(&bytes) {
                        Ok(value) if value.is_object() && value["jsonrpc"] == "2.0" => {
                            if send
                                .send(Wire::Message(
                                    value,
                                    String::from_utf8_lossy(&bytes).into_owned(),
                                ))
                                .is_err()
                            {
                                return;
                            }
                        }
                        _ => {
                            let _ = send.send(Wire::Invalid(format!(
                                "non-JSON-RPC stdout line: {}",
                                String::from_utf8_lossy(&bytes[..bytes.len().min(256)])
                            )));
                            return;
                        }
                    },
                    Err(error) => {
                        let _ = send.send(Wire::Invalid(error.to_string()));
                        return;
                    }
                }
            }
            let _ = send.send(Wire::Invalid("stdout message bound exceeded".into()));
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&stderr);
        let stderr_reader = thread::spawn(move || {
            let mut pipe = stderr_pipe;
            let mut chunk = [0; 4096];
            while let Ok(count) = pipe.read(&mut chunk) {
                if count == 0 {
                    break;
                }
                if let Ok(mut bytes) = captured.lock() {
                    let remaining = (2 * 1024 * 1024_usize).saturating_sub(bytes.len());
                    bytes.extend_from_slice(&chunk[..count.min(remaining)]);
                }
            }
        });
        Ok(Self {
            process,
            input: Some(input),
            events,
            readers: vec![stdout_reader, stderr_reader],
            stderr,
            pending: HashMap::new(),
            responses: HashMap::new(),
            anonymous_errors: Vec::new(),
            allow_anonymous_errors: false,
            transcript: Vec::new(),
            client_wire: Vec::new(),
            server_wire: Vec::new(),
            root: root.to_owned(),
            next_id: 100,
            deadline: Instant::now() + DEADLINE,
            finished: false,
        })
    }

    fn send(&mut self, value: Value) -> Result<()> {
        self.transcript
            .push(json!({"direction":"client","message":value}));
        self.raw(&(value.to_string() + "\n"))
    }

    fn raw(&mut self, line: &str) -> Result<()> {
        self.client_wire.push(line.to_owned());
        let input = self.input.as_mut().context("stdin already closed")?;
        input.write_all(line.as_bytes())?;
        input.flush()?;
        Ok(())
    }

    fn begin(
        &mut self,
        id: Value,
        method: &str,
        mut params: Value,
        token: Option<Value>,
    ) -> Result<()> {
        let k = key(&id)?;
        if self.pending.contains_key(&k) {
            bail!("duplicate active request ID");
        }
        let mut metadata = meta();
        if let Some(token) = &token {
            metadata["progressToken"] = token.clone();
        }
        params
            .as_object_mut()
            .context("params must be object")?
            .insert("_meta".into(), metadata);
        self.pending.insert(
            k,
            Pending {
                token,
                last_progress: None,
                canceled: false,
            },
        );
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
    }

    fn receive(&mut self) -> Result<bool> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .context("30-second wire deadline exceeded")?;
        match self
            .events
            .recv_timeout(remaining)
            .context("stdout EOF/deadline while awaiting message")?
        {
            Wire::Invalid(message) => bail!("{message}"),
            Wire::Eof => Ok(false),
            Wire::Message(message, raw) => {
                self.server_wire.push(raw);
                self.transcript
                    .push(json!({"direction":"server","message":message}));
                if message.get("id").is_none_or(Value::is_null) && message.get("error").is_some() {
                    if !self.allow_anonymous_errors {
                        bail!("unsolicited anonymous protocol error: {message}");
                    }
                    self.anonymous_errors.push(message);
                } else if let Some(id) = message.get("id") {
                    if message.get("method").is_some() {
                        bail!("modern server sent a prohibited request: {message}");
                    }
                    let k = key(id)?;
                    let pending = self
                        .pending
                        .get(&k)
                        .context("unsolicited or duplicate response ID")?;
                    if pending.canceled {
                        return Ok(true);
                    } // completion/cancel race, retained in transcript
                    if message.get("result").is_some() == message.get("error").is_some() {
                        bail!("response must have exactly one of result/error");
                    }
                    if let Some(result) = message.get("result") {
                        assert_eq!(result["resultType"], "complete", "{message}");
                    }
                    self.pending.remove(&k);
                    if self.responses.insert(k, message).is_some() {
                        bail!("duplicate response");
                    }
                } else if message["method"] == "notifications/progress" {
                    let token = &message["params"]["progressToken"];
                    let active = self
                        .pending
                        .values_mut()
                        .find(|p| !p.canceled && p.token.as_ref() == Some(token))
                        .context("progress token is not active")?;
                    let value = message["params"]["progress"]
                        .as_f64()
                        .filter(|p| p.is_finite())
                        .context("progress must be finite numeric")?;
                    if active
                        .last_progress
                        .is_some_and(|previous| value <= previous)
                    {
                        bail!("progress did not increase");
                    }
                    active.last_progress = Some(value);
                } else if message["method"] == "notifications/cancelled" {
                    bail!("server canceled a non-subscription operation");
                } else if message["method"].as_str().is_none() {
                    bail!("invalid notification: {message}");
                }
                Ok(true)
            }
        }
    }

    fn wait(&mut self, id: &Value) -> Result<Value> {
        let k = key(id)?;
        loop {
            if let Some(response) = self.responses.remove(&k) {
                return Ok(response);
            }
            if !self.receive()? {
                bail!("server exited before response {id}");
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = json!(self.next_id);
        self.next_id += 1;
        self.begin(id.clone(), method, params, None)?;
        self.wait(&id)
    }

    fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let response = self.request(method, params)?;
        if response.get("error").is_some() {
            bail!("JSON-RPC error: {response}");
        }
        Ok(response["result"].clone())
    }

    fn exact_request(&mut self, id: Value, method: &str, params: Value) -> Result<Value> {
        self.pending.insert(
            key(&id)?,
            Pending {
                token: None,
                last_progress: None,
                canceled: false,
            },
        );
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        self.wait(&id)
    }

    fn malformed_error(&mut self) -> Result<()> {
        let before = self.anonymous_errors.len();
        self.allow_anonymous_errors = true;
        self.raw("{not-json}\n")?;
        while self.anonymous_errors.len() == before {
            if !self.receive()? {
                bail!("EOF after malformed JSON request");
            }
        }
        let response = &self.anonymous_errors[before];
        assert_eq!(response["error"]["code"], -32700, "{response}");
        assert!(
            response.get("id").is_none(),
            "modern parse errors omit an unreadable ID: {response}"
        );
        self.allow_anonymous_errors = false;
        Ok(())
    }

    fn tool(&mut self, name: &str, args: Value) -> Result<Value> {
        self.rpc("tools/call", json!({"name":name,"arguments":args}))
    }

    fn ok(&mut self, name: &str, args: Value) -> Result<Value> {
        let result = self.tool(name, args)?;
        if result["isError"] == true {
            bail!("tool {name} failed: {result}");
        }
        result
            .get("structuredContent")
            .filter(|v| v.is_object())
            .cloned()
            .context("tool result missing structuredContent object")
    }

    fn doc(&mut self) -> Result<String> {
        let data = self.ok("new_document", json!({}))?;
        Ok(data["document_id"]
            .as_str()
            .context("new document handle missing")?
            .to_owned())
    }

    fn execute(&mut self, id: &str, command: &str, params: Value) -> Result<Value> {
        self.ok(
            "execute",
            json!({"document_id":id,"command":command,"params":params}),
        )
    }

    fn text(&mut self, id: &str) -> Result<Value> {
        self.ok("get_text", json!({"document_id":id}))
    }

    fn finish(mut self) -> Result<()> {
        self.input.take(); // EOF is the portable graceful shutdown signal.
        while self.receive()? {}
        loop {
            if let Some(status) = self.process.try_wait()? {
                if !status.success() {
                    bail!("CLI failed: {status}; stderr={}", self.stderr_text());
                }
                break;
            }
            if Instant::now() >= self.deadline {
                bail!("CLI failed to exit after EOF");
            }
            thread::sleep(Duration::from_millis(5));
        }
        for reader in self.readers.drain(..) {
            reader
                .join()
                .map_err(|_| anyhow!("stdio reader panicked"))?;
        }
        self.capture("PASS")?;
        self.finished = true;
        Ok(())
    }

    fn capture(&self, status: &str) -> Result<()> {
        let wire = std::env::var_os("HWPCRAFT_WRITE_MCP_FIXTURES").as_deref()
            == Some(std::ffi::OsStr::new("1"));
        let report = std::env::var_os("HWPCRAFT_WRITE_MCP_REPORT_FIXTURES").as_deref()
            == Some(std::ffi::OsStr::new("1"))
            && self.root.join("report.png").is_file();
        if !wire && !report {
            return Ok(());
        }
        let name: String = thread::current()
            .name()
            .unwrap_or("unnamed")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let destination = if report {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../output/implementation/0.17/mcp-report-artifacts")
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../output/implementation/0.17/mcp-modern-wire")
                .join(name)
        };
        std::fs::create_dir_all(&destination)?;
        std::fs::write(
            destination.join("client.raw.jsonl"),
            self.client_wire.concat(),
        )?;
        std::fs::write(
            destination.join("server.raw.jsonl"),
            self.server_wire.concat(),
        )?;
        std::fs::write(
            destination.join("events.jsonl"),
            self.transcript
                .iter()
                .map(|value| value.to_string() + "\n")
                .collect::<String>(),
        )?;
        std::fs::write(destination.join("stderr.log"), self.stderr_text())?;
        std::fs::write(
            destination.join("receipt.json"),
            serde_json::to_vec_pretty(
                &json!({"status":status,"protocolVersion":VERSION,"binary":env!("CARGO_BIN_EXE_hwpcraft-cli"),"engineVersion":hwpcraft_engine::VERSION.trim(),"ownedPid":self.process.id(),"isolatedRoot":self.root,"serverJsonLines":self.server_wire.len(),"stdinEofCleanup":status=="PASS","activeCancellation":"NOT_VERIFIED_BY_CLI_NO_DETERMINISTIC_BARRIER","pdfValidation":"PDF structure and independent PNG nonblank only; PDF glyph parity is separate"}),
            )?,
        )?;
        for entry in std::fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ["hwp", "hwpx", "pdf", "png"].contains(&ext))
            {
                std::fs::copy(
                    &path,
                    destination.join(path.file_name().context("fixture file name")?),
                )?;
            }
        }
        Ok(())
    }

    fn stderr_text(&self) -> String {
        self.stderr
            .lock()
            .map(|data| String::from_utf8_lossy(&data).into_owned())
            .unwrap_or_else(|_| "stderr capture unavailable".into())
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.capture("FAILED_OR_EARLY_RETURN");
        }
        self.input.take();
        // Only the Child spawned above is ever terminated, never existing PIDs.
        if self.process.try_wait().ok().flatten().is_none() {
            let _ = self.process.kill();
        }
        let _ = self.process.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

fn assert_cached(value: &Value, scope: &str, ttl: u64) {
    assert_eq!(value["resultType"], "complete");
    assert_eq!(value["cacheScope"], scope);
    assert_eq!(value["ttlMs"], ttl);
}

fn failed(response: &Value) -> bool {
    response.get("error").is_some() || response["result"]["isError"] == true
}

#[test]
fn modern_discovery_resources_prompts_completion_and_cache_are_real_wire_results() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    let discovery = cli.rpc("server/discover", json!({}))?;
    assert!(
        discovery["supportedVersions"]
            .as_array()
            .is_some_and(|versions| versions.iter().any(|v| v == VERSION))
    );
    assert_eq!(discovery["resultType"], "complete");
    let tools = cli.rpc("tools/list", json!({}))?;
    assert_cached(&tools, "public", 60_000);
    let definitions = tools["tools"].as_array().context("tools list")?;
    for name in ["new_document", "execute", "render_page", "close_document"] {
        assert!(definitions.iter().any(|tool| tool["name"] == name));
    }
    let resources = cli.rpc("resources/list", json!({}))?;
    assert_cached(&resources, "public", 60_000);
    assert!(!resources.to_string().contains("document_id\":"));
    let templates = cli.rpc("resources/templates/list", json!({}))?;
    assert_cached(&templates, "public", 60_000);
    assert!(
        templates
            .to_string()
            .contains("hwpcraft://documents/{document_id}")
    );
    for uri in [
        "hwpcraft://commands",
        "hwpcraft://hwp-actions",
        "hwpcraft://capabilities",
    ] {
        let resource = cli.rpc("resources/read", json!({"uri":uri}))?;
        assert_cached(&resource, "public", 60_000);
        let data: Value = serde_json::from_str(
            resource["contents"][0]["text"]
                .as_str()
                .context("resource text")?,
        )?;
        assert!(data.is_object() || data.is_array());
    }
    let resource_document = cli.doc()?;
    let document_uri = format!("hwpcraft://documents/{resource_document}");
    let before = cli.text(&resource_document)?;
    for uri in ["hwpcraft://capabilities", document_uri.as_str()] {
        for continuation in [
            json!({"requestState":"unissued-state"}),
            json!({"inputResponses":{}}),
        ] {
            let mut params = continuation;
            params["uri"] = json!(uri);
            let response = cli.request("resources/read", params)?;
            assert_eq!(response["error"]["code"], -32602);
            assert!(response.get("result").is_none());
        }
    }
    assert_eq!(cli.text(&resource_document)?, before);
    assert_cached(
        &cli.rpc("resources/read", json!({"uri":document_uri}))?,
        "private",
        0,
    );
    assert_cached(
        &cli.rpc("resources/read", json!({"uri":"hwpcraft://capabilities"}))?,
        "public",
        60_000,
    );
    let prompts = cli.rpc("prompts/list", json!({}))?;
    assert_cached(&prompts, "public", 60_000);
    let title = "무인 보고서 😀\nJSON 작업 데이터";
    let prompt = cli.rpc(
        "prompts/get",
        json!({"name":"report-authoring","arguments":{"title":title,"format":"pdf"}}),
    )?;
    let data: Value = serde_json::from_str(
        prompt["messages"][1]["content"]["text"]
            .as_str()
            .context("prompt taskData")?,
    )?;
    assert_eq!(data["taskData"]["title"], title);
    let completion = cli.rpc("completion/complete", json!({"ref":{"type":"ref/prompt","name":"report-authoring"},"argument":{"name":"format","value":"hw"}}))?;
    assert_eq!(completion["completion"]["values"], json!(["hwpx", "hwp"]));
    let private_ids = cli.rpc("completion/complete", json!({"ref":{"type":"ref/resource","uri":"hwpcraft://documents/{document_id}"},"argument":{"name":"document_id","value":""}}))?;
    assert_eq!(private_ids["completion"]["values"], json!([]));
    cli.finish()
}

#[test]
fn explicit_handles_isolate_interleaved_documents_and_refuse_implicit_fallbacks() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    let a = cli.doc()?;
    let b = cli.doc()?;
    assert_ne!(a, b);
    cli.ok("type_text", json!({"document_id":a,"text":"첫 문서 😀"}))?;
    cli.ok("type_text", json!({"document_id":b,"text":"다른 문서 🦀"}))?;
    cli.begin(
        json!(1),
        "tools/call",
        json!({"name":"get_text","arguments":{"document_id":a}}),
        None,
    )?;
    cli.begin(
        json!("1"),
        "tools/call",
        json!({"name":"get_text","arguments":{"document_id":b}}),
        None,
    )?;
    let one = cli.wait(&json!(1))?;
    let other = cli.wait(&json!("1"))?;
    assert!(one.to_string().contains("첫 문서 😀"));
    assert!(!one.to_string().contains("다른 문서"));
    assert!(other.to_string().contains("다른 문서 🦀"));
    assert!(!other.to_string().contains("첫 문서"));
    for args in [json!({}), json!({"document_id":"missing-handle"})] {
        let response = cli.request("tools/call", json!({"name":"get_text","arguments":args}))?;
        assert!(
            failed(&response),
            "implicit/unknown document unexpectedly accepted: {response}"
        );
    }
    let resource = cli.rpc(
        "resources/read",
        json!({"uri":format!("hwpcraft://documents/{a}")}),
    )?;
    assert_cached(&resource, "private", 0);
    assert!(resource.to_string().contains(&a));
    assert!(!resource.to_string().contains(&b));
    assert!(cli.ok("list_commands", json!({"query":"selection.range"}))?["items"].is_array());
    assert!(cli.ok("hwp_action", json!({"query":"CharShapeBold"}))?["items"].is_array());
    let closed = cli.tool("close_document", json!({"document_id":a}))?;
    assert_eq!(closed["isError"], true, "dirty document close must fail");
    assert!(cli.text(&a)?.to_string().contains("첫 문서 😀"));
    cli.ok("close_document", json!({"document_id":a,"force":true}))?;
    let response = cli.request(
        "tools/call",
        json!({"name":"get_text","arguments":{"document_id":a}}),
    )?;
    assert!(failed(&response));
    assert!(cli.text(&b)?.to_string().contains("다른 문서 🦀"));
    cli.ok("close_document", json!({"document_id":b,"force":true}))?;
    cli.finish()
}

fn decode_base64(text: &str) -> Result<Vec<u8>> {
    let mut value = 0_u32;
    let mut bits = 0_u32;
    let mut output = Vec::with_capacity(text.len() * 3 / 4);
    let mut padding = false;
    for byte in text.bytes() {
        if byte == b'=' {
            padding = true;
            continue;
        }
        if padding {
            bail!("data after base64 padding");
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => bail!("invalid base64 byte"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((value >> bits) as u8);
        }
        value &= (1_u32 << bits) - 1;
    }
    if bits >= 6 || value != 0 {
        bail!("invalid base64 tail");
    }
    Ok(output)
}

#[test]
fn unattended_report_roundtrips_hwp_hwpx_and_produces_real_pdf_and_png() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    let id = cli.doc()?;
    cli.ok(
        "type_text",
        json!({"document_id":id,"text":"검증 보고서 😀\n자동 작성 본문\n종합 결론"}),
    )?;
    let geometry = cli.execute(&id, "caret.get", json!({}))?;
    cli.execute(&id, "selection.range", json!({"start":{"section":0,"paragraph":0,"offset":0},"end":{"section":0,"paragraph":1,"offset":3},"expected_generation":geometry["generation"]}))?;
    let action = cli.ok("hwp_action", json!({"document_id":id,"id":"CharShapeBold"}))?;
    assert_eq!(action["command"], "format.bold");
    cli.execute(&id, "select.clear", json!({}))?;
    let generation = cli.execute(&id, "caret.get", json!({}))?["generation"].clone();
    cli.execute(&id, "selection.range", json!({"start":{"section":0,"paragraph":2,"offset":5},"end":{"section":0,"paragraph":2,"offset":5},"expected_generation":generation}))?;
    cli.execute(
        &id,
        "memo.create",
        json!({"text":"보고서 검토 메모","author":"MCP 검증"}),
    )?;
    cli.execute(
        &id,
        "page.header_footer.text",
        json!({"header":true,"apply_to":"both","text":"자동 보고서 머리말","fields":[]}),
    )?;
    cli.execute(
        &id,
        "note.insert",
        json!({"endnote":false,"paragraphs":["검증 각주 내용"]}),
    )?;
    cli.execute(
        &id,
        "table.create",
        json!({"rows":2,"columns":2,"width_mm":100.0}),
    )?;
    cli.ok("type_text", json!({"document_id":id,"text":"표 셀 😀"}))?;
    let before = cli.text(&id)?.to_string();
    assert!(
        before.contains("검증 보고서 😀")
            && before.contains("자동 작성 본문")
            && before.contains("표 셀 😀")
    );
    let mut reopened = Vec::new();
    for extension in ["hwpx", "hwp"] {
        let path = root.path().join(format!("무인 보고서.{extension}"));
        cli.ok("save_document", json!({"document_id":id,"path":path}))?;
        let bytes = std::fs::read(&path)?;
        if extension == "hwp" {
            assert!(bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]));
        } else {
            assert!(bytes.starts_with(b"PK"));
        }
        let opened = cli.ok("open_document", json!({"path":path}))?;
        let new_id = opened["document_id"]
            .as_str()
            .context("reopened document handle")?
            .to_owned();
        assert_ne!(new_id, id);
        let text = cli.text(&new_id)?.to_string();
        for expected in ["검증 보고서 😀", "자동 작성 본문", "종합 결론", "표 셀 😀"]
        {
            assert!(
                text.contains(expected),
                "{extension} lost {expected}: {text}"
            );
        }
        assert!(
            cli.execute(&new_id, "memo.list", json!({}))?
                .to_string()
                .contains("보고서 검토 메모")
        );
        assert!(
            cli.execute(
                &new_id,
                "page.header_footer.text.get",
                json!({"header":true,"apply_to":"both"})
            )?
            .to_string()
            .contains("자동 보고서 머리말")
        );
        assert!(
            cli.execute(&new_id, "context.v2.list", json!({"limit":1000}))?
                .to_string()
                .contains("footnote")
        );
        reopened.push(new_id);
    }
    let png = cli.tool(
        "render_page",
        json!({"document_id":id,"page":1,"scale":0.5}),
    )?;
    assert_ne!(png["isError"], true);
    let blocks = png["content"].as_array().context("render content")?;
    let image = blocks
        .iter()
        .find(|block| block["type"] == "image")
        .context("inline PNG block")?;
    assert_eq!(image["mimeType"], "image/png");
    let bytes = decode_base64(image["data"].as_str().context("PNG base64")?)?;
    std::fs::write(root.path().join("report.png"), &bytes)?;
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let raster = image::load_from_memory(&bytes)?.to_rgba8();
    assert!(raster.width() > 100 && raster.height() > 100);
    assert!(
        raster
            .pixels()
            .filter(|pixel| pixel.0[3] > 0 && pixel.0[..3].iter().any(|channel| *channel < 180))
            .count()
            > 30,
        "blank report raster"
    );
    let pdf = root.path().join("무인 보고서.pdf");
    cli.ok("save_document", json!({"document_id":id,"path":pdf}))?;
    let pdf_bytes = std::fs::read(pdf)?;
    assert!(pdf_bytes.starts_with(b"%PDF-"));
    assert!(pdf_bytes.windows(5).any(|window| window == b"%%EOF"));
    assert!(pdf_bytes.len() > 1000);
    // PDF structure is checked here; glyph-level PDF extraction/raster parity is
    // a separate validation stage, never claimed by this magic/EOF assertion.
    for handle in reopened.into_iter().chain(std::iter::once(id)) {
        cli.ok("close_document", json!({"document_id":handle}))?;
    }
    cli.finish()
}

#[test]
fn confined_roots_and_atomic_failures_preserve_document_and_existing_files() -> Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    let id = cli.doc()?;
    cli.ok("type_text", json!({"document_id":id,"text":"원본 보존 😀"}))?;
    let saved = root.path().join("existing.hwpx");
    cli.ok("save_document", json!({"document_id":id,"path":saved}))?;
    let original_file = std::fs::read(&saved)?;
    let original = cli.text(&id)?;
    let failed_batch = cli.tool("batch", json!({"document_id":id,"atomic":true,"commands":[{"command":"text.insert","params":{"text":"ROLLBACK-ME"}},{"command":"not.a.command","params":{}}]}))?;
    assert_eq!(failed_batch["isError"], true);
    assert_eq!(cli.text(&id)?, original);
    assert_eq!(std::fs::read(&saved)?, original_file);
    let outside_path = outside.path().join("must-not-overwrite.hwpx");
    std::fs::write(&outside_path, b"existing external sentinel")?;
    for (name, args) in [
        (
            "save_document",
            json!({"document_id":id,"path":outside_path}),
        ),
        (
            "open_document",
            json!({"document_id":id,"path":outside_path,"force":true}),
        ),
    ] {
        assert!(failed(
            &cli.request("tools/call", json!({"name":name,"arguments":args}))?
        ));
        assert_eq!(cli.text(&id)?, original);
        assert_eq!(std::fs::read(&outside_path)?, b"existing external sentinel");
    }
    let atomic_write = cli.tool("batch", json!({"document_id":id,"atomic":true,"commands":[{"command":"text.insert","params":{"text":"PREVENTED"}},{"command":"file.save","params":{"path":saved}}]}))?;
    assert_eq!(atomic_write["isError"], true);
    assert_eq!(cli.text(&id)?, original);
    assert_eq!(std::fs::read(saved)?, original_file);
    cli.ok("close_document", json!({"document_id":id}))?;
    cli.finish()
}

#[test]
fn modern_metadata_version_schema_and_syntax_errors_leave_stdout_pure_and_usable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    // The SDK's default decoder discards syntax errors for interoperability;
    // HwpCraft's strict framing must answer and recover on this actual channel.
    cli.malformed_error()?;
    assert_cached(&cli.rpc("tools/list", json!({}))?, "public", 60_000);
    for params in [
        json!({}),
        json!({"_meta":{"io.modelcontextprotocol/protocolVersion":VERSION}}),
        json!({"_meta":{"io.modelcontextprotocol/clientCapabilities":{}}}),
    ] {
        let response = cli.exact_request(json!("missing-meta"), "tools/list", params)?;
        assert_eq!(response["error"]["code"], -32602, "{response}");
    }
    let unsupported = cli.exact_request(json!("version"), "tools/list", json!({"_meta":{"io.modelcontextprotocol/protocolVersion":"1900-01-01","io.modelcontextprotocol/clientCapabilities":{}}}))?;
    assert_eq!(unsupported["error"]["code"], -32022, "{unsupported}");
    assert!(unsupported["error"]["data"]["supported"].is_array());
    for (method, params) in [
        ("not/a/method", json!({})),
        ("resources/read", json!({"uri":"hwpcraft://unknown"})),
        (
            "tools/call",
            json!({"name":"new_document","arguments":{"force":"wrong-type"}}),
        ),
    ] {
        let response = cli.request(method, params)?;
        assert!(response.get("error").is_some(), "{response}");
        assert_ne!(response["error"]["code"], -32002);
        assert_ne!(response["error"]["code"], -32042);
    }
    cli.malformed_error()?;
    assert_cached(&cli.rpc("tools/list", json!({}))?, "public", 60_000);
    cli.finish()
}

#[test]
fn progress_and_cancellation_notifications_are_correlated_without_false_active_cancel_claims()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    let id = cli.doc()?;
    cli.send(json!({"jsonrpc":"2.0","method":"notifications/example-unknown","params":{"text":"escaped\n한글 😀"}}))?;
    cli.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"unknown","reason":"test robustness"}}))?;
    for (request_id, token) in [
        (json!("with-string-token"), json!("job-한글")),
        (json!("with-int-token"), json!(17)),
    ] {
        cli.begin(
            request_id.clone(),
            "tools/call",
            json!({"name":"get_text","arguments":{"document_id":id}}),
            Some(token),
        )?;
        let response = cli.wait(&request_id)?;
        assert!(!failed(&response));
        // Progress is optional in the protocol. If sent, receive() has checked
        // the active token, strict increase and completion boundary.
        cli.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":request_id,"reason":"completion already observed"}}))?;
    }
    assert!(cli.text(&id)?.is_object());
    cli.ok("close_document", json!({"document_id":id}))?;
    // This proves unknown/late cancel robustness, not in-flight cancellation.
    // Real active cancellation is covered by deterministic worker checkpoints;
    // the CLI exposes no debug barrier and this test does not invent one.
    cli.finish()
}

#[test]
fn native_chart_authoring_via_mcp_preserves_state_and_roundtrips_real_doughnut_output() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let mut cli = Cli::start(root.path(), true)?;
    let id = cli.doc()?;
    cli.ok(
        "type_text",
        json!({"document_id":id,"text":"차트 보고서 😀\n기존 본문 유지"}),
    )?;
    let definition = |kind: &str, title: &str| json!({"kind":kind,"title":title,"categories":["전반기","후반기"],"series":[{"name":"매출","values":[4,6],"color":"#4472C4"}],"width_mm":120,"height_mm":75,"legend":false});
    let generation = cli.execute(&id, "caret.get", json!({}))?["generation"].clone();
    cli.execute(&id,"selection.range",json!({"start":{"section":0,"paragraph":0,"offset":0},"end":{"section":0,"paragraph":0,"offset":2},"expected_generation":generation}))?;
    let selected = cli.ok("get_selection", json!({"document_id":id}))?;
    let original_text = cli.text(&id)?;
    let refused=cli.tool("execute",json!({"document_id":id,"command":"chart.create","params":{"chart":{"kind":"column","categories":[],"series":[]}}}))?;
    assert_eq!(refused["isError"], true);
    assert_eq!(
        cli.ok("get_selection", json!({"document_id":id}))?,
        selected
    );
    assert_eq!(cli.text(&id)?, original_text);
    let generation = cli.execute(&id, "caret.get", json!({}))?["generation"].clone();
    cli.execute(&id,"selection.range",json!({"start":{"section":0,"paragraph":1,"offset":8},"end":{"section":0,"paragraph":1,"offset":8},"expected_generation":generation}))?;
    let created = cli.execute(
        &id,
        "chart.create",
        json!({"chart":definition("column","MCP 차트 생성")}),
    )?;
    assert_eq!(created["index"], 0);
    assert_eq!(created["nativeInterop"], "unverified");
    assert_eq!(
        cli.execute(&id, "chart.list", json!({}))?["items"]
            .as_array()
            .context("chart.list items")?
            .len(),
        1
    );
    let generation = cli.execute(&id, "caret.get", json!({}))?["generation"].clone();
    cli.execute(&id,"chart.configure",json!({"index":0,"chart":definition("doughnut","실제 도넛 MCP 검증"),"expected_generation":generation}))?;
    cli.execute(
        &id,
        "chart.data",
        json!({"index":0,"series":[{"values":["12","18"]}]}),
    )?;
    let expected_data = cli.execute(&id, "chart.data.get", json!({"index":0}))?;
    assert!(expected_data.to_string().contains("12") && expected_data.to_string().contains("18"));
    let body = cli.text(&id)?;
    for text in ["차트 보고서 😀", "기존 본문 유지"] {
        assert!(body["text"].as_str().context("body text")?.contains(text));
    }
    // The failed atomic batch really changes chart kind and replaces a selected
    // body range before failure; every one of these changes must be restored.
    let generation = cli.execute(&id, "caret.get", json!({}))?["generation"].clone();
    cli.execute(&id,"selection.range",json!({"start":{"section":0,"paragraph":0,"offset":0},"end":{"section":0,"paragraph":0,"offset":2},"expected_generation":generation}))?;
    let selection = cli.ok("get_selection", json!({"document_id":id}))?;
    let generation = cli.execute(&id, "caret.get", json!({}))?["generation"].clone();
    let failed_batch=cli.tool("batch",json!({"document_id":id,"atomic":true,"commands":[
        {"command":"chart.configure","params":{"index":0,"chart":definition("line","ROLLBACK CHART"),"expected_generation":generation}},
        {"command":"text.insert","params":{"text":"ROLLBACK BODY"}},
        {"command":"not.a.command","params":{}}
    ]}))?;
    assert_eq!(failed_batch["isError"], true);
    assert_eq!(failed_batch["structuredContent"]["rolledBack"], true);
    assert_eq!(
        failed_batch["structuredContent"]["results"]
            .as_array()
            .context("atomic chart results")?
            .len(),
        3
    );
    assert_eq!(failed_batch["structuredContent"]["results"][0]["ok"], true);
    assert_eq!(
        failed_batch["structuredContent"]["results"][0]["result"]["kind"],
        "line"
    );
    assert_eq!(failed_batch["structuredContent"]["results"][1]["ok"], true);
    assert_eq!(failed_batch["structuredContent"]["results"][2]["ok"], false);
    assert_eq!(
        cli.execute(&id, "chart.data.get", json!({"index":0}))?,
        expected_data
    );
    assert_eq!(cli.text(&id)?, body);
    assert_eq!(
        cli.ok("get_selection", json!({"document_id":id}))?,
        selection
    );
    let mut opened_handles = Vec::new();
    for extension in ["hwp", "hwpx"] {
        let editable = root.path().join(format!("MCP 도넛.{extension}"));
        cli.ok("save_document", json!({"document_id":id,"path":editable}))?;
        let reopened = cli.ok("open_document", json!({"path":editable}))?;
        let opened = reopened["document_id"]
            .as_str()
            .context("reopened chart document handle")?
            .to_owned();
        assert_ne!(opened, id);
        assert_eq!(
            cli.execute(&opened, "chart.list", json!({}))?["items"]
                .as_array()
                .context("reopened charts")?
                .len(),
            1
        );
        let data = cli.execute(&opened, "chart.data.get", json!({"index":0}))?;
        assert!(data.to_string().contains("12") && data.to_string().contains("18"));
        let text = cli.text(&opened)?;
        for expected in ["차트 보고서 😀", "기존 본문 유지"] {
            assert!(
                text["text"]
                    .as_str()
                    .context("reopened text")?
                    .contains(expected)
            );
        }
        let render = cli.tool(
            "render_page",
            json!({"document_id":opened,"page":1,"scale":1.0}),
        )?;
        assert_ne!(render["isError"], true);
        let block = render["content"]
            .as_array()
            .context("render blocks")?
            .iter()
            .find(|block| block["type"] == "image")
            .context("chart image block")?;
        assert_eq!(block["mimeType"], "image/png");
        let png = decode_base64(block["data"].as_str().context("chart image base64")?)?;
        let raster = image::load_from_memory(&png)?.to_rgba8();
        let colored = |pixel: &image::Rgba<u8>| {
            let [r, g, b, a] = pixel.0;
            a > 200 && r.max(g).max(b) - r.min(g).min(b) > 60
        };
        // Legend is explicitly disabled, but other colored output on the page
        // (for example an emoji drawn by a color font) must not widen the box:
        // measure the largest 8-connected colored region, which is the ring.
        let (width, height) = raster.dimensions();
        let mut seen = vec![false; (width * height) as usize];
        let mut points: Vec<(u32, u32)> = Vec::new();
        for (x, y, pixel) in raster.enumerate_pixels() {
            if seen[(y * width + x) as usize] || !colored(pixel) {
                continue;
            }
            seen[(y * width + x) as usize] = true;
            let mut region = vec![(x, y)];
            let mut next = 0;
            while next < region.len() {
                let (px, py) = region[next];
                next += 1;
                for (dx, dy) in [
                    (-1, -1),
                    (0, -1),
                    (1, -1),
                    (-1, 0),
                    (1, 0),
                    (-1, 1),
                    (0, 1),
                    (1, 1),
                ] {
                    let (nx, ny) = (px as i64 + dx, py as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                        continue;
                    }
                    let (nx, ny) = (nx as u32, ny as u32);
                    let index = (ny * width + nx) as usize;
                    if !seen[index] && colored(raster.get_pixel(nx, ny)) {
                        seen[index] = true;
                        region.push((nx, ny));
                    }
                }
            }
            if region.len() > points.len() {
                points = region;
            }
        }
        assert!(
            points.len() > 1000,
            "{extension} has no actual colored chart"
        );
        let left = points
            .iter()
            .map(|(x, _)| *x)
            .min()
            .context("colored left")?;
        let right = points
            .iter()
            .map(|(x, _)| *x)
            .max()
            .context("colored right")?;
        let top = points
            .iter()
            .map(|(_, y)| *y)
            .min()
            .context("colored top")?;
        let bottom = points
            .iter()
            .map(|(_, y)| *y)
            .max()
            .context("colored bottom")?;
        let cx = (left + right) / 2;
        let cy = (top + bottom) / 2;
        assert!(right - left > 100 && bottom - top > 100);
        for dx in [-2_i32, 0, 2] {
            for dy in [-2_i32, 0, 2] {
                let pixel = raster.get_pixel((cx as i32 + dx) as u32, (cy as i32 + dy) as u32);
                assert!(
                    pixel.0[..3].iter().all(|channel| *channel > 240),
                    "{extension} donut hole is filled at {cx},{cy}"
                );
            }
        }
        let ring_x = left + (right - left) * 7 / 8;
        assert!(
            colored(raster.get_pixel(ring_x, cy)),
            "{extension} ring annulus missing"
        );
        std::fs::write(root.path().join(format!("chart-{extension}.png")), png)?;
        let pdf = root.path().join(format!("chart-{extension}.pdf"));
        cli.ok("save_document", json!({"document_id":opened,"path":pdf}))?;
        let pdf_bytes = std::fs::read(pdf)?;
        assert!(pdf_bytes.starts_with(b"%PDF-") && pdf_bytes.windows(5).any(|p| p == b"%%EOF"));
        assert!(pdf_bytes.len() > 1000);
        opened_handles.push(opened);
    }
    assert_eq!(
        cli.ok("get_selection", json!({"document_id":id}))?,
        selection,
        "save/query/output must preserve the authoring selection"
    );
    for opened in opened_handles {
        cli.ok("close_document", json!({"document_id":opened}))?;
    }
    cli.ok("close_document", json!({"document_id":id}))?;
    cli.finish()
}
