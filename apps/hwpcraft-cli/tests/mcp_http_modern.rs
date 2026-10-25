//! Product routing through an owned CLI HTTP subprocess, not an SDK Probe.
//! Standard-library HTTP avoids another client dependency and captures raw wire.
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const VERSION: &str = "2026-07-28";
const MAX_RESPONSE: u64 = 32 * 1024 * 1024;
const TOTAL_DEADLINE: Duration = Duration::from_secs(30);

struct HttpCli {
    child: Child,
    address: SocketAddr,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    readers: Vec<JoinHandle<()>>,
    deadline: Instant,
    next_id: u64,
    wire: Vec<Value>,
    root: PathBuf,
    token: String,
    finished: bool,
}

struct Reply {
    status: u16,
    headers: String,
    body: Vec<u8>,
}

fn drain(mut pipe: impl Read + Send + 'static) -> (Arc<Mutex<Vec<u8>>>, JoinHandle<()>) {
    let data = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&data);
    let reader = thread::spawn(move || {
        let mut chunk = [0; 4096];
        while let Ok(count) = pipe.read(&mut chunk) {
            if count == 0 {
                break;
            }
            if let Ok(mut bytes) = capture.lock() {
                let remaining = (2 * 1024 * 1024_usize).saturating_sub(bytes.len());
                bytes.extend_from_slice(&chunk[..count.min(remaining)]);
            }
        }
    });
    (data, reader)
}

impl HttpCli {
    fn start(root: &Path) -> Result<Self> {
        let deadline = Instant::now() + TOTAL_DEADLINE;
        let mut errors = Vec::new();
        // Exercises the default token location (settings folder) with a known value.
        let token = "http-cli-fixture-token-0123456789abcdef".to_owned();
        std::fs::create_dir_all(root.join("config"))?;
        std::fs::write(root.join("config").join("mcp-http-token"), &token)?;
        // CLI has no listener-FD handoff. Reserve an OS-assigned port, release it,
        // then bound retries for the narrow bind race. Never use a fixed port.
        for _ in 0..3 {
            let reserved = TcpListener::bind("127.0.0.1:0")?;
            let address = reserved.local_addr()?;
            drop(reserved);
            let mut child = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
                .args(["mcp", "--transport", "http", "--listen"])
                .arg(address.to_string())
                .arg("--automation-read-root")
                .arg(root)
                .arg("--automation-write-root")
                .arg(root)
                .env("HWPCRAFT_CONFIG_DIR", root.join("config"))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("launch actual HTTP CLI")?;
            let (stdout, out_reader) = drain(child.stdout.take().context("child stdout")?);
            let (stderr, err_reader) = drain(child.stderr.take().context("child stderr")?);
            let mut server = Self {
                child,
                address,
                stdout,
                stderr,
                readers: vec![out_reader, err_reader],
                deadline,
                next_id: 1,
                wire: Vec::new(),
                root: root.to_owned(),
                token: token.clone(),
                finished: false,
            };
            let ready_deadline = (Instant::now() + Duration::from_secs(3)).min(deadline);
            while Instant::now() < ready_deadline {
                if let Some(status) = server.child.try_wait()? {
                    errors.push(format!(
                        "child exited {status}: {}",
                        server.log(&server.stderr)
                    ));
                    break;
                }
                // A successful TCP connect alone is insufficient: verify the
                // real MCP response and that our own child did not lose its bind.
                if let Ok(value) = server.rpc("server/discover", json!({}))
                    && value["supportedVersions"] == json!([VERSION])
                    && server.child.try_wait()?.is_none()
                {
                    return Ok(server);
                }
                thread::sleep(Duration::from_millis(20));
            }
            errors.push(format!(
                "listener readiness failed at {address}: {}",
                server.log(&server.stderr)
            ));
            // Dropping only this owned attempt kills/reaps it and joins readers.
        }
        bail!(
            "HTTP CLI could not become ready after three bounded attempts: {}",
            errors.join("; ")
        )
    }

    fn log(&self, source: &Arc<Mutex<Vec<u8>>>) -> String {
        source
            .lock()
            .map(|data| String::from_utf8_lossy(&data).into_owned())
            .unwrap_or_else(|_| "capture mutex poisoned".into())
    }

