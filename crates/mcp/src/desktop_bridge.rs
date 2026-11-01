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
        let token: AuthToken = sdk_http::load_or_create_token(&token_file)?;
        let listener = std::net::TcpListener::bind(listen).with_context(|| {
            format!(
                "{listen}에서 MCP 연결을 받을 수 없습니다(다른 프로그램이 쓰는 중일 수 있습니다)."
            )
        })?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let workers = Arc::new(DocumentWorkers::new(None));
        let wake: Wake = Arc::new(wake);
        let document = workers.attach_external(Arc::clone(&wake), info(session))?;
        let shutdown = CancellationToken::new();
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        let server = {
            let server_workers = Arc::clone(&workers);
            let shutdown = shutdown.clone();
            let failure = Arc::clone(&failure);
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
                            let service = HwpService::new(server_workers)?;
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
        })
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
            self.document.serve(&mut self.backend, budget)
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
        let fresh = self
            .workers
            .attach_external(Arc::clone(&self.wake), info(session))?;
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
}
