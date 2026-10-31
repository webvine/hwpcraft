//! Serialized, isolated document engines for the asynchronous MCP transport.
//! A !Send Headless/Session is constructed, used and dropped on its own thread.
//! Only owned JSON, cancellation tokens and reply channels cross that boundary.
use crate::document_subscriptions::{DocumentStamp, DocumentSubscriptions};
use crate::{Headless, tools};
use anyhow::{Result, anyhow, bail};
use hwpcraft_formats::file_access::AutomationRoots;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const MAX_DOCUMENTS: usize = 16;
const QUEUE_CAPACITY: usize = 64;
const STARTING: u8 = 0;
const ACTIVE: u8 = 1;
const CLOSED: u8 = 2;
/// With every slot taken, a new document may replace the least recently used
/// one that has been idle this long and has no unsaved changes. Modified
/// documents are never closed automatically.
const IDLE_EVICTION: Duration = Duration::from_secs(30 * 60);

type Registry = Mutex<HashMap<String, Worker>>;

#[derive(Clone)]
struct Worker {
    sender: mpsc::Sender<Job>,
    state: Arc<AtomicU8>,
    finished: CancellationToken,
    activity: Arc<Activity>,
    /// Called after a job is queued for an external document, whose jobs are
    /// served by another loop (the desktop window) that must be woken up.
    wake: Option<Wake>,
    /// Title and state of an external (desktop) document, kept by its owner.
    external: Option<Arc<Mutex<ExternalInfo>>>,
}

/// What `list_documents` shows for a desktop document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExternalInfo {
    pub(crate) title: String,
    pub(crate) path: Option<String>,
    pub(crate) dirty: bool,
}

/// One listed document. Only documents the desktop user attached are listed:
/// an id MCP created is a capability its client holds, never enumerated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentEntry {
    pub(crate) id: String,
    pub(crate) source: &'static str,
    pub(crate) info: ExternalInfo,
}

/// Wakes the loop that serves an external document's jobs.
pub(crate) type Wake = Arc<dyn Fn() + Send + Sync>;

/// What the async side may know about a worker without asking its thread.
/// Only a hint for choosing an eviction candidate: the worker thread itself
/// re-checks idleness and unsaved changes before it closes.
struct Activity {
    /// Jobs accepted for this worker and not yet finished.
    pending: AtomicUsize,
    last_used: Mutex<Instant>,
    dirty: AtomicBool,
}
impl Activity {
    fn new() -> Self {
        Self {
            pending: AtomicUsize::new(0),
            last_used: Mutex::new(Instant::now()),
            dirty: AtomicBool::new(false),
        }
    }
    fn last_used(&self) -> Instant {
        self.last_used
            .lock()
            .map_or_else(|_| Instant::now(), |time| *time)
    }
    fn idle_for(&self) -> Duration {
        self.last_used().elapsed()
    }
    fn evictable(&self, idle: Duration) -> bool {
        self.pending.load(Ordering::Acquire) == 0
            && !self.dirty.load(Ordering::Acquire)
            && self.idle_for() >= idle
    }
}
/// Ends one job's accounting, however the job ends.
struct JobDone<'a> {
    activity: &'a Activity,
    touch: bool,
}
impl Drop for JobDone<'_> {
    fn drop(&mut self) {
        if self.touch
            && let Ok(mut time) = self.activity.last_used.lock()
        {
            *time = Instant::now();
        }
        self.activity.pending.fetch_sub(1, Ordering::AcqRel);
    }
}

enum Operation {
    Call {
        tool: String,
        args: Value,
    },
    Close {
        force: bool,
    },
    /// Close only if still idle for `idle`, nothing else is queued and the
    /// document has no unsaved changes; decided on the worker thread.
    /// Internal: replies a bare `{"closed": bool}`, not a tool result.
    Evict {
        idle: Duration,
    },
    #[cfg(test)]
    Block {
        entered: oneshot::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    },
    #[cfg(test)]
    BlockThenCall {
        entered: oneshot::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
        tool: String,
        args: Value,
    },
    #[cfg(test)]
    Panic,
    #[cfg(test)]
    InvalidateRollback,
}
struct Job {
    operation: Operation,
    cancel: CancellationToken,
    reply: oneshot::Sender<Result<Value>>,
}

pub(crate) struct DocumentWorkers {
    subscriptions: Arc<DocumentSubscriptions>,
    roots: Option<Arc<AutomationRoots>>,
    registry: Arc<Registry>,
    live: Arc<AtomicUsize>,
    idle_eviction: Duration,
}

impl DocumentWorkers {
    pub(crate) fn new(roots: Option<Arc<AutomationRoots>>) -> Self {
        Self {
            subscriptions: Arc::new(DocumentSubscriptions::new()),
            roots,
            registry: Arc::new(Mutex::new(HashMap::new())),
            live: Arc::new(AtomicUsize::new(0)),
            idle_eviction: IDLE_EVICTION,
        }
    }

