//! Loopback-only, stateless Streamable HTTP using the official MCP SDK.
//!
//! Every request must present the server's bearer token. Loopback binding alone
//! does not stop other local processes or users, and the endpoint can open and
//! save files. Remote (non-loopback) access remains unsupported.

use anyhow::{Context, Result, bail, ensure};
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::{
    ServerHandler,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use std::{io::Write, net::SocketAddr, path::Path, sync::Arc};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

const MAX_REQUEST_BODY_BYTES: usize = 4 * 1024 * 1024;
const MIN_TOKEN_CHARS: usize = 32;

/// Shared secret every HTTP request presents as `Authorization: Bearer <token>`.
#[derive(Clone)]
pub(crate) struct AuthToken(Arc<str>);

impl AuthToken {
    pub(crate) fn new(token: &str) -> Result<Self> {
        let token = token.trim();
        ensure!(
            token.len() >= MIN_TOKEN_CHARS && token.bytes().all(|b| b.is_ascii_graphic()),
            "MCP HTTP token must be at least {MIN_TOKEN_CHARS} printable ASCII characters"
        );
        Ok(Self(token.into()))
    }

    fn accepts(&self, presented: &[u8]) -> bool {
        // Constant time for equal lengths; the length itself is not secret.
        let expected = self.0.as_bytes();
        presented.len() == expected.len()
            && presented
                .iter()
                .zip(expected)
                .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
                == 0
    }
}

/// Read the token stored at `path`, or create one there that only the current
/// user can read. The token is never printed; clients read it from the file.
pub(crate) fn load_or_create_token(path: &Path) -> Result<AuthToken> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            return AuthToken::new(&text)
                .with_context(|| format!("invalid MCP HTTP token file {}", path.display()));
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error).with_context(|| format!("cannot read {}", path.display()));
        }
        Err(_) => {}
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            AuthToken::new(&token)
        }
        // Another server created it first; use that one.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            AuthToken::new(&std::fs::read_to_string(path)?)
        }
        Err(error) => Err(error).with_context(|| format!("cannot create {}", path.display())),
    }
}

async fn require_token(
    State(token): State<AuthToken>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if presented.is_some_and(|value| token.accepts(value.as_bytes())) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
    )
        .into_response()
}

pub(crate) async fn serve<H: ServerHandler + Clone>(
    service: H,
    listen: SocketAddr,
    token: AuthToken,
    shutdown: CancellationToken,
) -> Result<()> {
    require_loopback(listen)?;
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("cannot bind MCP HTTP listener at {listen}"))?;
    serve_listener(service, listener, token, shutdown).await
}

/// Accept an already-bound listener so tests/embedders can discover an OS-assigned port.
pub(crate) async fn serve_listener<H: ServerHandler + Clone>(
    service: H,
    listener: TcpListener,
    token: AuthToken,
    shutdown: CancellationToken,
) -> Result<()> {
    serve_handler(service, listener, token, shutdown).await
}

fn require_loopback(address: SocketAddr) -> Result<()> {
    if !address.ip().is_loopback() {
        bail!(
            "MCP HTTP currently requires a loopback address; remote authentication is not configured"
        );
    }
    Ok(())
}