    fn post(&mut self, method: &str, params: &Value, id: &Value) -> Result<Reply> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .context("HTTP product 30-second deadline")?;
        let timeout = remaining.min(Duration::from_secs(3));
        let mut socket =
            TcpStream::connect_timeout(&self.address, timeout.min(Duration::from_millis(300)))?;
        socket.set_read_timeout(Some(timeout))?;
        socket.set_write_timeout(Some(timeout))?;
        let body =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let name = match method {
            "resources/read" | "resources/subscribe" | "resources/unsubscribe" => params.get("uri"),
            "tools/call" | "prompts/get" => params.get("name"),
            _ => None,
        }
        .and_then(Value::as_str);
        let extra = name
            .map(|name| format!("Mcp-Name: {name}\r\n"))
            .unwrap_or_default();
        let header = format!(
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nMCP-Protocol-Version: {VERSION}\r\nMcp-Method: {method}\r\n{extra}Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            self.address,
            self.token,
            body.len()
        );
        socket.write_all(header.as_bytes())?;
        socket.write_all(&body)?;
        let mut raw = Vec::new();
        socket.take(MAX_RESPONSE + 1).read_to_end(&mut raw)?;
        if raw.len() as u64 > MAX_RESPONSE {
            bail!("HTTP response exceeds 32 MiB");
        }
        self.wire.push(json!({"requestHeaders":header,"request":serde_json::from_slice::<Value>(&body)?,"responseRaw":String::from_utf8_lossy(&raw)}));
        Reply::parse(raw)
    }

    fn request(&mut self, method: &str, mut params: Value) -> Result<Value> {
        params.as_object_mut().context("request params object")?.insert("_meta".into(), json!({"io.modelcontextprotocol/protocolVersion":VERSION,"io.modelcontextprotocol/clientCapabilities":{}}));
        let id = json!(self.next_id);
        self.next_id += 1;
        let reply = self.post(method, &params, &id)?;
        let response: Value = serde_json::from_slice(&reply.body).with_context(|| {
            format!(
                "HTTP {} response was not JSON: {}",
                reply.status,
                String::from_utf8_lossy(&reply.body)
            )
        })?;
        assert_eq!(response["id"], id, "HTTP response ID mismatch");
        assert!(
            !reply
                .headers
                .to_ascii_lowercase()
                .contains("mcp-session-id"),
            "modern stateless HTTP unexpectedly requires a session"
        );
        if response.get("error").is_none() {
            assert_eq!(reply.status, 200, "{response}");
            assert_eq!(response["result"]["resultType"], "complete");
        }
        Ok(response)
    }

    fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let response = self.request(method, params)?;
        if response.get("error").is_some() {
            bail!("HTTP MCP error: {response}");
        }
        Ok(response["result"].clone())
    }

    fn tool(&mut self, name: &str, args: Value) -> Result<Value> {
        self.rpc("tools/call", json!({"name":name,"arguments":args}))
    }

    fn ok(&mut self, name: &str, args: Value) -> Result<Value> {
        let result = self.tool(name, args)?;
        if result["isError"] == true {
            bail!("HTTP tool {name} failed: {result}");
        }
        result
            .get("structuredContent")
            .filter(|value| value.is_object())
            .cloned()
            .context("structuredContent object missing")
    }

    fn document(&mut self) -> Result<String> {
        let data = self.ok("new_document", json!({}))?;
        Ok(data["document_id"]
            .as_str()
            .context("created document_id")?
            .to_owned())
    }

    fn capture(&self, status: &str) -> Result<()> {
        if std::env::var_os("HWPCRAFT_WRITE_MCP_HTTP_FIXTURES").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return Ok(());
        }
        let destination = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.17/mcp-http-product");
        std::fs::create_dir_all(&destination)?;
        std::fs::write(destination.join("stdout.log"), self.log(&self.stdout))?;
        std::fs::write(destination.join("stderr.log"), self.log(&self.stderr))?;
        std::fs::write(
            destination.join("http-wire.jsonl"),
            self.wire
                .iter()
                .map(|value| value.to_string() + "\n")
                .collect::<String>(),
        )?;
        std::fs::write(
            destination.join("receipt.json"),
            serde_json::to_vec_pretty(
                &json!({"status":status,"binary":env!("CARGO_BIN_EXE_hwpcraft-cli"),"ownedPid":self.child.id(),"address":self.address.to_string(),"protocolVersion":VERSION,"transport":"http","isolatedRoot":self.root,"requests":self.wire.len(),"processCleanup":"Only owned child killed/reaped after explicit document closes; no portable HTTP CLI shutdown endpoint","scope":"Actual product routing, document handles, saved/reopened files and PDF structure. Not remote authentication, extension or PDF glyph parity proof."}),
            )?,
        )?;
        for entry in std::fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ["hwp", "hwpx", "pdf"].contains(&ext))
            {
                std::fs::copy(
                    &path,
                    destination.join(path.file_name().context("own fixture name")?),
                )?;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        for reader in self.readers.drain(..) {
            reader
                .join()
                .map_err(|_| anyhow!("HTTP log reader panic"))?;
        }
        // HTTP mode must never print document content or protocol replies to stdout.
        assert!(
            self.log(&self.stdout).trim().is_empty(),
            "HTTP server leaked stdout: {}",
            self.log(&self.stdout)
        );
        self.capture("PASS")?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for HttpCli {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.capture("FAILED_OR_EARLY_RETURN");
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

impl Reply {
    fn parse(raw: Vec<u8>) -> Result<Self> {
        let split = raw
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .context("HTTP header boundary")?;
        let headers = String::from_utf8(raw[..split].to_vec())?;
        let status = headers
            .split_whitespace()
            .nth(1)
            .context("HTTP status")?
            .parse()?;
        let bytes = &raw[split + 4..];
        let body = if headers
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            chunks(bytes)?
        } else {
            bytes.to_vec()
        };
        Ok(Self {
            status,
            headers,
            body,
        })
    }
}

fn chunks(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    loop {
        let end = bytes
            .windows(2)
            .position(|part| part == b"\r\n")
            .context("HTTP chunk header")?;
        let size = usize::from_str_radix(
            std::str::from_utf8(&bytes[..end])?
                .split(';')
                .next()
                .context("HTTP chunk size")?,
            16,
        )?;
        bytes = &bytes[end + 2..];
        if size == 0 {
            return Ok(output);
        }
        let data = bytes.get(..size).context("truncated HTTP chunk")?;
        if bytes.get(size..size + 2) != Some(b"\r\n") {
            bail!("invalid HTTP chunk terminator");
        }
        output.extend_from_slice(data);
        bytes = &bytes[size + 2..];
    }
}

#[test]
fn actual_http_cli_authors_reopens_exports_and_isolates_explicit_documents() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut server = HttpCli::start(root.path())?;
    // Without the bearer token the product endpoint refuses before any MCP work.
    let mut socket = TcpStream::connect(server.address)?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    socket.write_all(
        format!(
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{{}}",
            server.address
        )
        .as_bytes(),
    )?;
    let mut raw = Vec::new();
    socket.read_to_end(&mut raw)?;
    assert!(
        String::from_utf8_lossy(&raw).starts_with("HTTP/1.1 401"),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    let tools = server.rpc("tools/list", json!({}))?;
    assert_eq!(tools["resultType"], "complete");
    assert_eq!(tools["cacheScope"], "public");
    let a = server.document()?;
    let b = server.document()?;
    assert_ne!(a, b);
    server.ok(
        "type_text",
        json!({"document_id":a,"text":"HTTP 실제 보고서 😀\n무인 작성과 재열기 검증"}),
    )?;
    server.ok("type_text", json!({"document_id":b,"text":"독립 문서 🦀"}))?;
    let original = server.ok("get_text", json!({"document_id":a}))?;
    assert!(original.to_string().contains("HTTP 실제 보고서 😀"));
    assert!(!original.to_string().contains("독립 문서"));
    let other = server.ok("get_text", json!({"document_id":b}))?;
    assert!(other.to_string().contains("독립 문서 🦀"));
    assert!(!other.to_string().contains("HTTP 실제 보고서"));
    for args in [json!({}), json!({"document_id":"unknown-own-fixture-id"})] {
        let response = server.request("tools/call", json!({"name":"get_text","arguments":args}))?;
        assert!(
            response.get("error").is_some() || response["result"]["isError"] == true,
            "missing/unknown handle accepted: {response}"
        );
    }
    assert_eq!(server.ok("get_text", json!({"document_id":a}))?, original);
    let path = root.path().join("HTTP 보고서.hwpx");
    server.ok("save_document", json!({"document_id":a,"path":path}))?;
    assert!(std::fs::read(&path)?.starts_with(b"PK"));
    let opened = server.ok("open_document", json!({"path":path}))?;
    let reopened = opened["document_id"]
        .as_str()
        .context("reopened HTTP document_id")?
        .to_owned();
    assert_ne!(reopened, a);
    let text = server.ok("get_text", json!({"document_id":reopened}))?;
    assert_eq!(text["text"], original["text"]);
    let read = server.rpc(
        "resources/read",
        json!({"uri":format!("hwpcraft://documents/{reopened}")}),
    )?;
    assert_eq!(read["ttlMs"], 0);
    assert_eq!(read["cacheScope"], "private");
    let pdf = root.path().join("HTTP 보고서.pdf");
    server.ok("save_document", json!({"document_id":reopened,"path":pdf}))?;
    let bytes = std::fs::read(pdf)?;
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.windows(5).any(|part| part == b"%%EOF"));
    assert!(bytes.len() > 1000);
    for id in [&a, &reopened] {
        server.ok("close_document", json!({"document_id":id}))?;
    }
    let closed = server.request(
        "tools/call",
        json!({"name":"get_text","arguments":{"document_id":a}}),
    )?;
    assert!(closed.get("error").is_some() || closed["result"]["isError"] == true);
    assert_eq!(server.ok("get_text", json!({"document_id":b}))?, other);
    server.ok("close_document", json!({"document_id":b,"force":true}))?;
    server.finish()
}

