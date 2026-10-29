//! MCP Tasks extension over the real SDK stream transport.
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

const DEADLINE: Duration = Duration::from_secs(60);
const SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

struct Client {
    input: WriteHalf<DuplexStream>,
    output: BufReader<ReadHalf<DuplexStream>>,
    next: u64,
}

fn meta(tasks: bool) -> Value {
    let capabilities = if tasks {
        json!({"extensions":{"io.modelcontextprotocol/tasks":{}}})
    } else {
        json!({})
    };
    json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
        "io.modelcontextprotocol/clientCapabilities":capabilities})
}

impl Client {
    fn start() -> Self {
        let (client, server) = tokio::io::duplex(1 << 20);
        let (read, write) = tokio::io::split(server);
        tokio::spawn(hwpcraft_mcp::serve_stream(None, read, write));
        let (output, input) = tokio::io::split(client);
        Self {
            input,
            output: BufReader::new(output),
            next: 1,
        }
    }

    async fn rpc(&mut self, method: &str, mut params: Value, tasks: bool) -> Value {
        let id = self.next;
        self.next += 1;
        params["_meta"] = meta(tasks);
        let mut line =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .unwrap();
        line.push(b'\n');
        tokio::time::timeout(DEADLINE, async {
            self.input.write_all(&line).await.unwrap();
            self.input.flush().await.unwrap();
            loop {
                let mut text = String::new();
                assert_ne!(self.output.read_line(&mut text).await.unwrap(), 0);
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["id"] == id {
                    return value;
                }
            }
        })
        .await
        .expect("MCP response deadline")
    }

    async fn tool(&mut self, name: &str, arguments: Value, tasks: bool) -> Value {
        self.rpc(
            "tools/call",
            json!({"name":name,"arguments":arguments}),
            tasks,
        )
        .await
    }

    /// Poll a task until it is terminal, as a client would.
    async fn settle(&mut self, task_id: &str) -> Value {
        tokio::time::timeout(DEADLINE, async {
            loop {
                let state = self.rpc("tasks/get", json!({"taskId":task_id}), true).await;
                let result = &state["result"];
                assert_eq!(result["taskId"], task_id, "{state}");
                assert_eq!(result["_meta"][SERVER_INFO]["name"], "hwpcraft", "{state}");
                if result["status"] != "working" {
                    return result.clone();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("task did not settle")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn long_file_tools_become_tasks_only_for_clients_that_declare_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::start();

    let discover = client.rpc("server/discover", json!({}), false).await;
    assert!(
        discover["result"]["capabilities"]["extensions"]
            .get("io.modelcontextprotocol/tasks")
            .is_some(),
        "{discover}"
    );

    let created = client.tool("new_document", json!({}), true).await;
    assert_eq!(created["result"]["resultType"], "complete", "{created}");
    let doc = created["result"]["structuredContent"]["document_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let typed = client
        .tool("type_text", json!({"document_id":doc,"text":"tasks"}), true)
        .await;
    assert_eq!(typed["result"]["resultType"], "complete", "{typed}");

    // An eligible save becomes a task handle that is observable at once.
    let path = dir.path().join("task.hwpx");
    let handle = client
        .tool(
            "save_document",
            json!({"document_id":doc,"path":path.to_str().unwrap()}),
            true,
        )
        .await;
    let task = &handle["result"];
    assert_eq!(task["resultType"], "task", "{handle}");
    assert_eq!(task["ttlMs"], 600_000);
    assert_eq!(task["pollIntervalMs"], 1000);
    assert_eq!(task["_meta"][SERVER_INFO]["name"], "hwpcraft");
    let task_id = task["taskId"].as_str().unwrap().to_owned();
    let done = client.settle(&task_id).await;
    assert_eq!(done["status"], "completed", "{done}");
    assert_eq!(done["result"]["isError"], false, "{done}");
    assert_eq!(done["result"]["structuredContent"]["document_id"], doc);
    assert!(std::fs::metadata(&path).unwrap().len() > 0);

    // Cancelling a finished task is acknowledged and changes nothing.
    let ack = client
        .rpc("tasks/cancel", json!({"taskId":task_id}), true)
        .await;
    assert_eq!(ack["result"]["resultType"], "complete", "{ack}");
    assert_eq!(client.settle(&task_id).await["status"], "completed");

    // A tool failure is a completed task whose result reports isError, exactly
    // as the direct call would.
    let bad = client
        .tool(
            "execute",
            json!({"document_id":doc,"command":"file.pdf",
                "params":{"path":dir.path().join("wrong.txt").to_str().unwrap()}}),
            true,
        )
        .await;
    let failed = client
        .settle(bad["result"]["taskId"].as_str().unwrap())
        .await;
    assert_eq!(failed["status"], "completed", "{failed}");
    assert_eq!(failed["result"]["isError"], true, "{failed}");

    // Clients without the extension, and quick tools, keep direct results.
    let direct = client
        .tool(
            "save_document",
            json!({"document_id":doc,"path":path.to_str().unwrap()}),
            false,
        )
        .await;
    assert_eq!(direct["result"]["resultType"], "complete", "{direct}");
    let text = client
        .tool("get_text", json!({"document_id":doc}), true)
        .await;
    assert_eq!(text["result"]["resultType"], "complete", "{text}");

    // tasks/* require the client capability on that request, and a real id.
    let undeclared = client
        .rpc("tasks/get", json!({"taskId":task_id}), false)
        .await;
    assert_eq!(undeclared["error"]["code"], -32021, "{undeclared}");
    let unknown = client
        .rpc("tasks/get", json!({"taskId":"not-a-task"}), true)
        .await;
    assert_eq!(unknown["error"]["code"], -32602, "{unknown}");
}