    /// When every slot is taken, close the least recently used idle document
    /// without unsaved changes so a new one can open. Otherwise do nothing;
    /// allocation then reports the limit as before.
    async fn make_room(&self, cancel: &CancellationToken) {
        let candidates = {
            let Ok(registry) = self.registry.lock() else {
                return;
            };
            let open = registry
                .values()
                .filter(|worker| worker.state.load(Ordering::Acquire) != CLOSED)
                .count();
            if open < MAX_DOCUMENTS {
                return;
            }
            let mut candidates: Vec<(Instant, String, Worker)> = registry
                .iter()
                .filter(|(_, worker)| {
                    worker.state.load(Ordering::Acquire) == ACTIVE
                        && worker.activity.evictable(self.idle_eviction)
                })
                .map(|(id, worker)| (worker.activity.last_used(), id.clone(), worker.clone()))
                .collect();
            candidates.sort_by_key(|(time, _, _)| *time);
            candidates
        };
        for (_, id, worker) in candidates {
            let operation = Operation::Evict {
                idle: self.idle_eviction,
            };
            let Ok(value) = dispatch(&worker, operation, cancel.clone()).await else {
                continue;
            };
            if value["closed"] == true {
                // Wait for the thread to exit so its slot is really free.
                let _ = cancel
                    .run_until_cancelled(worker.finished.cancelled())
                    .await;
                remove(&self.registry, &id);
                return;
            }
        }
    }

    #[cfg(test)]
    fn with_idle_eviction(mut self, idle: Duration) -> Self {
        self.idle_eviction = idle;
        self
    }

    pub(crate) fn subscriptions(&self) -> &DocumentSubscriptions {
        &self.subscriptions
    }

    pub(crate) async fn create(
        &self,
        tool: &str,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<(Option<String>, Value)> {
        match self.create_inner(tool, args, cancel).await {
            Ok(result) => Ok(result),
            Err(error) => Ok((None, error_result(&error.to_string()))),
        }
    }

    async fn create_inner(
        &self,
        tool: &str,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<(Option<String>, Value)> {
        if !matches!(
            tool,
            "new_document" | "open_document" | "open_protected_document"
        ) {
            bail!("새 문서는 new_document/open_document/open_protected_document로 만드세요.");
        }
        check_cancel(&cancel)?;
        self.make_room(&cancel).await;
        let (id, worker, ready) = self.allocate()?;
        let mut lease = CreationLease {
            registry: &self.registry,
            id: id.clone(),
            worker: worker.clone(),
            retained: false,
        };
        let initialized = cancel
            .run_until_cancelled(ready)
            .await
            .ok_or_else(cancelled)?
            .map_err(|_| anyhow!("문서 worker 초기화가 중단됐습니다."))?;
        initialized?;
        check_cancel(&cancel)?;
        let result = dispatch(
            &worker,
            Operation::Call {
                tool: tool.to_owned(),
                args,
            },
            cancel.clone(),
        )
        .await?;
        if result["isError"] == true {
            return Ok((None, result));
        }
        check_cancel(&cancel)?;
        worker
            .state
            .compare_exchange(STARTING, ACTIVE, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow!("새 문서 worker가 종료됐습니다."))?;
        self.subscriptions.register(&id)?;
        lease.retained = true;
        Ok((Some(id), result))
    }

    pub(crate) async fn call(
        &self,
        id: &str,
        tool: &str,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        check_cancel(&cancel)?;
        let worker = self.lookup(id)?;
        dispatch(
            &worker,
            Operation::Call {
                tool: tool.to_owned(),
                args,
            },
            cancel,
        )
        .await
    }

    pub(crate) async fn execute(
        &self,
        id: &str,
        command: &str,
        params: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.call(
            id,
            "execute",
            json!({"command":command,"params":params}),
            cancel,
        )
        .await
    }

    pub(crate) async fn close(
        &self,
        id: &str,
        force: bool,
        cancel: CancellationToken,
    ) -> Result<Value> {
        check_cancel(&cancel)?;
        let worker = self.lookup(id)?;
        let result = dispatch(&worker, Operation::Close { force }, cancel.clone()).await;
        if worker.state.load(Ordering::Acquire) == CLOSED {
            let finished = cancel
                .run_until_cancelled(worker.finished.cancelled())
                .await;
            remove(&self.registry, id);
            if finished.is_none() {
                return Err(cancelled());
            }
        }
        result
    }

    fn lookup(&self, id: &str) -> Result<Worker> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| anyhow!("문서 목록을 사용할 수 없습니다."))?;
        registry.retain(|_, worker| worker.state.load(Ordering::Acquire) != CLOSED);
        registry
            .get(id)
            .filter(|worker| worker.state.load(Ordering::Acquire) == ACTIVE)
            .cloned()
            .ok_or_else(|| anyhow!("문서 ID가 없거나 문서가 닫혔습니다."))
    }

