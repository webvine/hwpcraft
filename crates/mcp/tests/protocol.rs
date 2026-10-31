use hwpcraft_formats::file_access::AutomationRoots;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

// Real stdio subprocesses isolate each server and its settings override.
// None of these checks can read or write the user's actual settings folder.
mod autotext_roots {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::path::Path;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::mpsc::{self, Receiver};

    const CHILD_OPTIONS: &str = "HWPCRAFT_TEST_AUTOTEXT_MCP_OPTIONS";

    #[test]
    fn stdio_child() {
        let Some(options) = std::env::var_os(CHILD_OPTIONS) else {
            return;
        };
        assert!(std::env::var_os("HWPCRAFT_CONFIG_DIR").is_some());
        let options: Vec<String> = serde_json::from_str(&options.to_string_lossy()).unwrap();
        // Explicitly separate this libtest helper's own banner from the server.
        // The production CLI tests accept JSON on stdout from the first byte.
        println!("\nHWPCRAFT_AUTOTEXT_TEST_SERVER_START");
        std::io::stdout().flush().unwrap();
        hwpcraft_mcp::run_stdio(&options).unwrap();
    }

    struct McpChild {
        process: Child,
        input: Option<ChildStdin>,
        output: Receiver<std::io::Result<String>>,
        next_id: u64,
        current_document: Option<String>,
        pending: std::collections::BTreeMap<String, Value>,
    }

    impl McpChild {
        fn start(config: &Path, read: Option<&Path>, write: Option<&Path>) -> Self {
            let mut options = Vec::new();
            for (flag, root) in [
                ("--automation-read-root", read),
                ("--automation-write-root", write),
            ] {
                if let Some(root) = root {
                    options.push(flag.to_owned());
                    options.push(root.to_string_lossy().into_owned());
                }
            }
            let mut process = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "autotext_roots::stdio_child",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("HWPCRAFT_CONFIG_DIR", config)
                .env(CHILD_OPTIONS, serde_json::to_string(&options).unwrap())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let input = process.stdin.take();
            let stdout = process.stdout.take().unwrap();
            let (sender, output) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            // Only the exact, known libtest banner is allowed before the marker.
            loop {
                let line = output
                    .recv_timeout(DEADLINE)
                    .expect("libtest marker deadline")
                    .unwrap();
                if line == "HWPCRAFT_AUTOTEXT_TEST_SERVER_START" {
                    break;
                }
                assert!(
                    line.is_empty()
                        || line == "running 1 test"
                        || line.trim_end() == "test autotext_roots::stdio_child ...",
                    "unexpected helper stdout before server marker: {line:?}"
                );
            }
            let mut child = Self {
                process,
                input,
                output,
                next_id: 1,
                current_document: None,
                pending: Default::default(),
            };
            let discovery = child.rpc("server/discover", json!({}));
            assert!(
                discovery["supportedVersions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == VERSION)
            );
            child.ok("new_document", json!({}));
            child
        }

        fn send(&mut self, value: Value) {
            let input = self.input.as_mut().unwrap();
            writeln!(input, "{value}").unwrap();
            input.flush().unwrap();
        }

        fn rpc(&mut self, method: &str, params: Value) -> Value {
            let n = self.next_id;
            self.next_id += 1;
            let id = if n.is_multiple_of(2) {
                json!(format!("child-{n}"))
            } else {
                json!(n)
            };
            self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":with_meta(params)}));
            if let Some(response) = self.pending.remove(&id.to_string()) {
                assert!(response.get("error").is_none(), "{response}");
                return response["result"].clone();
            }
            let deadline = std::time::Instant::now() + DEADLINE;
            loop {
                let line = self
                    .output
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .expect("MCP child response deadline")
                    .unwrap();
                // After the explicit helper marker every byte is server JSONL.
                let response: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|error| panic!("non-JSON MCP stdout: {error}: {line:?}"));
                assert_eq!(response["jsonrpc"], "2.0", "{response}");
                if response.get("method").is_some() {
                    assert!(
                        response.get("id").is_none(),
                        "unexpected server request: {response}"
                    );
                    continue;
                }
                assert!(
                    !response["id"].is_null(),
                    "unsolicited protocol error: {response}"
                );
                if response["id"] != id {
                    assert!(
                        self.pending
                            .insert(response["id"].to_string(), response)
                            .is_none()
                    );
                    continue;
                }
                assert!(response.get("error").is_none(), "{response}");
                return response["result"].clone();
            }
        }

        fn tool(&mut self, name: &str, args: Value) -> Value {
            let args = scoped_arguments(self.current_document.as_deref(), name, args);
            let result = self.rpc("tools/call", json!({"name":name,"arguments":args}));
            if result["isError"] == false
                && let Some(id) = result["structuredContent"]["document_id"].as_str()
            {
                self.current_document = Some(id.to_owned());
            }
            result
        }

        fn ok(&mut self, name: &str, args: Value) -> Value {
            let result = self.tool(name, args);
            assert_eq!(result["isError"], false, "{result}");
            engine_value(&result)
        }

        fn execute(&mut self, command: &str, params: Value) -> Value {
            self.ok("execute", json!({"command":command,"params":params}))
        }

        fn denied(&mut self, command: &str, params: Value, reason: &str) {
            let result = self.tool("execute", json!({"command":command,"params":params}));
            assert_eq!(result["isError"], true, "{command}: {result}");
            let message = result["content"][0]["text"].as_str().unwrap();
            assert!(message.contains(reason), "{command}: {message}");
        }