async fn serve_handler<H>(
    service: H,
    listener: TcpListener,
    token: AuthToken,
    shutdown: CancellationToken,
) -> Result<()>
where
    H: ServerHandler + Clone,
{
    let address = listener
        .local_addr()
        .context("cannot inspect MCP listener")?;
    require_loopback(address)?;

    // Restrict both the hostname and port. DNS-rebinding domains and authorities
    // for unrelated local services must not pass merely because we bind locally.
    let allowed_hosts = [address.to_string(), format!("localhost:{}", address.port())];
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_stateless_protocol_metadata_required(true)
        .with_json_response(true)
        .with_allowed_hosts(allowed_hosts)
        // An empty SDK allowlist otherwise disables Origin validation.
        // Native MCP clients omit Origin; all browser origins are denied here.
        .enforce_origin_validation()
        // SDK enforcement consumes a bounded streaming body; Content-Length
        // and chunked transfer cannot bypass this limit.
        .with_max_request_body_bytes(MAX_REQUEST_BODY_BYTES)
        .with_cancellation_token(shutdown.child_token());
    let transport = StreamableHttpService::new(
        move || Ok(service.clone()),
        LocalSessionManager::default().into(),
        config,
    );
    // Authentication runs before routing, so unauthenticated callers learn nothing
    // about paths, methods or protocol versions.
    let router = axum::Router::new()
        .nest_service("/mcp", transport)
        .layer(middleware::from_fn_with_state(token, require_token));
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .context("MCP HTTP server failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::{
        ErrorData, RoleServer,
        model::{
            CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
            NumberOrString, ProgressNotificationParam, ProgressToken, ProtocolVersion,
            ServerCapabilities, ServerConfig,
        },
        service::RequestContext,
    };
    use serde_json::{Value, json};
    use std::{
        borrow::Cow,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Clone)]
    struct Probe {
        disconnected: Arc<AtomicBool>,
    }

    impl ServerHandler for Probe {
        fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
            Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
        }

        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
                .with_protocol_version(ProtocolVersion::V_2026_07_28)
                .with_server_info(Implementation::new("http-test-fixture", "1"))
        }

        async fn call_tool(
            &self,
            request: CallToolRequestParams,
            context: RequestContext<RoleServer>,
        ) -> std::result::Result<CallToolResponse, ErrorData> {
            if request.name == "progress" || request.name == "wait" {
                context
                    .peer
                    .notify_progress(ProgressNotificationParam::new(
                        ProgressToken(NumberOrString::String("http-progress".into())),
                        1.0,
                    ))
                    .await
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
            }
            if request.name == "wait" {
                context.ct.cancelled().await;
                self.disconnected.store(true, Ordering::SeqCst);
            }
            Ok(CallToolResult::success(vec![ContentBlock::text("own HTTP fixture")]).into())
        }
    }

    const TOKEN: &str = "test-token-0123456789abcdef0123456789abcdef";

    struct Running {
        address: SocketAddr,
        shutdown: CancellationToken,
        task: Option<tokio::task::JoinHandle<Result<()>>>,
        disconnected: Arc<AtomicBool>,
    }

    impl Running {
        async fn start() -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let shutdown = CancellationToken::new();
            let token = shutdown.clone();
            let disconnected = Arc::new(AtomicBool::new(false));
            let task = tokio::spawn(serve_handler(
                Probe {
                    disconnected: disconnected.clone(),
                },
                listener,
                AuthToken::new(TOKEN)?,
                token,
            ));
            Ok(Self {
                address,
                shutdown,
                task: Some(task),
                disconnected,
            })
        }

        async fn finish(mut self) -> Result<()> {
            self.shutdown.cancel();
            if let Some(task) = self.task.take() {
                tokio::time::timeout(Duration::from_secs(5), task).await???;
            }
            Ok(())
        }

        async fn request(
            &self,
            method: &str,
            path: &str,
            extra: &[(&str, &str)],
            body: &[u8],
        ) -> Result<Reply> {
            let bearer = format!("Bearer {TOKEN}");
            let mut extra = extra.to_vec();
            if !extra
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
            {
                extra.push(("Authorization", &bearer));
            }
            self.request_raw(method, path, &extra, body).await
        }

        /// Sends exactly the given headers; an empty header value is omitted.
        async fn request_raw(
            &self,
            method: &str,
            path: &str,
            extra: &[(&str, &str)],
            body: &[u8],
        ) -> Result<Reply> {
            let mut socket = tokio::net::TcpStream::connect(self.address).await?;
            let authority = self.address.to_string();
            let host = extra
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("Host"))
                .map_or(authority.as_str(), |(_, value)| *value);
            let mut head = format!(
                "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n",
                body.len()
            );
            for (name, value) in extra {
                if name.eq_ignore_ascii_case("Host") || value.is_empty() {
                    continue;
                }
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            head.push_str("\r\n");
            socket.write_all(head.as_bytes()).await?;
            socket.write_all(body).await?;
            let mut bytes = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut bytes)).await??;
            Reply::parse(bytes)
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.shutdown.cancel();
        }
    }

    struct Reply {
        status: u16,
        headers: String,
        body: Vec<u8>,
    }

    impl Reply {
        fn parse(bytes: Vec<u8>) -> Result<Self> {
            let split = bytes
                .windows(4)
                .position(|part| part == b"\r\n\r\n")
                .context("HTTP response has no header boundary")?;
            let headers = String::from_utf8(bytes[..split].to_vec())?;
            let status = headers
                .split_whitespace()
                .nth(1)
                .context("HTTP status missing")?
                .parse()?;
            let body = bytes[split + 4..].to_vec();
            let body = if headers
                .to_ascii_lowercase()
                .contains("transfer-encoding: chunked")
            {
                decode_chunks(&body)?
            } else {
                body
            };
            Ok(Self {
                status,
                headers,
                body,
            })
        }

        fn json(&self) -> Result<Value> {
            Ok(serde_json::from_slice(&self.body)?)
        }
    }

    fn decode_chunks(mut bytes: &[u8]) -> Result<Vec<u8>> {
        let mut body = Vec::new();
        loop {
            let end = bytes
                .windows(2)
                .position(|part| part == b"\r\n")
                .context("chunk header missing")?;
            let header = std::str::from_utf8(&bytes[..end])?;
            let size =
                usize::from_str_radix(header.split(';').next().context("chunk size missing")?, 16)?;
            bytes = &bytes[end + 2..];
            if size == 0 {
                return Ok(body);
            }
            let chunk = bytes.get(..size).context("truncated HTTP chunk")?;
            body.extend_from_slice(chunk);
            bytes = bytes.get(size + 2..).context("chunk terminator missing")?;
        }
    }

    fn request(method: &str) -> Value {
        json!({"jsonrpc":"2.0","id":"http-own","method":method,"params":{
            "_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientCapabilities":{}}
        }})
    }

    fn headers(method: &str) -> [(&str, &str); 2] {
        [
            ("MCP-Protocol-Version", "2026-07-28"),
            ("Mcp-Method", method),
        ]
    }

    #[test]
    fn rejects_non_loopback_binding() -> Result<()> {
        for address in ["0.0.0.0:0", "[::]:0", "192.0.2.1:8000"] {
            assert!(require_loopback(address.parse()?).is_err());
        }
        for address in ["127.0.0.1:0", "127.0.0.2:8000", "[::1]:0"] {
            require_loopback(address.parse()?)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn requests_without_the_bearer_token_are_rejected_before_routing() -> Result<()> {
        let server = Running::start().await?;
        let bytes = serde_json::to_vec(&request("server/discover"))?;
        let wrong = format!("Bearer {}", TOKEN.replace('0', "1"));
        let short = format!("Bearer {}", &TOKEN[..TOKEN.len() - 1]);
        let basic = format!("Basic {TOKEN}");
        for auth in ["", wrong.as_str(), short.as_str(), basic.as_str()] {
            let mut extra = headers("server/discover").to_vec();
            extra.push(("Authorization", auth));
            for (method, path) in [("POST", "/mcp"), ("GET", "/missing")] {
                let reply = server.request_raw(method, path, &extra, &bytes).await?;
                assert_eq!(reply.status, 401, "{method} {path} with {auth:?}");
                assert!(
                    reply
                        .headers
                        .to_ascii_lowercase()
                        .contains("www-authenticate: bearer")
                );
                assert!(reply.body.is_empty());
            }
        }
        let reply = server
            .request("POST", "/mcp", &headers("server/discover"), &bytes)
            .await?;
        assert_eq!(reply.status, 200);
        server.finish().await
    }

    #[test]
    fn token_files_are_created_once_reused_and_validated() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nested").join("mcp-http-token");
        let first = load_or_create_token(&path)?;
        assert!(first.0.len() >= 64);
        let second = load_or_create_token(&path)?;
        assert_eq!(first.0, second.0);
        assert!(second.accepts(std::fs::read_to_string(&path)?.as_bytes()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path)?.permissions().mode() & 0o777,
                0o600
            );
        }
        let short = dir.path().join("short");
        std::fs::write(&short, "too-short")?;
        assert!(load_or_create_token(&short).is_err());
        assert!(AuthToken::new("contains space 0123456789abcdef0123456789").is_err());
        Ok(())
    }

    #[tokio::test]
    async fn modern_discover_is_stateless_and_shutdown_stops_listener() -> Result<()> {
        let server = Running::start().await?;
        for _ in 0..2 {
            let reply = server
                .request(
                    "POST",
                    "/mcp",
                    &headers("server/discover"),
                    &serde_json::to_vec(&request("server/discover"))?,
                )
                .await?;
            assert_eq!(reply.status, 200);
            assert!(
                reply
                    .headers
                    .to_ascii_lowercase()
                    .contains("content-type: application/json")
            );
            assert!(
                !reply
                    .headers
                    .to_ascii_lowercase()
                    .contains("mcp-session-id")
            );
            let body = reply.json()?;
            assert_eq!(body["id"], "http-own");
            assert_eq!(body["result"]["resultType"], "complete");
            assert_eq!(body["result"]["supportedVersions"], json!(["2026-07-28"]));
        }
        let address = server.address;
        server.finish().await?;
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn routing_metadata_version_and_unknown_method_errors() -> Result<()> {
        let server = Running::start().await?;
        let bytes = serde_json::to_vec(&request("server/discover"))?;
        assert_eq!(
            server.request("GET", "/missing", &[], &[]).await?.status,
            404
        );
        for method in ["GET", "DELETE"] {
            assert_eq!(server.request(method, "/mcp", &[], &[]).await?.status, 405);
        }
        assert_eq!(
            server.request("POST", "/mcp", &[], &bytes).await?.status,
            400
        );
        let mismatch = [
            ("MCP-Protocol-Version", "2026-07-28"),
            ("Mcp-Method", "tools/list"),
        ];
        let reply = server.request("POST", "/mcp", &mismatch, &bytes).await?;
        assert_eq!(reply.status, 400);
        assert_eq!(reply.json()?["error"]["code"], -32020);
        let mut old = request("server/discover");
        old["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2025-06-18");
        let reply = server
            .request(
                "POST",
                "/mcp",
                &[
                    ("MCP-Protocol-Version", "2025-06-18"),
                    ("Mcp-Method", "server/discover"),
                ],
                &serde_json::to_vec(&old)?,
            )
            .await?;
        assert_eq!(reply.status, 400);
        assert_eq!(reply.json()?["error"]["code"], -32022);
        let reply = server
            .request(
                "POST",
                "/mcp",
                &headers("own/unknown"),
                &serde_json::to_vec(&request("own/unknown"))?,
            )
            .await?;
        assert_eq!(reply.status, 404);
        assert_eq!(reply.json()?["error"]["code"], -32601);
        server.finish().await
    }

    #[tokio::test]
    async fn rejects_browser_origins_rebinding_hosts_and_oversized_body() -> Result<()> {
        let server = Running::start().await?;
        let bytes = serde_json::to_vec(&request("server/discover"))?;
        let mut origin = headers("server/discover").to_vec();
        origin.push(("Origin", "https://attacker.invalid"));
        assert_eq!(
            server
                .request("POST", "/mcp", &origin, &bytes)
                .await?
                .status,
            403
        );
        let mut host = headers("server/discover").to_vec();
        host.push(("Host", "attacker.invalid"));
        let rejected = server.request("POST", "/mcp", &host, &bytes).await?;
        assert_eq!(rejected.status, 403);
        let oversized = vec![b' '; MAX_REQUEST_BODY_BYTES + 1];
        assert_eq!(
            server
                .request("POST", "/mcp", &headers("server/discover"), &oversized)
                .await?
                .status,
            413
        );
        server.finish().await
    }

    #[tokio::test]
    async fn notifications_have_no_body_and_progress_switches_to_sse() -> Result<()> {
        let server = Running::start().await?;
        let mut notification = request("notifications/cancelled");
        notification
            .as_object_mut()
            .context("fixture request object")?
            .remove("id");
        notification["params"]["requestId"] = json!("not-active");
        let reply = server
            .request(
                "POST",
                "/mcp",
                &headers("notifications/cancelled"),
                &serde_json::to_vec(&notification)?,
            )
            .await?;
        assert_eq!(reply.status, 202);
        assert!(reply.body.is_empty());
        let mut call = request("tools/call");
        call["params"]["name"] = json!("progress");
        call["params"]["arguments"] = json!({});
        call["params"]["_meta"]["progressToken"] = json!("http-progress");
        let mut extra = headers("tools/call").to_vec();
        extra.push(("Mcp-Name", "progress"));
        let reply = server
            .request("POST", "/mcp", &extra, &serde_json::to_vec(&call)?)
            .await?;
        assert_eq!(reply.status, 200);
        assert!(
            reply
                .headers
                .to_ascii_lowercase()
                .contains("content-type: text/event-stream")
        );
        let text = String::from_utf8(reply.body)?;
        let messages = text
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert!(
            messages
                .iter()
                .any(|value| value["method"] == "notifications/progress")
        );
        assert!(
            messages
                .iter()
                .any(|value| value["id"] == "http-own"
                    && value["result"]["resultType"] == "complete")
        );
        server.finish().await
    }

    #[tokio::test]
    async fn disconnecting_sse_cancels_the_own_inflight_handler() -> Result<()> {
        let server = Running::start().await?;
        let mut call = request("tools/call");
        call["params"]["name"] = json!("wait");
        call["params"]["arguments"] = json!({});
        call["params"]["_meta"]["progressToken"] = json!("http-progress");
        let body = serde_json::to_vec(&call)?;
        let mut socket = tokio::net::TcpStream::connect(server.address).await?;
        let head = format!(
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {TOKEN}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\nMcp-Name: wait\r\nContent-Length: {}\r\n\r\n",
            server.address,
            body.len()
        );
        socket.write_all(head.as_bytes()).await?;
        socket.write_all(&body).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut received = Vec::new();
            loop {
                let mut block = [0_u8; 2048];
                let read = socket.read(&mut block).await?;
                if read == 0 {
                    bail!("SSE closed before the handler progress checkpoint");
                }
                received.extend_from_slice(&block[..read]);
                if String::from_utf8_lossy(&received).contains("notifications/progress") {
                    break;
                }
                if received.len() > 16 * 1024 {
                    bail!("unexpected oversized SSE checkpoint");
                }
            }
            Result::<()>::Ok(())
        })
        .await??;
        assert!(!server.disconnected.load(Ordering::SeqCst));
        drop(socket);
        tokio::time::timeout(Duration::from_secs(5), async {
            while !server.disconnected.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        server.finish().await
    }
}
