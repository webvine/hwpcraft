use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

fn message(id: u32, method: &str, params: Value) -> String {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":{}
    });
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string() + "\n"
}
#[test]
fn stdio_subprocess_edits_korean_and_stdout_is_only_json_rpc() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("MCP 한글 문서.hwpx");
    let mut child = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args(["mcp", "--automation-write-root"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if send.send(line).is_err() {
                return;
            }
        }
    });
    let mut request = |id, params| -> Value {
        stdin
            .write_all(message(id, "tools/call", params).as_bytes())
            .unwrap();
        stdin.flush().unwrap();
        let line = receive.recv_timeout(std::time::Duration::from_secs(15));
        if line.is_err() {
            let _ = child.kill();
        }
        let response: Value = serde_json::from_str(&line.unwrap().unwrap()).unwrap();
        assert_eq!(response["id"], id);
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["resultType"], "complete");
        assert_eq!(response["result"]["isError"], false, "{response}");
        response
    };
    let created = request(1, json!({"name":"new_document","arguments":{}}));
    let document_id = created["result"]["structuredContent"]["document_id"]
        .as_str()
        .unwrap();
    request(
        2,
        json!({"name":"type_text","arguments":{"document_id":document_id,"text":"아래한글 😀 자동 편집"}}),
    );
    request(
        3,
        json!({"name":"save_document","arguments":{"document_id":document_id,"path":path}}),
    );
    let text = request(
        4,
        json!({"name":"get_text","arguments":{"document_id":document_id}}),
    );
    assert_eq!(
        text["result"]["structuredContent"]["text"],
        "아래한글 😀 자동 편집"
    );
    drop(stdin); // EOF is the MCP stdio shutdown signal.
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    reader.join().unwrap();
    assert!(
        receive.try_iter().next().is_none(),
        "unexpected stdout after final response"
    );
    let mut document = hwpcraft_engine::Session::new().unwrap();
    document.open(&path).unwrap();
    assert_eq!(document.text(), "아래한글 😀 자동 편집");
}

#[test]
fn legacy_bootstrap_requests_never_bypass_the_modern_protocol_boundary() {
    let mut requests =
        String::from("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{}}\n");
    requests.push_str(&message(2, "ping", json!({})));
    requests.push_str(&message(3, "initialize", json!({"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"own-probe","version":"1"}})));
    requests.push_str(&message(4, "server/discover", json!({})));
    let mut child = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(requests.as_bytes()).unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(values.len(), 4);
    for (value, id, code) in [
        (&values[0], 1, -32602),
        (&values[1], 2, -32601),
        (&values[2], 3, -32601),
    ] {
        assert_eq!(value["id"], id);
        assert_eq!(value["error"]["code"], code);
        assert!(value.get("result").is_none());
    }
    assert_eq!(values[3]["id"], 4);
    assert_eq!(values[3]["result"]["resultType"], "complete");
}

#[test]
fn invalid_options_fail_before_reading_stdin() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["--connect", "127.0.0.1:7981"],
        vec!["--automation-read-root"],
        vec!["--automation-write-root", ""],
        vec!["--automation-read-root", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
            .arg("mcp")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    let root = dir.path().to_str().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args([
            "mcp",
            "--automation-read-root",
            root,
            "--automation-read-root",
            root,
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .args(["mcp", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn responses_are_flushed_before_stdin_closes() {
    // Reader and writer handshake rather than sending EOF: catches unflushed stdio replies.
    let mut child = Command::new(env!("CARGO_BIN_EXE_hwpcraft-cli"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    input
        .write_all(message(1, "server/discover", json!({})).as_bytes())
        .unwrap();
    input.flush().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        BufReader::new(output)
            .read_until(b'\n', &mut bytes)
            .unwrap();
        tx.send(bytes).unwrap();
    });
    let response = rx.recv_timeout(std::time::Duration::from_secs(15));
    // Always clean up even if a buffering regression times out.
    if response.is_err() {
        let _ = child.kill();
    }
    drop(input);
    let status = child.wait().unwrap();
    reader.join().unwrap();
    let response: Value = serde_json::from_slice(&response.unwrap()).unwrap();
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["resultType"], "complete");
    assert!(response.get("error").is_none(), "{response}");
    assert!(status.success());
}