    fn allocate(&self) -> Result<(String, Worker, oneshot::Receiver<Result<()>>)> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| anyhow!("문서 목록을 사용할 수 없습니다."))?;
        registry.retain(|_, worker| worker.state.load(Ordering::Acquire) != CLOSED);
        if registry.len() >= MAX_DOCUMENTS {
            bail!("문서는 동시에 16개까지 열 수 있습니다.");
        }
        self.live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                if count < MAX_DOCUMENTS {
                    Some(count + 1)
                } else {
                    None
                }
            })
            .map_err(|_| {
                anyhow!("문서 worker는 종료 중인 요청을 포함해 16개까지 실행할 수 있습니다.")
            })?;
        let id = loop {
            let candidate = uuid::Uuid::new_v4().to_string();
            if !registry.contains_key(&candidate) {
                break candidate;
            }
        };
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let worker = Worker {
            sender,
            state: Arc::new(AtomicU8::new(STARTING)),
            finished: CancellationToken::new(),
            activity: Arc::new(Activity::new()),
            wake: None,
            external: None,
        };
        let (ready_sender, ready_receiver) = oneshot::channel();
        let roots = self.roots.clone();
        let activity = Arc::clone(&worker.activity);
        let exit = WorkerExit {
            subscriptions: Arc::clone(&self.subscriptions),
            registry: Arc::downgrade(&self.registry),
            id: id.clone(),
            state: Arc::clone(&worker.state),
            live: Arc::clone(&self.live),
            finished: worker.finished.clone(),
        };
        registry.insert(id.clone(), worker.clone());
        // The closure's exit guard also runs when OS thread creation fails.
        // Never hold the registry lock while that guard could remove its slot.
        drop(registry);
        let spawned = thread::Builder::new()
            .name(format!("hwpcraft-document-{id}"))
            .spawn(move || {
                let _exit = exit;
                // Engine panic means the document may be partially mutated.
                // Do not reuse it or expose the panic payload (possibly private).
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    run(
                        roots,
                        receiver,
                        ready_sender,
                        &_exit.state,
                        &_exit.subscriptions,
                        &_exit.id,
                        &activity,
                    )
                }));
            });
        if spawned.is_err() {
            worker.state.store(CLOSED, Ordering::Release);
            remove(&self.registry, &id);
            bail!("문서 worker 스레드를 만들지 못했습니다.");
        }
        Ok((id, worker, ready_receiver))
    }
}

impl DocumentWorkers {
    /// The attached desktop documents, by id. MCP-created documents are not listed.
    pub(crate) fn list(&self) -> Vec<DocumentEntry> {
        let Ok(registry) = self.registry.lock() else {
            return Vec::new();
        };
        let mut entries: Vec<DocumentEntry> = registry
            .iter()
            .filter(|(_, worker)| worker.state.load(Ordering::Acquire) == ACTIVE)
            .filter_map(|(id, worker)| {
                let info = worker.external.as_ref()?.lock().ok()?.clone();
                Some(DocumentEntry {
                    id: id.clone(),
                    source: "desktop",
                    info,
                })
            })
            .collect();
        entries.sort_by(|a, b| a.info.title.cmp(&b.info.title).then(a.id.cmp(&b.id)));
        entries
    }

    /// Register a document whose engine lives on the caller's thread (the desktop
    /// window). Its jobs are queued like any other document's and served by
    /// [`ExternalDocument::serve`]; `wake` is called after each job is queued.
    /// MCP cannot close or evict it; dropping the handle detaches it.
    pub(crate) fn attach_external(
        &self,
        wake: Wake,
        info: ExternalInfo,
    ) -> Result<ExternalDocument> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| anyhow!("문서 목록을 사용할 수 없습니다."))?;
        registry.retain(|_, worker| worker.state.load(Ordering::Acquire) != CLOSED);
        if registry.len() >= MAX_DOCUMENTS {
            bail!("문서는 동시에 16개까지 열 수 있습니다.");
        }
        let id = loop {
            let candidate = uuid::Uuid::new_v4().to_string();
            if !registry.contains_key(&candidate) {
                break candidate;
            }
        };
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let worker = Worker {
            sender,
            state: Arc::new(AtomicU8::new(ACTIVE)),
            finished: CancellationToken::new(),
            activity: Arc::new(Activity::new()),
            wake: Some(wake),
            external: Some(Arc::new(Mutex::new(info))),
        };
        self.subscriptions.register(&id)?;
        registry.insert(id.clone(), worker.clone());
        Ok(ExternalDocument {
            id,
            receiver,
            info: worker.external.clone().unwrap_or_default(),
            state: worker.state,
            activity: worker.activity,
            subscriptions: Arc::clone(&self.subscriptions),
            registry: Arc::downgrade(&self.registry),
        })
    }
}

/// A document served on its owner's thread; see [`DocumentWorkers::attach_external`].
pub(crate) struct ExternalDocument {
    id: String,
    receiver: mpsc::Receiver<Job>,
    info: Arc<Mutex<ExternalInfo>>,
    state: Arc<AtomicU8>,
    activity: Arc<Activity>,
    subscriptions: Arc<DocumentSubscriptions>,
    registry: Weak<Registry>,
}