        fn finish(mut self) {
            self.input.take();
            let deadline = std::time::Instant::now() + DEADLINE;
            loop {
                if let Some(status) = self.process.try_wait().unwrap() {
                    assert!(status.success(), "{status}");
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "MCP child EOF cleanup deadline"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Drop for McpChild {
        fn drop(&mut self) {
            self.input.take();
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }

    fn seed(config: &Path) -> Vec<u8> {
        std::fs::create_dir_all(config).unwrap();
        let bytes = serde_json::to_vec(
            &json!({"entries":[{"name":"인사말","abbreviation":"hello","text":"저장한 인사 😀"}]}),
        )
        .unwrap();
        std::fs::write(config.join("autotext.json"), &bytes).unwrap();
        bytes
    }

    fn denied_all(child: &mut McpChild, reason: &str) {
        child.denied("tools.autotext.list", json!({}), reason);
        child.denied("tools.autotext.insert", json!({"name":"인사말"}), reason);
        child.ok("type_text", json!({"text":"hello"}));
        child.denied("tools.autotext.expand", json!({}), reason);
        child.denied(
            "tools.autotext.add",
            json!({"name":"새 항목","text":"새 내용"}),
            reason,
        );
        child.denied("tools.autotext.delete", json!({"name":"인사말"}), reason);
        assert_eq!(child.ok("get_text", json!({}))["text"], "hello");
    }

    #[test]
    fn all_autotext_paths_enforce_roots_for_existing_and_missing_store() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        for existing in [true, false] {
            let config = dir.path().join(if existing {
                "outside-existing"
            } else {
                "outside-missing"
            });
            let original = existing.then(|| seed(&config));
            let mut child = McpChild::start(&config, Some(&allowed), Some(&allowed));
            denied_all(&mut child, "outside the");
            child.finish();
            if let Some(original) = original {
                assert_eq!(
                    std::fs::read(config.join("autotext.json")).unwrap(),
                    original
                );
            } else {
                assert!(!config.exists());
            }
            assert!(!config.join("autotext.json.tmp").exists());
        }
    }

    #[test]
    fn read_and_write_authorities_are_both_required_before_update_reads() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("settings");
        let other = dir.path().join("other");
        std::fs::create_dir(&other).unwrap();
        let original = seed(&config);
        // A read-only store still supports list, insertion and abbreviation expansion.
        let mut child = McpChild::start(&config, Some(&config), None);
        assert_eq!(
            child.execute("tools.autotext.list", json!({}))["entries"][0]["text"],
            "저장한 인사 😀"
        );
        child.execute("tools.autotext.insert", json!({"name":"인사말"}));
        assert_eq!(child.ok("get_text", json!({}))["text"], "저장한 인사 😀");
        child.execute("edit.undo", json!({}));
        child.ok("type_text", json!({"text":"hello"}));
        child.execute("tools.autotext.expand", json!({}));
        assert_eq!(child.ok("get_text", json!({}))["text"], "저장한 인사 😀");
        child.execute("edit.undo", json!({}));
        assert_eq!(child.ok("get_text", json!({}))["text"], "hello");
        child.finish();
        assert_eq!(
            std::fs::read(config.join("autotext.json")).unwrap(),
            original
        );
        // Malformed bytes prove forbidden writes are rejected before parsing the store.
        let malformed = b"{not valid JSON";
        std::fs::write(config.join("autotext.json"), malformed).unwrap();
        for (read, write, reason) in [
            (Some(config.as_path()), None, "write authority is absent"),
            (
                Some(config.as_path()),
                Some(other.as_path()),
                "outside the write root",
            ),
            (None, Some(config.as_path()), "read authority is absent"),
            (
                Some(other.as_path()),
                Some(config.as_path()),
                "outside the read root",
            ),
        ] {
            let mut child = McpChild::start(&config, read, write);
            for command in ["tools.autotext.add", "tools.autotext.delete"] {
                let params = if command.ends_with(".add") {
                    json!({"name":"새 항목","text":"내용"})
                } else {
                    json!({"name":"인사말"})
                };
                child.denied(command, params, reason);
            }
            child.finish();
            assert_eq!(
                std::fs::read(config.join("autotext.json")).unwrap(),
                malformed
            );
            assert!(!config.join("autotext.json.tmp").exists());
        }
        let mut child = McpChild::start(&config, None, Some(&config));
        denied_all(&mut child, "read authority is absent");
        child.finish();
        std::fs::remove_file(config.join("autotext.json")).unwrap();
        let mut child = McpChild::start(&config, Some(&config), None);
        assert_eq!(
            child.execute("tools.autotext.list", json!({}))["entries"],
            json!([])
        );
        child.denied(
            "tools.autotext.add",
            json!({"name":"새 항목","text":"내용"}),
            "write authority is absent",
        );
        child.denied(
            "tools.autotext.delete",
            json!({"name":"없는 항목"}),
            "write authority is absent",
        );
        child.finish();
        assert!(!config.join("autotext.json").exists());
        let mut child = McpChild::start(&config, Some(&config), Some(&config));
        assert_eq!(
            child.execute("tools.autotext.list", json!({}))["entries"],
            json!([])
        );
        child.denied(
            "tools.autotext.delete",
            json!({"name":"없는 항목"}),
            "상용구가 없습니다",
        );
        assert!(!config.join("autotext.json").exists());
        child.execute(
            "tools.autotext.add",
            json!({"name":"새 항목","text":"처음 저장"}),
        );
        child.execute("tools.autotext.delete", json!({"name":"새 항목"}));
        child.finish();
        let stored: Value =
            serde_json::from_slice(&std::fs::read(config.join("autotext.json")).unwrap()).unwrap();
        assert_eq!(stored["entries"], json!([]));
    }

    #[test]
    fn permitted_settings_preserve_legacy_limits_persistence_and_atomic_document_contract() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("settings");
        // No roots: the GUI/CLI-compatible persistent store remains usable.
        let mut child = McpChild::start(&config, None, None);
        assert_eq!(
            child.execute("tools.autotext.list", json!({}))["entries"],
            json!([])
        );
        child.execute(
            "tools.autotext.add",
            json!({"name":"인사말","abbreviation":"hello","text":"첫 인사"}),
        );
        child.execute("tools.autotext.add", json!({"name":"인사말","abbreviation":"hello","text":"수정한 인사 😀","overwrite":true}));
        child.execute(
            "tools.autotext.add",
            json!({"name":"한도","text":"a".repeat(100_000)}),
        );
        let original = std::fs::read(config.join("autotext.json")).unwrap();
        child.denied(
            "tools.autotext.add",
            json!({"name":"초과","text":"a".repeat(100_001)}),
            "100 KB",
        );
        assert_eq!(
            std::fs::read(config.join("autotext.json")).unwrap(),
            original
        );
        child.execute("tools.autotext.delete", json!({"name":"한도"}));
        child.finish();
        // Both authorities: a new process reads, updates and deletes the same store.
        let mut child = McpChild::start(&config, Some(&config), Some(&config));
        let original = std::fs::read(config.join("autotext.json")).unwrap();
        child.ok("type_text", json!({"text":"보존"}));
        child.execute("format.character", json!({"bold":true}));
        let pending = child.execute("format.character.get", json!({}));
        for command in ["tools.autotext.add", "tools.autotext.delete"] {
            let params = if command.ends_with(".add") {
                json!({"name":"새 항목","text":"내용"})
            } else {
                json!({"name":"인사말"})
            };
            let result = child.tool(
                "batch",
                json!({"atomic":true,"commands":[
                    {"command":"text.insert","params":{"text":"실행되면 안 됨"}},
                    {"command":command,"params":params}
                ]}),
            );
            assert_eq!(result["isError"], true, "{result}");
            assert!(
                result["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("atomic batch")
            );
            assert_eq!(child.ok("get_text", json!({}))["text"], "보존");
            assert_eq!(child.execute("format.character.get", json!({})), pending);
            assert_eq!(
                std::fs::read(config.join("autotext.json")).unwrap(),
                original
            );
        }
        // Store reads and document-only insertion remain eligible for rollback.
        let result = child.ok(
            "batch",
            json!({"atomic":true,"commands":[
                {"command":"tools.autotext.list","params":{}},
                {"command":"tools.autotext.insert","params":{"name":"인사말"}}
            ]}),
        );
        assert_eq!(result["rolledBack"], false);
        assert_eq!(
            child.ok("get_text", json!({}))["text"],
            "보존수정한 인사 😀"
        );
        let result = child.tool(
            "batch",
            json!({"atomic":true,"commands":[
                {"command":"tools.autotext.insert","params":{"name":"인사말"}},
                {"command":"not-a-command","params":{}}
            ]}),
        );
        assert_eq!(result["isError"], true);
        let result: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(result["rolledBack"], true);
        assert_eq!(
            child.ok("get_text", json!({}))["text"],
            "보존수정한 인사 😀"
        );
        assert_eq!(
            std::fs::read(config.join("autotext.json")).unwrap(),
            original
        );
        child.execute("tools.autotext.add", json!({"name":"허용","text":"등록"}));
        child.execute("tools.autotext.delete", json!({"name":"허용"}));
        child.finish();
    }
}

const VERSION: &str = "2026-07-28";
const DEADLINE: Duration = Duration::from_secs(30);

fn with_meta(mut params: Value) -> Value {
    params
        .as_object_mut()
        .expect("request params must be an object")
        .insert(
            "_meta".into(),
            json!({"io.modelcontextprotocol/protocolVersion": VERSION,
            "io.modelcontextprotocol/clientCapabilities": {}}),
        );
    params
}

fn scoped_arguments(current: Option<&str>, name: &str, mut args: Value) -> Value {
    let pure = matches!(name, "list_commands" | "list_documents")
        || (name == "hwp_action" && args.get("id").is_none());
    if !pure && args.get("document_id").is_none() {
        if let Some(id) = current {
            args.as_object_mut()
                .expect("tool arguments object")
                .insert("document_id".into(), json!(id));
        } else {
            assert!(
                matches!(
                    name,
                    "new_document" | "open_document" | "open_protected_document"
                ),
                "test client has no explicit document for {name}"
            );
        }
    }
    args
}

fn engine_value(result: &Value) -> Value {
    let mut value = result.get("structuredContent").cloned().unwrap_or_else(|| {
        let text = result["content"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|item| item["text"].as_str())
            .expect("text result");
        serde_json::from_str(text).unwrap()
    });
    // document_id belongs to the protocol envelope, not the engine assertion.
    if let Some(object) = value.as_object_mut() {
        object.remove("document_id");
        if object.len() == 1 && object.contains_key("items") {
            return object.remove("items").unwrap();
        }
    }
    value
}

/// Synchronous test facade over the real async SDK transport. Only this client
/// remembers a selected document: every scoped wire request contains its UUID.
struct TestClient {
    runtime: Option<tokio::runtime::Runtime>,
    input: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    output: tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    server: tokio::task::JoinHandle<anyhow::Result<()>>,
    next_id: u64,
    current_document: Option<String>,
    pending: std::collections::BTreeMap<String, Value>,
    notifications: Vec<Value>,
}

impl TestClient {
    fn new(roots: Option<Arc<AutomationRoots>>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (read, write) = tokio::io::split(server);
        let server = runtime.spawn(hwpcraft_mcp::serve_stream(roots, read, write));
        let (output, input) = tokio::io::split(client);
        Self {
            runtime: Some(runtime),
            input,
            output: tokio::io::BufReader::new(output),
            server,
            next_id: 1,
            current_document: None,
            pending: Default::default(),
            notifications: Vec::new(),
        }
    }

    fn send_bytes(&mut self, bytes: &[u8]) {
        self.runtime.as_ref().unwrap().block_on(async {
            tokio::time::timeout(DEADLINE, async {
                self.input.write_all(bytes).await.unwrap();
                self.input.flush().await.unwrap();
            })
            .await
            .expect("MCP write deadline");
        });
    }

    fn receive(&mut self, id: Option<&Value>) -> Value {
        let key = id.map(Value::to_string);
        if let Some(value) = key.as_ref().and_then(|key| self.pending.remove(key)) {
            return value;
        }
        self.runtime.as_ref().unwrap().block_on(async {
            tokio::time::timeout(DEADLINE, async {
                loop {
                    let mut line = String::new();
                    assert_ne!(
                        self.output.read_line(&mut line).await.unwrap(),
                        0,
                        "MCP stream closed before response {id:?}"
                    );
                    let value: Value = serde_json::from_str(&line)
                        .unwrap_or_else(|error| panic!("non-JSON stdout: {error}: {line:?}"));
                    assert_eq!(value["jsonrpc"], "2.0", "{value}");
                    if value.get("method").is_some() {
                        assert!(
                            value.get("id").is_none(),
                            "unexpected server request: {value}"
                        );
                        self.notifications.push(value);
                        continue;
                    }
                    let response_id = value.get("id").filter(|id| !id.is_null());
                    if id == response_id {
                        return value;
                    }
                    assert!(response_id.is_some(), "unsolicited error: {value}");
                    assert!(
                        self.pending
                            .insert(response_id.unwrap().to_string(), value)
                            .is_none(),
                        "duplicate response ID"
                    );
                }
            })
            .await
            .expect("MCP response deadline")
        })
    }

    fn exact(&mut self, id: Value, method: &str, params: Value) -> Value {
        let mut bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,
            "method":method,"params":params}))
        .unwrap();
        bytes.push(b'\n');
        self.send_bytes(&bytes);
        self.receive(Some(&id))
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        // Alternate string and integer IDs so response routing cannot conflate them.
        let n = self.next_id;
        self.next_id += 1;
        let id = if n.is_multiple_of(2) {
            json!(format!("request-{n}"))
        } else {
            json!(n)
        };
        self.exact(id, method, with_meta(params))
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.block_on(async {
                let _ = self.input.shutdown().await;
                if tokio::time::timeout(Duration::from_secs(5), &mut self.server)
                    .await
                    .is_err()
                {
                    self.server.abort();
                    let _ = (&mut self.server).await;
                }
            });
            runtime.shutdown_timeout(Duration::from_secs(2));
        }
    }
}

