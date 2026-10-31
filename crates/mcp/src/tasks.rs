//! MCP Tasks extension (`io.modelcontextprotocol/tasks`) for long file work.
//!
//! Only opening and saving become tasks: a 381-page PDF export takes 11–22 s
//! and longer on a busy machine, near common client request timeouts. Everything else stays a
//! direct result. A task is created only when the request itself declares the
//! extension in its client capabilities; other clients see no change.
//!
//! Limits, stated in discovery and docs/mcp.md:
//! - status is observed by polling `tasks/get`; no task notifications are sent
//!   (the SDK cannot route them through `subscriptions/listen` yet);
//! - `tasks/cancel` is cooperative: a running engine call finishes, but a save
//!   cancelled before its file is replaced leaves the existing file unchanged.
use rmcp::task_manager::TaskOptions;
use serde_json::Value;

/// Completed tasks stay observable this long after creation (and the SDK keeps
/// terminal ones for one more window).
pub(crate) const TTL_MS: u64 = 10 * 60 * 1000;
pub(crate) const POLL_INTERVAL_MS: u64 = 1000;
/// Tasks still running at once. Beyond this an eligible call runs directly and
/// returns its result, which the extension allows, so a client that floods
/// task-creating calls cannot pile up unbounded background work.
pub(crate) const MAX_RUNNING: usize = 64;

/// Engine commands that read or write a whole document file.
const FILE_COMMANDS: &[&str] = &[
    "file.open",
    "file.open_protected",
    "file.save",
    "file.save_as",
    "file.save_hml",
    "file.save_pages",
    "file.save_block",
    "file.pdf",
    "file.print_pdf",
];

/// Would this tool call run as a task for a client that supports tasks?
pub(crate) fn eligible(tool: &str, args: &Value) -> bool {
    match tool {
        "open_document" | "open_protected_document" | "save_document" => true,
        "execute" => args["command"]
            .as_str()
            .is_some_and(|command| FILE_COMMANDS.contains(&command)),
        _ => false,
    }
}

pub(crate) fn options() -> TaskOptions {
    TaskOptions::new()
        .with_ttl_ms(TTL_MS)
        .with_poll_interval_ms(POLL_INTERVAL_MS)
}

#[cfg(test)]
mod tests {
    use super::eligible;
    use serde_json::json;

    #[test]
    fn only_whole_file_work_becomes_a_task() {
        for tool in ["open_document", "open_protected_document", "save_document"] {
            assert!(eligible(tool, &json!({})), "{tool}");
        }
        for command in ["file.pdf", "file.save_as", "file.open", "file.print_pdf"] {
            assert!(
                eligible("execute", &json!({"command":command})),
                "{command}"
            );
        }
        for (tool, args) in [
            ("execute", json!({"command":"text.insert"})),
            ("execute", json!({"command":"file.new"})),
            ("execute", json!({})),
            ("new_document", json!({})),
            ("render_page", json!({})),
            ("batch", json!({"commands":[{"command":"file.save"}]})),
            ("get_text", json!({})),
        ] {
            assert!(!eligible(tool, &args), "{tool} {args}");
        }
    }
}
