//! The AI edit panel's run on the window's document.
//!
//! While a run is open the panel's CLI connects with a token made for that run
//! alone; requests carrying it are marked [`AiClient`] with the run's number by
//! the token check. Those requests may only work on the window's document and
//! may not open, create, save or close documents (engine commands that touch
//! files are refused by the session itself, see `Session::set_commands_restricted`).
//! Other clients keep reading, but their edits to the window's document wait for
//! the run to end.
//!
//! Every check compares the run number, not only whether some run is open: a
//! request that passed the token check of one run is refused once that run ended,
//! even if another run has begun since, and a job it queued is refused when the
//! window comes to it after its run (see `ExternalDocument::serve`).

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// Marks a request that presented the token of the run with this number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AiClient(pub(crate) u64);

tokio::task_local! {
    /// The AI run a request belongs to, while its call is being queued: the job it
    /// queues carries the number, so the window can refuse it after the run. Work
    /// moved to another tokio task (`tokio::spawn`) loses it: keep AI calls on theirs.
    pub(crate) static AI_RUN: u64;
}

/// The run of the AI request whose call is being queued on this task, if any.
pub(crate) fn current_ai_run() -> Option<u64> {
    AI_RUN.try_with(|run| *run).ok()
}

/// Why an AI request whose run is over is refused.
pub(crate) const RUN_ENDED: &str = "AI 편집 실행이 끝났습니다.";

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
    /// The last run number handed out (runs count from 1).
    runs: AtomicU64,
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
    number: u64,
    token: Arc<str>,
    document_id: String,
    /// Cancelled when the run ends: ends the run's subscription streams.
    ended: CancellationToken,
}

