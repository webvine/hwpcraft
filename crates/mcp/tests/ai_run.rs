//! AI edit panel runs over real HTTP on the desktop bridge: every restricted
//! command is refused on every path (execute, batch, hwp_action, macro.run) and
//! nothing is written; batches, atomic rollbacks and formatting inside a run
//! still work and undo in one step.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hwpcraft_engine::{Command, Session};
use hwpcraft_mcp::desktop_bridge::DesktopBridge;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Settings-folder commands would write to the real folder if a refusal leaked,
/// so each test runs again in a child process with its own settings folder.
fn in_own_settings_folder(name: &str) -> Option<tempfile::TempDir> {
    if std::env::var_os("HWPCRAFT_AI_RUN_CHILD").is_some() {
        return None;
    }
    let config = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            name,
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "--include-ignored",
        ])
        .env("HWPCRAFT_AI_RUN_CHILD", "1")
        .env("HWPCRAFT_CONFIG_DIR", config.path())
        .output()
        .unwrap();
    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(config)
}

/// One blocking MCP tools/call over HTTP; returns the tool result object.
fn call(address: &str, token: &str, tool: &str, args: Value) -> Value {
    let body = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":args,"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientCapabilities":{}}}}))
    .unwrap();
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    write!(
        socket,
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\nMcp-Name: {tool}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    socket.write_all(&body).unwrap();
    let mut reply = String::new();
    socket.read_to_string(&mut reply).unwrap();
    let start = reply.find("{\"").expect(&reply);
    let end = reply.rfind('}').expect(&reply);
    let message: Value = serde_json::from_str(&reply[start..=end]).expect(&reply);
    message["result"].clone()
}

fn body(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap_or("null"))
        .unwrap_or(Value::Null)
}

/// Serve the window between frames until the client thread is done.
fn serve_until<T>(
    bridge: &mut DesktopBridge,
    session: &mut Session,
    client: std::thread::JoinHandle<T>,
) -> T {
    let started = Instant::now();
    while !client.is_finished() {
        let _ = bridge.serve(session, Duration::from_millis(50));
        assert!(started.elapsed() < Duration::from_secs(120), "client stuck");
        std::thread::sleep(Duration::from_millis(5));
    }
    client.join().unwrap()
}

/// Every file under `dir` with its bytes, to prove nothing was written.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.clone(), std::fs::read(&p).unwrap_or_default());
            }
        }
    }
    out
}

/// A 1×1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0, 5, 0, 1, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0, 0,
    0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The command's own example with its paths pointed into `dir`: pictures and
/// documents at files that exist (a leaked read would succeed), anything else at
/// a new name (a leaked write would create it).
fn retarget(value: &Value, dir: &Path) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = match (k.as_str(), v.as_str()) {
                        ("path" | "file" | "output", Some(p)) => {
                            let ext = Path::new(p)
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_ascii_lowercase();
                            let target = match ext.as_str() {
                                "png" | "jpg" | "bmp" | "gif" => dir.join("picture.png"),
                                "hwp" | "hwpx" => dir.join("other.hwpx"),
                                _ => dir.join(format!(
                                    "leak.{}",
                                    if ext.is_empty() { "out" } else { &ext }
                                )),
                            };
                            json!(target)
                        }
                        ("dir" | "folder", Some(_)) => json!(dir.join("leak-dir")),
                        _ => retarget(v, dir),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| retarget(v, dir)).collect()),
        other => other.clone(),
    }
}

fn start(dir: &Path, session: &Session) -> (DesktopBridge, String, String) {
    let token_file = dir.join("token");
    let bridge = DesktopBridge::start(
        "127.0.0.1:0".parse().unwrap(),
        Some(token_file.clone()),
        session,
        || {},
    )
    .unwrap();
    let user = std::fs::read_to_string(&token_file)
        .unwrap()
        .trim()
        .to_owned();
    let address = bridge
        .url()
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .to_owned();
    (bridge, user, address)
}

const REFUSED: &str = "AI 편집 중에는";

#[test]
fn restricted_commands_are_refused_on_every_path_and_write_nothing() {
    let Some(_config) =
        in_own_settings_folder("restricted_commands_are_refused_on_every_path_and_write_nothing")
    else {
        return run_restricted();
    };
}

