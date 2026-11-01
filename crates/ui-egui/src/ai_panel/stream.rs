//! Reads the JSON lines Claude Code (`--output-format stream-json`) and Codex
//! (`exec --json`) print while they work, and turns them into the few events the
//! AI panel shows: the conversation id, each MCP tool call, the assistant's text
//! and the end of the turn. Unknown lines and fields are ignored, so a newer CLI
//! that adds events still shows its answer.
use serde_json::Value;

/// Which local CLI produced the stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    #[default]
    Claude,
    Codex,
}

/// One thing the panel shows or remembers.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Conversation id to pass back next time (`--resume` / `exec resume`).
    Session(String),
    /// A tool call began. `label` is what the folded line shows: the engine
    /// command for `execute` (e.g. `table.create`), else the tool name.
    ToolStarted { id: String, label: String },
    /// The same call's tool name and arguments, kept with the conversation history.
    Call {
        id: String,
        tool: String,
        args: Value,
    },
    /// A tool call ended; `error` holds the reason it failed.
    ToolFinished { id: String, error: Option<String> },
    /// A complete piece of assistant text. The last one before `Done` is the answer.
    Text(String),
    /// The turn ended. `answer` is the CLI's own final text when it gives one.
    Done {
        answer: Option<String>,
        error: Option<String>,
    },
}

/// The name the panel shows for a tool call.
pub(super) fn tool_label(tool: &str, arguments: &Value) -> String {
    let tool = tool.strip_prefix("mcp__hwpcraft__").unwrap_or(tool);
    match (tool, arguments) {
        ("execute", a) => a["command"].as_str().unwrap_or(tool).to_owned(),
        ("hwp_action", a) => a["id"]
            .as_str()
            .map_or_else(|| tool.to_owned(), |id| format!("hwp_action {id}")),
        _ => tool.to_owned(),
    }
}

/// Parse one stdout line. Blank or non-JSON lines give nothing.
pub fn parse_line(engine: Engine, line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    match engine {
        Engine::Claude => claude(&v),
        Engine::Codex => codex(&v),
    }
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn claude(v: &Value) -> Vec<Event> {
    let mut out = Vec::new();
    match v["type"].as_str() {
        Some("system") if v["subtype"] == "init" => {
            if let Some(id) = text(&v["session_id"]) {
                out.push(Event::Session(id));
            }
        }
        Some("assistant") => {
            for c in v["message"]["content"].as_array().into_iter().flatten() {
                match c["type"].as_str() {
                    Some("tool_use") => {
                        let id = c["id"].as_str().unwrap_or_default().to_owned();
                        let tool = c["name"].as_str().unwrap_or("?");
                        out.push(Event::ToolStarted {
                            id: id.clone(),
                            label: tool_label(tool, &c["input"]),
                        });
                        out.push(Event::Call {
                            id,
                            tool: tool
                                .strip_prefix("mcp__hwpcraft__")
                                .unwrap_or(tool)
                                .to_owned(),
                            args: c["input"].clone(),
                        });
                    }
                    Some("text") => out.extend(text(&c["text"]).map(Event::Text)),
                    _ => {}
                }
            }
        }
        Some("user") => {
            for c in v["message"]["content"].as_array().into_iter().flatten() {
                if c["type"] == "tool_result" {
                    let error = (c["is_error"] == true).then(|| match &c["content"] {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    });
                    out.push(Event::ToolFinished {
                        id: c["tool_use_id"].as_str().unwrap_or_default().to_owned(),
                        error,
                    });
                }
            }
        }
        Some("result") => {
            if let Some(id) = text(&v["session_id"]) {
                out.push(Event::Session(id));
            }
            let failed = v["is_error"] == true || v["subtype"] != "success";
            let answer = text(&v["result"]);
            out.push(Event::Done {
                error: failed.then(|| {
                    answer
                        .clone()
                        .or_else(|| text(&v["subtype"]))
                        .unwrap_or_else(|| "알 수 없는 오류".to_owned())
                }),
                answer: (!failed).then_some(answer).flatten(),
            });
        }
        _ => {}
    }
    out
}

fn codex(v: &Value) -> Vec<Event> {
    let item = &v["item"];
    match (v["type"].as_str(), item["type"].as_str()) {
        (Some("thread.started"), _) => text(&v["thread_id"])
            .map(Event::Session)
            .into_iter()
            .collect(),
        (Some("item.started"), Some("mcp_tool_call")) => {
            let id = item["id"].as_str().unwrap_or_default().to_owned();
            let tool = item["tool"].as_str().unwrap_or("?");
            vec![
                Event::ToolStarted {
                    id: id.clone(),
                    label: tool_label(tool, &item["arguments"]),
                },
                Event::Call {
                    id,
                    tool: tool.to_owned(),
                    args: item["arguments"].clone(),
                },
            ]
        }
        (Some("item.completed"), Some("mcp_tool_call")) => {
            let error = text(&item["error"]["message"])
                .or_else(|| (item["status"] == "failed").then(|| "실패".to_owned()));
            vec![Event::ToolFinished {
                id: item["id"].as_str().unwrap_or_default().to_owned(),
                error,
            }]
        }
        (Some("item.completed"), Some("agent_message")) => {
            text(&item["text"]).map(Event::Text).into_iter().collect()
        }
        (Some("turn.completed"), _) => vec![Event::Done {
            answer: None,
            error: None,
        }],
        (Some("turn.failed"), _) => vec![Event::Done {
            answer: None,
            error: Some(text(&v["error"]["message"]).unwrap_or_else(|| "실패".to_owned())),
        }],
        (Some("error"), _) => vec![Event::Done {
            answer: None,
            error: Some(text(&v["message"]).unwrap_or_else(|| "오류".to_owned())),
        }],
        _ => Vec::new(),
    }
}

/// What one turn adds up to, built from its events in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    pub session: Option<String>,
    /// (id, label, finished, error) per tool call, in start order.
    pub tools: Vec<(String, String, bool, Option<String>)>,
    pub texts: Vec<String>,
    /// (id, tool, arguments) per call, for the history (see `Event::Call`).
    pub calls: Vec<(String, String, Value)>,
    pub done: bool,
    pub answer: Option<String>,
    pub error: Option<String>,
}