fn rpc(s: &mut TestClient, method: &str, params: Value) -> Value {
    s.rpc(method, params)
}
fn discover_and_create(s: &mut TestClient) {
    // MCP 2026 is stateless: discovery replaces the removed initialize handshake.
    let r = rpc(s, "server/discover", json!({}));
    assert!(r.get("error").is_none(), "{r}");
    assert!(
        r["result"]["supportedVersions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == VERSION)
    );
    assert_eq!(r["result"]["resultType"], "complete");
    let info = &r["result"]["_meta"]["io.modelcontextprotocol/serverInfo"];
    assert_eq!(info["name"], "hwpcraft");
    assert_eq!(info["version"], hwpcraft_engine::VERSION.trim());
    tool(s, "new_document", json!({}));
}
fn server() -> TestClient {
    let mut s = TestClient::new(None);
    discover_and_create(&mut s);
    s
}
fn raw_tool(s: &mut TestClient, name: &str, args: Value) -> Value {
    let args = scoped_arguments(s.current_document.as_deref(), name, args);
    let frame = rpc(s, "tools/call", json!({"name":name,"arguments":args}));
    if let Some(error) = frame.get("error") {
        // Schema violations are protocol errors in MCP 2026, rather than engine
        // tool failures. Keep the original negative assertions explicit.
        assert_eq!(error["code"], -32602, "unexpected protocol error: {frame}");
        return json!({"isError":true,"rpcError":error,
            "content":[{"type":"text","text":error.to_string()}]});
    }
    let result = frame["result"].clone();
    if result["isError"] == false
        && let Some(id) = result["structuredContent"]["document_id"].as_str()
    {
        s.current_document = Some(id.to_owned());
    }
    result
}
fn tool(s: &mut TestClient, name: &str, args: Value) -> Value {
    let r = raw_tool(s, name, args);
    assert_eq!(r["isError"], false, "{name}: {r}");
    engine_value(&r)
}
fn execute(s: &mut TestClient, command: &str, params: Value) -> Value {
    tool(s, "execute", json!({"command":command,"params":params}))
}

#[test]
fn memo_and_nested_table_workflow_survives_both_formats_through_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"검토 본문"}));
    tool(&mut s, "select_text", json!({"text":"검토"}));
    let memo = execute(
        &mut s,
        "memo.create",
        json!({"text":"확인 😀","author":"검토자"}),
    );
    let id = memo["id"].clone();
    execute(
        &mut s,
        "memo.update",
        json!({"id":id,"text":"수정한 확인 😀"}),
    );
    let mut address = json!({"section":0,"paragraph":0,"path":[]});
    for depth in 1..=2 {
        let made = execute(
            &mut s,
            "context.table.create",
            json!({"address":address,"rows":1,"columns":1}),
        );
        address = made["address"].clone();
        assert_eq!(address["path"].as_array().unwrap().len(), depth);
    }
    execute(
        &mut s,
        "context.replace",
        json!({"address":address,"start":0,"end":0,"text":"깊은 셀 😀"}),
    );
    execute(
        &mut s,
        "context.character",
        json!({"address":address,"format":{"bold":true,"fontSize":1400}}),
    );
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("review-context.{extension}"));
        tool(&mut s, "save_document", json!({"path":path}));
        tool(&mut s, "new_document", json!({"force":true}));
        tool(&mut s, "open_document", json!({"path":path}));
        let item = execute(&mut s, "memo.get", json!({"id":id}));
        assert_eq!(item["text"], "수정한 확인 😀", "{extension}");
        assert_eq!(item["author"], "검토자");
        assert_eq!(item["anchor"], "검토");
        let listing = execute(&mut s, "context.list", json!({"limit":100}));
        let cell = listing["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["text"] == "깊은 셀 😀")
            .unwrap();
        assert_eq!(cell["depth"], 2);
        let inspected = execute(&mut s, "context.get", json!({"address":cell["address"]}));
        assert_eq!(inspected["character"]["bold"], true);
        assert_eq!(inspected["character"]["fontSize"], 1400);
        let png = raw_tool(&mut s, "render_page", json!({"page":1,"scale":0.5}));
        assert_eq!(png["isError"], false);
        assert_eq!(png["content"][0]["type"], "image");
    }
    execute(&mut s, "memo.delete", json!({"id":id}));
    assert_eq!(execute(&mut s, "memo.list", json!({})), json!([]));
    execute(&mut s, "edit.undo", json!({}));
    assert_eq!(
        execute(&mut s, "memo.get", json!({"id":id}))["anchor"],
        "검토"
    );
}
fn fails(s: &mut TestClient, name: &str, args: Value) {
    let r = raw_tool(s, name, args);
    assert_eq!(r["isError"], true, "{name}: {r}");
}

#[test]
fn typed_regions_edit_header_note_and_textbox_through_mcp() {
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"영역 본문"}));
    execute(
        &mut s,
        "page.header_footer.text",
        json!({"header":true,"text":"머리말 영역"}),
    );
    execute(
        &mut s,
        "note.insert",
        json!({"endnote":false,"paragraphs":["각주 영역"]}),
    );
    execute(
        &mut s,
        "shape.insert",
        json!({"kind":"textbox","width_mm":80.0,"height_mm":30.0,"paragraphs":["상자 영역"]}),
    );
    for kind in ["header", "footnote", "textbox"] {
        let list = execute(&mut s, "context.v2.list", json!({"limit":100}));
        let row = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == kind)
            .unwrap();
        let address = row["address"].clone();
        let state = execute(&mut s, "context.v2.get", json!({"address":address}));
        let text = state["text"].as_str().unwrap();
        let byte = text.find("영역").unwrap();
        let start = text[..byte].chars().count();
        let edited = execute(
            &mut s,
            "context.v2.replace",
            json!({"address":address,"start":start,"end":start+2,"text":"확인😀","expected_generation":state["generation"]}),
        );
        let formatted = execute(
            &mut s,
            "context.v2.character",
            json!({"address":address,"start":start,"end":start+3,"format":{"bold":true,"fontSize":1400},"expected_generation":edited["generation"]}),
        );
        fails(
            &mut s,
            "execute",
            json!({"command":"context.v2.replace","params":{"address":address,"start":0,"end":0,"text":"stale","expected_generation":state["generation"]}}),
        );
        let inspected = execute(
            &mut s,
            "context.v2.get",
            json!({"address":address,"offset":start}),
        );
        assert_eq!(inspected["generation"], formatted["generation"]);
        assert!(inspected["text"].as_str().unwrap().contains("확인😀"));
        assert_eq!(inspected["character"]["bold"], true);
        assert_eq!(inspected["character"]["fontSize"], 1400);
    }
    let dir = tempfile::tempdir().unwrap();
    for extension in ["hwp", "hwpx"] {
        let path = dir.path().join(format!("regions.{extension}"));
        tool(&mut s, "save_document", json!({"path":path}));
        if std::env::var_os("HWPCRAFT_WRITE_REGION_FIXTURES").is_some() {
            let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../output/implementation/0.17/fixtures");
            std::fs::create_dir_all(&output).unwrap();
            std::fs::copy(&path, output.join(format!("regions.{extension}"))).unwrap();
        }
        tool(&mut s, "new_document", json!({"force":true}));
        tool(&mut s, "open_document", json!({"path":path}));
        let rows = execute(&mut s, "context.v2.list", json!({"limit":100}));
        for kind in ["header", "footnote", "textbox"] {
            assert!(
                rows["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["kind"] == kind
                        && row["text"].as_str().unwrap().contains("확인😀")),
                "{extension} {kind}: {rows}"
            );
        }
        let png = raw_tool(&mut s, "render_page", json!({"page":1,"scale":0.5}));
        assert_eq!(png["isError"], false);
        assert_eq!(png["content"][0]["type"], "image");
    }
}