fn run_restricted() {
    let config = PathBuf::from(std::env::var_os("HWPCRAFT_CONFIG_DIR").unwrap());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("picture.png"), PNG).unwrap();
    let mut other = Session::new().unwrap();
    other.execute(Command::Insert("다른 문서".into())).unwrap();
    other.save(&dir.path().join("other.hwpx")).unwrap();
    let mut session = Session::new().unwrap();
    session
        .execute(Command::Insert("사용자 글".into()))
        .unwrap();
    let (mut bridge, user, address) = start(dir.path(), &session);
    let before = files(dir.path());
    let settings_before = files(&config);
    let run = bridge.begin_ai_run(&mut session).unwrap();

    let examples: BTreeMap<&str, Value> = hwpcraft_engine::command_help_entries()
        .map(|(id, _, example)| (id, example))
        .collect();
    let cases: Vec<(String, Value)> = hwpcraft_engine::RESTRICTED_COMMANDS
        .iter()
        .map(|id| {
            let example = examples.get(id).cloned().unwrap_or(json!({}));
            let example = if example.is_object() {
                example
            } else {
                json!({})
            };
            (id.to_string(), retarget(&example, dir.path()))
        })
        .chain(["file.save", "file.save_as"].map(|id| {
            // The window's PDF export runs as a job of its own.
            (
                id.to_string(),
                json!({"path": dir.path().join(format!("{id}.pdf"))}),
            )
        }))
        .collect();
    let leak = dir.path().join("leak.hwpx");
    let picture = dir.path().join("picture.png");
    let (a, t, i) = (address.clone(), run.token.clone(), run.document_id.clone());
    let client = std::thread::spawn(move || {
        let mut one_by_one = Vec::new();
        for (id, params) in &cases {
            one_by_one.push((
                id.clone(),
                call(
                    &a,
                    &t,
                    "execute",
                    json!({"document_id": i, "command": id, "params": params}),
                ),
            ));
        }
        let steps: Vec<Value> = cases
            .iter()
            .map(|(id, params)| json!({"command": id, "params": params}))
            .collect();
        let batch = call(
            &a,
            &t,
            "batch",
            json!({"document_id": i, "commands": steps, "keepGoing": true}),
        );
        let actions: Vec<(String, Value)> = [
            ("FileSave", json!({})),
            ("FileSaveAs", json!({"path": leak})),
            ("FileOpen", json!({"path": leak})),
            ("FileNew", json!({})),
            ("FilePassword", json!({"password": "모르는 암호"})),
        ]
        .into_iter()
        .map(|(id, params)| {
            (
                id.to_string(),
                call(
                    &a,
                    &t,
                    "hwp_action",
                    json!({"document_id": i, "id": id, "params": params}),
                ),
            )
        })
        .collect();
        let macro_run = call(
            &a,
            &t,
            "execute",
            json!({"document_id": i, "command": "macro.run", "params": {"steps": [
                {"command": "text.insert", "params": {"text": " 매크로"}},
                {"command": "insert.picture", "params": {"path": picture}}]}}),
        );
        (one_by_one, batch, actions, macro_run)
    });
    let (one_by_one, batch, actions, macro_run) = serve_until(&mut bridge, &mut session, client);

    // Collect every leak before failing, so one run names them all.
    let mut leaks = Vec::new();
    for (id, result) in &one_by_one {
        if result["isError"] != true || !result.to_string().contains(REFUSED) {
            leaks.push(format!("execute {id}"));
        }
    }
    let batch_body = body(&batch);
    let steps = batch_body["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if steps.len() != one_by_one.len() {
        leaks.push(format!(
            "batch ran {} of {} steps: {batch_body}",
            steps.len(),
            one_by_one.len()
        ));
    }
    for step in &steps {
        if step["ok"] != false || !step["error"].as_str().unwrap_or("").contains(REFUSED) {
            leaks.push(format!("batch {}", step["command"]));
        }
    }
    for (id, result) in &actions {
        if result["isError"] != true || !result.to_string().contains(REFUSED) {
            leaks.push(format!("hwp_action {id}"));
        }
    }
    if macro_run["isError"] != true || !macro_run.to_string().contains(REFUSED) {
        leaks.push(format!("macro.run step insert.picture: {macro_run}"));
    }
    let written: Vec<_> = files(dir.path())
        .into_iter()
        .filter(|(p, bytes)| before.get(p) != Some(bytes))
        .map(|(p, _)| p)
        .collect();
    assert!(
        leaks.is_empty() && written.is_empty(),
        "not refused: {leaks:#?}
written: {written:#?}"
    );
    // The macro rolled back its first step too; nothing changed, nothing written.
    assert_eq!(session.text(), "사용자 글");
    assert!(!bridge.end_ai_run(&mut session).unwrap());
    assert_eq!(files(dir.path()), before);
    assert_eq!(files(&config), settings_before);

    // The run is over: the user's own client may save again.
    let after = dir.path().join("after.hwpx");
    let id = run.document_id.clone();
    let (a, u, p) = (address.clone(), user.clone(), after.clone());
    let client = std::thread::spawn(move || {
        call(
            &a,
            &u,
            "execute",
            json!({"document_id": id, "command": "file.save_as", "params": {"path": p}}),
        )
    });
    let saved = serve_until(&mut bridge, &mut session, client);
    assert_eq!(saved["isError"], false, "{saved}");
    assert!(after.exists());
}

