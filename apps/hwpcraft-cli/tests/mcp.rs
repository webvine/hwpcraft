use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

fn message(id: u32, method: &str, params: Value) -> String {
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
    stdin.write_all(message(1,"initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}})).as_bytes()).unwrap();
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .unwrap();
    stdin
        .write_all(
            message(
                2,
                "tools/call",
                json!({"name":"type_text","arguments":{"text":"아래한글 😀 자동 편집"}}),
            )
            .as_bytes(),
        )
        .unwrap();
    stdin
        .write_all(
            message(
                3,
                "tools/call",
                json!({"name":"save_document","arguments":{"path":path}}),
            )
            .as_bytes(),
        )
        .unwrap();
    stdin
        .write_all(message(4, "tools/call", json!({"name":"get_text"})).as_bytes())
        .unwrap();
    drop(stdin); // EOF is the MCP stdio shutdown signal.
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let responses: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdout must contain only JSON-RPC"))
        .collect();
    assert_eq!(responses.len(), 4, "{stdout}");
    for response in &responses {
        assert!(response.get("error").is_none(), "{response}");
    }
    assert_eq!(responses[2]["result"]["isError"], false);
    let text: Value = serde_json::from_str(
        responses[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(text["text"], "아래한글 😀 자동 편집");
    let mut document = hwpcraft_engine::Session::new().unwrap();
    document.open(&path).unwrap();
    assert_eq!(document.text(), "아래한글 😀 자동 편집");
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
        .write_all(message(1, "ping", json!({})).as_bytes())
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
    assert!(status.success());
}
