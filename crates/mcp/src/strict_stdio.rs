//! Bounded JSONL framing; SDK message types and request routing stay unchanged.
//! A dedicated writer completes every accepted line even if its caller cancels.

use rmcp::{
    ErrorData, RoleServer,
    model::{
        ClientJsonRpcMessage, ClientRequest, GetMeta, ProtocolVersion, RequestId,
        ServerJsonRpcMessage,
    },
    transport::Transport,
};
use serde_json::Value;
use std::io::{self, Write};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;

const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const WRITE_QUEUE_CAPACITY: usize = 16;

#[derive(Debug)]
enum WriteJob {
    Line(Vec<u8>, oneshot::Sender<io::Result<()>>),
    Close(oneshot::Sender<io::Result<()>>),
}

enum Pending {
    Unqueued(Vec<u8>),
    Queued(oneshot::Receiver<io::Result<()>>),
}

pub(crate) struct StrictStdio<R> {
    subscription_shutdown: CancellationToken,
    read: BufReader<R>,
    line: Vec<u8>,
    discarding: bool,
    eof: bool,
    writer: mpsc::Sender<WriteJob>,
    pending: Option<Pending>,
    close_ack: Option<oneshot::Receiver<io::Result<()>>>,
    closed: bool,
}

#[cfg(test)]
pub(crate) fn new<R, W>(read: R, write: W) -> StrictStdio<R>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    new_with_subscription_shutdown(read, write, CancellationToken::new())
}

pub(crate) fn new_with_subscription_shutdown<R, W>(
    read: R,
    write: W,
    subscription_shutdown: CancellationToken,
) -> StrictStdio<R>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let (writer, receiver) = mpsc::channel(WRITE_QUEUE_CAPACITY);
    tokio::spawn(write_actor(write, receiver));
    StrictStdio {
        subscription_shutdown,
        read: BufReader::new(read),
        line: Vec::new(),
        discarding: false,
        eof: false,
        writer,
        pending: None,
        close_ack: None,
        closed: false,
    }
}

impl<R> Drop for StrictStdio<R> {
    fn drop(&mut self) {
        self.subscription_shutdown.cancel();
    }
}

fn broken_pipe() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "MCP output is closed")
}

// The SDK's bootstrap loop accepts legacy ping before selecting a lifecycle.
// This server deliberately supports only stateless, inline-metadata requests.
// Validate that boundary before the SDK can take its legacy bootstrap branch.
fn modern_request_error(message: &ClientJsonRpcMessage) -> Option<ErrorData> {
    let ClientJsonRpcMessage::Request(request) = message else {
        return None;
    };
    let version = ProtocolVersion::V_2026_07_28;
    let meta = request.request.get_meta();
    let missing = meta.missing_required_keys(&version);
    if !missing.is_empty() {
        return Some(ErrorData::invalid_params(
            format!(
                "request _meta is missing or has malformed required fields: {}; supported protocol version: {}",
                missing.join(", "),
                version.as_str()
            ),
            None,
        ));
    }
    if let Some(requested) = meta.protocol_version()
        && requested != version
    {
        return Some(ErrorData::unsupported_protocol_version(
            requested,
            &[version],
        ));
    }
    if matches!(
        request.request,
        ClientRequest::PingRequest(_) | ClientRequest::InitializeRequest(_)
    ) {
        return Some(ErrorData::new(
            rmcp::model::ErrorCode(-32601),
            "legacy lifecycle methods are not provided; supported protocol version: 2026-07-28; use server/discover with per-request _meta",
            None,
        ));
    }
    None
}

async fn write_actor<W: AsyncWrite + Unpin>(mut write: W, mut jobs: mpsc::Receiver<WriteJob>) {
    while let Some(job) = jobs.recv().await {
        match job {
            WriteJob::Line(bytes, ack) => {
                let result = async {
                    write.write_all(&bytes).await?;
                    write.flush().await
                }
                .await;
                let failed = result.is_err();
                let _ = ack.send(result);
                if failed {
                    break;
                }
            }
            WriteJob::Close(ack) => {
                let result = async {
                    write.flush().await?;
                    write.shutdown().await
                }
                .await;
                let _ = ack.send(result);
                return;
            }
        }
    }
    let _ = write.flush().await;
    let _ = write.shutdown().await;
}