#[test]
fn batches_rollbacks_and_formatting_in_a_run_undo_in_one_step() {
    let Some(_config) =
        in_own_settings_folder("batches_rollbacks_and_formatting_in_a_run_undo_in_one_step")
    else {
        return run_formatting();
    };
}

fn run_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::new().unwrap();
    session
        .execute(Command::Insert("사용자 글".into()))
        .unwrap();
    let (mut bridge, user, address) = start(dir.path(), &session);
    let run = bridge.begin_ai_run(&mut session).unwrap();
    let (a, t, u, i) = (
        address.clone(),
        run.token.clone(),
        user.clone(),
        run.document_id.clone(),
    );
    let client = std::thread::spawn(move || {
        let batch = |token: &str, commands: Value, extra: Value| {
            let mut args = json!({"document_id": i, "commands": commands});
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            call(&a, token, "batch", args)
        };
        [
            // An atomic batch that fails leaves the document as it was.
            batch(
                &t,
                json!([{"command": "text.insert", "params": {"text": " 사라짐"}},
                       {"command": "no.such.command"}]),
                json!({"atomic": true}),
            ),
            // Character and paragraph formatting.
            batch(
                &t,
                json!([{"command": "select.text", "params": {"text": "사용자"}},
                       {"command": "format.character", "params": {"bold": true}},
                       {"command": "format.paragraph", "params": {"alignment": "center"}}]),
                json!({"atomic": true}),
            ),
            // keepGoing: the failure is reported, the other edits stay.
            batch(
                &t,
                json!([{"command": "caret.move", "params": {"motion": "document_end"}},
                       {"command": "no.such.command"},
                       {"command": "text.insert", "params": {"text": " 둘"}}]),
                json!({"keepGoing": true}),
            ),
            // A macro whose last step fails is rolled back whole.
            call(
                &a,
                &t,
                "execute",
                json!({"document_id": i, "command": "macro.run", "params": {"steps": [
                {"command": "text.insert", "params": {"text": " 셋"}},
                {"command": "no.such.command"}]}}),
            ),
            // Another client may read the window's document but not edit it.
            call(&a, &u, "get_text", json!({"document_id": i})),
            batch(
                &u,
                json!([{"command": "text.insert", "params": {"text": " 남"}}]),
                json!({}),
            ),
        ]
    });
    let [
        failed_atomic,
        formatted,
        kept,
        failed_macro,
        other_read,
        other_edit,
    ] = serve_until(&mut bridge, &mut session, client);
    assert_eq!(failed_atomic["isError"], true, "{failed_atomic}");
    assert_eq!(body(&failed_atomic)["rolledBack"], true, "{failed_atomic}");
    assert_eq!(formatted["isError"], false, "{formatted}");
    assert_eq!(kept["isError"], true, "{kept}");
    assert_eq!(failed_macro["isError"], true, "{failed_macro}");
    assert_eq!(other_read["isError"], false, "{other_read}");
    assert_eq!(other_edit["isError"], true, "{other_edit}");
    assert_eq!(session.text(), "사용자 글 둘");

    let bold = |s: &mut Session| {
        s.run("select.text", json!({"text": "사용자"})).unwrap();
        s.run("format.character.get", json!({})).unwrap()["bold"].clone()
    };
    let alignment =
        |s: &mut Session| s.run("format.paragraph.get", json!({})).unwrap()["alignment"].clone();
    assert!(bridge.end_ai_run(&mut session).unwrap());
    assert_eq!(bold(&mut session), true);
    assert_eq!(alignment(&mut session), "center");
    let edited = session.text().to_owned();
    // One undo returns to the text and formatting before the run.
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.text(), "사용자 글");
    assert_eq!(bold(&mut session), false);
    assert_ne!(alignment(&mut session), "center");
    session.execute(Command::Redo).unwrap();
    assert_eq!(session.text(), edited);
    assert_eq!(bold(&mut session), true);
    // The user's step before the run is still there.
    session.execute(Command::Undo).unwrap();
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.text(), "");
}
