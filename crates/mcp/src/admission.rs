//! Tool calls on one stdio connection reach their document's queue in the order
//! they were read. The SDK runs every request as its own task, so without this a
//! client that sends several calls before reading the answers saw them run in any
//! order (MCP 2nd review B-D1). The stdio transport numbers each `tools/call` as
//! it reads it ([`Arrival`]); a call waits until every earlier one has been put on
//! its queue, is put on its own, and lets the next one go. Only the hand-off is
//! ordered: the work itself still runs on the document's worker, and calls on
//! other documents still run in parallel.
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

/// The read order of a `tools/call` on its connection and that connection's gate,
/// set by the transport. The transport also releases the number when the answer
/// goes out or the call is cancelled, for calls the SDK answers without
/// `call_tool` (a schema violation, for example).
#[derive(Clone)]
pub(crate) struct Arrival {
    pub(crate) number: u64,
    pub(crate) admission: Arc<Admission>,
}

/// How long a call waits for an earlier one that never arrives (cancelled before
/// the SDK started it, or refused before `call_tool`): then it goes anyway.
const PREDECESSOR_WAIT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct Admission {
    /// The next number to admit, and numbers already done ahead of it.
    state: Mutex<(u64, BTreeSet<u64>)>,
    turn: Notify,
}

impl Admission {
    /// Wait until every call read before `ticket` has been handed on.
    pub(crate) async fn wait(&self, ticket: u64) {
        let deadline = tokio::time::Instant::now() + PREDECESSOR_WAIT;
        loop {
            let turn = self.turn.notified();
            if self.admitted(ticket) {
                return;
            }
            if tokio::time::timeout_at(deadline, turn).await.is_err() {
                return;
            }
        }
    }

    pub(crate) fn admitted(&self, ticket: u64) -> bool {
        self.state
            .lock()
            .map(|state| state.0 >= ticket)
            .unwrap_or(true)
    }

    /// `ticket` has been handed on (or will never be): let later calls go.
    pub(crate) fn release(&self, ticket: u64) {
        if let Ok(mut state) = self.state.lock() {
            let (next, done) = &mut *state;
            if ticket >= *next {
                done.insert(ticket);
                while done.remove(next) {
                    *next += 1;
                }
            }
        }
        self.turn.notify_waiters();
    }
}

/// Releases its ticket when dropped, so a call cancelled while waiting never holds
/// the ones after it.
pub(crate) struct Ticket<'a> {
    admission: &'a Admission,
    number: u64,
}

impl<'a> Ticket<'a> {
    pub(crate) fn new(admission: &'a Admission, number: u64) -> Self {
        Self { admission, number }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        self.admission.release(self.number);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn calls_are_admitted_in_ticket_order() {
        let admission = Arc::new(Admission::default());
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut tasks = Vec::new();
        // Started in reverse; admitted 0, 1, 2, 3.
        for ticket in (0..4).rev() {
            let (admission, order) = (admission.clone(), order.clone());
            tasks.push(tokio::spawn(async move {
                admission.wait(ticket).await;
                order.lock().unwrap().push(ticket);
                admission.release(ticket);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), [0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn a_missing_predecessor_holds_a_call_only_briefly() {
        let admission = Admission::default();
        // Ticket 0 never comes; ticket 1 goes after the bounded wait.
        let started = std::time::Instant::now();
        admission.wait(1).await;
        let waited = started.elapsed();
        assert!(
            waited >= PREDECESSOR_WAIT && waited < PREDECESSOR_WAIT * 3,
            "{waited:?}"
        );
        admission.release(1);
        // Once 0 is released late, the gate moves past both.
        admission.release(0);
        assert!(admission.admitted(2));
    }
}