#[test]
fn lifecycle_errors_resources_and_bounded_framing() {
    let mut s = TestClient::new(None);
    // There is no initialize state or implicit document in MCP 2026.
    let missing = s.exact(json!("missing-meta"), "tools/list", json!({}));
    assert_eq!(missing["error"]["code"], -32602, "{missing}");
    for old in ["2025-06-18", "1900-01-01"] {
        let mut params = with_meta(json!({}));
        params["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!(old);
        let rejected = s.exact(json!(format!("old-{old}")), "tools/list", params);
        assert_eq!(rejected["error"]["code"], -32022, "{rejected}");
        assert!(
            rejected["error"]["data"]["supported"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == VERSION)
        );
    }
    let tools = rpc(&mut s, "tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    for name in [
        "new_document",
        "open_document",
        "open_protected_document",
        "execute",
        "batch",
        "render_page",
        "close_document",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing {name}"
        );
    }
    let missing_handle = rpc(
        &mut s,
        "tools/call",
        json!({"name":"get_text","arguments":{}}),
    );
    assert_eq!(missing_handle["error"]["code"], -32602);
    // Pure queries are explicitly handle-free, including before any document.
    for (name, args) in [
        ("list_commands", json!({"query":"format.bold"})),
        ("hwp_action", json!({"query":"CharShapeBold"})),
    ] {
        let result = rpc(&mut s, "tools/call", json!({"name":name,"arguments":args}));
        assert_eq!(result["result"]["isError"], false, "{result}");
        assert!(
            result["result"]["structuredContent"]
                .get("document_id")
                .is_none()
        );
    }
    discover_and_create(&mut s);
    let id = s.current_document.clone().unwrap();
    assert_eq!(
        rpc(&mut s, "resources/list", json!({}))["result"]["resources"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let templates = rpc(&mut s, "resources/templates/list", json!({}));
    assert!(
        templates
            .to_string()
            .contains("hwpcraft://documents/{document_id}")
    );
    for uri in [
        format!("hwpcraft://documents/{id}"),
        "hwpcraft://commands".into(),
        "hwpcraft://hwp-actions".into(),
        "hwpcraft://capabilities".into(),
    ] {
        let r = rpc(&mut s, "resources/read", json!({"uri":uri}));
        assert!(
            serde_json::from_str::<Value>(r["result"]["contents"][0]["text"].as_str().unwrap())
                .is_ok()
        );
    }
    let missing = rpc(&mut s, "resources/read", json!({"uri":"missing"}));
    assert!(missing.get("error").is_some(), "{missing}");
    assert_ne!(
        missing["error"]["code"], -32002,
        "removed legacy error code"
    );
    assert_eq!(
        rpc(&mut s, "tools/call", json!({"name":"screenshot"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        rpc(
            &mut s,
            "tools/call",
            json!({"name":"type_text","arguments":[]})
        )["error"]["code"],
        // The SDK routes a request that cannot decode as CallToolRequest to
        // the unsupported custom-request branch. Valid tool input with a
        // schema violation is separately asserted as -32602 by raw_tool.
        -32601
    );
    assert_eq!(rpc(&mut s, "missing", json!({}))["error"]["code"], -32601);
    assert_eq!(rpc(&mut s, "ping", json!({}))["error"]["code"], -32601);
    for malformed in [
        "[]",
        "null",
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
    ] {
        s.send_bytes(format!("{malformed}\n").as_bytes());
        let expected = if malformed.contains("\"id\":1") {
            Some(json!(1))
        } else {
            None
        };
        let response = s.receive(expected.as_ref());
        assert_eq!(response["error"]["code"], -32600, "{response}");
    }
    s.send_bytes(b"not json\n\xff\n");
    assert_eq!(s.receive(None)["error"]["code"], -32700);
    assert_eq!(s.receive(None)["error"]["code"], -32700);
    assert!(
        rpc(&mut s, "server/discover", json!({}))
            .get("result")
            .is_some()
    );
    let mut huge = vec![b'x'; 4 * 1024 * 1024 + 2];
    huge.push(b'\n');
    s.send_bytes(&huge);
    assert_eq!(s.receive(None)["error"]["code"], -32600);
    assert!(
        rpc(&mut s, "server/discover", json!({}))
            .get("result")
            .is_some()
    );
    // Typed response demultiplexing keeps numeric 7 distinct from string "7",
    // even when the caller awaits them in reverse order.
    let mut pipelined = Vec::new();
    for id in [json!(7), json!("7")] {
        pipelined.extend(
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,
            "method":"server/discover","params":with_meta(json!({}))}))
            .unwrap(),
        );
        pipelined.push(b'\n');
    }
    s.send_bytes(&pipelined);
    assert_eq!(s.receive(Some(&json!("7")))["id"], "7");
    assert_eq!(s.receive(Some(&json!(7)))["id"], 7);
    // Handle-free create makes an independent document, not a connection-global
    // replacement. It never changes this test client's selected document.
    tool(&mut s, "type_text", json!({"text":"FIRST"}));
    let created = rpc(
        &mut s,
        "tools/call",
        json!({"name":"new_document","arguments":{}}),
    );
    assert_eq!(created["result"]["isError"], false, "{created}");
    let second = created["result"]["structuredContent"]["document_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(second, id);
    let other = rpc(
        &mut s,
        "tools/call",
        json!({"name":"get_text","arguments":{"document_id":second}}),
    );
    assert_eq!(engine_value(&other["result"])["text"], "");
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], "FIRST");
    let closed = rpc(
        &mut s,
        "tools/call",
        json!({"name":"close_document","arguments":{"document_id":second}}),
    );
    assert_eq!(closed["result"]["isError"], false, "{closed}");
    let stale = rpc(
        &mut s,
        "tools/call",
        json!({"name":"get_text","arguments":{"document_id":second}}),
    );
    assert_eq!(stale["result"]["isError"], true, "{stale}");
    // Missing handles remain rejected even after this connection created a doc.
    let missing = rpc(
        &mut s,
        "tools/call",
        json!({"name":"type_text","arguments":{"text":"MUST NOT INSERT"}}),
    );
    assert_eq!(missing["error"]["code"], -32602);
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], "FIRST");
    // EOF is a framing boundary too: oversized and unterminated records must
    // produce an error rather than disappearing or executing a partial record.
    for (bytes, code) in [
        (vec![b'x'; 4 * 1024 * 1024 + 2], -32600),
        (b"{\"jsonrpc\":\"2.0\"}".to_vec(), -32700),
    ] {
        let mut eof = TestClient::new(None);
        eof.send_bytes(&bytes);
        eof.runtime
            .as_ref()
            .unwrap()
            .block_on(eof.input.shutdown())
            .unwrap();
        assert_eq!(eof.receive(None)["error"]["code"], code);
    }
}

#[test]
fn command_discovery_has_examples_for_every_exposed_command() {
    let mut s = server();
    let commands = tool(&mut s, "list_commands", json!({}));
    let commands = commands.as_array().unwrap();
    assert!(commands.len() >= 140);
    let mut ids = std::collections::BTreeSet::new();
    for command in commands {
        assert!(
            ids.insert(command["id"].as_str().unwrap()),
            "duplicate: {command}"
        );
        assert!(
            command["paramsExample"].is_object(),
            "missing parameter example: {command}"
        );
    }
    let queried = tool(&mut s, "list_commands", json!({"query":"FORMAT.BOLD"}));
    assert_eq!(queried.as_array().unwrap().len(), 1);
    assert_eq!(queried[0]["id"], "format.bold");
}

#[test]
fn published_examples_edit_formatting_page_and_table() {
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"보고서 예제"}));
    let commands = tool(&mut s, "list_commands", json!({}));
    let example = |id: &str| {
        commands
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap_or_else(|| panic!("command missing from discovery: {id}"))["paramsExample"]
            .clone()
    };
    for id in [
        "format.character",
        "format.paragraph",
        "format.style.create",
        "format.numbering.create",
        "format.numbering.apply",
        "format.outline.shape",
        "format.numbering.restart",
        "format.paragraph.apply",
        "page.setup",
        "page.columns",
        "page.header_footer",
        "page.number",
        "page.hide",
        "page.border_fill",
    ] {
        execute(&mut s, id, example(id));
    }
    execute(&mut s, "insert.table", json!({"rows":3,"columns":3}));
    let document = tool(&mut s, "inspect_document", json!({}));
    let cell = document["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["cursor"]["cell"].is_object())
        .nth(8)
        .unwrap()["cursor"]
        .clone();
    execute(&mut s, "selection.set", json!({"cursor":cell}));
    execute(
        &mut s,
        "table.properties",
        json!({"props":{"hasCaption":true}}),
    );
    for id in [
        "table.properties",
        "table.cell_properties",
        "table.style",
        "table.formula",
        "table.caption_text",
    ] {
        execute(&mut s, id, example(id));
    }
}