impl Turn {
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Session(id) => self.session = Some(id),
            Event::ToolStarted { id, label } => self.tools.push((id, label, false, None)),
            Event::Call { id, tool, args } => self.calls.push((id, tool, args)),
            Event::ToolFinished { id, error } => {
                if let Some(t) = self.tools.iter_mut().find(|t| t.0 == id && !t.2) {
                    t.2 = true;
                    t.3 = error;
                }
            }
            Event::Text(t) => self.texts.push(t),
            Event::Done { answer, error } => {
                self.done = true;
                self.answer = answer.or_else(|| self.texts.last().cloned());
                self.error = error;
            }
        }
    }
    /// The call shown while running: the latest one not yet finished.
    pub fn running(&self) -> Option<&str> {
        self.tools.iter().rev().find(|t| !t.2).map(|t| t.1.as_str())
    }
    /// Calls that ended with an error.
    pub fn failed(&self) -> usize {
        self.tools.iter().filter(|t| t.3.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(engine: Engine, fixture: &str) -> Turn {
        let mut t = Turn::default();
        for line in fixture.lines() {
            for e in parse_line(engine, line) {
                t.apply(e);
            }
        }
        t
    }
    fn labels(t: &Turn) -> Vec<&str> {
        t.tools.iter().map(|x| x.1.as_str()).collect()
    }

    #[test]
    fn a_claude_turn_gives_its_session_tools_and_final_answer() {
        let t = turn(Engine::Claude, include_str!("fixtures/claude-turn.jsonl"));
        assert_eq!(
            t.session.as_deref(),
            Some("2db12c05-1a1a-448b-be50-cef779cec910")
        );
        assert_eq!(
            labels(&t),
            ["list_documents", "get_text", "type_text", "get_text"]
        );
        assert!(
            t.tools.iter().all(|x| x.2 && x.3.is_none()),
            "{:?}",
            t.tools
        );
        assert!(t.done && t.error.is_none());
        assert!(
            t.answer
                .as_deref()
                .unwrap()
                .contains("안녕하세요 AI 편집 시험")
        );
        // Narration between calls is kept but is not the answer.
        assert_eq!(
            t.texts.first().map(String::as_str),
            Some("Empty document; type directly.")
        );
        assert_eq!(t.running(), None);
    }

    #[test]
    fn a_resumed_claude_turn_keeps_the_same_session() {
        let t = turn(Engine::Claude, include_str!("fixtures/claude-resume.jsonl"));
        assert_eq!(
            t.session.as_deref(),
            Some("2db12c05-1a1a-448b-be50-cef779cec910")
        );
        assert!(t.done && t.error.is_none() && t.answer.is_some());
    }

    #[test]
    fn a_codex_turn_labels_execute_and_hwp_action_calls() {
        let t = turn(Engine::Codex, include_str!("fixtures/codex-turn.jsonl"));
        assert!(t.session.as_deref().unwrap().starts_with("01a19660-51ed"));
        assert_eq!(
            labels(&t),
            [
                "list_documents",
                "list_commands",
                "hwp_action",
                "inspect_document",
                "hwp_action MoveDocEnd",
                "type_text",
                "get_text"
            ]
        );
        assert_eq!(t.failed(), 0);
        assert!(t.done && t.error.is_none());
        // Codex has no final-result line: the last agent message is the answer.
        assert!(t.answer.as_deref().unwrap().contains("Codex 편집 시험"));
        let resumed = turn(Engine::Codex, include_str!("fixtures/codex-resume.jsonl"));
        assert!(resumed.done && resumed.answer.is_some());
    }

    #[test]
    fn a_refused_codex_call_is_reported_as_a_failed_call() {
        let t = turn(
            Engine::Codex,
            include_str!("fixtures/codex-approval-denied.jsonl"),
        );
        assert_eq!(t.failed(), 1);
        let failed = t.tools.iter().find(|x| x.3.is_some()).unwrap();
        assert_eq!(failed.1, "caret.move");
        assert!(failed.3.as_deref().unwrap().contains("requires approva"));
    }

    #[test]
    fn a_running_call_is_the_latest_unfinished_one() {
        let mut t = Turn::default();
        let line = r#"{"type":"item.started","item":{"id":"a","type":"mcp_tool_call","tool":"execute","arguments":{"command":"table.create"}}}"#;
        parse_line(Engine::Codex, line)
            .into_iter()
            .for_each(|e| t.apply(e));
        assert_eq!(t.running(), Some("table.create"));
        assert!(!t.done);
    }

    #[test]
    fn errors_and_junk_lines() {
        assert!(parse_line(Engine::Claude, "not json").is_empty());
        assert!(parse_line(Engine::Codex, "").is_empty());
        assert_eq!(
            parse_line(
                Engine::Codex,
                r#"{"type":"turn.failed","error":{"message":"quota"}}"#
            ),
            [Event::Done {
                answer: None,
                error: Some("quota".into())
            }]
        );
        assert_eq!(
            parse_line(
                Engine::Claude,
                r#"{"type":"result","subtype":"error_max_turns","is_error":true,"session_id":"s"}"#
            ),
            [
                Event::Session("s".into()),
                Event::Done {
                    answer: None,
                    error: Some("error_max_turns".into())
                }
            ]
        );
    }
}
