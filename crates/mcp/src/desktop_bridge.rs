//! Owner: core area. MCP access to the document open in a desktop window (C58).
//!
//! The window starts the same loopback HTTP server as `hwpcraft-cli mcp
//! --transport http` (bearer token from the settings folder) on a thread of its
//! own, and attaches its document as an external document: requests for it are
//! queued, the window is woken, and the window runs them on its own `Session`
//! between frames ([`DesktopBridge::serve`]). The window's document is listed by
//! `list_documents`; documents an MCP client creates are separate, as with the
//! CLI server. MCP cannot close the window's document or swap another one in, and
//! when the user opens or makes another document in the window, the old
//! `document_id` ends and the new document gets one of its own.
use crate::Headless;
use crate::ai_gate::AiGate;
use crate::document_workers::{DocumentWorkers, ExternalDocument, ExternalInfo, Wake};
use crate::sdk_http::{self, AuthToken};
use crate::sdk_service::HwpService;
use anyhow::{Context, Result, anyhow, bail};
use hwpcraft_engine::Session;
use std::{
    net::SocketAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

/// Where the desktop server listens unless told otherwise (the CLI default is 7082,
/// so both can run at once).
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7083";
/// How long `end_ai_run` keeps running requests the AI sent before it ended.
const AI_DRAIN_LIMIT: Duration = Duration::from_secs(3);
/// One serving slice (and quiet wait) while draining.
const AI_DRAIN_SLICE: Duration = Duration::from_millis(20);

pub struct DesktopBridge {
    document: ExternalDocument,
    /// `Session::document_serial` of the document `document` stands for.
    serial: u64,
    workers: Arc<DocumentWorkers>,
    wake: Wake,
    /// Holds a placeholder session between `serve` calls; the window's session is
    /// swapped in while its requests run.
    backend: Headless,
    shutdown: CancellationToken,
    server: Option<JoinHandle<()>>,
    url: String,
    token_file: PathBuf,
    /// The server thread's error, if it stopped on its own.
    failure: Arc<Mutex<Option<String>>>,
    /// The AI edit panel's run, if one is open.
    ai_gate: Arc<AiGate>,
    /// The run `end_ai_run` is draining: its queued jobs still run.
    draining: Option<u64>,
}

/// An open AI run: the panel gives its CLI this address and token.
pub struct AiRun {
    /// The MCP endpoint, e.g. `http://127.0.0.1:7083/mcp`.
    pub url: String,
    /// Bearer token for this run only; it stops working at `end_ai_run`.
    pub token: String,
    /// The window document's `document_id`, the only document the run may edit.
    pub document_id: String,
}

impl DesktopBridge {
    /// Listen on `listen` (loopback only) with the bearer token in `token_file`
    /// (default: `mcp-http-token` in the settings folder, created if missing) and
    /// attach `session` as the window's document. `wake` is called from the server
    /// thread whenever a request for it arrives (request a repaint).
    pub fn start(
        listen: SocketAddr,
        token_file: Option<PathBuf>,
        session: &Session,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        if !listen.ip().is_loopback() {
            bail!("MCP 연결은 이 컴퓨터(127.0.0.1)에서만 받을 수 있습니다.");
        }
        let token_file = match token_file {
            Some(path) => path,
            None => hwpcraft_engine::config_dir()
                .map(|dir| dir.join("mcp-http-token"))
                .context("설정 폴더가 없어 MCP 토큰 파일을 둘 곳이 없습니다.")?,
        };
        let ai_gate = Arc::new(AiGate::default());
        let token: AuthToken =
            sdk_http::load_or_create_token(&token_file)?.with_ai_gate(Arc::clone(&ai_gate));
        let listener = std::net::TcpListener::bind(listen).with_context(|| {
            format!(
                "{listen}에서 MCP 연결을 받을 수 없습니다(다른 프로그램이 쓰는 중일 수 있습니다)."
            )
        })?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let workers = Arc::new(DocumentWorkers::new(None));
        let wake: Wake = Arc::new(wake);
        let document = workers.attach_external(Arc::clone(&wake), info(session), None)?;
        let shutdown = CancellationToken::new();
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        let server = {
            let server_workers = Arc::clone(&workers);
            let shutdown = shutdown.clone();
            let failure = Arc::clone(&failure);
            let ai_gate = Arc::clone(&ai_gate);
            std::thread::Builder::new()
                .name("hwpcraft-desktop-mcp".into())
                .spawn(move || {
                    let result = (|| -> Result<()> {
                        let runtime = tokio::runtime::Builder::new_multi_thread()
                            .worker_threads(2)
                            .enable_all()
                            .build()?;
                        runtime.block_on(async move {
                            let listener = tokio::net::TcpListener::from_std(listener)?;
                            let service = HwpService::new(server_workers)?.with_ai_gate(ai_gate);
                            sdk_http::serve_listener(service, listener, token, shutdown).await
                        })
                    })();
                    if let Err(error) = result
                        && let Ok(mut slot) = failure.lock()
                    {
                        *slot = Some(format!("{error:#}"));
                    }
                })
                .context("MCP 서버 스레드를 만들지 못했습니다.")?
        };
        let mut backend = Headless::new(None)?;
        backend.window = true;
        Ok(Self {
            document,
            serial: session.document_serial,
            workers,
            wake,
            backend,
            shutdown,
            server: Some(server),
            url: format!("http://{address}/mcp"),
            token_file,
            failure,
            ai_gate,
            draining: None,
        })
    }

    /// Open an AI run on the window's document: its edits until `end_ai_run`
    /// become one undo step, commands that touch files are refused, and other
    /// clients may read but not edit this document. Returns the run's address and
    /// one-time token for the panel's CLI.
    pub fn begin_ai_run(&mut self, session: &mut Session) -> Result<AiRun> {
        if self.ai_gate.active() {
            bail!("AI 편집이 이미 실행 중입니다.");
        }
        self.follow(session)?;
        session.begin_undo_group()?;
        session.set_commands_restricted(true);
        let token = self.ai_gate.begin(self.document.id());
        Ok(AiRun {
            url: self.url.clone(),
            token,
            document_id: self.document.id().to_owned(),
        })
    }

    /// Close the AI run: its token stops working first; requests it already sent
    /// still run under the run's rules and inside its group; then its edits
    /// become one undo step. Returns whether the document changed.
    pub fn end_ai_run(&mut self, session: &mut Session) -> Result<bool> {
        self.draining = self.ai_gate.end();
        // A request that passed the token check may still be queued (the CLI was
        // stopped mid-call). Run what is queued while the restriction and the
        // group still hold, until the queue stays empty for a moment.
        let deadline = std::time::Instant::now() + AI_DRAIN_LIMIT;
        let mut quiet = 0;
        let mut drained = Ok(());
        // Also until every AI request already past the token check is answered.
        while (quiet < 2 || self.ai_gate.in_flight() > 0) && std::time::Instant::now() < deadline {
            match self.serve(session, AI_DRAIN_SLICE) {
                Ok(0) => {
                    quiet += 1;
                    std::thread::sleep(AI_DRAIN_SLICE);
                }
                Ok(_) => quiet = 0,
                // A failed request (an engine panic the window caught): stop
                // draining, but still lift the restriction and close the group,
                // or the user's next edits would join the AI's undo step.
                Err(error) => {
                    drained = Err(error);
                    break;
                }
            }
        }
        // From here a job the run still has queued is refused (`serve`).
        self.draining = None;
        session.set_commands_restricted(false);
        let changed = if session.undo_group_active() {
            session.end_undo_group()?
        } else {
            false
        };
        drained.map(|()| changed)
    }

    /// End an AI run whose connection is already gone (the window lost it):
    /// lift the restriction and close the group. Returns whether the document
    /// changed.
    pub fn end_orphaned_ai_run(session: &mut Session) -> Result<bool> {
        session.set_commands_restricted(false);
        if !session.undo_group_active() {
            return Ok(false);
        }
        session.end_undo_group()
    }

    /// True between `begin_ai_run` and `end_ai_run`.
    pub fn ai_run_active(&self) -> bool {
        self.ai_gate.active()
    }

    /// The MCP endpoint, e.g. `http://127.0.0.1:7083/mcp`.
    pub fn url(&self) -> &str {
        &self.url
    }
    /// The file holding the bearer token clients must send.
    pub fn token_file(&self) -> &Path {
        &self.token_file
    }
    /// The window document's `document_id` (also returned by `list_documents`).
    pub fn document_id(&self) -> &str {
        self.document.id()
    }
    /// Why the server stopped, if it stopped on its own (for example a lost port).
    pub fn failure(&self) -> Option<String> {
        self.failure.lock().ok().and_then(|slot| slot.clone())
    }

    /// Run the queued requests on the window's `session` for up to `budget` (at
    /// least one) and refresh its listed title and state. Returns how many ran;
    /// the window redraws when this is not zero.
    pub fn serve(&mut self, session: &mut Session, budget: Duration) -> Result<usize> {
        self.follow(session)?;
        std::mem::swap(&mut self.backend.session, session);
        // This runs on the window's thread: an engine panic must not take the
        // window, and the user's unsaved document, down with it.
        let served = catch_unwind(AssertUnwindSafe(|| {
            let run = self.ai_gate.current().or(self.draining);
            self.document.serve(&mut self.backend, budget, run)
        }));
        std::mem::swap(&mut self.backend.session, session);
        let Ok(served) = served else {
            // The document may be partly changed: the user keeps it and decides.
            // Clients lose this id (queued requests fail) and find the document
            // again under a new one.
            self.backend.usable = true;
            self.reattach(session)?;
            return Err(anyhow!(
                "MCP 요청을 처리하다 내부 오류가 났습니다. 문서를 확인하세요. 이 문서의 MCP document_id가 새로 바뀌었습니다."
            ));
        };
        if !self.backend.usable {
            // An atomic batch could not restore the document; the window shows
            // the error and keeps the document as it is for the user to decide.
            // The worker marked the id closed, so clients get a new one.
            self.backend.usable = true;
            self.reattach(session)?;
            return Err(anyhow!(
                "MCP 일괄 작업을 되돌리지 못했습니다. 문서를 확인하고 필요하면 저장하지 말고 다시 여세요. 이 문서의 MCP document_id가 새로 바뀌었습니다."
            ));
        }
        self.document.set_info(info(session));
        Ok(served)
    }

    /// Refresh the listed title and unsaved state (after the user opens, saves or
    /// edits a document in the window).
    pub fn update(&mut self, session: &Session) {
        if self.follow(session).is_err() {
            return;
        }
        self.document.set_info(info(session));
    }

    /// A new `document_id` when the window now holds another document.
    fn follow(&mut self, session: &Session) -> Result<()> {
        if session.document_serial == self.serial {
            return Ok(());
        }
        self.reattach(session)
    }

    /// End the current `document_id` (its queued requests fail, subscribers hear it
    /// close) and attach `session` under a new one.
    fn reattach(&mut self, session: &Session) -> Result<()> {
        let fresh = self.workers.attach_external(
            Arc::clone(&self.wake),
            info(session),
            Some(&self.document),
        )?;
        drop(std::mem::replace(&mut self.document, fresh));
        self.serial = session.document_serial;
        Ok(())
    }
}

impl Drop for DesktopBridge {
    fn drop(&mut self) {
        self.shutdown.cancel();
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn info(session: &Session) -> ExternalInfo {
    ExternalInfo {
        title: session
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(
                || "새 문서".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            ),
        path: session.path.as_ref().map(|p| {
            std::path::absolute(p)
                .unwrap_or_else(|_| p.clone())
                .display()
                .to_string()
        }),
        dirty: session.dirty(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use hwpcraft_engine::Command;
    use serde_json::{Value, json};
    use std::io::{Read, Write};
    use std::time::Instant;

    /// One blocking MCP tools/call over HTTP; returns the tool result object.
    fn call(address: &str, token: &str, tool: &str, args: Value) -> Value {
        let body = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":tool,"arguments":args,"_meta":{
                "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientCapabilities":{}}}}))
        .unwrap();
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(20)))
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

    fn text(result: &Value) -> Value {
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn a_client_edits_the_window_document_between_frames() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let token_file = dir.path().join("token");
        let mut session = Session::new()?;
        session.execute(Command::Insert("창 문서".into()))?;
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let woken = Arc::clone(&wakes);
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(token_file.clone()),
            &session,
            move || {
                woken.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            },
        )?;
        let token = std::fs::read_to_string(&token_file)?.trim().to_owned();
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let id = bridge.document_id().to_owned();
        // The "window" thread serves while a client thread calls.
        let client = std::thread::spawn(move || {
            let inserted = call(
                &address,
                &token,
                "execute",
                json!({"document_id": id, "command": "text.insert", "params": {"text": " + MCP"}}),
            );
            let closed = call(
                &address,
                &token,
                "close_document",
                json!({"document_id": id}),
            );
            let created = call(&address, &token, "new_document", json!({}));
            (inserted, closed, created)
        });
        let started = Instant::now();
        while !client.is_finished() {
            bridge.serve(&mut session, Duration::from_millis(50))?;
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "client did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let (inserted, closed, created) = client.join().unwrap();
        assert_eq!(inserted["isError"], false, "{inserted}");
        assert_eq!(session.text(), "창 문서 + MCP");
        assert!(session.dirty());
        assert_eq!(closed["isError"], true, "{closed}");
        // An MCP-created document is separate from the window's.
        assert_eq!(created["isError"], false, "{created}");
        assert_ne!(text(&created)["document_id"], json!(bridge.document_id()));
        assert!(wakes.load(std::sync::atomic::Ordering::SeqCst) >= 2);
        assert!(bridge.failure().is_none());
        // Stopping detaches the document and frees the port.
        let url = bridge.url().to_owned();
        drop(bridge);
        let address = url.trim_start_matches("http://").trim_end_matches("/mcp");
        assert!(std::net::TcpStream::connect(address).is_err());
        Ok(())
    }

    /// Serve on the "window" thread until `client` finishes.
    fn serve_until<T>(
        bridge: &mut DesktopBridge,
        session: &mut Session,
        client: std::thread::JoinHandle<T>,
    ) -> T {
        let started = Instant::now();
        while !client.is_finished() {
            let _ = bridge.serve(session, Duration::from_millis(50));
            assert!(started.elapsed() < Duration::from_secs(30), "client stuck");
            std::thread::sleep(Duration::from_millis(5));
        }
        client.join().unwrap()
    }

    #[test]
    fn the_window_document_is_never_swapped_and_its_id_follows_the_window() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let token_file = dir.path().join("token");
        let mut session = Session::new()?;
        session.execute(Command::Insert("사용자 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(token_file.clone()),
            &session,
            || {},
        )?;
        let token = std::fs::read_to_string(&token_file)?.trim().to_owned();
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let id = bridge.document_id().to_owned();
        let other = dir.path().join("other.hwpx");
        Session::new()?.save(&other)?;
        let (a, t, i) = (address.clone(), token.clone(), id.clone());
        let client = std::thread::spawn(move || {
            [
                call(
                    &a,
                    &t,
                    "new_document",
                    json!({"document_id": i, "force": true}),
                ),
                call(
                    &a,
                    &t,
                    "open_document",
                    json!({"document_id": i, "path": other, "force": true}),
                ),
                call(
                    &a,
                    &t,
                    "execute",
                    json!({"document_id": i, "command": "file.new", "params": {"force": true}}),
                ),
            ]
        });
        for refused in serve_until(&mut bridge, &mut session, client) {
            assert_eq!(refused["isError"], true, "{refused}");
        }
        assert_eq!(session.text(), "사용자 글", "the user's document stays");

        // The user opens another document in the window: the old id ends.
        session.reset()?;
        bridge.update(&session);
        let fresh = bridge.document_id().to_owned();
        assert_ne!(fresh, id);
        let (a, t, i, f) = (address.clone(), token.clone(), id.clone(), fresh.clone());
        let client = std::thread::spawn(move || {
            (
                call(&a, &t, "get_text", json!({"document_id": i})),
                call(
                    &a,
                    &t,
                    "execute",
                    json!({"document_id": f, "command": "text.insert", "params": {"text": "새 창 문서"}}),
                ),
            )
        });
        let (old, new) = serve_until(&mut bridge, &mut session, client);
        assert_eq!(old["isError"], true, "the old id is gone: {old}");
        assert_eq!(new["isError"], false, "{new}");
        assert_eq!(session.text(), "새 창 문서");
        Ok(())
    }

    /// A PDF export of the window's document is prepared a few pages per frame and
    /// written on another thread, so the window keeps drawing; an edit made by the
    /// user while pages are prepared fails the export instead of mixing states.
    #[test]
    fn a_window_pdf_export_spans_frames() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let token_file = dir.path().join("token");
        let mut session = Session::new()?;
        // Full pages of text: preparing each takes a noticeable part of a frame.
        let line = "한글 문서를 PDF로 내보내는 동안에도 창은 계속 그려야 합니다. ".repeat(40);
        for n in 0..40 {
            session.execute(Command::Insert(format!("{n}쪽 {line}")))?;
            session.run("insert.page_break", json!({}))?;
        }
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(token_file.clone()),
            &session,
            || {},
        )?;
        let token = std::fs::read_to_string(&token_file)?.trim().to_owned();
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let id = bridge.document_id().to_owned();
        let pdf = dir.path().join("window.pdf");
        let (a, t, i, path) = (address.clone(), token.clone(), id.clone(), pdf.clone());
        let client = std::thread::spawn(move || {
            call(
                &a,
                &t,
                "save_document",
                json!({"document_id": i, "path": path}),
            )
        });
        let started = Instant::now();
        let (mut frames, mut longest) = (0, Duration::ZERO);
        while !client.is_finished() {
            let frame = Instant::now();
            if bridge.serve(&mut session, Duration::from_millis(5))? > 0 {
                frames += 1;
            }
            longest = longest.max(frame.elapsed());
            assert!(started.elapsed() < Duration::from_secs(120), "export stuck");
            std::thread::sleep(Duration::from_millis(2));
        }
        let total = started.elapsed();
        let saved = client.join().unwrap();
        assert_eq!(saved["isError"], false, "{saved}");
        assert!(frames > 1, "the export took {frames} frame(s) in {total:?}");
        // No frame holds the window for the whole export.
        assert!(
            longest * 3 < total,
            "longest frame {longest:?} of {total:?} over {frames} frames"
        );
        assert!(std::fs::read(&pdf)?.starts_with(b"%PDF-"));

        // The user types while pages are prepared: the export fails, no file.
        let other = dir.path().join("edited.pdf");
        let (a, t, i, path) = (address, token, id, other.clone());
        let client = std::thread::spawn(move || {
            call(
                &a,
                &t,
                "save_document",
                json!({"document_id": i, "path": path}),
            )
        });
        let mut typed = false;
        while !client.is_finished() {
            let served = bridge.serve(&mut session, Duration::from_millis(1))?;
            if served > 0 && !typed {
                session.execute(Command::Insert("사용자 입력".into()))?;
                typed = true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let refused = client.join().unwrap();
        assert_eq!(refused["isError"], true, "{refused}");
        assert!(!other.exists());
        Ok(())
    }

    #[test]
    fn the_window_gets_a_new_id_when_every_place_is_taken() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let token_file = dir.path().join("token");
        let mut session = Session::new()?;
        session.execute(Command::Insert("첫 문서".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(token_file.clone()),
            &session,
            || {},
        )?;
        let token = std::fs::read_to_string(&token_file)?.trim().to_owned();
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        // Clients fill every other place.
        for _ in 1..16 {
            let created = call(&address, &token, "new_document", json!({}));
            assert_eq!(created["isError"], false, "{created}");
        }
        let old = bridge.document_id().to_owned();
        // The user opens another document in the window.
        let mut other = Session::new()?;
        other.execute(Command::Insert("둘째 문서".into()))?;
        bridge.serve(&mut other, Duration::from_millis(10))?;
        assert_ne!(bridge.document_id(), old);
        let listed = call(&address, &token, "list_documents", json!({}));
        let ids: Vec<String> = text(&listed)["documents"]
            .as_array()
            .map(|d| {
                d.iter()
                    .filter_map(|x| x["document_id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        assert!(ids.iter().any(|id| id == bridge.document_id()), "{listed}");
        assert!(!ids.contains(&old), "{listed}");
        Ok(())
    }

    #[test]
    fn a_panic_writing_the_window_pdf_answers_and_keeps_the_document() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let token_file = dir.path().join("token");
        let mut session = Session::new()?;
        session.execute(Command::Insert("창 문서".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(token_file.clone()),
            &session,
            || {},
        )?;
        let token = std::fs::read_to_string(&token_file)?.trim().to_owned();
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let id = bridge.document_id().to_owned();
        let pdf = dir.path().join("panics.pdf");
        *crate::document_workers::PANIC_PDF_WRITER_AT
            .lock()
            .map_err(|_| anyhow!("hook"))? = Some(pdf.clone());
        let saved = {
            let (a, t, i, p) = (address.clone(), token.clone(), id.clone(), pdf.clone());
            std::thread::spawn(move || {
                call(
                    &a,
                    &t,
                    "save_document",
                    json!({"document_id": i, "path": p}),
                )
            })
        };
        let saved = serve_until(&mut bridge, &mut session, saved);
        *crate::document_workers::PANIC_PDF_WRITER_AT
            .lock()
            .map_err(|_| anyhow!("hook"))? = None;
        // The request is answered as a failure, without the panic's text.
        assert_eq!(saved["isError"], true, "{saved}");
        assert!(
            saved.to_string().contains("PDF를 만들다 내부 오류"),
            "{saved}"
        );
        assert!(!saved.to_string().contains("private-test"), "{saved}");
        assert!(!pdf.exists());
        // The window's document keeps its id and still answers.
        assert_eq!(bridge.document_id(), id);
        let listed = call(&address, &token, "list_documents", json!({}));
        assert!(listed.to_string().contains(&id), "{listed}");
        let read = {
            let (a, t, i) = (address.clone(), token.clone(), id.clone());
            std::thread::spawn(move || call(&a, &t, "get_text", json!({"document_id": i})))
        };
        let read = serve_until(&mut bridge, &mut session, read);
        assert_eq!(read["isError"], false, "{read}");
        Ok(())
    }

    #[test]
    fn an_engine_panic_does_not_take_the_window_down() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::new()?;
        session.execute(Command::Insert("남는 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(dir.path().join("token")),
            &session,
            || {},
        )?;
        let id = bridge.document_id().to_owned();
        bridge.workers.queue_panic(&id)?;
        assert!(
            bridge
                .serve(&mut session, Duration::from_millis(50))
                .is_err()
        );
        assert_eq!(session.text(), "남는 글", "the window keeps its document");
        assert_ne!(bridge.document_id(), id, "clients get a new id");
        assert_eq!(bridge.serve(&mut session, Duration::from_millis(10))?, 0);
        Ok(())
    }

    #[test]
    fn only_loopback_addresses_are_accepted() -> Result<()> {
        let session = Session::new()?;
        assert!(DesktopBridge::start("0.0.0.0:0".parse()?, None, &session, || {}).is_err());
        Ok(())
    }

    #[test]
    fn requests_queued_when_an_ai_run_ends_still_follow_its_rules() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::new()?;
        session.execute(Command::Insert("사용자 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(dir.path().join("token")),
            &session,
            || {},
        )?;
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let run = bridge.begin_ai_run(&mut session)?;
        let saved = dir.path().join("late.hwpx");
        // Both calls pass the token check and wait in the queue: the window does
        // not serve until the run ends (a CLI stopped mid-call).
        let save = {
            let (a, t, i, p) = (
                address.clone(),
                run.token.clone(),
                run.document_id.clone(),
                saved.clone(),
            );
            std::thread::spawn(move || {
                call(
                    &a,
                    &t,
                    "execute",
                    json!({"document_id": i, "command": "file.save_as", "params": {"path": p}}),
                )
            })
        };
        let insert = {
            let (a, t, i) = (address.clone(), run.token.clone(), run.document_id.clone());
            std::thread::spawn(move || {
                call(
                    &a,
                    &t,
                    "execute",
                    json!({"document_id": i, "command": "text.insert", "params": {"text": " 늦은 편집"}}),
                )
            })
        };
        std::thread::sleep(Duration::from_millis(500));
        assert!(bridge.end_ai_run(&mut session)?);
        let (save, insert) = (save.join().unwrap(), insert.join().unwrap());
        assert_eq!(save["isError"], true, "{save}");
        assert!(!saved.exists());
        assert_eq!(insert["isError"], false, "{insert}");
        assert_eq!(session.text(), "사용자 글 늦은 편집");
        // The late edit is inside the run's single undo step.
        session.execute(Command::Undo)?;
        assert_eq!(session.text(), "사용자 글");
        Ok(())
    }

    #[test]
    fn a_job_the_ai_queued_is_refused_once_its_run_is_over() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::new()?;
        session.execute(Command::Insert("사용자 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(dir.path().join("token")),
            &session,
            || {},
        )?;
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let run = bridge.begin_ai_run(&mut session)?;
        let insert = {
            let (a, t, i) = (address.clone(), run.token.clone(), run.document_id.clone());
            std::thread::spawn(move || {
                call(
                    &a,
                    &t,
                    "execute",
                    json!({"document_id": i, "command": "text.insert", "params": {"text": " 늦은 편집"}}),
                )
            })
        };
        std::thread::sleep(Duration::from_millis(500));
        // What `end_ai_run` does when its drain runs out of time before the
        // window reached the job (a PDF export holding the queue): the run ends
        // and its restriction is lifted with the job still queued.
        bridge.ai_gate.end();
        session.set_commands_restricted(false);
        session.end_undo_group()?;
        let insert = serve_until(&mut bridge, &mut session, insert);
        assert_eq!(insert["isError"], true, "{insert}");
        assert!(
            text(&insert)
                .to_string()
                .contains(crate::ai_gate::RUN_ENDED),
            "{insert}"
        );
        assert_eq!(session.text(), "사용자 글");
        Ok(())
    }

    #[test]
    fn an_ai_run_always_lifts_its_rules_even_when_draining_fails() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::new()?;
        session.execute(Command::Insert("사용자 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(dir.path().join("token")),
            &session,
            || {},
        )?;
        let run = bridge.begin_ai_run(&mut session)?;
        session.execute(Command::Insert(" AI".into()))?;
        // A queued request panics while the run ends.
        bridge.workers.queue_panic(&run.document_id)?;
        assert!(bridge.end_ai_run(&mut session).is_err());
        assert!(!session.undo_group_active());
        session.run("edit.undo", json!({}))?;
        assert_eq!(session.text(), "사용자 글");
        Ok(())
    }

    #[test]
    fn an_ai_run_whose_connection_is_gone_still_closes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut session = Session::new()?;
        session.execute(Command::Insert("사용자 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(dir.path().join("token")),
            &session,
            || {},
        )?;
        bridge.begin_ai_run(&mut session)?;
        session.execute(Command::Insert(" AI".into()))?;
        drop(bridge);
        assert!(DesktopBridge::end_orphaned_ai_run(&mut session)?);
        assert!(!session.undo_group_active());
        session.run("edit.undo", json!({}))?;
        assert_eq!(session.text(), "사용자 글");
        assert!(!DesktopBridge::end_orphaned_ai_run(&mut session)?);
        Ok(())
    }

    /// HTTP status of a tools/call made with `token`.
    fn status(address: &str, token: &str) -> String {
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_documents","arguments":{}}}"#;
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        write!(
            socket,
            "POST /mcp HTTP/1.1
Host: {address}
Authorization: Bearer {token}
Content-Type: application/json
Connection: close
Content-Length: {}

",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
        let mut reply = String::new();
        socket.read_to_string(&mut reply).unwrap();
        reply.lines().next().unwrap_or_default().to_owned()
    }

    #[test]
    fn an_ai_run_edits_only_the_window_document_and_undoes_in_one_step() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let token_file = dir.path().join("token");
        let mut session = Session::new()?;
        session.execute(Command::Insert("사용자 글".into()))?;
        let mut bridge = DesktopBridge::start(
            "127.0.0.1:0".parse()?,
            Some(token_file.clone()),
            &session,
            || {},
        )?;
        let user = std::fs::read_to_string(&token_file)?.trim().to_owned();
        let address = bridge
            .url()
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_owned();
        let run = bridge.begin_ai_run(&mut session)?;
        assert!(bridge.ai_run_active());
        assert!(bridge.begin_ai_run(&mut session).is_err());
        let saved = dir.path().join("saved.hwpx");
        let (a, ai, u, id, path) = (
            address.clone(),
            run.token.clone(),
            user.clone(),
            run.document_id.clone(),
            saved.clone(),
        );
        let client = std::thread::spawn(move || {
            let insert = |token: &str, text: &str| {
                call(
                    &a,
                    token,
                    "execute",
                    json!({"document_id": id, "command": "text.insert", "params": {"text": text}}),
                )
            };
            [
                insert(&ai, " 하나"),
                insert(&ai, " 둘"),
                call(
                    &a,
                    &ai,
                    "save_document",
                    json!({"document_id": id, "path": path}),
                ),
                call(
                    &a,
                    &ai,
                    "execute",
                    json!({"document_id": id, "command": "file.save_as", "params": {"path": path}}),
                ),
                call(
                    &a,
                    &ai,
                    "execute",
                    json!({"document_id": id, "command": "edit.undo", "params": {}}),
                ),
                call(&a, &ai, "new_document", json!({})),
                insert(&u, " 사용자"),
                call(&a, &u, "get_text", json!({"document_id": id})),
            ]
        });
        let [one, two, save, save_as, undo, new, user_edit, user_read] =
            serve_until(&mut bridge, &mut session, client);
        for ok in [&one, &two, &user_read] {
            assert_eq!(ok["isError"], false, "{ok}");
        }
        for refused in [&save, &save_as, &undo, &new, &user_edit] {
            assert_eq!(refused["isError"], true, "{refused}");
        }
        assert!(!saved.exists());
        assert_eq!(session.text(), "사용자 글 하나 둘");
        assert!(bridge.end_ai_run(&mut session)?);
        assert!(!bridge.ai_run_active());
        // The run's token is gone; the user's token still works.
        assert!(
            status(&address, &run.token).contains("401"),
            "{}",
            status(&address, &run.token)
        );
        assert!(
            !status(&address, &user).contains("401"),
            "{}",
            status(&address, &user)
        );
        session.execute(Command::Undo)?;
        assert_eq!(session.text(), "사용자 글");
        session.execute(Command::Redo)?;
        assert_eq!(session.text(), "사용자 글 하나 둘");
        Ok(())
    }
}