#[test]
fn korean_edit_format_table_roundtrip_and_png_through_mcp_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = server();
    tool(&mut s, "new_document", json!({}));
    tool(
        &mut s,
        "type_text",
        json!({"text":"가😀한글 보고서\n두 번째 한글 문단"}),
    );
    let selected = tool(&mut s, "select_text", json!({"text":"한글"}));
    assert_eq!(selected["anchor"], 2);
    assert_eq!(selected["cursor"]["offset"], 4);
    execute(
        &mut s,
        "format.character",
        json!({"bold":true,"fontSize":1400}),
    );
    assert_eq!(
        execute(&mut s, "format.character.get", json!({}))["bold"],
        true
    );
    tool(&mut s, "select_text", json!({"text":"한글","occurrence":2}));
    tool(&mut s, "type_text", json!({"text":"수정"}));
    execute(&mut s, "edit.undo", json!({}));
    execute(&mut s, "edit.redo", json!({}));
    let doc = tool(&mut s, "inspect_document", json!({"limit":1}));
    assert_eq!(doc["blocks"][0]["text"], "가😀한글 보고서");
    assert_eq!(doc["nextStart"], 1);
    assert_eq!(
        tool(&mut s, "get_text", json!({"start":1}))["text"],
        "두 번째 수정 문단"
    );
    execute(
        &mut s,
        "selection.set",
        json!({"cursor":{"section":0,"paragraph":1,"offset":10,"cell":null}}),
    );
    execute(&mut s, "paragraph.split", json!({}));
    execute(&mut s, "insert.table", json!({"rows":2,"columns":2}));
    let doc = tool(&mut s, "inspect_document", json!({}));
    let cell = doc["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["cursor"]["cell"].is_object())
        .unwrap()["cursor"]
        .clone();
    execute(&mut s, "selection.set", json!({"cursor":cell}));
    tool(&mut s, "type_text", json!({"text":"표 셀 내용"}));
    let text = tool(&mut s, "get_text", json!({}))["text"].clone();
    for extension in ["hwpx", "hwp"] {
        let path = dir.path().join(format!("한글 문서.{extension}"));
        tool(&mut s, "save_document", json!({"path":path}));
        tool(&mut s, "new_document", json!({}));
        tool(&mut s, "open_document", json!({"path":path}));
        assert_eq!(tool(&mut s, "get_text", json!({}))["text"], text);
        tool(&mut s, "select_text", json!({"text":"한글"}));
        assert_eq!(
            execute(&mut s, "format.character.get", json!({}))["bold"],
            true
        );
    }
    for extension in ["pdf", "txt"] {
        let path = dir.path().join(format!("내보내기.{extension}"));
        tool(&mut s, "save_document", json!({"path":path}));
        assert!(std::fs::metadata(path).unwrap().len() > 0);
    }
    let png = raw_tool(&mut s, "render_page", json!({"page":1,"scale":0.25}));
    assert_eq!(png["isError"], false, "{png}");
    assert_eq!(png["content"][0]["mimeType"], "image/png");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png["content"][0]["data"].as_str().unwrap())
        .unwrap();
    assert!(image::load_from_memory(&bytes).is_ok());
}

#[test]
fn failed_calls_preserve_document_and_batch_reports_partial_results() {
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"보존"}));
    let before = tool(&mut s, "inspect_document", json!({}));
    for (name, args) in [
        ("new_document", json!({})),
        ("new_document", json!({"force":"true"})),
        ("open_document", json!({"path":"missing.hwpx","force":true})),
        ("select_text", json!({"text":"없는 글"})),
        ("select_text", json!({"text":"보존","occurrence":0})),
        (
            "execute",
            json!({"command":"format.character","params":{"madeUp":true}}),
        ),
        ("render_page", json!({"page":0})),
        ("render_page", json!({"scale":999})),
        ("get_text", json!({"limit":0})),
        ("type_text", json!({"text":123})),
    ] {
        fails(&mut s, name, args);
    }
    assert_eq!(tool(&mut s, "inspect_document", json!({})), before);
    let r = raw_tool(
        &mut s,
        "batch",
        json!({"commands":[{"command":"text.insert","params":{"text":"A"}},{"command":"not-a-command"},{"command":"text.insert","params":{"text":"B"}}]}),
    );
    assert_eq!(r["isError"], true);
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], "보존A");
    let r = raw_tool(
        &mut s,
        "batch",
        json!({"keepGoing":true,"commands":[{"command":"not-a-command"},{"command":"text.insert","params":{"text":"B"}}]}),
    );
    assert_eq!(r["isError"], true);
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], "보존AB");
    fails(
        &mut s,
        "batch",
        json!({"commands":[{"command":"text.insert","params":{"text":"C"}},{"command":"text.insert","params":[]}]}),
    );
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], "보존AB");
    tool(&mut s, "new_document", json!({"force":true}));
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], "");
}

#[test]
fn roots_apply_to_tools_aliases_batch_pictures_and_printing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in");
    let out = dir.path().join("out");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&out).unwrap();
    let seed = input.join("seed.hwpx");
    let mut initial = server();
    tool(&mut initial, "save_document", json!({"path":seed}));
    let outside = dir.path().join("outside.hwpx");
    std::fs::copy(&seed, &outside).unwrap();
    let roots = AutomationRoots::new(Some(&input), Some(&out)).unwrap();
    let mut s = TestClient::new(roots);
    discover_and_create(&mut s);
    tool(&mut s, "open_document", json!({"path":seed}));
    fails(&mut s, "open_document", json!({"path":outside}));
    fails(&mut s, "save_document", json!({}));
    for command in ["file.save", "file.save_as", "file.pdf", "file.print_pdf"] {
        let params = if command == "file.print_pdf" {
            json!({"path":dir.path().join("forbidden.pdf"),"pages":"all"})
        } else {
            json!({"path":dir.path().join("forbidden.pdf")})
        };
        fails(
            &mut s,
            "execute",
            json!({"command":command,"params":params}),
        );
        assert!(!dir.path().join("forbidden.pdf").exists());
    }
    let png = dir.path().join("outside.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(&png)
        .unwrap();
    for command in ["insert.picture", "picture.insert"] {
        fails(
            &mut s,
            "batch",
            json!({"commands":[{"command":command,"params":{"path":png}}]}),
        );
    }
    tool(
        &mut s,
        "save_document",
        json!({"path":out.join("new/deep/ok.hwpx")}),
    );
    execute(
        &mut s,
        "file.print_pdf",
        json!({"path":out.join("print.pdf"),"pages":"all"}),
    );
    assert!(out.join("print.pdf").exists());
    let mut read_only = TestClient::new(AutomationRoots::new(Some(&input), None).unwrap());
    discover_and_create(&mut read_only);
    fails(
        &mut read_only,
        "save_document",
        json!({"path":input.join("no.hwpx")}),
    );
    assert!(!input.join("no.hwpx").exists());
}

#[test]
fn unsupported_save_never_truncates_and_exports_keep_dirty_state() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("keep.odt");
    std::fs::write(&target, b"preserve").unwrap();
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"보존"}));
    fails(&mut s, "save_document", json!({"path":target}));
    assert_eq!(std::fs::read(target).unwrap(), b"preserve");
    tool(
        &mut s,
        "save_document",
        json!({"path":dir.path().join("out.txt")}),
    );
    assert_eq!(
        tool(&mut s, "inspect_document", json!({}))["document"]["dirty"],
        true
    );
}

#[test]
fn public_parameter_set_adapter_is_discoverable_and_atomic() {
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"공개 인자"}));
    tool(&mut s, "select_text", json!({"text":"공개 인자"}));
    tool(
        &mut s,
        "hwp_action",
        json!({"id":"CharShape","parameterSet":{"Height":1800,"Bold":1,"TextColor":0x563412}}),
    );
    let shape = execute(&mut s, "format.character.get", json!({}));
    assert_eq!(shape["fontSize"], 1800);
    assert_eq!(shape["bold"], true);
    assert_eq!(shape["textColor"], "#123456");
    let before = tool(&mut s, "get_text", json!({}));
    fails(
        &mut s,
        "hwp_action",
        json!({"id":"CharShape","parameterSet":{"Height":1200,"Unexpected":true}}),
    );
    assert_eq!(execute(&mut s, "format.character.get", json!({})), shape);
    assert_eq!(tool(&mut s, "get_text", json!({})), before);
    let actions = tool(&mut s, "hwp_action", json!({"query":"CharShape"}));
    let entry = actions
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "CharShape")
        .unwrap();
    assert_eq!(entry["parameterSet"]["name"], "CharShape");
    assert_eq!(entry["parameterSet"]["complete"], false);
}

#[test]
fn mcp_png_renders_highlights_and_undo_removes_them() {
    use base64::Engine;
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"형광펜 PNG 출력"}));
    tool(&mut s, "select_text", json!({"text":"형광펜"}));
    execute(&mut s, "format.highlight", json!({"color":"#ff00ff"}));
    let magenta_pixels = |s: &mut TestClient| {
        let r = raw_tool(s, "render_page", json!({"page":1,"scale":1.0}));
        assert_eq!(r["isError"], false);
        let image = r["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["type"] == "image")
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(image["data"].as_str().unwrap())
            .unwrap();
        let rgba = image::load_from_memory(&bytes).unwrap().to_rgba8();
        rgba.pixels()
            .filter(|p| {
                p[0] > 180
                    && p[2] > 180
                    && p[0].saturating_sub(p[1]) > 30
                    && p[2].saturating_sub(p[1]) > 30
            })
            .count()
    };
    assert!(magenta_pixels(&mut s) > 50);
    execute(&mut s, "edit.undo", json!({}));
    assert_eq!(magenta_pixels(&mut s), 0);
}