impl AiGate {
    /// Open a run on `document_id` and return its token.
    pub(crate) fn begin(&self, document_id: &str) -> String {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let number = self.runs.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut run) = self.run.lock() {
            if let Some(old) = run.take() {
                old.ended.cancel();
            }
            *run = Some(Run {
                number,
                token: token.as_str().into(),
                document_id: document_id.to_owned(),
                ended: CancellationToken::new(),
            });
        }
        token
    }

    /// Close the run; its token stops working and its subscriptions end. Returns
    /// the number of the run that was open.
    pub(crate) fn end(&self) -> Option<u64> {
        let run = self.run.lock().ok()?.take()?;
        run.ended.cancel();
        Some(run.number)
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
        self.current().is_some()
    }

    /// The open run's number.
    pub(crate) fn current(&self) -> Option<u64> {
        self.run.lock().ok()?.as_ref().map(|run| run.number)
    }

    /// The open run's number if `presented` is its token (constant time for
    /// equal lengths).
    pub(crate) fn accepts(&self, presented: &[u8]) -> Option<u64> {
        let run = self.run.lock().ok()?;
        let run = run.as_ref()?;
        let expected = run.token.as_bytes();
        (presented.len() == expected.len()
            && presented
                .iter()
                .zip(expected)
                .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
                == 0)
            .then_some(run.number)
    }

    /// Why `tool` with `args` may not run now, if it may not. `ai` is the run
    /// number of a request that presented a run's token.
    pub(crate) fn refusal(&self, tool: &str, args: &Value, ai: Option<u64>) -> Option<String> {
        let Ok(run) = self.run.lock() else {
            // A poisoned gate refuses the AI rather than letting it through.
            return ai.map(|_| RUN_ENDED.into());
        };
        let document = args.get("document_id").and_then(Value::as_str);
        match (run.as_ref(), ai) {
            (Some(run), Some(number)) if run.number == number => {
                if AI_REFUSED.contains(&tool) {
                    Some(format!(
                        "AI 편집 중에는 `{tool}`를 쓸 수 없습니다(문서 열기·만들기·저장·닫기는 사람이 합니다)."
                    ))
                } else if document.is_some_and(|id| id != run.document_id) {
                    // The id a continued conversation remembers ends when the window
                    // opens a document again: name the one this run may use.
                    Some(format!(
                        "AI 편집은 이 창의 문서만 고칠 수 있습니다. 이 창 문서의 document_id는 \"{}\"입니다.",
                        run.document_id
                    ))
                } else {
                    None
                }
            }
            // Its own run is over (another may have begun since).
            (_, Some(_)) => Some(RUN_ENDED.into()),
            (None, None) => None,
            (Some(run), None) => (WRITES.contains(&tool)
                && document == Some(run.document_id.as_str()))
            .then(|| "AI가 이 문서를 편집하는 중입니다. 끝난 뒤 다시 보내세요.".into()),
        }
    }

    /// Whether an AI request of run `ai` may read the resource of `document_id`:
    /// only the window's document, only while its run is open.
    pub(crate) fn may_read(&self, ai: u64, document_id: &str) -> bool {
        self.run.lock().is_ok_and(|run| {
            run.as_ref()
                .is_some_and(|run| run.number == ai && run.document_id == document_id)
        })
    }

    /// The signal that ends run `ai`'s subscriptions; none when it is over.
    pub(crate) fn ended(&self, ai: u64) -> Option<CancellationToken> {
        let run = self.run.lock().ok()?;
        run.as_ref()
            .filter(|run| run.number == ai)
            .map(|run| run.ended.clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        assert!(gate.accepts(b"anything").is_none());
        let token = gate.begin("doc-1");
        assert!(token.len() >= 32);
        let first = gate.accepts(token.as_bytes()).unwrap();
        assert!(gate.accepts(b"wrong").is_none());
        assert_eq!(gate.end(), Some(first));
        assert!(gate.accepts(token.as_bytes()).is_none());
        let next = gate.begin("doc-1");
        assert_ne!(next, token);
        assert!(gate.accepts(token.as_bytes()).is_none());
        assert!(gate.accepts(next.as_bytes()).unwrap() > first);
    }

    #[test]
    fn the_ai_edits_only_the_window_document_and_touches_no_files() {
        let gate = AiGate::default();
        let window = json!({"document_id": "doc-1"});
        gate.begin("doc-1");
        let ai = gate.current();
        assert!(gate.refusal("execute", &window, ai).is_none());
        assert!(gate.refusal("get_text", &window, ai).is_none());
        assert!(gate.refusal("list_documents", &json!({}), ai).is_none());
        for tool in AI_REFUSED {
            assert!(gate.refusal(tool, &window, ai).is_some(), "{tool}");
        }
        let other = gate
            .refusal("execute", &json!({"document_id": "doc-2"}), ai)
            .unwrap_or_default();
        assert!(other.contains("\"doc-1\""), "{other}");
        gate.end();
        assert!(gate.refusal("get_text", &window, ai).is_some());
    }

    #[test]
    fn a_request_of_an_ended_run_is_refused_in_the_next_run() {
        let gate = AiGate::default();
        let window = json!({"document_id": "doc-1"});
        gate.begin("doc-1");
        let old = gate.current();
        gate.end();
        gate.begin("doc-1");
        assert_eq!(
            gate.refusal("type_text", &window, old).as_deref(),
            Some(RUN_ENDED)
        );
        assert!(gate.refusal("type_text", &window, gate.current()).is_none());
    }

    #[test]
    fn the_ai_reads_only_the_window_document_while_its_run_is_open() {
        let gate = AiGate::default();
        gate.begin("doc-1");
        let ai = gate.current().unwrap();
        assert!(gate.may_read(ai, "doc-1"));
        assert!(!gate.may_read(ai, "doc-2"));
        let ended = gate.ended(ai).unwrap();
        assert!(!ended.is_cancelled());
        gate.end();
        assert!(ended.is_cancelled());
        assert!(!gate.may_read(ai, "doc-1"));
        assert!(gate.ended(ai).is_none());
    }

    #[test]
    fn other_clients_read_but_wait_to_edit_the_window_document() {
        let gate = AiGate::default();
        let window = json!({"document_id": "doc-1"});
        let other = json!({"document_id": "doc-2"});
        assert!(gate.refusal("execute", &window, None).is_none());
        gate.begin("doc-1");
        for tool in WRITES {
            assert!(gate.refusal(tool, &window, None).is_some(), "{tool}");
            assert!(gate.refusal(tool, &other, None).is_none(), "{tool}");
        }
        assert!(gate.refusal("get_text", &window, None).is_none());
        assert!(gate.refusal("render_page", &window, None).is_none());
        gate.end();
        assert!(gate.refusal("execute", &window, None).is_none());
    }
}
