//! The AI edit panel's run on the window's document.
//!
//! While a run is open the panel's CLI connects with a token made for that run
//! alone; requests carrying it are marked [`AiClient`] by the token check. Those
//! requests may only work on the window's document and may not open, create,
//! save or close documents (engine commands that touch files are refused by the
//! session itself, see `Session::set_commands_restricted`). Other clients keep
//! reading, but their edits to the window's document wait for the run to end.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use serde_json::Value;

/// Marks a request that presented the open run's token.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AiClient;

/// Tools the AI may not call: they open, create, save or close documents.
const AI_REFUSED: &[&str] = &[
    "new_document",
    "open_document",
    "open_protected_document",
    "save_document",
    "close_document",
];

/// Tools that change a document or its selection; refused to other clients on
/// the window's document while the AI runs.
const WRITES: &[&str] = &[
    "type_text",
    "execute",
    "batch",
    "hwp_action",
    "select_text",
    "save_document",
    "close_document",
];

#[derive(Default)]
pub(crate) struct AiGate {
    run: Mutex<Option<Run>>,
    /// AI requests past the token check and not yet answered. Ending a run waits
    /// for them, so none slips in after the window stopped serving the run.
    in_flight: AtomicUsize,
}

/// Held while an admitted AI request is being answered.
pub(crate) struct AiFlight(Arc<AiGate>);

impl Drop for AiFlight {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Run {
    token: Arc<str>,
    document_id: String,
}

impl AiGate {
    /// Open a run on `document_id` and return its token.
    pub(crate) fn begin(&self, document_id: &str) -> String {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        if let Ok(mut run) = self.run.lock() {
            *run = Some(Run {
                token: token.as_str().into(),
                document_id: document_id.to_owned(),
            });
        }
        token
    }

    /// Close the run; its token stops working.
    pub(crate) fn end(&self) {
        if let Ok(mut run) = self.run.lock() {
            *run = None;
        }
    }

    /// Count an admitted AI request until the returned guard drops.
    pub(crate) fn admit(self: &Arc<Self>) -> AiFlight {
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        AiFlight(Arc::clone(self))
    }

    /// AI requests admitted and not yet answered.
    pub(crate) fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Acquire)
    }

    pub(crate) fn active(&self) -> bool {
        self.run.lock().is_ok_and(|run| run.is_some())
    }

    /// Whether `presented` is the open run's token (constant time for equal lengths).
    pub(crate) fn accepts(&self, presented: &[u8]) -> bool {
        let Ok(run) = self.run.lock() else {
            return false;
        };
        let Some(run) = run.as_ref() else {
            return false;
        };
        let expected = run.token.as_bytes();
        presented.len() == expected.len()
            && presented
                .iter()
                .zip(expected)
                .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
                == 0
    }

    /// Why `tool` with `args` may not run now, if it may not. `ai` is true for a
    /// request that presented the run's token.
    pub(crate) fn refusal(&self, tool: &str, args: &Value, ai: bool) -> Option<String> {
        let run = self.run.lock().ok()?;
        let document = args.get("document_id").and_then(Value::as_str);
        match run.as_ref() {
            None if ai => Some("AI 편집 실행이 끝났습니다.".into()),
            None => None,
            Some(run) if ai => {
                if AI_REFUSED.contains(&tool) {
                    Some(format!(
                        "AI 편집 중에는 `{tool}`를 쓸 수 없습니다(문서 열기·만들기·저장·닫기는 사람이 합니다)."
                    ))
                } else if document.is_some_and(|id| id != run.document_id) {
                    Some("AI 편집은 이 창의 문서만 고칠 수 있습니다.".into())
                } else {
                    None
                }
            }
            Some(run) => (WRITES.contains(&tool) && document == Some(run.document_id.as_str()))
                .then(|| "AI가 이 문서를 편집하는 중입니다. 끝난 뒤 다시 보내세요.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn admitted_requests_are_counted_until_answered() {
        let gate = Arc::new(AiGate::default());
        let first = gate.admit();
        let second = gate.admit();
        assert_eq!(gate.in_flight(), 2);
        drop(first);
        assert_eq!(gate.in_flight(), 1);
        drop(second);
        assert_eq!(gate.in_flight(), 0);
    }

    #[test]
    fn a_run_token_works_only_while_the_run_is_open() {
        let gate = AiGate::default();
        assert!(!gate.accepts(b"anything"));
        let token = gate.begin("doc-1");
        assert!(token.len() >= 32);
        assert!(gate.accepts(token.as_bytes()));
        assert!(!gate.accepts(b"wrong"));
        gate.end();
        assert!(!gate.accepts(token.as_bytes()));
        let next = gate.begin("doc-1");
        assert_ne!(next, token);
        assert!(!gate.accepts(token.as_bytes()));
    }

    #[test]
    fn the_ai_edits_only_the_window_document_and_touches_no_files() {
        let gate = AiGate::default();
        let window = json!({"document_id": "doc-1"});
        gate.begin("doc-1");
        assert!(gate.refusal("execute", &window, true).is_none());
        assert!(gate.refusal("get_text", &window, true).is_none());
        assert!(gate.refusal("list_documents", &json!({}), true).is_none());
        for tool in AI_REFUSED {
            assert!(gate.refusal(tool, &window, true).is_some(), "{tool}");
        }
        assert!(
            gate.refusal("execute", &json!({"document_id": "doc-2"}), true)
                .is_some()
        );
        gate.end();
        assert!(gate.refusal("get_text", &window, true).is_some());
    }

    #[test]
    fn other_clients_read_but_wait_to_edit_the_window_document() {
        let gate = AiGate::default();
        let window = json!({"document_id": "doc-1"});
        let other = json!({"document_id": "doc-2"});
        assert!(gate.refusal("execute", &window, false).is_none());
        gate.begin("doc-1");
        for tool in WRITES {
            assert!(gate.refusal(tool, &window, false).is_some(), "{tool}");
            assert!(gate.refusal(tool, &other, false).is_none(), "{tool}");
        }
        assert!(gate.refusal("get_text", &window, false).is_none());
        assert!(gate.refusal("render_page", &window, false).is_none());
        gate.end();
        assert!(gate.refusal("execute", &window, false).is_none());
    }
}