#[test]
fn public_document_and_previous_paragraph_actions_keep_selection_contracts() {
    let mut s = server();
    tool(&mut s, "type_text", json!({"text":"앞😀 문단\n뒤 문단"}));
    let r = tool(&mut s, "hwp_action", json!({"id":"MovePrevParaEnd"}));
    assert_eq!(r["mapping"], "mapped");
    assert_eq!(r["result"]["cursor"]["paragraph"], 0);
    assert_eq!(r["result"]["cursor"]["offset"], 5);
    tool(&mut s, "hwp_action", json!({"id":"MoveDocEnd"}));
    let r = tool(&mut s, "hwp_action", json!({"id":"MoveSelPrevParaEnd"}));
    assert_eq!(r["result"]["cursor"]["offset"], 5);
    assert_eq!(
        tool(&mut s, "get_selection", json!({}))["text"],
        "\n뒤 문단"
    );
    let r = tool(&mut s, "hwp_action", json!({"id":"MoveDocBegin"}));
    assert_eq!(r["result"]["cursor"]["offset"], 0);
    assert_eq!(
        tool(&mut s, "get_selection", json!({}))["hasSelection"],
        false
    );
    tool(&mut s, "hwp_action", json!({"id":"MoveSelDocEnd"}));
    assert_eq!(
        tool(&mut s, "get_selection", json!({}))["text"],
        "앞😀 문단\n뒤 문단"
    );
    tool(&mut s, "hwp_action", json!({"id":"MoveDocEnd"}));
    tool(&mut s, "hwp_action", json!({"id":"MoveSelDocBegin"}));
    assert_eq!(
        tool(&mut s, "get_selection", json!({}))["text"],
        "앞😀 문단\n뒤 문단"
    );
    // Existing automation clients retain the longer aliases as well.
    tool(&mut s, "hwp_action", json!({"id":"MoveDocumentBegin"}));
    let r = tool(&mut s, "hwp_action", json!({"id":"MoveDocumentEnd"}));
    assert_eq!(r["result"]["cursor"]["paragraph"], 1);
    assert_eq!(r["result"]["cursor"]["offset"], 4);
}

#[test]
fn hangul_action_aliases_selection_and_atomic_batch() {
    let mut s = server();
    tool(
        &mut s,
        "type_text",
        json!({"text":"첫째 줄 낱말
둘째"}),
    );
    // Listing and lookup of Hangul automation names.
    let listed = tool(&mut s, "hwp_action", json!({"query":"movelineend"}));
    assert_eq!(listed[0]["id"], "MoveLineEnd");
    assert_eq!(listed[0]["command"], "caret.move");
    let r = tool(&mut s, "hwp_action", json!({"id":"MoveDocumentBegin"}));
    assert_eq!(r["command"], "caret.move");
    assert_eq!(r["result"]["cursor"]["paragraph"], 0);
    // Defaults merge under the caller's params.
    let r = tool(
        &mut s,
        "hwp_action",
        json!({"id":"MoveSelNextWord","params":{"extend":false}}),
    );
    assert_eq!(r["result"]["anchor"], Value::Null);
    tool(&mut s, "hwp_action", json!({"id":"DeleteWord"}));
    fails(&mut s, "hwp_action", json!({"id":"NoSuchHangulAction"}));
    fails(
        &mut s,
        "hwp_action",
        json!({"id":"MoveLineEnd","params":[]}),
    );
    let selection = tool(&mut s, "get_selection", json!({}));
    assert_eq!(selection["hasSelection"], false);
    // list_commands exposes the reverse index.
    let commands = tool(&mut s, "list_commands", json!({"query":"caret.move"}));
    assert!(
        commands[0]["hwpActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "MoveLineEnd")
    );

    // An atomic batch rolls everything back on failure and keeps nothing.
    let before = tool(&mut s, "get_text", json!({}))["text"].clone();
    let r = raw_tool(
        &mut s,
        "batch",
        json!({"atomic":true,"commands":[
            {"command":"text.insert","params":{"text":"추가"}},
            {"command":"paragraph.split"},
            {"command":"caret.move","params":{"motion":"nowhere"}}
        ]}),
    );
    assert_eq!(r["isError"], true);
    let body: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["rolledBack"], true);
    assert_eq!(body["results"].as_array().unwrap().len(), 3);
    assert_eq!(tool(&mut s, "get_text", json!({}))["text"], before);
    assert_eq!(
        execute(&mut s, "document.inspect", json!({"limit":1}))["document"]["dirty"],
        true
    );
    // A successful atomic batch keeps its work.
    let r = tool(
        &mut s,
        "batch",
        json!({"atomic":true,"commands":[{"command":"text.insert","params":{"text":"유지"}}]}),
    );
    assert_eq!(r["rolledBack"], false);
    assert!(
        tool(&mut s, "get_text", json!({}))["text"]
            .as_str()
            .unwrap()
            .contains("유지")
    );
    fails(
        &mut s,
        "batch",
        json!({"atomic":true,"commands":[{"command":"file.new","params":{"force":true}}]}),
    );
}

/// The project goal: an agent writes a short Korean report through the MCP alone
/// and ends with saved HWPX, a PDF and a rendered page.
#[test]
fn korean_report_authored_end_to_end_through_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = server();
    tool(&mut s, "new_document", json!({}));
    tool(
        &mut s,
        "type_text",
        json!({"text":"주간 업무 보고서
1. 개요
이번 주 진행 내용을 정리합니다.
2. 실적"}),
    );
    // Title: bold, larger, centred — via Hangul action names and engine commands.
    tool(&mut s, "select_text", json!({"text":"주간 업무 보고서"}));
    tool(&mut s, "hwp_action", json!({"id":"CharShapeBold"}));
    execute(&mut s, "format.character", json!({"fontSize":1800}));
    tool(
        &mut s,
        "hwp_action",
        json!({"id":"ParagraphShapeAlignCenter"}),
    );
    // Heading numbering on "1. 개요".
    tool(&mut s, "select_text", json!({"text":"1. 개요"}));
    execute(
        &mut s,
        "format.numbering.apply",
        json!({"kind":"number","level":0}),
    );
    // A table with a caption at the end, a footnote, a footer page number and today's date.
    tool(&mut s, "hwp_action", json!({"id":"MoveDocumentEnd"}));
    execute(&mut s, "paragraph.split", json!({}));
    let table = execute(&mut s, "insert.table", json!({"rows":2,"columns":3}))["table"].clone();
    assert!(
        table.is_object(),
        "insert.table returns the new table's address"
    );
    let doc = tool(&mut s, "inspect_document", json!({}));
    let cell = doc["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["cursor"]["cell"].is_object())
        .unwrap()["cursor"]
        .clone();
    execute(&mut s, "selection.set", json!({"cursor":cell}));
    tool(&mut s, "type_text", json!({"text":"항목"}));
    execute(
        &mut s,
        "table.properties",
        json!({"table":table,"props":{"hasCaption":true}}),
    );
    execute(
        &mut s,
        "table.caption_text",
        json!({"table":table,"text":"표 1 주간 실적"}),
    );
    tool(&mut s, "hwp_action", json!({"id":"MoveDocumentBegin"}));
    tool(&mut s, "hwp_action", json!({"id":"MoveLineEnd"}));
    execute(
        &mut s,
        "insert.footnote",
        json!({"text":"작성 기준일은 보고서 날짜입니다."}),
    );
    execute(
        &mut s,
        "page.number",
        json!({"position":"bottom_center","format":0,"dash":false}),
    );
    tool(&mut s, "hwp_action", json!({"id":"MoveDocumentEnd"}));
    let r = tool(
        &mut s,
        "batch",
        json!({"atomic":true,"commands":[
            {"command":"paragraph.split"},
            {"command":"insert.auto_text","params":{"kind":"date","format":"%Y-%m-%d"}}
        ]}),
    );
    assert_eq!(r["rolledBack"], false);
    let stats = execute(&mut s, "document.statistics", json!({}));
    assert_eq!(stats["tables"], 1);
    assert_eq!(stats["footnotes"], 1);
    assert!(stats["words"].as_u64().unwrap() >= 8);
    // Outputs: HWPX, PDF, inline PNG.
    let hwpx = dir.path().join("주간 보고서.hwpx");
    tool(&mut s, "save_document", json!({"path":hwpx}));
    let pdf = dir.path().join("주간 보고서.pdf");
    tool(&mut s, "save_document", json!({"path":pdf}));
    assert!(std::fs::metadata(&pdf).unwrap().len() > 1000);
    let page = raw_tool(&mut s, "render_page", json!({"page":1,"scale":0.5}));
    assert_eq!(page["isError"], false);
    assert_eq!(page["content"][0]["type"], "image");
    // Re-open and verify the structure survived.
    tool(&mut s, "new_document", json!({"force":true}));
    tool(&mut s, "open_document", json!({"path":hwpx}));
    let text = tool(&mut s, "get_text", json!({}))["text"].clone();
    assert!(text.as_str().unwrap().starts_with("주간 업무 보고서"));
    assert_eq!(
        execute(&mut s, "document.statistics", json!({}))["tables"],
        1
    );
    tool(&mut s, "select_text", json!({"text":"주간 업무 보고서"}));
    assert_eq!(
        execute(&mut s, "format.character.get", json!({}))["bold"],
        true
    );
}

#[test]
fn change_tracked_save_is_refused_unless_accepted() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/issue5169_viewtext_changetracking.hwp");
    if !sample.exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("tracked.hwp");
    std::fs::copy(&sample, &input).unwrap();
    let mut s = server();
    tool(&mut s, "open_document", json!({"path":input}));
    let state = execute(&mut s, "document.track_changes", json!({}));
    assert_eq!(state["tracked"], true, "{state}");
    let plain = dir.path().join("plain.hwpx");
    fails(&mut s, "save_document", json!({"path":plain}));
    assert!(!plain.exists());
    // Saving over the source is refused too, and leaves it intact.
    let before = std::fs::read(&input).unwrap();
    fails(&mut s, "save_document", json!({}));
    assert_eq!(std::fs::read(&input).unwrap(), before);
    let accepted = dir.path().join("accepted.hwp");
    tool(
        &mut s,
        "save_document",
        json!({"path":accepted,"acceptTrackChanges":true}),
    );
    assert!(accepted.exists());
    let state = execute(&mut s, "document.track_changes", json!({}));
    assert_eq!(state["tracked"], false, "{state}");
}

#[test]
fn distribution_documents_never_hand_out_text_or_save_through_any_tool() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/20250130-hongbo-no.hwp");
    if !sample.exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("dist.hwp");
    std::fs::copy(&sample, &input).unwrap();
    let mut s = server();
    let opened = tool(&mut s, "open_document", json!({"path":input}));
    // Words from the sample's first page.
    let leaks = |v: &Value| {
        let text = v.to_string();
        ["봉화댐", "보도자료", "수자원공사"]
            .iter()
            .any(|w| text.contains(w))
    };
    assert!(!leaks(&opened), "{opened}");
    for (name, args, refused) in [
        ("list_commands", json!({}), Some(false)),
        ("get_text", json!({}), Some(true)),
        ("inspect_document", json!({}), None),
        ("get_selection", json!({}), None),
        ("select_text", json!({"text":"봉화댐"}), Some(true)),
        ("type_text", json!({"text":"x"}), Some(true)),
        (
            "execute",
            json!({"command":"document.text","params":{}}),
            Some(true),
        ),
        ("execute", json!({"command":"select.all","params":{}}), None),
        ("get_selection", json!({}), None),
        (
            "execute",
            json!({"command":"edit.find","params":{"query":"봉화댐"}}),
            Some(false),
        ),
        (
            "execute",
            json!({"command":"clipboard.copy","params":{}}),
            Some(true),
        ),
        ("hwp_action", json!({"id":"SelectAll"}), None),
        ("hwp_action", json!({"id":"Copy"}), Some(true)),
        ("hwp_action", json!({"id":"Delete"}), Some(true)),
        ("hwp_action", json!({"query":"Copy"}), Some(false)),
        (
            "batch",
            json!({"commands":[{"command":"document.text"}]}),
            Some(true),
        ),
        (
            "batch",
            json!({"commands":[{"command":"text.insert","params":{"text":"x"}}]}),
            Some(true),
        ),
        ("save_document", json!({}), Some(true)),
        (
            "save_document",
            json!({"path":dir.path().join("o.hwp")}),
            Some(true),
        ),
        (
            "save_document",
            json!({"path":dir.path().join("o.hwpx")}),
            Some(true),
        ),
        (
            "save_document",
            json!({"path":dir.path().join("o.txt")}),
            Some(true),
        ),
        (
            "save_document",
            json!({"path":dir.path().join("o.pdf")}),
            Some(true),
        ),
        ("render_page", json!({"page":1}), Some(false)),
        ("list_documents", json!({}), Some(false)),
    ] {
        let r = raw_tool(&mut s, name, args.clone());
        let failed = r["isError"] == true;
        assert!(!leaks(&r), "{name} {args} leaks text: {r}");
        if let Some(refused) = refused {
            assert_eq!(failed, refused, "{name} {args}: {r}");
        }
        if failed && refused == Some(true) && name != "batch" {
            let text = r["content"][0]["text"].as_str().unwrap_or_default();
            assert!(
                text.contains(hwpcraft_engine::DISTRIBUTION_READ_ONLY),
                "{name} {args}: {r}"
            );
        }
    }
    for ext in ["hwp", "hwpx", "txt", "pdf"] {
        assert!(!dir.path().join(format!("o.{ext}")).exists(), "o.{ext}");
    }
    assert_eq!(
        std::fs::read(&input).unwrap(),
        std::fs::read(&sample).unwrap()
    );
    tool(&mut s, "close_document", json!({}));
}

#[test]
fn picture_effects_save_as_hwp_is_refused_unless_dropped() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../rhwp/samples/hwpx/aift.hwpx");
    if !sample.exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("aift.hwpx");
    std::fs::copy(&sample, &input).unwrap();
    let mut s = server();
    tool(&mut s, "open_document", json!({"path":input}));
    // Hangul saved one picture shadow; HWPX keeps it.
    let kept = dir.path().join("kept.hwpx");
    let saved = tool(&mut s, "save_document", json!({"path":kept}));
    assert!(saved.get("droppedPictureEffects").is_none(), "{saved}");
    let hwp = dir.path().join("out.hwp");
    std::fs::write(&hwp, b"preserve").unwrap();
    fails(&mut s, "save_document", json!({"path":hwp}));
    assert_eq!(std::fs::read(&hwp).unwrap(), b"preserve");
    let dropped = tool(
        &mut s,
        "save_document",
        json!({"path":hwp,"dropPictureEffects":true}),
    );
    assert_eq!(dropped["droppedPictureEffects"], 1, "{dropped}");
    assert_ne!(std::fs::read(&hwp).unwrap(), b"preserve");
    // Saving again needs no drop: the effects left the document.
    tool(&mut s, "save_document", json!({"path":hwp}));
}

#[test]
fn docx_export_reports_what_it_could_not_carry() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = server();
    tool(
        &mut s,
        "type_text",
        json!({"text":"보고서 제목
본문 문단"}),
    );
    let path = dir.path().join("report.docx");
    let saved = tool(&mut s, "save_document", json!({"path":path}));
    assert_eq!(saved["omitted"], json!([]), "{saved}");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..2], b"PK");
    // DOCX is an export: the document stays unsaved.
    assert_eq!(
        tool(&mut s, "inspect_document", json!({}))["document"]["dirty"],
        true
    );
}

