//! URI-only document invalidations for MCP 2026 subscriptions/listen.
//!
//! The SDK owns acknowledgment, request-ID correlation and transport cancellation.
//! A listen future owns its receiver and permit; no detached notification task is
//! retained. One initial invalidation resynchronizes an accepted URI after the SDK
//! acknowledgment (which precedes this callback). Subsequent invalidations are
//! coalesced changes, not an ordered editing log. Closed IDs leave no tombstones.
use anyhow::{Result, bail};
use hwpcraft_engine::{Cursor, Session};
use rmcp::{
    ErrorData,
    model::{ErrorCode, SubscriptionFilter},
    service::SubscriptionContext,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, watch};
use tokio_util::sync::CancellationToken;

const PREFIX: &str = "hwpcraft://documents/";
const MAX_DOCUMENTS: usize = 16;
const MAX_SUBSCRIPTIONS: usize = 64;
/// Application error code outside the JSON-RPC reserved range (-32768..=-32000).
pub(crate) const SUBSCRIPTION_CAPACITY_REACHED: i32 = -31503;
type Revision = Arc<()>;
type Documents = BTreeMap<String, Revision>;

/// Cheap resource invalidation stamp. document.inspect includes selection and
/// history flags as well as document content, dirty state and the saved path.
/// Capturing this stamp does not lay out, serialize or clone the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentStamp {
    generation: u64,
    dirty: bool,
    path: Option<PathBuf>,
    cursor: Cursor,
    anchor: Option<usize>,
    span_anchor: Option<Cursor>,
    can_undo: bool,
    can_redo: bool,
}

impl DocumentStamp {
    pub(crate) fn capture(session: &Session) -> Self {
        Self {
            generation: session.generation,
            dirty: session.dirty(),
            path: session.path.clone(),
            cursor: session.cursor,
            anchor: session.anchor,
            span_anchor: session.span_anchor,
            can_undo: session.can_undo(),
            can_redo: session.can_redo(),
        }
    }
}

pub(crate) struct DocumentSubscriptions {
    documents: Mutex<Documents>,
    changed: watch::Sender<()>,
    permits: Arc<Semaphore>,
}

impl Default for DocumentSubscriptions {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentSubscriptions {
    pub(crate) fn new() -> Self {
        let (changed, _) = watch::channel(());
        Self {
            documents: Mutex::new(BTreeMap::new()),
            changed,
            permits: Arc::new(Semaphore::new(MAX_SUBSCRIPTIONS)),
        }
    }

    /// Called only when creation has succeeded and a handle will be published.
    pub(crate) fn register(&self, id: &str) -> Result<()> {
        if document_id(&format!("{PREFIX}{id}")) != Some(id) {
            bail!("document subscription handle is not a canonical UUID");
        }
        let mut documents = self
            .documents
            .lock()
            .map_err(|_| anyhow::anyhow!("document subscription registry unavailable"))?;
        if documents.contains_key(id) {
            bail!("document subscription handle already exists");
        }
        if documents.len() >= MAX_DOCUMENTS {
            bail!("document subscription registry is full (16 documents)");
        }
        documents.insert(id.to_owned(), Arc::new(()));
        // Registering a different handle is not a change to an existing URI.
        Ok(())
    }

    /// Call inside the document worker after a command, including a cancelled
    /// synchronous command that actually changed its state. Unknown/unpublished
    /// handles deliberately do not create registry entries.
    pub(crate) fn changed(&self, id: &str) {
        let changed = self.documents.lock().ok().is_some_and(|mut documents| {
            if let Some(revision) = documents.get_mut(id) {
                *revision = Arc::new(());
                true
            } else {
                false
            }
        });
        if changed {
            self.changed.send_replace(());
        }
    }

    /// Called on actual worker exit/close, including panic teardown. Active
    /// listeners observe disappearance, emit a final URI invalidation and drop
    /// that URI. A multi-URI listen continues for its other live documents.
    pub(crate) fn closed(&self, id: &str) {
        let changed = self
            .documents
            .lock()
            .ok()
            .is_some_and(|mut documents| documents.remove(id).is_some());
        if changed {
            self.changed.send_replace(());
        }
    }

