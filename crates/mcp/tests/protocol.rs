use hwpcraft_formats::file_access::AutomationRoots;
use hwpcraft_mcp::{Headless, Server};
use serde_json::{Value, json};

fn rpc(s: &mut Server, method: &str, params: Value) -> Value {
    serde_json::from_str(
        &s.handle_line(
            &json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn initialize(s: &mut Server) {
    let r = rpc(
        s,
        "initialize",
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
    );
    assert_eq!(r["result"]["serverInfo"]["name"], "hwpcraft");
    assert!(
        s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none()
    );
}
fn server() -> Server {
    let mut s = Server::new(Headless::new(None).unwrap());
    initialize(&mut s);
    s
}
fn raw_tool(s: &mut Server, name: &str, args: Value) -> Value {
    rpc(s, "tools/call", json!({"name":name,"arguments":args}))["result"].clone()
}
fn tool(s: &mut Server, name: &str, args: Value) -> Value {
    let r = raw_tool(s, name, args);
    assert_eq!(r["isError"], false, "{name}: {r}");
    serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
}
fn execute(s: &mut Server, command: &str, params: Value) -> Value {
    tool(s, "execute", json!({"command":command,"params":params}))
}
fn fails(s: &mut Server, name: &str, args: Value) {
    let r = raw_tool(s, name, args);
    assert_eq!(r["isError"], true, "{name}: {r}");
}

#[test]
fn lifecycle_errors_resources_and_bounded_framing() {
    let mut s = Server::new(Headless::new(None).unwrap());
    assert_eq!(
        rpc(&mut s, "tools/list", json!({}))["error"]["code"],
        -32000
    );
    initialize(&mut s);
    assert_eq!(
        rpc(&mut s, "tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    assert_eq!(
        rpc(&mut s, "resources/list", json!({}))["result"]["resources"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    for uri in [
        "hwpcraft://document",
        "hwpcraft://commands",
        "hwpcraft://hwp-actions",
    ] {
        let r = rpc(&mut s, "resources/read", json!({"uri":uri}));
        assert!(
            serde_json::from_str::<Value>(r["result"]["contents"][0]["text"].as_str().unwrap())
                .is_ok()
        );
    }
    assert_eq!(
        rpc(&mut s, "resources/read", json!({"uri":"missing"}))["error"]["code"],
        -32002
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
        -32602
    );
    assert_eq!(rpc(&mut s, "missing", json!({}))["error"]["code"], -32601);
    for malformed in [
        "[]",
        "null",
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
    ] {
        let r: Value = serde_json::from_str(&s.handle_line(malformed).unwrap()).unwrap();
        assert_eq!(r["error"]["code"], -32600);
    }
    let input = b"not json\n\xff\n{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    s.serve(&input[..], &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    let replies: Vec<Value> = output
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1]["error"]["code"], -32700);
    assert_eq!(replies[2]["id"], 8);
    let huge = vec![b'x'; 4 * 1024 * 1024 + 2];
    let mut output = Vec::new();
    s.serve(huge.as_slice(), &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap().lines().count(), 1);
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
    let mut s = Server::new(Headless::new(roots).unwrap());
    initialize(&mut s);
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
    let mut read_only =
        Server::new(Headless::new(AutomationRoots::new(Some(&input), None).unwrap()).unwrap());
    initialize(&mut read_only);
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
    let target = dir.path().join("keep.docx");
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
    execute(&mut s, "insert.table", json!({"rows":2,"columns":3}));
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
        json!({"props":{"hasCaption":true}}),
    );
    execute(
        &mut s,
        "table.caption_text",
        json!({"text":"표 1 주간 실적"}),
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