impl ExternalDocument {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    /// Update what `list_documents` shows (title, path, unsaved changes).
    pub(crate) fn set_info(&self, info: ExternalInfo) {
        if let Ok(mut current) = self.info.lock()
            && *current != info
        {
            *current = info;
        }
    }
    /// Run queued jobs on `backend` until none is left or `budget` has passed
    /// (at least one runs). Returns how many ran.
    pub(crate) fn serve(&mut self, backend: &mut Headless, budget: Duration) -> usize {
        let started = Instant::now();
        let mut served = 0;
        while let Ok(job) = self.receiver.try_recv() {
            served += 1;
            process(
                backend,
                job,
                &self.state,
                &self.subscriptions,
                &self.id,
                &self.activity,
                true,
            );
            if started.elapsed() >= budget {
                break;
            }
        }
        served
    }
}

impl Drop for ExternalDocument {
    fn drop(&mut self) {
        self.state.store(CLOSED, Ordering::Release);
        self.subscriptions.closed(&self.id);
        if let Some(registry) = self.registry.upgrade() {
            remove(&registry, &self.id);
        }
        self.receiver.close();
        while let Ok(job) = self.receiver.try_recv() {
            let _ = job
                .reply
                .send(Err(anyhow!("데스크톱 문서 연결이 끊겼습니다.")));
        }
    }
}

impl Drop for DocumentWorkers {
    fn drop(&mut self) {
        self.subscriptions.close_all();
        if let Ok(mut registry) = self.registry.lock() {
            for worker in registry.values() {
                worker.state.store(CLOSED, Ordering::Release);
            }
            registry.clear();
        }
        // A running synchronous engine call completes on its own thread. No
        // thread is killed, joined on the async runtime, or moved between threads.
    }
}

struct CreationLease<'a> {
    registry: &'a Arc<Registry>,
    id: String,
    worker: Worker,
    retained: bool,
}
impl Drop for CreationLease<'_> {
    fn drop(&mut self) {
        if !self.retained {
            self.worker.state.store(CLOSED, Ordering::Release);
            remove(self.registry, &self.id);
        }
    }
}
struct WorkerExit {
    subscriptions: Arc<DocumentSubscriptions>,
    registry: Weak<Registry>,
    id: String,
    state: Arc<AtomicU8>,
    live: Arc<AtomicUsize>,
    finished: CancellationToken,
}
impl Drop for WorkerExit {
    fn drop(&mut self) {
        self.subscriptions.closed(&self.id);
        self.state.store(CLOSED, Ordering::Release);
        if let Some(registry) = self.registry.upgrade() {
            remove(&registry, &self.id);
        }
        self.live.fetch_sub(1, Ordering::AcqRel);
        self.finished.cancel();
    }
}
fn remove(registry: &Registry, id: &str) {
    if let Ok(mut entries) = registry.lock() {
        entries.remove(id);
    }
}
fn cancelled() -> anyhow::Error {
    anyhow!("문서 요청이 취소됐습니다.")
}
fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    Ok(())
}
fn error_result(message: &str) -> Value {
    tools::result(json!({"error":message}), true)
}

async fn dispatch(
    worker: &Worker,
    operation: Operation,
    cancel: CancellationToken,
) -> Result<Value> {
    check_cancel(&cancel)?;
    // Dropping an SDK request future cancels only this queued job, not its
    // caller's token or another request that happens to share the parent token.
    let cancel = cancel.child_token();
    let cancellation_guard = cancel.clone().drop_guard();
    if worker.state.load(Ordering::Acquire) == CLOSED {
        bail!("문서가 닫혔습니다.");
    }
    let (reply, receive) = oneshot::channel();
    worker.activity.pending.fetch_add(1, Ordering::AcqRel);
    worker
        .sender
        .try_send(Job {
            operation,
            cancel: cancel.clone(),
            reply,
        })
        .inspect_err(|_| {
            worker.activity.pending.fetch_sub(1, Ordering::AcqRel);
        })
        .map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => {
                anyhow!("문서 요청 대기열이 가득 찼습니다(64개).")
            }
            mpsc::error::TrySendError::Closed(_) => {
                worker.state.store(CLOSED, Ordering::Release);
                anyhow!("문서 worker가 종료됐습니다.")
            }
        })?;
    if let Some(wake) = &worker.wake {
        wake();
    }
    let result = cancel
        .run_until_cancelled(receive)
        .await
        .ok_or_else(cancelled)?
        .map_err(|_| {
            worker.state.store(CLOSED, Ordering::Release);
            anyhow!("문서 worker가 종료돼 요청을 완료하지 못했습니다.")
        })?;
    check_cancel(&cancel)?;
    let _ = cancellation_guard.disarm();
    result
}