    pub(crate) fn close_all(&self) {
        let changed = self.documents.lock().ok().is_some_and(|mut documents| {
            let changed = !documents.is_empty();
            documents.clear();
            changed
        });
        if changed {
            self.changed.send_replace(());
        }
    }

    /// Accept at most 16 explicit, live document URIs. Neither static resources
    /// nor list-change categories are accepted. Unknown IDs are never enumerated.
    /// The SDK additionally intersects this with requested/advertised filters.
    pub(crate) fn accepted_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let mut accepted = BTreeSet::new();
        if let Ok(documents) = self.documents.lock()
            && let Some(uris) = &requested.resource_subscriptions
        {
            for uri in uris {
                if document_id(uri).is_some_and(|id| documents.contains_key(id)) {
                    accepted.insert(uri.clone());
                    if accepted.len() == MAX_DOCUMENTS {
                        break;
                    }
                }
            }
        }
        let mut filter = SubscriptionFilter::new();
        if !accepted.is_empty() {
            filter.resource_subscriptions = Some(accepted.into_iter().collect());
        }
        Some(filter)
    }

    fn snapshot(&self) -> std::result::Result<Documents, ErrorData> {
        self.documents
            .lock()
            .map(|documents| documents.clone())
            .map_err(|_| {
                ErrorData::internal_error("document subscription registry unavailable", None)
            })
    }

    /// SDK invokes this only after the acknowledgment is sent. The first update
    /// for each accepted URI is an explicit initial resync, not proof of an edit.
    /// Cancellation is checked both at loop boundaries and while awaiting sends.
    pub(crate) async fn listen(
        &self,
        context: SubscriptionContext,
    ) -> std::result::Result<(), ErrorData> {
        self.listen_until(context, CancellationToken::new()).await
    }

    /// A stdio EOF must stop only this peer's subscriptions before the SDK's
    /// finite-response drain. Ordinary tool responses retain that drain. HTTP
    /// cancellation is supplied by its request context instead.
    pub(crate) async fn listen_until(
        &self,
        context: SubscriptionContext,
        transport_end: CancellationToken,
    ) -> std::result::Result<(), ErrorData> {
        if transport_end.is_cancelled() || context.request_context().ct.is_cancelled() {
            return Ok(());
        }
        let Some(accepted) = context.accepted().resource_subscriptions.as_ref() else {
            return Ok(());
        };
        if accepted.is_empty() {
            return Ok(());
        }
        let _permit = self.permits.clone().try_acquire_owned().map_err(|_| {
            ErrorData::new(
                ErrorCode(SUBSCRIPTION_CAPACITY_REACHED),
                "document subscription capacity reached (64)",
                None,
            )
        })?;
        let cancel = context.request_context().ct.clone();
        let sink = context.sink();
        // Subscribe before the baseline snapshot so any later change remains
        // observable even while an initial resync is being written.
        let mut signal = self.changed.subscribe();
        let current = self.snapshot()?;
        let mut observed: BTreeMap<String, Option<Revision>> = accepted
            .iter()
            // SDK intersection can retain duplicate requested entries. The
            // accepted filter contains at most 16 distinct URIs; deduplicate
            // the entire accepted vector rather than its first 16 entries.
            .map(|uri| {
                let revision = document_id(uri).and_then(|id| current.get(id)).cloned();
                (uri.clone(), revision)
            })
            .collect();
        for uri in observed.keys() {
            if cancel.is_cancelled() || transport_end.is_cancelled() {
                return Ok(());
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                _ = transport_end.cancelled() => return Ok(()),
                sent = sink.notify_resource_updated(uri.clone()) => {
                    if sent.is_err() { return Ok(()); }
                }
            }
        }
        // A document may have closed after filter acceptance but before listen.
        observed.retain(|_, revision| revision.is_some());
        while !observed.is_empty() {
            if cancel.is_cancelled() || transport_end.is_cancelled() {
                return Ok(());
            }
            let current = self.snapshot()?;
            let mut closed = Vec::new();
            for (uri, previous) in &mut observed {
                let next = document_id(uri).and_then(|id| current.get(id));
                let same = match (previous.as_ref(), next) {
                    (Some(before), Some(after)) => Arc::ptr_eq(before, after),
                    (None, None) => true,
                    _ => false,
                };
                if same {
                    continue;
                }
                if cancel.is_cancelled() || transport_end.is_cancelled() {
                    return Ok(());
                }
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Ok(()),
                _ = transport_end.cancelled() => return Ok(()),
                    sent = sink.notify_resource_updated(uri.clone()) => {
                        if sent.is_err() { return Ok(()); }
                    }
                }
                *previous = next.cloned();
                if next.is_none() {
                    closed.push(uri.clone());
                }
            }
            for uri in closed {
                observed.remove(&uri);
            }
            if observed.is_empty() || cancel.is_cancelled() || transport_end.is_cancelled() {
                return Ok(());
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                _ = transport_end.cancelled() => return Ok(()),
                changed = signal.changed() => {
                    if changed.is_err() { return Ok(()); }
                }
            }
        }
        Ok(())
    }
}