/// The final goal end to end: an AI writes a whole report through MCP alone (the
/// 보고서 style set, a cover with its date field, outline headings, table of contents
/// and 표 차례, body, captioned tables and picture, chart, a cross reference, footnote,
/// a memo, page numbers in header and footer, a two-column part), saves it as HWPX,
/// HWP, PDF and DOCX, and every reopened file still has the same pages, renders every
/// page, the table of contents still points at the right pages and nothing failed
/// silently. docs/mcp-commands.md "보고서 작성 순서" follows these steps.
#[test]
fn ai_report_authored_saved_and_reopened_in_every_format() {
    report_round_trip(false, false);
}

/// The same report with HWPX pages compared too: a chart next to a footnote, a
/// captioned table and a picture once came back a page lower from HWPX.
#[test]
fn ai_report_hwpx_reopen_keeps_every_page() {
    report_round_trip(true, false);
}

/// The same report under the 보고서 style set (larger, spaced headings) and with a
/// reviewer's memo. Its HWP reopen once laid out a heading a page lower than the
/// live document, so the saved table of contents was off by a page (stale line
/// positions from incremental layout; RHWP 0038 and fresh rows for a new document's
/// HWP save).
#[test]
fn ai_report_with_style_set_keeps_hwp_toc_pages() {
    report_round_trip(true, true);
}

