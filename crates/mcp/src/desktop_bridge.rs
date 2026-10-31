//! Owner: core area. MCP access to the document open in a desktop window (C58).
//!
//! The window starts the same loopback HTTP server as `hwpcraft-cli mcp
//! --transport http` (bearer token from the settings folder) on a thread of its
//! own, and attaches its document as an external document: requests for it are
//! queued, the window is woken, and the window runs them on its own `Session`
//! between frames ([`DesktopBridge::serve`]). The window's document is listed by
//! `list_documents`; documents an MCP client creates are separate, as with the
//! CLI server. MCP cannot close the window's document.
use crate::Headless;
use crate::document_workers::{DocumentWorkers, ExternalDocument, ExternalInfo};
use crate::sdk_http::{self, AuthToken};
use crate::sdk_service::HwpService;
use anyhow::{Context, Result, anyhow, bail};
use hwpcraft_engine::Session;
use std::{
    net::SocketAddr,
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
        let document = workers.attach_external(Arc::new(wake), info(session))?;
        let shutdown = CancellationToken::new();
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        let server = {
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
                            let service = HwpService::new(workers)?;
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
        Ok(Self {
            document,
            backend: Headless::new(None)?,
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
        std::mem::swap(&mut self.backend.session, session);
        let served = self.document.serve(&mut self.backend, budget);
        std::mem::swap(&mut self.backend.session, session);
        if !self.backend.usable {
            // An atomic batch could not restore the document; the window shows
            // the error and keeps the document as it is for the user to decide.
            self.backend.usable = true;
            self.document.set_info(info(session));
            return Err(anyhow!(
                "MCP 일괄 작업을 되돌리지 못했습니다. 문서를 확인하고 필요하면 저장하지 말고 다시 여세요."
            ));
        }
        self.document.set_info(info(session));
        Ok(served)
    }

    /// Refresh the listed title and unsaved state (after the user opens, saves or
    /// edits a document in the window).
    pub fn update(&self, session: &Session) {
        self.document.set_info(info(session));
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

    #[test]
    fn only_loopback_addresses_are_accepted() -> Result<()> {
        let session = Session::new()?;
        assert!(DesktopBridge::start("0.0.0.0:0".parse()?, None, &session, || {}).is_err());
        Ok(())
    }
}