fn document_id(uri: &str) -> Option<&str> {
    let id = uri.strip_prefix(PREFIX)?;
    if id.len() != 36 {
        return None;
    }
    let uuid = uuid::Uuid::parse_str(id).ok()?;
    (uuid.get_version_num() == 4 && uuid.to_string() == id).then_some(id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hwpcraft_engine::Command;
    use serde_json::json;

    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    use crate::{document_workers::DocumentWorkers, sdk_service::HwpService};
    use serde_json::Value;
    use std::{collections::VecDeque, time::Duration};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    const META: &str = "io.modelcontextprotocol/subscriptionId";
    const TIMEOUT: Duration = Duration::from_secs(5);

    /// Two peers can share the exact production service while retaining separate
    /// SDK transports and request-ID namespaces, as HTTP service clones do.
    struct Wire {
        read: tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        write: tokio::io::WriteHalf<tokio::io::DuplexStream>,
        server: tokio::task::JoinHandle<Result<()>>,
        buffered: VecDeque<Value>,
        next: usize,
    }
    impl Wire {
        fn start(service: HwpService) -> Self {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (read, write) = tokio::io::split(server);
            let server = tokio::spawn(crate::sdk_runtime::run_stream(service, read, write));
            let (read, write) = tokio::io::split(client);
            Self {
                read: tokio::io::BufReader::new(read),
                write,
                server,
                buffered: VecDeque::new(),
                next: 1000,
            }
        }
        async fn send(&mut self, id: Option<Value>, method: &str, mut params: Value) {
            params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientCapabilities":{}});
            let mut request = json!({"jsonrpc":"2.0","method":method,"params":params});
            if let Some(id) = id {
                request["id"] = id;
            }
            let mut bytes = serde_json::to_vec(&request).unwrap();
            bytes.push(b'\n');
            tokio::time::timeout(TIMEOUT, self.write.write_all(&bytes))
                .await
                .unwrap()
                .unwrap();
            self.write.flush().await.unwrap();
        }
        async fn line(&mut self) -> Value {
            let mut line = String::new();
            let count = tokio::time::timeout(TIMEOUT, self.read.read_line(&mut line))
                .await
                .unwrap()
                .unwrap();
            assert_ne!(count, 0, "subscription stream ended unexpectedly");
            let value: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(value["jsonrpc"], "2.0", "{value}");
            value
        }
        async fn matching(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
            if let Some(index) = self.buffered.iter().position(&predicate) {
                return self.buffered.remove(index).unwrap();
            }
            loop {
                let value = self.line().await;
                if predicate(&value) {
                    return value;
                }
                self.buffered.push_back(value);
            }
        }
        async fn raw_rpc(&mut self, method: &str, params: Value) -> Value {
            let id = json!(format!("rpc-{}", self.next));
            self.next += 1;
            self.send(Some(id.clone()), method, params).await;
            self.matching(|value| value.get("id") == Some(&id)).await
        }
        async fn rpc(&mut self, method: &str, params: Value) -> Value {
            let frame = self.raw_rpc(method, params).await;
            assert!(frame.get("error").is_none(), "{frame}");
            frame["result"].clone()
        }
        async fn tool(&mut self, tool: &str, args: Value) -> Value {
            let result = self
                .rpc("tools/call", json!({"name":tool,"arguments":args}))
                .await;
            assert_eq!(result["isError"], false, "{result}");
            result["structuredContent"].clone()
        }
        async fn create(&mut self) -> String {
            self.tool("new_document", json!({})).await["document_id"]
                .as_str()
                .unwrap()
                .to_owned()
        }
        async fn listen(&mut self, subscription: Value, uris: Vec<String>) -> Value {
            self.send(
                Some(subscription.clone()),
                "subscriptions/listen",
                json!({
                "notifications":{"toolsListChanged":true,"resourcesListChanged":true,
                    "promptsListChanged":true,"resourceSubscriptions":uris}}),
            )
            .await;
            let ack = self.subscription(&subscription).await;
            assert_eq!(
                ack["method"], "notifications/subscriptions/acknowledged",
                "{ack}"
            );
            assert!(
                ack["params"]["notifications"]
                    .get("toolsListChanged")
                    .is_none()
            );
            assert!(
                ack["params"]["notifications"]
                    .get("resourcesListChanged")
                    .is_none()
            );
            assert!(
                ack["params"]["notifications"]
                    .get("promptsListChanged")
                    .is_none()
            );
            ack
        }
        async fn subscription(&mut self, subscription: &Value) -> Value {
            self.matching(|value| {
                value["params"]["_meta"][META] == *subscription
                    || value.get("id") == Some(subscription)
            })
            .await
        }
        async fn updated(&mut self, subscription: &Value, uri: &str) -> Value {
            let value = self.subscription(subscription).await;
            assert_eq!(
                value["method"], "notifications/resources/updated",
                "{value}"
            );
            assert_eq!(value["params"]["uri"], uri, "{value}");
            let params = value["params"].as_object().unwrap();
            assert_eq!(
                params.len(),
                2,
                "URI-only invalidation leaked extra fields: {value}"
            );
            assert_eq!(params["_meta"].as_object().unwrap().len(), 1);
            value
        }
        async fn quiet(&mut self, subscription: &Value) {
            assert!(
                !self
                    .buffered
                    .iter()
                    .any(|value| value["params"]["_meta"][META] == *subscription),
                "unexpected buffered update"
            );
            let until = tokio::time::Instant::now() + Duration::from_millis(80);
            loop {
                let mut line = String::new();
                let read = tokio::time::timeout_at(until, self.read.read_line(&mut line)).await;
                let Ok(read) = read else {
                    break;
                };
                assert_ne!(read.unwrap(), 0);
                let value: Value = serde_json::from_str(&line).unwrap();
                assert_ne!(
                    value["params"]["_meta"][META], *subscription,
                    "notification after cancellation or unrelated document change: {value}"
                );
                self.buffered.push_back(value);
            }
        }
        async fn finish(mut self) {
            self.write.shutdown().await.unwrap();
            tokio::time::timeout(TIMEOUT, &mut self.server)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }
    impl Drop for Wire {
        fn drop(&mut self) {
            self.server.abort();
        }
    }
    fn shared() -> HwpService {
        HwpService::new(Arc::new(DocumentWorkers::new(None))).unwrap()
    }

    #[tokio::test]
    async fn actual_peers_keep_same_subscription_id_and_document_events_isolated() -> Result<()> {
        let workers = Arc::new(DocumentWorkers::new(None));
        let service = HwpService::new(workers.clone()).unwrap();
        let mut first = Wire::start(service.clone());
        let mut second = Wire::start(service);
        let a = first.create().await;
        let b = second.create().await;
        let uri_a = format!("{PREFIX}{a}");
        let uri_b = format!("{PREFIX}{b}");
        let subscription = json!("same-listen-id");
        let ack_a = first
            .listen(subscription.clone(), vec![uri_a.clone()])
            .await;
        let ack_b = second
            .listen(subscription.clone(), vec![uri_b.clone()])
            .await;
        assert_eq!(
            ack_a["params"]["notifications"]["resourceSubscriptions"],
            json!([uri_a])
        );
        assert_eq!(
            ack_b["params"]["notifications"]["resourceSubscriptions"],
            json!([uri_b])
        );
        // Initial resyncs are consumed separately and are not edit evidence.
        first.updated(&subscription, &uri_a).await;
        second.updated(&subscription, &uri_b).await;
        first.quiet(&subscription).await;
        second.quiet(&subscription).await;
        first
            .tool(
                "type_text",
                json!({"document_id":a,"text":"private A 한글😀"}),
            )
            .await;
        let changed = first.updated(&subscription, &uri_a).await;
        assert!(!changed.to_string().contains("private A"));
        second.quiet(&subscription).await;
        // The originating client is not the routing key: the explicit URI is.
        first
            .tool("type_text", json!({"document_id":b,"text":"private B"}))
            .await;
        second.updated(&subscription, &uri_b).await;
        first.quiet(&subscription).await;
        let inspect = first
            .tool("inspect_document", json!({"document_id":a}))
            .await;
        assert_eq!(inspect["blocks"][0]["text"], "private A 한글😀");
        first.quiet(&subscription).await;
        let invalid = first
            .raw_rpc(
                "tools/call",
                json!({"name":"type_text",
            "arguments":{"document_id":a,"text":123}}),
            )
            .await;
        assert!(invalid.get("error").is_some());
        first.quiet(&subscription).await;
        assert_eq!(
            workers.subscriptions().permits.available_permits(),
            MAX_SUBSCRIPTIONS - 2
        );
        // EOF must gracefully finish this subscription before the SDK's
        // five-second finite-response drain, without disabling another peer.
        first.write.shutdown().await.unwrap();
        let complete =
            tokio::time::timeout(Duration::from_secs(2), first.subscription(&subscription))
                .await
                .unwrap();
        assert_eq!(complete["result"]["resultType"], "complete");
        assert_eq!(complete["result"]["_meta"][META], subscription);
        assert_eq!(
            workers.subscriptions().permits.available_permits(),
            MAX_SUBSCRIPTIONS - 1
        );
        second
            .tool(
                "type_text",
                json!({"document_id":a,"text":" after peer EOF"}),
            )
            .await;
        let mut late = String::new();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), first.read.read_line(&mut late))
                .await
                .unwrap()
                .unwrap(),
            0,
            "notification after the subscription completed: {late}"
        );
        tokio::time::timeout(Duration::from_secs(2), first.finish())
            .await
            .unwrap();
        assert_eq!(
            workers.subscriptions().permits.available_permits(),
            MAX_SUBSCRIPTIONS - 1
        );
        // Dropping one peer must not retain its notification sink, or cancel the
        // other peer's subscription that happens to have the same request ID.
        second
            .tool("type_text", json!({"document_id":b,"text":" still live"}))
            .await;
        second.updated(&subscription, &uri_b).await;
        second.finish().await;
        assert_eq!(
            workers.subscriptions().permits.available_permits(),
            MAX_SUBSCRIPTIONS
        );
        Ok(())
    }

    #[tokio::test]
    async fn cancellation_stops_one_subscription_while_typed_other_id_remains_live() -> Result<()> {
        let mut client = Wire::start(shared());
        let doc = client.create().await;
        let uri = format!("{PREFIX}{doc}");
        let numeric = json!(7);
        let textual = json!("7");
        client.listen(numeric.clone(), vec![uri.clone()]).await;
        client.updated(&numeric, &uri).await;
        client.listen(textual.clone(), vec![uri.clone()]).await;
        client.updated(&textual, &uri).await;
        client
            .send(
                None,
                "notifications/cancelled",
                json!({"requestId":numeric}),
            )
            .await;
        client.rpc("server/discover", json!({})).await;
        client
            .tool(
                "type_text",
                json!({"document_id":doc,"text":"after cancellation"}),
            )
            .await;
        client.updated(&textual, &uri).await;
        client.quiet(&numeric).await;
        client
            .send(
                None,
                "notifications/cancelled",
                json!({"requestId":textual}),
            )
            .await;
        client.rpc("server/discover", json!({})).await;
        client
            .tool(
                "type_text",
                json!({"document_id":doc,"text":" no observers"}),
            )
            .await;
        client.quiet(&numeric).await;
        client.quiet(&textual).await;
        client.finish().await;
        Ok(())
    }

    #[tokio::test]
    async fn close_invalidates_only_its_uri_and_gracefully_completes_last_watch() -> Result<()> {
        let mut client = Wire::start(shared());
        let a = client.create().await;
        let b = client.create().await;
        let uri_a = format!("{PREFIX}{a}");
        let uri_b = format!("{PREFIX}{b}");
        let subscription = json!("two-documents");
        // SDK intersection retains duplicates; they must not consume the
        // distinct-URI bound and silently drop the second document.
        let mut requested = vec![uri_a.clone(); 20];
        requested.push(uri_b.clone());
        client.listen(subscription.clone(), requested).await;
        let mut initial = BTreeSet::new();
        for _ in 0..2 {
            let value = client.subscription(&subscription).await;
            assert_eq!(value["method"], "notifications/resources/updated");
            initial.insert(value["params"]["uri"].as_str().unwrap().to_owned());
        }
        assert_eq!(initial, BTreeSet::from([uri_a.clone(), uri_b.clone()]));
        client
            .tool("close_document", json!({"document_id":a}))
            .await;
        client.updated(&subscription, &uri_a).await;
        let gone = client.raw_rpc("resources/read", json!({"uri":uri_a})).await;
        assert!(gone.get("error").is_some(), "{gone}");
        client
            .tool(
                "type_text",
                json!({"document_id":b,"text":"survives first close"}),
            )
            .await;
        client.updated(&subscription, &uri_b).await;
        // A refused dirty close is not an actual deletion/invalidation.
        let refused = client
            .rpc(
                "tools/call",
                json!({"name":"close_document","arguments":{"document_id":b}}),
            )
            .await;
        assert_eq!(refused["isError"], true);
        client.quiet(&subscription).await;
        client
            .tool("close_document", json!({"document_id":b,"force":true}))
            .await;
        client.updated(&subscription, &uri_b).await;
        let complete = client.subscription(&subscription).await;
        assert_eq!(complete["id"], subscription);
        assert_eq!(complete["result"]["resultType"], "complete");
        assert_eq!(complete["result"]["_meta"][META], subscription);
        let rejected = json!("closed-and-static");
        let ack = client
            .listen(
                rejected.clone(),
                vec![
                    uri_a,
                    uri_b,
                    "hwpcraft://commands".into(),
                    format!("{PREFIX}{}", id()),
                ],
            )
            .await;
        assert_eq!(ack["params"]["notifications"], json!({}));
        let ended = client.subscription(&rejected).await;
        assert_eq!(ended["result"]["resultType"], "complete");
        client.finish().await;
        Ok(())
    }

    #[tokio::test]
    async fn save_path_and_selection_invalidate_without_exposing_path_or_document_text()
    -> Result<()> {
        let mut client = Wire::start(shared());
        let doc = client.create().await;
        let uri = format!("{PREFIX}{doc}");
        let subscription = json!("saved-state");
        client.listen(subscription.clone(), vec![uri.clone()]).await;
        client.updated(&subscription, &uri).await;
        client
            .tool(
                "type_text",
                json!({"document_id":doc,"text":"private report"}),
            )
            .await;
        client.updated(&subscription, &uri).await;
        client
            .tool("select_text", json!({"document_id":doc,"text":"report"}))
            .await;
        client.updated(&subscription, &uri).await;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("private-saved-path-한글.hwpx");
        client
            .tool("save_document", json!({"document_id":doc,"path":path}))
            .await;
        let value = client.updated(&subscription, &uri).await;
        assert!(!value.to_string().contains("private-saved-path"));
        client
            .tool("save_document", json!({"document_id":doc,"path":path}))
            .await;
        client.quiet(&subscription).await;
        client
            .tool(
                "execute",
                json!({"document_id":doc,"command":"edit.undo","params":{}}),
            )
            .await;
        client.updated(&subscription, &uri).await;
        client
            .tool(
                "execute",
                json!({"document_id":doc,"command":"edit.redo","params":{}}),
            )
            .await;
        client.updated(&subscription, &uri).await;
        client.finish().await;
        Ok(())
    }

    #[test]
    fn filter_accepts_only_requested_live_canonical_document_uris() -> Result<()> {
        let hub = DocumentSubscriptions::new();
        let own = id();
        let other = id();
        hub.register(&own)?;
        let uri = format!("{PREFIX}{own}");
        let request: SubscriptionFilter = serde_json::from_value(json!({
            "toolsListChanged":true,"promptsListChanged":true,"resourcesListChanged":true,
            "resourceSubscriptions":[uri,uri,format!("{PREFIX}{other}"),
                "hwpcraft://commands",format!("{uri}?secret=1"),format!("{uri}/"),
                format!("{PREFIX}{}",own.to_uppercase()),format!("{PREFIX}../{own}")]
        }))?;
        let accepted = hub.accepted_filter(&request).unwrap();
        assert_eq!(accepted.resource_subscriptions, Some(vec![uri]));
        assert_eq!(accepted.tools_list_changed, None);
        assert_eq!(accepted.prompts_list_changed, None);
        assert_eq!(accepted.resources_list_changed, None);
        hub.closed(&own);
        assert_eq!(
            hub.accepted_filter(&request).unwrap(),
            SubscriptionFilter::new()
        );
        Ok(())
    }

    #[test]
    fn registry_is_bounded_and_does_not_retain_closed_handles() -> Result<()> {
        let hub = DocumentSubscriptions::new();
        for _ in 0..1000 {
            let own = id();
            hub.register(&own)?;
            hub.changed(&own);
            hub.closed(&own);
        }
        assert!(hub.snapshot().unwrap().is_empty());
        for _ in 0..MAX_DOCUMENTS {
            hub.register(&id())?;
        }
        assert!(hub.register(&id()).is_err());
        hub.changed("unpublished");
        assert_eq!(hub.snapshot().unwrap().len(), MAX_DOCUMENTS);
        hub.close_all();
        assert!(hub.snapshot().unwrap().is_empty());
        assert_eq!(hub.permits.available_permits(), MAX_SUBSCRIPTIONS);
        Ok(())
    }

    #[test]
    fn stamps_track_real_resource_state_without_serializing_document() -> Result<()> {
        let mut session = Session::new()?;
        let blank = DocumentStamp::capture(&session);
        session.run("document.inspect", json!({}))?;
        assert_eq!(DocumentStamp::capture(&session), blank);
        session.execute(Command::Insert("한글😀".into()))?;
        let inserted = DocumentStamp::capture(&session);
        assert_ne!(inserted, blank);
        session.run(
            "selection.set",
            json!({"cursor":{"section":0,"paragraph":0,"offset":0}}),
        )?;
        assert_ne!(DocumentStamp::capture(&session), inserted);
        let selected = DocumentStamp::capture(&session);
        assert!(session.run("not.a.command", json!({})).is_err());
        assert_eq!(DocumentStamp::capture(&session), selected);
        Ok(())
    }
}