fn run(
    roots: Option<Arc<AutomationRoots>>,
    mut receiver: mpsc::Receiver<Job>,
    ready: oneshot::Sender<Result<()>>,
    state: &AtomicU8,
    subscriptions: &DocumentSubscriptions,
    document_id: &str,
    activity: &Activity,
) {
    let mut backend = match Headless::new(roots) {
        Ok(backend) => backend,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    while let Some(job) = receiver.blocking_recv() {
        if process(
            &mut backend,
            job,
            state,
            subscriptions,
            document_id,
            activity,
            false,
        ) {
            break;
        }
    }
}

/// Run one job on `backend`; true when the document is now closed. An external
/// document (the desktop window's) is never closed or evicted through MCP.
fn process(
    backend: &mut Headless,
    job: Job,
    state: &AtomicU8,
    subscriptions: &DocumentSubscriptions,
    document_id: &str,
    activity: &Activity,
    external: bool,
) -> bool {
    let _done = JobDone {
        activity,
        // An eviction check must not make the document look used.
        touch: !matches!(job.operation, Operation::Evict { .. }),
    };
    if state.load(Ordering::Acquire) == CLOSED {
        let _ = job.reply.send(Err(anyhow!("문서가 닫혔습니다.")));
        return true;
    }
    if job.cancel.is_cancelled() {
        let _ = job.reply.send(Err(cancelled()));
        return false;
    }
    // A request cancelled while its synchronous call runs must not replace
    // a file: atomic writes check this scope just before they persist.
    let _cancel_scope = hwpcraft_formats::file_access::cancel_scope({
        let token = job.cancel.clone();
        move || token.is_cancelled()
    });
    let (result, closed) = match job.operation {
        Operation::Call { tool, args } => {
            let before = DocumentStamp::capture(&backend.session);
            let roots = backend.roots.clone();
            let result = hwpcraft_formats::file_access::confine(roots.as_ref(), || {
                tools::call(backend, &tool, args)
            });
            if backend.usable && before != DocumentStamp::capture(&backend.session) {
                subscriptions.changed(document_id);
            }
            (result, false)
        }
        Operation::Close { .. } if external => (
            error_result(
                "데스크톱 창에서 연 문서는 MCP로 닫을 수 없습니다. 데스크톱에서 닫거나 연결을 끄세요.",
            ),
            false,
        ),
        Operation::Evict { .. } if external => (json!({"closed":false}), false),
        Operation::Close { force } => match backend.require_discard(force) {
            Ok(()) => {
                state.store(CLOSED, Ordering::Release);
                (tools::result(json!({"closed":true}), false), true)
            }
            Err(error) => (error_result(&error.to_string()), false),
        },
        Operation::Evict { idle } => {
            let quiet =
                activity.pending.load(Ordering::Acquire) == 1 && activity.idle_for() >= idle;
            if quiet && backend.require_discard(false).is_ok() {
                state.store(CLOSED, Ordering::Release);
                (json!({"closed":true}), true)
            } else {
                (json!({"closed":false}), false)
            }
        }
        #[cfg(test)]
        Operation::Block { entered, release } => {
            let _ = entered.send(());
            let _ = release.recv();
            (tools::result(json!({"blocked":false}), false), false)
        }
        #[cfg(test)]
        Operation::BlockThenCall {
            entered,
            release,
            tool,
            args,
        } => {
            let _ = entered.send(());
            let _ = release.recv();
            (tools::call(backend, &tool, args), false)
        }
        #[cfg(test)]
        Operation::InvalidateRollback => {
            let result = (|| -> Result<()> {
                let checkpoint = backend.session.full_checkpoint()?;
                backend
                    .session
                    .core
                    .create_blank_document_native()
                    .map_err(|e| anyhow!("{e}"))?;
                crate::atomic_batch::rollback(backend, checkpoint)
            })();
            (
                match result {
                    Ok(()) => tools::result(json!({"unexpectedRestoreSuccess":true}), false),
                    Err(error) => error_result(&error.to_string()),
                },
                false,
            )
        }
        #[cfg(test)]
        Operation::Panic => std::panic::resume_unwind(Box::new("private-test-panic-password")),
    };
    if !backend.usable {
        state.store(CLOSED, Ordering::Release);
    }
    activity
        .dirty
        .store(backend.usable && backend.session.dirty(), Ordering::Release);
    // Cancellation after a synchronous call does not rewind it: file side
    // effects and path/password replacement cannot be generally rolled back.
    if job.cancel.is_cancelled() {
        let _ = job.reply.send(Err(cancelled()));
    } else {
        let _ = job.reply.send(Ok(result));
    }
    closed || state.load(Ordering::Acquire) == CLOSED
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hwpcraft_engine::{Command, Session};
    use std::time::Duration;

    fn data(result: Value) -> Value {
        assert_eq!(result["isError"], false, "{result}");
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }
    async fn document(workers: &DocumentWorkers) -> String {
        let (id, result) = workers
            .create("new_document", json!({}), CancellationToken::new())
            .await
            .unwrap();
        data(result);
        id.unwrap()
    }
    fn enqueue(
        worker: &Worker,
        operation: Operation,
        cancel: CancellationToken,
    ) -> oneshot::Receiver<Result<Value>> {
        let (reply, receive) = oneshot::channel();
        assert!(
            worker
                .sender
                .try_send(Job {
                    operation,
                    cancel,
                    reply
                })
                .is_ok()
        );
        receive
    }
    async fn paused(
        workers: &DocumentWorkers,
        id: &str,
    ) -> (
        Worker,
        std::sync::mpsc::Sender<()>,
        oneshot::Receiver<Result<Value>>,
    ) {
        let worker = workers.lookup(id).unwrap();
        let (entered, started) = oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let reply = enqueue(
            &worker,
            Operation::Block {
                entered,
                release: gate,
            },
            CancellationToken::new(),
        );
        tokio::time::timeout(Duration::from_secs(10), started)
            .await
            .unwrap()
            .unwrap();
        (worker, release, reply)
    }
    async fn queued(worker: &Worker, count: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while worker.sender.capacity() != QUEUE_CAPACITY - count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_slots_reuse_only_the_oldest_idle_unmodified_document() {
        let workers = DocumentWorkers::new(None).with_idle_eviction(Duration::from_millis(200));
        // The oldest document has unsaved changes and must survive.
        let modified = document(&workers).await;
        let typed = workers
            .call(
                &modified,
                "type_text",
                json!({"text":"unsaved"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(typed["isError"], false, "{typed}");
        let mut clean = Vec::new();
        for _ in 1..MAX_DOCUMENTS {
            clean.push(document(&workers).await);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        // A recently used clean document is not idle and must survive too.
        let recent = clean[0].clone();
        workers
            .call(&recent, "get_text", json!({}), CancellationToken::new())
            .await
            .unwrap();

        let replacement = document(&workers).await;
        assert!(workers.lookup(&replacement).is_ok());
        assert!(
            workers.lookup(&modified).is_ok(),
            "modified document closed"
        );
        assert!(workers.lookup(&recent).is_ok(), "recent document closed");
        assert!(
            workers.lookup(&clean[1]).is_err(),
            "oldest idle clean document kept"
        );
        assert_eq!(workers.registry.lock().unwrap().len(), MAX_DOCUMENTS);

        // Without an idle unmodified document the limit still applies.
        for id in &clean[2..] {
            workers
                .call(
                    id,
                    "type_text",
                    json!({"text":"x"}),
                    CancellationToken::new(),
                )
                .await
                .unwrap();
        }
        workers
            .call(
                &replacement,
                "type_text",
                json!({"text":"x"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        workers
            .call(
                &recent,
                "type_text",
                json!({"text":"x"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let (id, refused) = workers
            .create("new_document", json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert!(id.is_none());
        assert_eq!(refused["isError"], true, "{refused}");
        assert!(workers.lookup(&modified).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn save_cancelled_while_running_keeps_the_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kept.hwpx");
        std::fs::write(&path, b"original bytes").unwrap();
        let target = path.to_str().unwrap().to_owned();
        let workers = DocumentWorkers::new(None);
        let id = document(&workers).await;
        let worker = workers.lookup(&id).unwrap();
        let (entered, started) = oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let cancel = CancellationToken::new();
        let reply = enqueue(
            &worker,
            Operation::BlockThenCall {
                entered,
                release: gate,
                tool: "save_document".into(),
                args: json!({"path":target}),
            },
            cancel.clone(),
        );
        tokio::time::timeout(Duration::from_secs(10), started)
            .await
            .unwrap()
            .unwrap();
        // The save has started on the worker; the client now cancels it.
        cancel.cancel();
        release.send(()).unwrap();
        // A cancelled request gets no tool result; the transport sends no response.
        let outcome = tokio::time::timeout(Duration::from_secs(30), reply)
            .await
            .unwrap()
            .unwrap();
        assert!(outcome.is_err(), "{outcome:?}");
        assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "temp file left"
        );
        // The scope ends with the job: the next, live save does replace the file.
        let saved = workers
            .call(
                &id,
                "save_document",
                json!({"path":target}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(saved["isError"], false, "{saved}");
        assert_ne!(std::fs::read(&path).unwrap(), b"original bytes");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn documents_serialize_independently_and_close_checks_dirty_on_worker_queue() {
        let workers = DocumentWorkers::new(None);
        let first = document(&workers).await;
        let second = document(&workers).await;
        assert_ne!(first, second);
        assert_eq!(uuid::Uuid::parse_str(&first).unwrap().get_version_num(), 4);
        let (a, b) = tokio::join!(biased;
            workers.execute(&first,"text.insert",json!({"text":"첫"}),CancellationToken::new()),
            workers.call(&first,"type_text",json!({"text":"째😀"}),CancellationToken::new())
        );
        data(a.unwrap());
        data(b.unwrap());
        assert_eq!(
            data(
                workers
                    .call(&first, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            "첫째😀"
        );
        assert_eq!(
            data(
                workers
                    .call(&second, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            ""
        );
        let refused = workers
            .close(&first, false, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(refused["isError"], true);
        assert!(workers.lookup(&first).is_ok());
        assert_eq!(
            data(
                workers
                    .close(&first, true, CancellationToken::new())
                    .await
                    .unwrap()
            )["closed"],
            true
        );
        assert!(
            workers
                .call(&first, "get_text", json!({}), CancellationToken::new())
                .await
                .is_err()
        );
        data(
            workers
                .close(&second, false, CancellationToken::new())
                .await
                .unwrap(),
        );
        assert!(workers.registry.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_creation_leaves_no_handle_and_document_count_is_bounded() {
        let workers = DocumentWorkers::new(None);
        let dir = tempfile::tempdir().unwrap();
        let (id, result) = workers
            .create(
                "open_document",
                json!({"path":dir.path().join("missing.hwp")}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(id.is_none());
        assert_eq!(result["isError"], true);
        assert!(workers.registry.lock().unwrap().is_empty());
        let mut ids = Vec::new();
        for _ in 0..MAX_DOCUMENTS {
            ids.push(document(&workers).await);
        }
        let (id, result) = workers
            .create("new_document", json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert!(id.is_none());
        assert_eq!(result["isError"], true);
        assert_eq!(workers.registry.lock().unwrap().len(), MAX_DOCUMENTS);
        for id in ids {
            data(
                workers
                    .close(&id, false, CancellationToken::new())
                    .await
                    .unwrap(),
            );
        }
        assert!(workers.registry.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_and_dropped_queued_requests_do_not_mutate_or_block_other_documents() {
        let workers = Arc::new(DocumentWorkers::new(None));
        let first = document(&workers).await;
        let second = document(&workers).await;
        let (worker, release, block) = paused(&workers, &first).await;
        let explicit = CancellationToken::new();
        let sent = explicit.clone();
        let owner = Arc::clone(&workers);
        let id = first.clone();
        let cancel_task = tokio::spawn(async move {
            owner
                .call(&id, "type_text", json!({"text":"취소되면 안 들어감"}), sent)
                .await
        });
        queued(&worker, 1).await;
        let owner = Arc::clone(&workers);
        let id = first.clone();
        let parent = CancellationToken::new();
        let request = parent.clone();
        let dropped = tokio::spawn(async move {
            owner
                .call(
                    &id,
                    "type_text",
                    json!({"text":"drop되면 안 들어감"}),
                    request,
                )
                .await
        });
        queued(&worker, 2).await;
        explicit.cancel();
        assert!(cancel_task.await.unwrap().is_err());
        dropped.abort();
        assert!(dropped.await.unwrap_err().is_cancelled());
        assert!(
            !parent.is_cancelled(),
            "aborting a job must not cancel its caller token"
        );
        let mut read_replies = Vec::new();
        for _ in 2..QUEUE_CAPACITY {
            read_replies.push(enqueue(
                &worker,
                Operation::Call {
                    tool: "get_text".into(),
                    args: json!({}),
                },
                CancellationToken::new(),
            ));
        }
        assert_eq!(worker.sender.capacity(), 0);
        let full = workers
            .call(
                &first,
                "type_text",
                json!({"text":"대기열 초과"}),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(full.to_string().contains("64"));
        data(
            workers
                .execute(
                    &second,
                    "text.insert",
                    json!({"text":"다른 문서는 계속"}),
                    CancellationToken::new(),
                )
                .await
                .unwrap(),
        );
        release.send(()).unwrap();
        data(block.await.unwrap().unwrap());
        for reply in read_replies {
            assert_eq!(data(reply.await.unwrap().unwrap())["text"], "");
        }
        assert_eq!(
            data(
                workers
                    .call(&first, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            ""
        );
        assert_eq!(
            data(
                workers
                    .call(&second, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            "다른 문서는 계속"
        );
        data(
            workers
                .close(&first, false, CancellationToken::new())
                .await
                .unwrap(),
        );
        data(
            workers
                .close(&second, true, CancellationToken::new())
                .await
                .unwrap(),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_close_rejects_unfinished_jobs_after_its_queue_position() {
        let workers = DocumentWorkers::new(None);
        let id = document(&workers).await;
        let (worker, release, block) = paused(&workers, &id).await;
        let close = enqueue(
            &worker,
            Operation::Close { force: false },
            CancellationToken::new(),
        );
        let tail = enqueue(
            &worker,
            Operation::Call {
                tool: "type_text".into(),
                args: json!({"text":"닫힌 뒤 실행 금지"}),
            },
            CancellationToken::new(),
        );
        release.send(()).unwrap();
        data(block.await.unwrap().unwrap());
        assert_eq!(data(close.await.unwrap().unwrap())["closed"], true);
        assert!(tail.await.is_err());
        assert!(
            workers
                .call(&id, "get_text", json!({}), CancellationToken::new())
                .await
                .is_err()
        );
        assert!(workers.registry.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn roots_and_protected_replacement_reuse_existing_headless_contract() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let path = allowed.join("protected.hwpx");
        let outside = dir.path().join("outside.hwpx");
        let password = "private-worker-password";
        {
            let mut source = Session::new().unwrap();
            source
                .execute(Command::Insert("자체 암호 문서😀".into()))
                .unwrap();
            source.set_password(Some(password.into())).unwrap();
            source.save(&path).unwrap();
        }
        std::fs::write(&outside, "원래 파일 바이트".as_bytes()).unwrap();
        let before = std::fs::read(&outside).unwrap();
        let roots = AutomationRoots::new(Some(&allowed), Some(&allowed)).unwrap();
        let workers = DocumentWorkers::new(roots);
        let (missing, result) = workers
            .create(
                "open_document",
                json!({"path":outside}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(missing.is_none());
        assert_eq!(result["isError"], true);
        let (id, result) = workers
            .create(
                "open_protected_document",
                json!({"path":path,"password":password}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(!result.to_string().contains(password));
        data(result);
        let id = id.unwrap();
        let failed = workers
            .call(
                &id,
                "open_protected_document",
                json!({"path":path,"password":"different-private-password","force":true}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(failed["isError"], true);
        assert!(!failed.to_string().contains("different-private-password"));
        assert_eq!(
            data(
                workers
                    .call(&id, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            "자체 암호 문서😀"
        );
        let denied = workers
            .call(
                &id,
                "save_document",
                json!({"path":outside}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(denied["isError"], true);
        assert_eq!(std::fs::read(&outside).unwrap(), before);
        data(
            workers
                .call(
                    &id,
                    "save_document",
                    json!({"path":allowed.join("resaved.hwpx")}),
                    CancellationToken::new(),
                )
                .await
                .unwrap(),
        );
        assert!(hwpcraft_formats::open(&allowed.join("resaved.hwpx")).is_err());
        assert!(
            hwpcraft_formats::open_with_password(&allowed.join("resaved.hwpx"), password).is_ok()
        );
        data(
            workers
                .close(&id, false, CancellationToken::new())
                .await
                .unwrap(),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn worker_panic_invalidates_only_its_document_without_exposing_payload() {
        let workers = DocumentWorkers::new(None);
        let broken = document(&workers).await;
        let other = document(&workers).await;
        let worker = workers.lookup(&broken).unwrap();
        let panic = enqueue(&worker, Operation::Panic, CancellationToken::new());
        assert!(panic.await.is_err());
        let error = workers
            .call(&broken, "get_text", json!({}), CancellationToken::new())
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("private-test-panic-password"));
        data(
            workers
                .call(
                    &other,
                    "type_text",
                    json!({"text":"격리된 문서는 유지"}),
                    CancellationToken::new(),
                )
                .await
                .unwrap(),
        );
        assert_eq!(
            data(
                workers
                    .call(&other, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            "격리된 문서는 유지"
        );
        data(
            workers
                .close(&other, true, CancellationToken::new())
                .await
                .unwrap(),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_atomic_restore_closes_only_its_document_and_pending_jobs() {
        let workers = DocumentWorkers::new(None);
        let first = document(&workers).await;
        let other = document(&workers).await;
        let (worker, release, blocked) = paused(&workers, &first).await;
        let failure = enqueue(
            &worker,
            Operation::InvalidateRollback,
            CancellationToken::new(),
        );
        let pending = enqueue(
            &worker,
            Operation::Call {
                tool: "get_text".into(),
                args: json!({}),
            },
            CancellationToken::new(),
        );
        release.send(()).unwrap();
        data(blocked.await.unwrap().unwrap());
        assert_eq!(failure.await.unwrap().unwrap()["isError"], true);
        assert!(pending.await.is_err());
        tokio::time::timeout(Duration::from_secs(10), worker.finished.cancelled())
            .await
            .unwrap();
        assert!(workers.lookup(&first).is_err());
        data(
            workers
                .call(
                    &other,
                    "type_text",
                    json!({"text":"복원 오류와 격리된 문서"}),
                    CancellationToken::new(),
                )
                .await
                .unwrap(),
        );
        assert_eq!(
            data(
                workers
                    .call(&other, "get_text", json!({}), CancellationToken::new())
                    .await
                    .unwrap()
            )["text"],
            "복원 오류와 격리된 문서"
        );
        data(
            workers
                .close(&other, true, CancellationToken::new())
                .await
                .unwrap(),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn precancelled_creation_has_no_slot_and_requests_are_send() {
        fn require_send<T: Send>(_: T) {}
        let workers = DocumentWorkers::new(None);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (id, result) = workers
            .create("new_document", json!({}), cancel)
            .await
            .unwrap();
        assert!(id.is_none());
        assert_eq!(result["isError"], true);
        assert!(workers.registry.lock().unwrap().is_empty());
        require_send(workers.create("new_document", json!({}), CancellationToken::new()));
        require_send(workers.call("opaque", "get_text", json!({}), CancellationToken::new()));
        require_send(workers.execute(
            "opaque",
            "text.insert",
            json!({"text":"값"}),
            CancellationToken::new(),
        ));
        require_send(workers.close("opaque", false, CancellationToken::new()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unpublished_cancelled_worker_keeps_live_permit_until_safe_thread_exit() {
        let workers = DocumentWorkers::new(None);
        let (id, worker, ready) = workers.allocate().unwrap();
        ready.await.unwrap().unwrap();
        let lease = CreationLease {
            registry: &workers.registry,
            id: id.clone(),
            worker: worker.clone(),
            retained: false,
        };
        let (entered, started) = oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let reply = enqueue(
            &worker,
            Operation::Block {
                entered,
                release: gate,
            },
            CancellationToken::new(),
        );
        started.await.unwrap();
        drop(lease);
        assert!(workers.registry.lock().unwrap().is_empty());
        assert_eq!(workers.live.load(Ordering::Acquire), 1);
        assert!(!worker.finished.is_cancelled());
        release.send(()).unwrap();
        let _ = reply.await;
        tokio::time::timeout(Duration::from_secs(10), worker.finished.cancelled())
            .await
            .unwrap();
        assert_eq!(workers.live.load(Ordering::Acquire), 0);
    }
}