fn report_round_trip(strict_hwpx: bool, style_set: bool) {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("figure.png");
    image::RgbaImage::from_pixel(48, 30, image::Rgba([68, 114, 196, 255]))
        .save(&png)
        .unwrap();
    let mut s = server();
    let heading = |s: &mut TestClient, style: u32, text: &str| {
        tool(s, "type_text", json!({ "text": text }));
        execute(s, "style.apply", json!({ "id": style }));
        tool(s, "type_text", json!({"text":"\n"}));
        execute(s, "style.apply", json!({"id":0}));
    };
    // The style set first: headings and body take its sizes and spacing as typed.
    if style_set {
        let set = execute(&mut s, "format.style.set.apply", json!({"name":"보고서"}));
        assert_eq!(set["name"], "보고서", "{set}");
    }
    tool(
        &mut s,
        "type_text",
        json!({"text":"2026년 3분기 사업 실적 보고서\n작성: 경영기획팀\n작성일: "}),
    );
    let date = execute(&mut s, "insert.date_field", json!({"kind":"created"}));
    assert!(
        date["text"].as_str().is_some_and(|t| t.contains('년')),
        "{date}"
    );
    tool(&mut s, "type_text", json!({"text":"\n"}));
    execute(&mut s, "insert.page_break", json!({}));
    tool(&mut s, "type_text", json!({"text":"[[차례]]\n"}));
    execute(&mut s, "insert.page_break", json!({}));
    for c in 1..=2 {
        heading(&mut s, 2, &format!("제{c}장 사업 추진 현황"));
        for sec in 1..=2 {
            heading(&mut s, 3, &format!("{c}.{sec} 세부 과제"));
            let body = (1..=3)
                .map(|k| format!("{c}.{sec}-{k} 이번 분기에는 계획한 과제를 차질 없이 진행했으며 세부 일정과 예산 집행률을 함께 점검했습니다."))
                .collect::<Vec<_>>()
                .join("\n");
            tool(&mut s, "type_text", json!({ "text": body + "\n" }));
            if sec == 1 {
                execute(&mut s, "insert.table", json!({"rows":2,"columns":2}));
                let doc = tool(&mut s, "inspect_document", json!({"limit":1000}));
                let cell = doc["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|b| b["cursor"]["cell"].is_object())
                    .unwrap()["cursor"]
                    .clone();
                execute(&mut s, "selection.set", json!({ "cursor": cell }));
                tool(&mut s, "type_text", json!({"text":"과제"}));
                execute(
                    &mut s,
                    "table.properties",
                    json!({"props":{"hasCaption":true}}),
                );
                execute(
                    &mut s,
                    "table.caption_text",
                    json!({ "text": format!("{c}장 예산 집행") }),
                );
            } else {
                let made = execute(
                    &mut s,
                    "picture.insert",
                    json!({"path":png,"width_mm":60.0,"treat_as_char":true}),
                );
                execute(
                    &mut s,
                    "object.properties",
                    json!({"address":made["address"],"props":{"hasCaption":true}}),
                );
            }
            tool(&mut s, "hwp_action", json!({"id":"MoveDocumentEnd"}));
            tool(&mut s, "type_text", json!({"text":"\n"}));
        }
        if c == 1 {
            execute(
                &mut s,
                "note.insert",
                json!({"endnote":false,"paragraphs":["9월 말 결산 기준입니다."]}),
            );
            execute(
                &mut s,
                "chart.create",
                json!({"chart":{"kind":"column","title":"분기별 매출",
                "categories":["1분기","2분기","3분기"],"series":[{"name":"매출","values":[12,19,23]}]}}),
            );
            tool(&mut s, "hwp_action", json!({"id":"MoveDocumentEnd"}));
            tool(&mut s, "type_text", json!({"text":"\n"}));
        } else {
            execute(
                &mut s,
                "page.column_def_break",
                json!({"count":2,"spacing_mm":8.0}),
            );
            for k in 1..=4 {
                tool(
                    &mut s,
                    "type_text",
                    json!({ "text": format!("다단 구간 {k}: 현장 점검 결과를 두 단으로 정리합니다.\n") }),
                );
            }
            execute(
                &mut s,
                "page.column_def_break",
                json!({"count":1,"spacing_mm":0.0}),
            );
        }
    }
    // Header with a page number field, plus a page number at the bottom: both are
    // auto numbers next to the footnote's and the captions' (HWPX once refused this).
    execute(
        &mut s,
        "page.header_footer.text",
        json!({"header":true,"apply_to":"both","text":"사업 실적 보고서 ","fields":[{"offset":10,"kind":"page_number"}]}),
    );
    execute(
        &mut s,
        "page.number",
        json!({"position":"bottom_center","format":0,"dash":true,"start":1}),
    );
    // A reviewer's memo on a body sentence.
    if style_set {
        tool(&mut s, "select_text", json!({"text":"1.1-1 이번 분기에는"}));
        execute(
            &mut s,
            "memo.create",
            json!({"text":"집행률 수치를 재무팀과 맞춰 주세요.","author":"검토자"}),
        );
    }
    tool(&mut s, "select_text", json!({"text":"[[차례]]"}));
    tool(&mut s, "hwp_action", json!({"id":"Delete"}));
    let toc = execute(
        &mut s,
        "tools.toc",
        json!({"levels":2,"title":"차례","leader":"dot"}),
    );
    assert_eq!(toc["entries"], 6, "{toc}");
    // A 표 차례 right after the contents: captions as shown, with their pages.
    let tables = execute(&mut s, "tools.toc", json!({"kind":"tables"}));
    assert_eq!(tables["entries"], 2, "{tables}");
    // "표 1을 참조하세요." with the number as a cross reference that stays current.
    tool(&mut s, "hwp_action", json!({"id":"MoveDocumentEnd"}));
    tool(&mut s, "type_text", json!({"text":"\n자세한 내용은 표 "}));
    let reference = execute(
        &mut s,
        "insert.cross_reference",
        json!({"target":{"table":1},"show":"number"}),
    );
    assert_eq!(reference["text"], "1", "{reference}");
    tool(&mut s, "type_text", json!({"text":"을 참조하세요."}));

    let summary = tool(&mut s, "inspect_document", json!({"limit":1}));
    let pages = summary["document"]["pages"].as_u64().unwrap();
    assert!(pages >= 4, "{summary}");
    for page in 1..=pages {
        let png = raw_tool(&mut s, "render_page", json!({"page":page,"scale":0.25}));
        assert_eq!(png["isError"], false, "page {page}");
    }
    let outline = |s: &mut TestClient| -> Vec<(String, u64)> {
        execute(s, "document.outline", json!({}))["outline"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o.get("cellIndex").is_none())
            .map(|o| {
                (
                    o["title"].as_str().unwrap().to_owned(),
                    o["page"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let live = outline(&mut s);
    assert_eq!(live.len(), 6, "{live:?}");
    let table_lines = |s: &mut TestClient| -> Vec<String> {
        tool(s, "get_text", json!({"limit":1000}))["text"]
            .as_str()
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("표 ") && l.contains('\t'))
            .map(str::to_owned)
            .collect()
    };
    let live_tables = table_lines(&mut s);
    assert_eq!(live_tables.len(), 2, "{live_tables:?}");
    assert!(
        live_tables[0].starts_with("표 1 1장 예산 집행\t"),
        "{live_tables:?}"
    );

    for ext in ["hwpx", "hwp", "pdf"] {
        tool(
            &mut s,
            "save_document",
            json!({"path": dir.path().join(format!("report.{ext}"))}),
        );
    }
    let pdf = std::fs::read(dir.path().join("report.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    // 차례, 표 차례 and the cross reference are links to their pages in the PDF too.
    let gotos = String::from_utf8_lossy(&pdf).matches("/S /GoTo").count();
    assert!(gotos >= 9, "{gotos} GoTo links");
    let docx = tool(
        &mut s,
        "save_document",
        json!({"path": dir.path().join("report.docx")}),
    );
    // The chart is pictured from its page; nothing else is left out.
    assert_eq!(
        docx["omitted"],
        json!(["그리기 개체·글상자·차트(그림으로 씀) 1개"]),
        "{docx}"
    );

    for ext in ["hwpx", "hwp"] {
        let opened = tool(
            &mut s,
            "open_document",
            json!({"path": dir.path().join(format!("report.{ext}")), "force": true}),
        );
        let reopened = outline(&mut s);
        if ext == "hwp" || strict_hwpx {
            assert_eq!(opened["pages"].as_u64(), Some(pages), "{ext}: {opened}");
            assert_eq!(reopened, live, "{ext}");
        } else {
            let titles = |o: &[(String, u64)]| o.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
            assert_eq!(titles(&reopened), titles(&live), "{ext}");
        }
        // Each table of contents line ends with the page of its heading.
        let text = tool(&mut s, "get_text", json!({"limit":1000}))["text"]
            .as_str()
            .unwrap()
            .to_owned();
        for (title, page) in &live {
            let line = text
                .lines()
                .find(|l| l.contains(title.as_str()) && l.contains('\t'))
                .unwrap_or_else(|| panic!("{ext}: no TOC line for {title}"));
            assert!(
                line.ends_with(&format!("\t{page}")),
                "{ext}: {line} vs page {page}"
            );
        }
        assert!(text.contains("자세한 내용은 표 1을 참조하세요."), "{ext}");
        if ext == "hwp" || strict_hwpx {
            assert_eq!(table_lines(&mut s), live_tables, "{ext}");
        }
        // Every page still renders; the memo, the date field and the style set stay.
        let reopened_pages = opened["pages"].as_u64().unwrap();
        for page in 1..=reopened_pages {
            let png = raw_tool(&mut s, "render_page", json!({"page":page,"scale":0.25}));
            assert_eq!(png["isError"], false, "{ext}: page {page}");
        }
        let memos = execute(&mut s, "memo.list", json!({}));
        let expected = usize::from(style_set);
        assert_eq!(
            memos.as_array().map(Vec::len),
            Some(expected),
            "{ext}: {memos}"
        );
        if style_set {
            assert_eq!(memos[0]["author"], "검토자", "{ext}: {memos}");
        }
        let dates = execute(&mut s, "tools.date_field.update", json!({}));
        assert_eq!(dates["fields"], 1, "{ext}: {dates}");
        assert!(text.contains("작성일: "), "{ext}");
        let styles = execute(&mut s, "format.style.export", json!({}));
        let h1 = styles["styles"]
            .as_array()
            .and_then(|a| a.iter().find(|st| st["name"] == "개요 1"))
            .cloned()
            .unwrap_or_default();
        if style_set {
            assert_eq!(h1["character"]["fontSize"], 1600, "{ext}: {h1}");
        }
        let references = execute(&mut s, "tools.cross_reference.list", json!({}));
        // `execute` hands back the list itself.
        let number = &references[0];
        assert_eq!(number["show"], "number", "{ext}: {references}");
        assert_eq!(number["expected"], number["text"], "{ext}: {references}");
    }
}