struct BoundedOutput(Vec<u8>);

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size >= MAX_RESPONSE_BYTES)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "MCP response exceeds 32 MiB",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode(message: &ServerJsonRpcMessage) -> io::Result<Vec<u8>> {
    let mut bytes = BoundedOutput(Vec::new());
    serde_json::to_writer(&mut bytes, message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.0.push(b'\n');
    Ok(bytes.0)
}

enum Frame {
    Line,
    Oversized,
    Unterminated,
    End,
}

impl<R: AsyncRead + Send + Unpin + 'static> StrictStdio<R> {
    async fn frame(&mut self) -> io::Result<Frame> {
        loop {
            let available = self.read.fill_buf().await?;
            if available.is_empty() {
                self.eof = true;
                self.subscription_shutdown.cancel();
                return Ok(if !self.line.is_empty() && !self.discarding {
                    self.line.clear();
                    Frame::Unterminated
                } else {
                    Frame::End
                });
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let content = newline.unwrap_or(available.len());
            let consumed = content + usize::from(newline.is_some());
            if self.discarding {
                self.read.consume(consumed);
                if newline.is_some() {
                    self.discarding = false;
                }
                continue;
            }
            if self.line.len().saturating_add(content) > MAX_LINE_BYTES {
                self.line.clear();
                self.discarding = newline.is_none();
                self.read.consume(consumed);
                return Ok(Frame::Oversized);
            }
            // No await separates the bounded append and input consumption.
            // Cancelling fill_buf consumes nothing; partial state lives in self.
            self.line.extend_from_slice(&available[..content]);
            self.read.consume(consumed);
            if newline.is_some() {
                return Ok(Frame::Line);
            }
        }
    }

    async fn finish_pending(&mut self) -> io::Result<()> {
        if matches!(self.pending, Some(Pending::Unqueued(_))) {
            let sender = self.writer.clone();
            let permit = sender.reserve().await.map_err(|_| broken_pipe())?;
            // Reserve first, then move the error event. A cancelled reservation
            // leaves the complete event in self, not in a dropped send future.
            if let Some(Pending::Unqueued(bytes)) = self.pending.take() {
                let (ack, received) = oneshot::channel();
                permit.send(WriteJob::Line(bytes, ack));
                self.pending = Some(Pending::Queued(received));
            }
        }
        if let Some(Pending::Queued(ack)) = self.pending.as_mut() {
            let result = ack.await.map_err(|_| broken_pipe())?;
            self.pending = None;
            result?;
        }
        Ok(())
    }

    fn error(&mut self, error: ErrorData, id: Option<RequestId>) -> io::Result<()> {
        self.pending = Some(Pending::Unqueued(encode(&ServerJsonRpcMessage::error(
            error, id,
        ))?));
        Ok(())
    }
}

impl<R: AsyncRead + Send + Unpin + 'static> Transport<RoleServer> for StrictStdio<R> {
    type Error = io::Error;

    fn send(
        &mut self,
        message: ServerJsonRpcMessage,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let bytes = encode(&message);
        let writer = self.writer.clone();
        async move {
            let (ack, received) = oneshot::channel();
            writer
                .send(WriteJob::Line(bytes?, ack))
                .await
                .map_err(|_| broken_pipe())?;
            // The writer actor owns the entire accepted line independently of
            // this acknowledgement future's lifetime.
            received.await.map_err(|_| broken_pipe())?
        }
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            if self.closed || self.finish_pending().await.is_err() {
                self.closed = true;
                self.subscription_shutdown.cancel();
                return None;
            }
            if self.eof {
                return None;
            }
            let frame = match self.frame().await {
                Ok(frame) => frame,
                Err(_) => {
                    self.eof = true;
                    self.subscription_shutdown.cancel();
                    return None;
                }
            };
            let result = match frame {
                Frame::End => return None,
                Frame::Oversized => self.error(
                    ErrorData::invalid_request("MCP line exceeds 4 MiB", None),
                    None,
                ),
                Frame::Unterminated => self.error(
                    ErrorData::parse_error("Unterminated JSON-RPC line", None),
                    None,
                ),
                Frame::Line => {
                    let mut bytes = std::mem::take(&mut self.line);
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    if bytes.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    match serde_json::from_slice::<Value>(&bytes) {
                        Err(_) => self.error(ErrorData::parse_error("Parse error", None), None),
                        Ok(value) => {
                            let id = value.get("id").and_then(|id| {
                                serde_json::from_value::<RequestId>(id.clone()).ok()
                            });
                            match serde_json::from_value::<ClientJsonRpcMessage>(value) {
                                Ok(message) => match modern_request_error(&message) {
                                    Some(error) => self.error(error, id),
                                    None => return Some(message),
                                },
                                Err(_) => self
                                    .error(ErrorData::invalid_request("Invalid request", None), id),
                            }
                        }
                    }
                }
            };
            if result.is_err() {
                self.closed = true;
                return None;
            }
        }
    }

    async fn close(&mut self) -> io::Result<()> {
        self.subscription_shutdown.cancel();
        if self.closed {
            return Ok(());
        }
        self.finish_pending().await?;
        if self.close_ack.is_none() {
            let sender = self.writer.clone();
            let permit = sender.reserve().await.map_err(|_| broken_pipe())?;
            let (ack, received) = oneshot::channel();
            permit.send(WriteJob::Close(ack));
            self.close_ack = Some(received);
        }
        let result = match self.close_ack.as_mut() {
            Some(ack) => ack.await.map_err(|_| broken_pipe())?,
            None => return Err(broken_pipe()),
        };
        self.closed = true;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context as _;
    use rmcp::{
        ServerHandler, ServiceExt,
        model::{ProtocolVersion, ServerCapabilities, ServerConfig},
    };
    use serde_json::json;
    use std::{
        borrow::Cow,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll},
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Clone, Default)]
    struct Capture {
        bytes: Arc<Mutex<Vec<u8>>>,
        shutdown: Arc<AtomicBool>,
    }

    impl Capture {
        fn bytes(&self) -> io::Result<Vec<u8>> {
            self.bytes
                .lock()
                .map(|bytes| bytes.clone())
                .map_err(|_| io::Error::other("capture lock poisoned"))
        }
        fn messages(&self) -> anyhow::Result<Vec<Value>> {
            self.bytes()?
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(serde_json::from_slice::<Value>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        }
    }

    impl AsyncWrite for Capture {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            let result = self
                .bytes
                .lock()
                .map_err(|_| io::Error::other("capture lock poisoned"))
                .map(|mut output| {
                    output.extend_from_slice(bytes);
                    bytes.len()
                });
            Poll::Ready(result)
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.shutdown.store(true, Ordering::SeqCst);
            Poll::Ready(Ok(()))
        }
    }

    fn valid(id: Value) -> anyhow::Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":id,"method":"tools/list","params":{
                "_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}
            }}),
        )?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    fn error(code: i32) -> ServerJsonRpcMessage {
        ServerJsonRpcMessage::error(
            ErrorData::new(rmcp::model::ErrorCode(code), "own fixture", None),
            None,
        )
    }

    fn id(message: ClientJsonRpcMessage) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(message)?["id"].clone())
    }

    #[tokio::test]
    async fn malformed_lines_recover_with_modern_error_ids_and_notifications_stay_silent()
    -> anyhow::Result<()> {
        let mut bytes = b"\n \t\r\n{not-json}\n{\"jsonrpc\":\"2.0\",\"method\":3,\"id\":17}\n{\"jsonrpc\":\"2.0\",\"method\":3,\"id\":null}\n".to_vec();
        bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":9}}\r\n");
        bytes.extend(valid(json!("한글😀"))?);
        let output = Capture::default();
        let mut transport = new(io::Cursor::new(bytes), output.clone());
        let notification = transport.receive().await.context("notification lost")?;
        assert!(serde_json::to_value(notification)?.get("id").is_none());
        assert_eq!(
            id(transport.receive().await.context("request lost")?)?,
            "한글😀"
        );
        assert!(transport.receive().await.is_none());
        transport.close().await?;
        let errors = output.messages()?;
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0]["error"]["code"], -32700);
        assert!(errors[0].get("id").is_none());
        assert_eq!(errors[1]["error"]["code"], -32600);
        assert_eq!(errors[1]["id"], 17);
        assert!(errors[2].get("id").is_none());
        assert!(output.shutdown.load(Ordering::SeqCst));
        Ok(())
    }

    #[tokio::test]
    async fn invalid_utf8_and_unterminated_eof_are_parse_errors_without_id() -> anyhow::Result<()> {
        let output = Capture::default();
        let mut transport = new(io::Cursor::new(b"\xff\n{\"id\":1".to_vec()), output.clone());
        assert!(transport.receive().await.is_none());
        transport.close().await?;
        let errors = output.messages()?;
        assert_eq!(errors.len(), 2);
        for error in errors {
            assert_eq!(error["error"]["code"], -32700);
            assert!(error.get("id").is_none());
        }
        Ok(())
    }

    #[tokio::test]
    async fn partial_read_survives_receive_future_cancellation() -> anyhow::Result<()> {
        let (mut peer, read) = tokio::io::duplex(4096);
        let output = Capture::default();
        let mut transport = new(read, output.clone());
        let bytes = valid(json!("resumed"))?;
        peer.write_all(&bytes[..13]).await?;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), transport.receive())
                .await
                .is_err()
        );
        assert_eq!(transport.line, bytes[..13]);
        peer.write_all(&bytes[13..]).await?;
        assert_eq!(
            id(transport.receive().await.context("partial request lost")?)?,
            "resumed"
        );
        transport.close().await?;
        assert!(output.messages()?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn overflow_discard_survives_cancellation_and_recovers_at_one_newline()
    -> anyhow::Result<()> {
        let (mut peer, read) = tokio::io::duplex(8192);
        let producer = tokio::spawn(async move {
            peer.write_all(&vec![b'x'; MAX_LINE_BYTES + 17]).await?;
            io::Result::Ok(peer)
        });
        let output = Capture::default();
        let mut transport = new(read, output.clone());
        assert!(
            tokio::time::timeout(Duration::from_millis(300), transport.receive())
                .await
                .is_err()
        );
        let mut peer = producer.await??;
        assert!(transport.discarding);
        assert!(transport.line.is_empty());
        peer.write_all(b"discarded continuation\n").await?;
        peer.write_all(&valid(json!(91))?).await?;
        assert_eq!(
            id(transport
                .receive()
                .await
                .context("overflow recovery lost")?)?,
            91
        );
        transport.close().await?;
        let errors = output.messages()?;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["error"]["code"], -32600);
        Ok(())
    }

    #[tokio::test]
    async fn queued_parse_error_survives_receive_cancellation_exactly_once() -> anyhow::Result<()> {
        let (write, output) = tokio::io::duplex(8);
        let mut input = b"{bad}\n".to_vec();
        input.extend(valid(json!(23))?);
        let mut transport = new(io::Cursor::new(input), write);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), transport.receive())
                .await
                .is_err()
        );
        assert!(matches!(transport.pending, Some(Pending::Queued(_))));
        let mut output = BufReader::new(output);
        let mut line = Vec::new();
        let (request, read) =
            tokio::join!(transport.receive(), output.read_until(b'\n', &mut line));
        read?;
        assert_eq!(
            id(request.context("request after cancelled ack lost")?)?,
            23
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&line)?["error"]["code"],
            -32700
        );
        transport.close().await?;
        let mut tail = Vec::new();
        output.read_to_end(&mut tail).await?;
        assert!(tail.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn unqueued_parse_error_survives_full_queue_reservation_cancellation()
    -> anyhow::Result<()> {
        let (write, mut output) = tokio::io::duplex(8);
        let mut input = b"{bad}\n".to_vec();
        input.extend(valid(json!(24))?);
        let mut transport = new(io::Cursor::new(input), write);
        let (ack, _) = oneshot::channel();
        transport
            .writer
            .try_send(WriteJob::Line(encode(&error(-32603))?, ack))?;
        let first_byte = output.read_u8().await?;
        for _ in 0..WRITE_QUEUE_CAPACITY {
            let (ack, _) = oneshot::channel();
            transport
                .writer
                .try_send(WriteJob::Line(encode(&error(-32603))?, ack))?;
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), transport.receive())
                .await
                .is_err()
        );
        assert!(matches!(transport.pending, Some(Pending::Unqueued(_))));
        let mut output = BufReader::new(output);
        let drain = async {
            let mut messages = Vec::new();
            for index in 0..WRITE_QUEUE_CAPACITY + 2 {
                let mut line = if index == 0 {
                    vec![first_byte]
                } else {
                    Vec::new()
                };
                output.read_until(b'\n', &mut line).await?;
                messages.push(serde_json::from_slice::<Value>(&line)?);
            }
            Ok::<_, anyhow::Error>(messages)
        };
        let (request, messages) = tokio::join!(transport.receive(), drain);
        assert_eq!(
            id(request.context("request after cancelled reservation lost")?)?,
            24
        );
        assert_eq!(
            messages?
                .iter()
                .filter(|message| message["error"]["code"] == -32700)
                .count(),
            1
        );
        transport.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_send_finishes_its_line_before_the_next_writer_job() -> anyhow::Result<()> {
        let (write, mut output) = tokio::io::duplex(8);
        let mut transport = new(io::Cursor::new(Vec::<u8>::new()), write);
        let first = tokio::spawn(transport.send(error(-32700)));
        let byte = output.read_u8().await?;
        first.abort();
        let _ = first.await;
        let second = tokio::spawn(transport.send(error(-32600)));
        let mut output = BufReader::new(output);
        let mut line = vec![byte];
        output.read_until(b'\n', &mut line).await?;
        assert_eq!(
            serde_json::from_slice::<Value>(&line)?["error"]["code"],
            -32700
        );
        line.clear();
        output.read_until(b'\n', &mut line).await?;
        assert_eq!(
            serde_json::from_slice::<Value>(&line)?["error"]["code"],
            -32600
        );
        second.await??;
        transport.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn oversized_response_is_rejected_before_any_output_and_next_send_works()
    -> anyhow::Result<()> {
        let output = Capture::default();
        let mut transport = new(io::Cursor::new(Vec::<u8>::new()), output.clone());
        let huge = ServerJsonRpcMessage::error(
            ErrorData::new(
                rmcp::model::ErrorCode(-32603),
                "x".repeat(MAX_RESPONSE_BYTES),
                None,
            ),
            None,
        );
        assert_eq!(
            transport
                .send(huge)
                .await
                .err()
                .context("huge response accepted")?
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(output.bytes()?.is_empty());
        transport.send(error(-32600)).await?;
        transport.close().await?;
        assert_eq!(output.messages()?.len(), 1);
        Ok(())
    }

    #[derive(Clone)]
    struct Probe;
    impl ServerHandler for Probe {
        fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
            Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
        }
        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
                .with_protocol_version(ProtocolVersion::V_2026_07_28)
        }
    }

    #[tokio::test]
    async fn sdk_wire_survives_invalid_json_before_and_between_normal_requests()
    -> anyhow::Result<()> {
        let (client, server) = tokio::io::duplex(8192);
        let (read, write) = tokio::io::split(server);
        let server = tokio::spawn(async move {
            let running = Probe.serve(new(read, write)).await?;
            running.waiting().await?;
            Ok::<(), anyhow::Error>(())
        });
        let (read, mut write) = tokio::io::split(client);
        let mut read = BufReader::new(read);
        for id in [1, 2] {
            write.write_all(b"{not-json}\n").await?;
            write.write_all(&valid(json!(id))?).await?;
            for expected_error in [true, false] {
                let mut line = Vec::new();
                tokio::time::timeout(Duration::from_secs(5), read.read_until(b'\n', &mut line))
                    .await??;
                let message: Value = serde_json::from_slice(&line)?;
                if expected_error {
                    assert_eq!(message["error"]["code"], -32700);
                    assert!(message.get("id").is_none());
                } else {
                    assert_eq!(message["id"], id);
                    assert_eq!(message["result"]["resultType"], "complete");
                }
            }
        }
        write.shutdown().await?;
        tokio::time::timeout(Duration::from_secs(5), server).await???;
        Ok(())
    }
}