// Actual product SSE subscription checks over independently owned TCP streams.
// Reuses HttpCli's finite requests while parsing long-lived responses incrementally.
// Official wire contract: params.notifications.resourceSubscriptions (no filter).
// Sources: pinned rmcp3.7 subscription model/service, HwpCraft
// document_subscriptions.rs, official SDK subscription docs for 2026-07-28.

enum ProductSseEvent {
    Headers { status: u16, text: String },
    Message(Value),
    Eof,
    Fault(String),
}

struct ProductSseRead {
    reader: std::io::BufReader<TcpStream>,
    canceled: Arc<std::sync::atomic::AtomicBool>,
    deadline: Instant,
    raw: Arc<Mutex<Vec<u8>>>,
    bytes: usize,
}

impl ProductSseRead {
    // Consume only bytes actually read. A timeout cannot lose a partially read
    // UTF-8 character, chunk header, chunk payload, CRLF or SSE data line.
    fn take(&mut self, maximum: usize) -> Result<Option<Vec<u8>>> {
        loop {
            if self.canceled.load(std::sync::atomic::Ordering::SeqCst) {
                bail!("own SSE client disconnected");
            }
            if Instant::now() >= self.deadline {
                bail!("HTTP subscription global deadline exceeded");
            }
            let bytes = match std::io::BufRead::fill_buf(&mut self.reader) {
                Ok([]) => return Ok(None),
                Ok(bytes) => bytes[..maximum.min(bytes.len())].to_vec(),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            std::io::BufRead::consume(&mut self.reader, bytes.len());
            self.bytes += bytes.len();
            if self.bytes > MAX_RESPONSE as usize {
                bail!("SSE response exceeds 32 MiB");
            }
            self.raw
                .lock()
                .map_err(|_| anyhow!("SSE raw capture mutex"))?
                .extend_from_slice(&bytes);
            return Ok(Some(bytes));
        }
    }

    fn exact(&mut self, count: usize) -> Result<Vec<u8>> {
        if count > 4 * 1024 * 1024 {
            bail!("SSE HTTP chunk exceeds 4 MiB");
        }
        let mut bytes = Vec::with_capacity(count);
        while bytes.len() < count {
            let chunk = self
                .take(count - bytes.len())?
                .context("EOF inside HTTP chunk")?;
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn line(&mut self, maximum: usize) -> Result<Option<Vec<u8>>> {
        let mut line = Vec::new();
        loop {
            let Some(byte) = self.take(1)? else {
                if line.is_empty() {
                    return Ok(None);
                }
                bail!("EOF inside HTTP header/chunk line");
            };
            line.extend_from_slice(&byte);
            if line.len() > maximum {
                bail!("HTTP header/chunk line exceeds bound");
            }
            if line.ends_with(b"\n") {
                return Ok(Some(line));
            }
        }
    }
}

struct ProductSseDecoder {
    line: Vec<u8>,
    data: Vec<String>,
    messages: usize,
}

impl ProductSseDecoder {
    fn feed(
        &mut self,
        bytes: &[u8],
        send: &std::sync::mpsc::Sender<ProductSseEvent>,
    ) -> Result<()> {
        for &byte in bytes {
            if byte != b'\n' {
                self.line.push(byte);
                if self.line.len() > 4 * 1024 * 1024 {
                    bail!("SSE data line exceeds 4 MiB");
                }
                continue;
            }
            if self.line.last() == Some(&b'\r') {
                self.line.pop();
            }
            let line = String::from_utf8(std::mem::take(&mut self.line))?;
            if line.is_empty() {
                if self.data.is_empty() {
                    continue;
                }
                let text = std::mem::take(&mut self.data).join("\n");
                let message: Value = serde_json::from_str(&text).context("SSE data is not JSON")?;
                if !message.is_object() || message["jsonrpc"] != "2.0" {
                    bail!("SSE data is not a JSON-RPC object");
                }
                self.messages += 1;
                if self.messages > 512 {
                    bail!("subscription message bound exceeded");
                }
                send.send(ProductSseEvent::Message(message))
                    .map_err(|_| anyhow!("SSE observer disconnected"))?;
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
                if self.data.iter().map(String::len).sum::<usize>() > 4 * 1024 * 1024 {
                    bail!("SSE event exceeds 4 MiB");
                }
            }
            // SSE comments, id/event/retry fields are framing, not MCP data.
        }
        Ok(())
    }

    fn finish(self) -> Result<()> {
        if !self.line.is_empty() || !self.data.is_empty() {
            bail!("truncated SSE event at HTTP EOF");
        }
        Ok(())
    }
}

fn read_product_sse(
    mut wire: ProductSseRead,
    send: &std::sync::mpsc::Sender<ProductSseEvent>,
) -> Result<()> {
    let mut head = Vec::new();
    loop {
        let line = wire
            .line(16 * 1024)?
            .context("EOF before SSE HTTP headers")?;
        head.extend_from_slice(&line);
        if head.len() > 64 * 1024 {
            bail!("SSE HTTP headers exceed 64 KiB");
        }
        if line == b"\r\n" {
            break;
        }
    }
    let text = String::from_utf8(head)?;
    let status = text
        .split_whitespace()
        .nth(1)
        .context("SSE HTTP status")?
        .parse()?;
    let chunked = text.lines().any(|line| {
        line.to_ascii_lowercase().starts_with("transfer-encoding:")
            && line.to_ascii_lowercase().contains("chunked")
    });
    send.send(ProductSseEvent::Headers { status, text })
        .map_err(|_| anyhow!("SSE header observer gone"))?;
    let mut sse = ProductSseDecoder {
        line: Vec::new(),
        data: Vec::new(),
        messages: 0,
    };
    if chunked {
        loop {
            let line = wire.line(4096)?.context("EOF before next HTTP chunk")?;
            let header = std::str::from_utf8(&line)?.trim_end_matches(['\r', '\n']);
            let size = usize::from_str_radix(header.split(';').next().context("chunk size")?, 16)?;
            if size == 0 {
                loop {
                    if wire.line(16 * 1024)?.context("EOF in HTTP trailers")? == b"\r\n" {
                        break;
                    }
                }
                break;
            }
            let bytes = wire.exact(size)?;
            if wire.exact(2)? != b"\r\n" {
                bail!("HTTP chunk has no CRLF terminator");
            }
            sse.feed(&bytes, send)?;
        }
    } else {
        while let Some(bytes) = wire.take(4096)? {
            sse.feed(&bytes, send)?;
        }
    }
    sse.finish()?;
    send.send(ProductSseEvent::Eof)
        .map_err(|_| anyhow!("SSE EOF observer gone"))?;
    Ok(())
}

struct ProductSubscription {
    socket: TcpStream,
    canceled: Arc<std::sync::atomic::AtomicBool>,
    reader: Option<JoinHandle<()>>,
    events: std::sync::mpsc::Receiver<ProductSseEvent>,
    raw: Arc<Mutex<Vec<u8>>>,
    messages: Vec<Value>,
    request: Value,
    subscription_id: Value,
    uri: String,
    deadline: Instant,
    complete: bool,
}

impl ProductSubscription {
    fn open(
        address: SocketAddr,
        token: &str,
        uri: String,
        subscription_id: Value,
        deadline: Instant,
    ) -> Result<Self> {
        let mut socket = TcpStream::connect_timeout(&address, Duration::from_millis(500))?;
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        let request = json!({"jsonrpc":"2.0","id":subscription_id,"method":"subscriptions/listen","params":{
            "_meta":{"io.modelcontextprotocol/protocolVersion":VERSION,"io.modelcontextprotocol/clientCapabilities":{}},
            "notifications":{"resourceSubscriptions":[uri]}
        }});
        let body = serde_json::to_vec(&request)?;
        let headers = format!(
            "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nMCP-Protocol-Version: {VERSION}\r\nMcp-Method: subscriptions/listen\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        socket.write_all(headers.as_bytes())?;
        socket.write_all(&body)?;
        let read = socket.try_clone()?;
        let canceled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let (send, events) = std::sync::mpsc::channel();
        let wire = ProductSseRead {
            reader: std::io::BufReader::new(read),
            canceled: Arc::clone(&canceled),
            deadline,
            raw: Arc::clone(&raw),
            bytes: 0,
        };
        let reader = thread::spawn(move || {
            if let Err(error) = read_product_sse(wire, &send) {
                let _ = send.send(ProductSseEvent::Fault(format!("{error:#}")));
            }
        });
        let mut stream = Self {
            socket,
            canceled,
            reader: Some(reader),
            events,
            raw,
            messages: Vec::new(),
            request,
            subscription_id,
            uri,
            deadline,
            complete: false,
        };
        match stream.event(Duration::from_secs(3))? {
            ProductSseEvent::Headers { status, text } => {
                assert_eq!(status, 200, "{text}");
                assert!(
                    text.to_ascii_lowercase()
                        .contains("content-type: text/event-stream"),
                    "{text}"
                );
                assert!(
                    !text.to_ascii_lowercase().contains("mcp-session-id"),
                    "modern listen must be stateless"
                );
            }
            _ => bail!("SSE response did not begin with HTTP headers"),
        }
        Ok(stream)
    }

    fn event(&mut self, limit: Duration) -> Result<ProductSseEvent> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .context("subscription test deadline")?;
        match self
            .events
            .recv_timeout(limit.min(remaining))
            .context("subscription event deadline/EOF")?
        {
            ProductSseEvent::Fault(message) => bail!("SSE reader failed: {message}"),
            ProductSseEvent::Message(value) => {
                self.messages.push(value.clone());
                Ok(ProductSseEvent::Message(value))
            }
            other => Ok(other),
        }
    }

    fn message(&mut self) -> Result<Value> {
        match self.event(Duration::from_secs(3))? {
            ProductSseEvent::Message(value) => Ok(value),
            _ => bail!("SSE ended before expected MCP message"),
        }
    }

    fn ack_and_initial_resync(&mut self) -> Result<()> {
        let ack = self.message()?;
        assert!(ack.get("id").is_none());
        assert_eq!(ack["method"], "notifications/subscriptions/acknowledged");
        assert_eq!(
            ack["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
            self.subscription_id
        );
        assert_eq!(
            ack["params"]["notifications"]["resourceSubscriptions"],
            json!([self.uri])
        );
        self.updated()?; // This is initial resync, explicitly not edit evidence.
        Ok(())
    }

    fn updated(&mut self) -> Result<()> {
        let update = self.message()?;
        assert!(update.get("id").is_none());
        assert_eq!(update["method"], "notifications/resources/updated");
        assert_eq!(
            update["params"]["uri"], self.uri,
            "another document's update leaked"
        );
        assert_eq!(
            update["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
            self.subscription_id
        );
        assert_eq!(
            update["params"].as_object().context("update params")?.len(),
            2,
            "URI-only update leaked document payload"
        );
        assert_eq!(
            update["params"]["_meta"]
                .as_object()
                .context("update meta")?
                .len(),
            1
        );
        Ok(())
    }

    fn quiet(&mut self) -> Result<()> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .context("quiet deadline")?;
        match self
            .events
            .recv_timeout(Duration::from_millis(100).min(remaining))
        {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
            Ok(ProductSseEvent::Message(message)) => {
                bail!("unrelated/duplicate/unsolicited stream message: {message}")
            }
            Ok(ProductSseEvent::Fault(error)) => bail!("SSE quiet reader fault: {error}"),
            _ => bail!("another URI/peer unexpectedly ended the active subscription"),
        }
    }

    fn final_complete(&mut self) -> Result<()> {
        let response = self.message()?;
        assert_eq!(response["id"], self.subscription_id);
        assert_eq!(response["result"]["resultType"], "complete");
        assert_eq!(
            response["result"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
            self.subscription_id
        );
        match self.event(Duration::from_secs(3))? {
            ProductSseEvent::Eof => {}
            _ => bail!("subscription sent a message after final complete"),
        }
        self.complete = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        self.canceled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.socket.shutdown(std::net::Shutdown::Both)?;
        if let Some(reader) = self.reader.take() {
            reader.join().map_err(|_| anyhow!("SSE reader panic"))?;
        }
        Ok(())
    }

    fn capture(&self) -> Result<Value> {
        let bytes = self.raw.lock().map_err(|_| anyhow!("capture lock"))?;
        Ok(
            json!({"request":self.request,"rawResponse":String::from_utf8_lossy(&bytes),"messages":self.messages,"finalComplete":self.complete,"clientDisconnected":self.canceled.load(std::sync::atomic::Ordering::SeqCst)}),
        )
    }

    fn preserve_stream(&self) -> Result<()> {
        if std::env::var_os("HWPCRAFT_WRITE_MCP_HTTP_FIXTURES").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return Ok(());
        }
        let out = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.17/mcp-http-subscriptions-product");
        std::fs::create_dir_all(&out)?;
        let port = self.socket.local_addr()?.port();
        std::fs::write(
            out.join(format!("stream-{port}.raw")),
            self.raw
                .lock()
                .map_err(|_| anyhow!("raw stream capture lock"))?
                .as_slice(),
        )?;
        std::fs::write(
            out.join(format!("stream-{port}.json")),
            serde_json::to_vec_pretty(&self.capture()?)?,
        )?;
        Ok(())
    }
}

impl Drop for ProductSubscription {
    fn drop(&mut self) {
        let _ = self.preserve_stream();
        self.canceled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn actual_http_subscriptions_ack_resync_change_close_and_disconnect_are_uri_scoped() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut server = HttpCli::start(root.path())?;
    let a = server.document()?;
    let b = server.document()?;
    let uri_a = format!("hwpcraft://documents/{a}");
    let uri_b = format!("hwpcraft://documents/{b}");
    // Same request ID is legal across these independent HTTP response streams.
    let listen = json!("same-http-listen-id");
    let mut first = ProductSubscription::open(
        server.address,
        &server.token,
        uri_a.clone(),
        listen.clone(),
        server.deadline,
    )?;
    let mut second = ProductSubscription::open(
        server.address,
        &server.token,
        uri_b.clone(),
        listen.clone(),
        server.deadline,
    )?;
    first.ack_and_initial_resync()?;
    second.ack_and_initial_resync()?;
    first.quiet()?;
    second.quiet()?;
    for uri in [&uri_a, &uri_b] {
        let read = server.rpc("resources/read", json!({"uri":uri}))?;
        assert_eq!(read["ttlMs"], 0);
        assert_eq!(read["cacheScope"], "private");
    }
    server.ok(
        "type_text",
        json!({"document_id":a,"text":"HTTP stream A 한글😀"}),
    )?;
    first.updated()?;
    second.quiet()?;
    assert!(
        server.ok("get_text", json!({"document_id":a}))?["text"]
            .as_str()
            .context("A text")?
            .contains("stream A")
    );
    // Originating TCP client is not an implicit document identity; the URI is.
    server.ok(
        "type_text",
        json!({"document_id":b,"text":"HTTP stream B 독립"}),
    )?;
    second.updated()?;
    first.quiet()?;
    server.ok("close_document", json!({"document_id":a,"force":true}))?;
    first.updated()?;
    first.final_complete()?;
    let missing = server.request("resources/read", json!({"uri":uri_a}))?;
    assert_eq!(missing["error"]["code"], -32602);
    second.quiet()?;
    assert!(
        server.ok("get_text", json!({"document_id":b}))?["text"]
            .as_str()
            .context("B text")?
            .contains("stream B")
    );
    // A real network disconnect cancels only this listen request, not document
    // B, the shared worker registry, or a later independently opened listener.
    second.disconnect()?;
    server.ok(
        "type_text",
        json!({"document_id":b,"text":" after disconnect"}),
    )?;
    assert!(
        server.ok("get_text", json!({"document_id":b}))?["text"]
            .as_str()
            .context("B after disconnect")?
            .contains("after disconnect")
    );
    let mut resumed = ProductSubscription::open(
        server.address,
        &server.token,
        uri_b.clone(),
        listen,
        server.deadline,
    )?;
    resumed.ack_and_initial_resync()?;
    resumed.quiet()?;
    server.ok(
        "type_text",
        json!({"document_id":b,"text":" after reconnect"}),
    )?;
    resumed.updated()?;
    let read = server.rpc("resources/read", json!({"uri":uri_b}))?;
    assert_eq!(read["ttlMs"], 0);
    assert_eq!(read["cacheScope"], "private");
    server.ok("close_document", json!({"document_id":b,"force":true}))?;
    resumed.updated()?;
    resumed.final_complete()?;
    assert_eq!(
        server.request("resources/read", json!({"uri":uri_b}))?["error"]["code"],
        -32602
    );
    // Service remains responsive after both stream completions and a disconnect.
    assert_eq!(
        server.rpc("server/discover", json!({}))?["supportedVersions"],
        json!([VERSION])
    );
    let captures = json!([first.capture()?, second.capture()?, resumed.capture()?]);
    drop(first);
    drop(second);
    drop(resumed);
    let stdout = Arc::clone(&server.stdout);
    let stderr = Arc::clone(&server.stderr);
    let rpc_wire = server.wire.clone();
    let owned_pid = server.child.id();
    server.finish()?;
    if std::env::var_os("HWPCRAFT_WRITE_MCP_HTTP_FIXTURES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let out = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.17/mcp-http-subscriptions-product");
        std::fs::create_dir_all(&out)?;
        std::fs::write(
            out.join("streams.json"),
            serde_json::to_vec_pretty(&captures)?,
        )?;
        std::fs::write(
            out.join("rpc-wire.jsonl"),
            rpc_wire
                .iter()
                .map(|v| v.to_string() + "\n")
                .collect::<String>(),
        )?;
        std::fs::write(
            out.join("stdout.log"),
            stdout
                .lock()
                .map_err(|_| anyhow!("stdout lock"))?
                .as_slice(),
        )?;
        std::fs::write(
            out.join("stderr.log"),
            stderr
                .lock()
                .map_err(|_| anyhow!("stderr lock"))?
                .as_slice(),
        )?;
        std::fs::write(
            out.join("receipt.json"),
            serde_json::to_vec_pretty(
                &json!({"status":"PASS","protocolVersion":VERSION,"ownedPid":owned_pid,"transport":"actual CLI HTTP /mcp","independentStreams":2,"sameListenRequestId":true,"resyncConsumedSeparately":true,"networkDisconnect":true,"documentCloseFinalComplete":true,"ownProcessCleanup":true}),
            )?,
        )?;
    }
    Ok(())
}
