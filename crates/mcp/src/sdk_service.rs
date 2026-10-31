//! Current-protocol, stateless MCP adapter over isolated shared-engine workers.
use crate::{
    catalog, document_workers::DocumentWorkers, output_schemas, progress, sdk_catalog, tasks, tools,
};
use anyhow::{Context, Result, bail};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    model::*,
    service::RequestContext,
    task_manager::{TaskExit, TaskManager},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const DOCUMENT: &str = "document_id";
const VERSION: ProtocolVersion = ProtocolVersion::V_2026_07_28;
const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
/// Application error code, outside the JSON-RPC reserved range (-32768..=-32000)
/// as MCP 2026-07-28 requires for codes it does not define.
pub(crate) const COMPLETION_RATE_LIMITED: i32 = -31429;

#[derive(Clone)]
pub(crate) struct HwpService {
    subscription_shutdown: Option<CancellationToken>,
    workers: Arc<DocumentWorkers>,
    tools: Arc<Vec<Tool>>,
    validators: Arc<HashMap<String, jsonschema::Validator>>,
    capabilities: ServerCapabilities,
    completion_window: Arc<Mutex<VecDeque<Instant>>>,
    tasks: TaskManager,
}

impl HwpService {
    pub(crate) fn new(workers: Arc<DocumentWorkers>) -> Result<Self> {
        let mut definitions = tools::tool_definitions()
            .as_array()
            .cloned()
            .context("tool definitions must be an array")?;
        definitions.push(json!({"name":"close_document","description":"Close the explicitly identified document worker. Unsaved changes require force=true. Other documents are preserved.",
            "inputSchema":{"type":"object","properties":{"force":{"type":"boolean"}},"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":true,"openWorldHint":false}}));
        definitions.push(json!({"name":"list_documents","description":"List the documents a desktop HwpCraft user has shared with MCP (title, path, unsaved state) and their document_id. Documents created through MCP are not listed: their document_id is held only by the client that created it.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}));
        let mut typed = Vec::new();
        let mut validators = HashMap::new();
        for mut definition in definitions {
            let name = definition["name"]
                .as_str()
                .context("tool name missing")?
                .to_owned();
            let schema = definition["inputSchema"]
                .as_object_mut()
                .context("tool input schema missing")?;
            schema.insert("$schema".into(), json!(DIALECT));
            let independent = matches!(name.as_str(), "list_commands" | "list_documents");
            if !independent {
                let properties = schema
                    .get_mut("properties")
                    .and_then(Value::as_object_mut)
                    .context("tool properties missing")?;
                properties.insert(DOCUMENT.into(), json!({"type":"string","minLength":1,"maxLength":128,
                    "description":"Explicit document handle returned by new_document or an open tool; never inferred from a previous request."}));
                if name == "hwp_action" {
                    schema.insert("if".into(), json!({"required":["id"]}));
                    schema.insert("then".into(), json!({"required":[DOCUMENT]}));
                } else if !matches!(
                    name.as_str(),
                    "new_document" | "open_document" | "open_protected_document"
                ) {
                    schema
                        .entry("required")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .context("tool required fields must be an array")?
                        .push(json!(DOCUMENT));
                }
            }
            let validator = schema_validator(&Value::Object(schema.clone()))?;
            validators.insert(name.clone(), validator);
            match output_schemas::declared(&name, DIALECT) {
                Some(schema) => {
                    schema_validator(&schema)?;
                    definition["outputSchema"] = schema;
                }
                None => bail!("tool {name} has no declared output schema"),
            }
            if matches!(
                name.as_str(),
                "new_document" | "open_document" | "open_protected_document"
            ) {
                let description = definition["description"].as_str().unwrap_or("").to_owned();
                definition["description"] = json!(format!(
                    "{description} Omit document_id to create an isolated document; supply it to replace only that existing document, respecting force."
                ));
            }
            typed.push(serde_json::from_value(definition)?);
        }
        Ok(Self {
            subscription_shutdown: None,
            completion_window: Arc::new(Mutex::new(VecDeque::new())),
            tasks: TaskManager::new(),
            workers,
            tools: Arc::new(typed),
            validators: Arc::new(validators),
            capabilities: serde_json::from_value(
                json!({"tools":{},"resources":{"subscribe":true},"prompts":{},"completions":{},
                    "extensions":{"io.modelcontextprotocol/tasks":{}}}),
            )?,
        })
    }

    pub(crate) fn with_subscription_shutdown(mut self, shutdown: CancellationToken) -> Self {
        self.subscription_shutdown = Some(shutdown);
        self
    }

    /// Run a long file tool as an MCP task. The task owns its cancellation:
    /// it outlives this request, and only `tasks/cancel` stops it.
    fn spawn_task(&self, name: String, args: Value) -> CreateTaskResult {
        let service = self.clone();
        let task = self.tasks.spawn(tasks::options(), move |task| {
            Box::pin(async move {
                let cancel = CancellationToken::new();
                let link = tokio::spawn({
                    let (task, cancel) = (task.clone(), cancel.clone());
                    async move {
                        task.cancelled().await;
                        cancel.cancel();
                    }
                });
                let status = task.clone();
                let outcome = progress::run(
                    service.dispatch(&name, args, cancel),
                    progress::HEARTBEAT,
                    move |_, message| {
                        if let Some(message) = message {
                            status.set_status_message(message);
                        }
                        async {}
                    },
                )
                .await;
                link.abort();
                if outcome.is_err() && task.is_cancel_requested() {
                    return Err(TaskExit::Cancelled);
                }
                let (id, value) = outcome.unwrap_or_else(|error| {
                    (
                        None,
                        tools::result(json!({"message":error.to_string()}), true),
                    )
                });
                match tool_result(value, id.as_deref()).map_err(TaskExit::Error)? {
                    CallToolResponse::Complete(result) => Ok(result),
                    _ => Err(TaskExit::Error(McpError::internal_error(
                        "task produced a non-final tool result",
                        None,
                    ))),
                }
            })
        });
        CreateTaskResult::new(task).with_meta(server_meta())
    }

    async fn dispatch(
        &self,
        name: &str,
        mut args: Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(Option<String>, Value)> {
        let id = args
            .as_object_mut()
            .and_then(|map| map.remove(DOCUMENT))
            .and_then(|value| value.as_str().map(str::to_owned));
        if name == "list_commands" {
            return Ok((
                None,
                tools::result(
                    catalog::commands(args["query"].as_str().unwrap_or("")),
                    false,
                ),
            ));
        }
        if name == "list_documents" {
            let documents: Vec<Value> = self
                .workers
                .list()
                .into_iter()
                .map(|entry| {
                    json!({DOCUMENT:entry.id,"title":entry.info.title,"path":entry.info.path,
                        "source":entry.source,"dirty":entry.info.dirty})
                })
                .collect();
            return Ok((None, tools::result(json!({"documents":documents}), false)));
        }
        if name == "hwp_action" && args.get("id").is_none() {
            return Ok((
                None,
                tools::result(
                    catalog::actions(args["query"].as_str().unwrap_or("")),
                    false,
                ),
            ));
        }
        if matches!(
            name,
            "new_document" | "open_document" | "open_protected_document"
        ) && id.is_none()
        {
            return self.workers.create(name, args, cancel).await;
        }
        let id = id.context("document_id is required for this operation")?;
        let value = if name == "close_document" {
            self.workers
                .close(&id, args["force"].as_bool().unwrap_or(false), cancel)
                .await?
        } else {
            self.workers.call(&id, name, args, cancel).await?
        };
        Ok((Some(id), value))
    }
}

fn schema_validator(schema: &Value) -> Result<jsonschema::Validator> {
    if schema
        .get("$schema")
        .is_some_and(|value| value.as_str() != Some(DIALECT))
    {
        bail!("unsupported tool schema dialect");
    }
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .offline()
        .build(schema)
        .context("invalid offline JSON Schema 2020-12 tool definition")
}
const SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

fn server_info() -> Implementation {
    Implementation::new("hwpcraft", hwpcraft_engine::VERSION.trim())
}

fn server_meta() -> MetaObject {
    let mut meta = MetaObject::default();
    meta.0.insert(SERVER_INFO.to_owned(), json!(server_info()));
    meta
}

/// Build a typed result. Every result names this server in `_meta` (2026-07-28
/// per-response protocol field) so stateless clients need no prior discover.
fn decode<T: DeserializeOwned>(mut value: Value) -> std::result::Result<T, McpError> {
    if let Some(result) = value.as_object_mut() {
        let meta = result.entry("_meta").or_insert_with(|| json!({}));
        if let Some(meta) = meta.as_object_mut() {
            meta.entry(SERVER_INFO)
                .or_insert_with(|| json!(server_info()));
        }
    }
    serde_json::from_value(value).map_err(|error| McpError::internal_error(error.to_string(), None))
}
fn cursor(request: &Option<PaginatedRequestParams>) -> std::result::Result<(), McpError> {
    if request
        .as_ref()
        .is_some_and(|request| request.cursor.is_some())
    {
        return Err(McpError::invalid_params(
            "the complete list has no pagination cursor",
            None,
        ));
    }
    Ok(())
}
fn tool_result(
    mut value: Value,
    id: Option<&str>,
) -> std::result::Result<CallToolResponse, McpError> {
    let is_error = value["isError"].as_bool().unwrap_or(false);
    let parsed = value
        .as_object_mut()
        .and_then(|map| map.remove(tools::PARSED));
    let data = match parsed {
        Some(data) => data,
        None => {
            let text = value["content"].as_array().and_then(|blocks| {
                blocks
                    .iter()
                    .find_map(|block| block.get("text").and_then(Value::as_str))
            });
            match text {
                Some(text) => {
                    serde_json::from_str(text).unwrap_or_else(|_| json!({"message":text}))
                }
                None => value.clone(),
            }
        }
    };
    let mut structured = match data {
        Value::Object(map) => Value::Object(map),
        Value::Array(items) => json!({"items":items}),
        other => json!({"value":other}),
    };
    if let Some(id) = id {
        structured[DOCUMENT] = json!(id);
    }
    // A tool execution error always names its cause in `message`, the field the
    // declared output schema requires of an error; other fields are kept.
    if is_error && structured.get("message").is_none() {
        let cause = structured["error"]
            .as_str()
            .or_else(|| {
                structured["results"]
                    .as_array()?
                    .iter()
                    .find_map(|entry| entry["error"].as_str())
            })
            .unwrap_or("tool execution failed")
            .to_owned();
        structured["message"] = json!(cause);
    }
    // Move non-text blocks (render_page's base64 PNG) instead of copying them.
    let mut content: Vec<Value> = match value.get_mut("content").map(Value::take) {
        Some(Value::Array(blocks)) => blocks
            .into_iter()
            .filter(|block| block["type"] != "text")
            .collect(),
        _ => Vec::new(),
    };
    content.push(json!({"type":"text","text":structured.to_string()}));
    let mut result = serde_json::Map::new();
    result.insert("resultType".into(), json!("complete"));
    result.insert("content".into(), Value::Array(content));
    result.insert("structuredContent".into(), structured);
    result.insert("isError".into(), json!(is_error));
    let result: CallToolResult = decode(Value::Object(result))?;
    Ok(result.into())
}

impl ServerHandler for HwpService {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        self.workers.subscriptions().accepted_filter(requested)
    }

    async fn listen(
        &self,
        context: rmcp::service::SubscriptionContext,
    ) -> std::result::Result<(), McpError> {
        match &self.subscription_shutdown {
            Some(shutdown) => {
                self.workers
                    .subscriptions()
                    .listen_until(context, shutdown.clone())
                    .await
            }
            None => self.workers.subscriptions().listen(context).await,
        }
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[VERSION])
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(self.capabilities.clone()).with_protocol_version(VERSION)
            .with_server_info(server_info())
            .with_instructions("MCP 2026-07-28 authoring server. Every request supplies protocolVersion and clientCapabilities in protocol _meta. new_document/open_document/open_protected_document return document_id; pass it on EVERY document-scoped tool call. No implicit current document. Discover all engine commands through list_commands; use execute, batch and hwp_action to author. Cursor offsets are Unicode scalars; render_page and page.layout.get pages are one-based. Query caret.get for generation before selection.range or addressed edits. Use context.v2 for supported nested regions and tools.autotext.body for formatted snippets. Atomic batches restore document edits and reject persistent file/settings writes. Save HWP/HWPX explicitly, inspect/reopen and render PNG/PDF for verification. Close documents explicitly. Requests for one document run in order, so a long save or PDF export delays later calls on that document only; send progressToken to receive a heartbeat every 5 s. A save cancelled while running leaves the existing file unchanged. Clients that declare the io.modelcontextprotocol/tasks extension get a task for open/save/PDF tools (open_document, open_protected_document, save_document, execute file.*): poll tasks/get at pollIntervalMs; no task notifications are sent; tasks/cancel is cooperative and only prevents a file replacement that has not happened yet. Unsupported operations report errors; do not infer full Hancom feature parity from command registration.")
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        cursor(&request)?;
        decode(
            json!({"resultType":"complete","tools":self.tools.as_ref(),"ttlMs":60000,"cacheScope":"public"}),
        )
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, McpError> {
        let name = request.name.as_ref();
        let validator = self
            .validators
            .get(name)
            .ok_or_else(|| McpError::invalid_params("unknown tool", None))?;
        if request.input_responses.is_some() || request.request_state.is_some() {
            return Err(McpError::invalid_params(
                "this operation has no outstanding multi-round-trip input request",
                None,
            ));
        }
        let args = Value::Object(request.arguments.unwrap_or_default());
        if let Some(error) = validator.iter_errors(&args).next() {
            return Err(McpError::invalid_params(
                format!(
                    "tool input schema violation at {} ({})",
                    error.instance_path(),
                    error.schema_path()
                ),
                None,
            ));
        }
        let client_tasks = context
            .client_capabilities()
            .is_some_and(|capabilities| capabilities.supports_tasks());
        if client_tasks
            && tasks::eligible(name, &args)
            && self.tasks.running_task_count() < tasks::MAX_RUNNING
        {
            return Ok(CallToolResponse::Task(
                self.spawn_task(name.to_owned(), args),
            ));
        }
        let work = self.dispatch(name, args, context.ct.clone());
        let outcome = match context.meta.get_progress_token() {
            Some(token) => {
                let (peer, cancel) = (context.peer.clone(), context.ct.clone());
                progress::run(work, progress::HEARTBEAT, move |value, message| {
                    let (peer, cancel) = (peer.clone(), cancel.clone());
                    let mut param = ProgressNotificationParam::new(token.clone(), value);
                    param.message = message;
                    async move {
                        if !cancel.is_cancelled() {
                            let _ = peer.notify_progress(param).await;
                        }
                    }
                })
                .await
            }
            None => work.await,
        };
        let (id, value) = outcome.unwrap_or_else(|error| {
            (
                None,
                tools::result(json!({"message":error.to_string()}), true),
            )
        });
        tool_result(value, id.as_deref())
    }
    async fn get_task(
        &self,
        request: GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<GetTaskResult, McpError> {
        let mut result = GetTaskResult::new(self.tasks.get_task(&request.task_id)?);
        result.meta = Some(server_meta());
        Ok(result)
    }
    async fn update_task(
        &self,
        request: UpdateTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<(), McpError> {
        self.tasks
            .update_task(&request.task_id, request.input_responses)
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<(), McpError> {
        self.tasks.cancel_task(&request.task_id)
    }
    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourcesResult, McpError> {
        cursor(&request)?;
        decode(sdk_catalog::list_resources())
    }
    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, McpError> {
        cursor(&request)?;
        decode(sdk_catalog::list_resource_templates())
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResponse, McpError> {
        if request.input_responses.is_some() || request.request_state.is_some() {
            return Err(McpError::invalid_params(
                "this resource read has no outstanding multi-round-trip input request",
                None,
            ));
        }
        let uri = request.uri;
        if let Some(value) = sdk_catalog::static_resource(&uri) {
            let result: ReadResourceResult = decode(value)?;
            return Ok(result.into());
        }
        let data = if let Some(id) = uri.strip_prefix("hwpcraft://documents/") {
            let value = self
                .workers
                .execute(id, "document.inspect", json!({}), context.ct.clone())
                .await
                .map_err(|error| McpError::resource_not_found(error.to_string(), None))?;
            if value["isError"] == true {
                return Err(McpError::resource_not_found(
                    "document inspection failed",
                    None,
                ));
            }
            let text = value["content"]
                .as_array()
                .and_then(|items| items.iter().find_map(|item| item["text"].as_str()));
            let mut data: Value = text
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .unwrap_or(Value::Null);
            if data.is_object() {
                data[DOCUMENT] = json!(id);
            }
            data
        } else {
            return Err(McpError::resource_not_found(
                "unknown HwpCraft resource",
                None,
            ));
        };
        let result: ReadResourceResult = decode(
            json!({"resultType":"complete","ttlMs":0,"cacheScope":"private","contents":[
            {"uri":uri,"mimeType":"application/json","text":data.to_string()}]}),
        )?;
        Ok(result.into())
    }
    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListPromptsResult, McpError> {
        cursor(&request)?;
        decode(sdk_catalog::list_prompts())
    }
    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<GetPromptResponse, McpError> {
        let value = serde_json::to_value(request)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let result: GetPromptResult = decode(sdk_catalog::prompt(value)?)?;
        Ok(result.into())
    }
    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CompleteResult, McpError> {
        {
            let mut window = self
                .completion_window
                .lock()
                .map_err(|_| McpError::internal_error("completion limiter unavailable", None))?;
            let now = Instant::now();
            while window
                .front()
                .is_some_and(|time| now.saturating_duration_since(*time) >= Duration::from_secs(1))
            {
                window.pop_front();
            }
            if window.len() >= 30 {
                return Err(McpError::new(
                    ErrorCode(COMPLETION_RATE_LIMITED),
                    "completion rate limit exceeded",
                    Some(json!({"retryAfterMs":1000})),
                ));
            }
            window.push_back(now);
        }
        let value = serde_json::to_value(request)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        decode(sdk_catalog::completion(value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only documents a desktop user shared are listed; MCP-created handles stay private.
    #[tokio::test]
    async fn list_documents_shows_shared_desktop_documents_only() -> Result<()> {
        let workers = Arc::new(DocumentWorkers::new(None));
        let service = HwpService::new(workers.clone())?;
        let list = |service: HwpService| async move {
            let (_, value) = service
                .dispatch("list_documents", json!({}), CancellationToken::new())
                .await?;
            let response = tool_result(value, None).map_err(anyhow::Error::msg)?;
            let result = serde_json::to_value(ServerResult::from(response))?;
            let structured = result["structuredContent"].clone();
            assert!(service.validators.contains_key("list_documents"));
            let schema = output_schemas::declared("list_documents", DIALECT).context("schema")?;
            assert!(
                schema_validator(&schema)?.is_valid(&structured),
                "{structured}"
            );
            anyhow::Ok(structured["documents"].clone())
        };
        let (created, _) = service
            .dispatch("new_document", json!({}), CancellationToken::new())
            .await?;
        assert!(created.is_some());
        assert_eq!(list(service.clone()).await?, json!([]));
        let shared = workers.attach_external(
            Arc::new(|| {}),
            crate::document_workers::ExternalInfo {
                title: "보고서.hwpx".into(),
                path: Some("C:/docs/보고서.hwpx".into()),
                dirty: true,
            },
        )?;
        let listed = list(service.clone()).await?;
        assert_eq!(
            listed,
            json!([{"document_id":shared.id(),"title":"보고서.hwpx","path":"C:/docs/보고서.hwpx","source":"desktop","dirty":true}])
        );
        drop(shared);
        assert_eq!(list(service).await?, json!([]));
        Ok(())
    }

    #[test]
    fn schemas_require_explicit_document_context_and_preserve_pure_discovery() -> Result<()> {
        let service = HwpService::new(Arc::new(DocumentWorkers::new(None)))?;
        for name in [
            "execute",
            "batch",
            "get_text",
            "render_page",
            "get_selection",
            "save_document",
            "close_document",
        ] {
            let validator = service.validators.get(name).context("tool validator")?;
            assert!(!validator.is_valid(&json!({})), "{name}");
        }
        assert!(service.validators["new_document"].is_valid(&json!({})));
        assert!(service.validators["list_commands"].is_valid(&json!({"query":"표"})));
        assert!(service.validators["list_documents"].is_valid(&json!({})));
        assert!(!service.validators["list_documents"].is_valid(&json!({"document_id":"x"})));
        assert!(service.validators["hwp_action"].is_valid(&json!({"query":"Bold"})));
        assert!(!service.validators["hwp_action"].is_valid(&json!({"id":"CharShapeBold"})));
        assert!(
            service.validators["hwp_action"]
                .is_valid(&json!({"id":"CharShapeBold","document_id":"explicit-id"}))
        );
        assert!(!service.validators["new_document"].is_valid(&json!({"force":"yes"})));
        assert!(!service.validators["new_document"].is_valid(&json!({"unknown":true})));
        assert_eq!(service.get_info().protocol_version, VERSION);
        assert_eq!(service.supported_protocol_versions().as_ref(), &[VERSION]);
        Ok(())
    }

    #[test]
    fn schema_validation_is_offline_and_dialect_is_explicit() -> Result<()> {
        let validator = schema_validator(
            &json!({"$schema":DIALECT,"type":"object","properties":{"n":{"type":"integer","minimum":1}},"required":["n"],"additionalProperties":false}),
        )?;
        assert!(validator.is_valid(&json!({"n":2})));
        assert!(!validator.is_valid(&json!({"n":0})));
        assert!(!validator.is_valid(&json!({"n":2,"extra":true})));
        assert!(
            schema_validator(
                &json!({"$schema":"https://example.invalid/unknown-dialect","type":"object"})
            )
            .is_err()
        );
        assert!(
            schema_validator(
                &json!({"$schema":DIALECT,"$ref":"https://example.invalid/private-schema"})
            )
            .is_err()
        );
        assert!(
            schema_validator(&json!({"$schema":DIALECT,"$ref":"file:///private-schema"})).is_err()
        );
        Ok(())
    }

    #[test]
    fn structured_output_and_serialized_text_are_consistent_and_arrays_stay_typed() -> Result<()> {
        let response = tool_result(
            tools::result(json!({"pages":2,"text":"한글😀"}), false),
            Some("doc-id"),
        )
        .map_err(anyhow::Error::msg)?;
        let value = serde_json::to_value(ServerResult::from(response))?;
        assert_eq!(value["resultType"], "complete");
        assert_eq!(value["structuredContent"]["document_id"], "doc-id");
        let text = value["content"][0]["text"]
            .as_str()
            .context("text content")?;
        assert_eq!(
            serde_json::from_str::<Value>(text)?,
            value["structuredContent"]
        );
        // The value carried beside the text for the response stays internal.
        assert!(!value.to_string().contains(tools::PARSED), "{value}");
        // A result built without it (render_page) is still read from its text.
        let response = tool_result(
            json!({"content":[{"type":"text","text":"{\"page\":1}"}],"isError":false}),
            None,
        )
        .map_err(anyhow::Error::msg)?;
        assert_eq!(
            serde_json::to_value(ServerResult::from(response))?["structuredContent"]["page"],
            1
        );
        let response = tool_result(tools::result(json!(["one", "two"]), false), None)
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            serde_json::to_value(ServerResult::from(response))?["structuredContent"]["items"],
            json!(["one", "two"])
        );
        let response = tool_result(
            tools::result(json!({"message":"bounded failure"}), true),
            None,
        )
        .map_err(anyhow::Error::msg)?;
        assert_eq!(
            serde_json::to_value(ServerResult::from(response))?["isError"],
            true
        );
        Ok(())
    }

    #[test]
    fn application_error_codes_stay_outside_json_rpc_reserved_range() {
        for code in [
            COMPLETION_RATE_LIMITED,
            crate::document_subscriptions::SUBSCRIPTION_CAPACITY_REACHED,
        ] {
            assert!(!(-32768..=-32000).contains(&code), "{code}");
        }
    }

    /// Run every tool, successes and failures, and check each `structuredContent`
    /// against the tool's declared output schema, as an SDK client would.
    #[tokio::test]
    async fn every_tool_result_conforms_to_its_declared_output_schema() -> Result<()> {
        use hwpcraft_engine::{Command, Session};
        let service = HwpService::new(Arc::new(DocumentWorkers::new(None)))?;
        let mut validators = HashMap::new();
        for tool in service.tools.iter() {
            let schema = Value::Object(
                tool.output_schema
                    .as_deref()
                    .cloned()
                    .with_context(|| format!("{} declares no output schema", tool.name))?,
            );
            validators.insert(tool.name.to_string(), schema_validator(&schema)?);
        }
        let mut succeeded = std::collections::BTreeSet::new();
        let mut failed = std::collections::BTreeSet::new();
        let call = async |name: &str, args: Value| -> Result<(bool, Value)> {
            // As in call_tool: a dispatch failure becomes a tool execution error.
            let (id, value) = service
                .dispatch(name, args, CancellationToken::new())
                .await
                .unwrap_or_else(|error| {
                    (
                        None,
                        tools::result(json!({"message":error.to_string()}), true),
                    )
                });
            let response = tool_result(value, id.as_deref()).map_err(anyhow::Error::msg)?;
            let result = serde_json::to_value(ServerResult::from(response))?;
            let structured = result["structuredContent"].clone();
            let errors: Vec<_> = validators[name]
                .iter_errors(&structured)
                .map(|error| format!("{} ({})", error.instance_path(), error.schema_path()))
                .collect();
            assert!(errors.is_empty(), "{name}: {errors:?}\n{structured}");
            Ok((result["isError"] == true, structured))
        };
        let dir = tempfile::tempdir()?;
        let saved = dir.path().join("schema.hwpx");
        let protected = dir.path().join("protected.hwpx");
        let mut source = Session::new()?;
        source.execute(Command::Insert("암호 문서".into()))?;
        source.set_password(Some("schema-password".into()))?;
        source.save(&protected)?;
        let (_, created) = call("new_document", json!({})).await?;
        let id = created[DOCUMENT]
            .as_str()
            .context("document_id")?
            .to_owned();
        succeeded.insert("new_document");
        let doc = |extra: Value| {
            let mut args = extra;
            args[DOCUMENT] = json!(id);
            args
        };
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        let cases = [
            ("list_commands", json!({"query":"표"}), false),
            ("list_documents", json!({}), false),
            (
                "type_text",
                doc(json!({"text":"첫째 문단 😀\n둘째 문단"})),
                false,
            ),
            ("select_text", doc(json!({"text":"둘째"})), false),
            ("select_text", doc(json!({"text":"없는 글자"})), true),
            ("get_selection", doc(json!({})), false),
            ("get_text", doc(json!({"limit":1})), false),
            ("get_text", doc(json!({"start":99})), true),
            ("inspect_document", doc(json!({})), false),
            ("execute", doc(json!({"command":"select.get"})), false),
            ("execute", doc(json!({"command":"chart.types"})), false),
            ("execute", doc(json!({"command":"no.such.command"})), true),
            (
                "batch",
                doc(
                    json!({"commands":[{"command":"text.insert","params":{"text":"!"}}],"atomic":true}),
                ),
                false,
            ),
            (
                "batch",
                doc(
                    json!({"commands":[{"command":"text.insert","params":{"text":"?"}},{"command":"no.such","params":{}}],"atomic":true}),
                ),
                true,
            ),
            ("hwp_action", json!({"query":"Bold"}), false),
            ("hwp_action", doc(json!({"id":"MoveLineEnd"})), false),
            ("hwp_action", doc(json!({"id":"NoSuchAction"})), true),
            ("render_page", doc(json!({"page":1,"scale":0.25})), false),
            ("render_page", doc(json!({"page":99})), true),
            // Unsaved changes refuse replacement and closing without force.
            ("new_document", doc(json!({})), true),
            ("close_document", doc(json!({})), true),
            ("save_document", doc(json!({"path":path(&saved)})), false),
            (
                "save_document",
                doc(json!({"path":path(&dir.path().join("x.odt"))})),
                true,
            ),
            ("open_document", json!({"path":path(&saved)}), false),
            (
                "open_document",
                json!({"path":path(&dir.path().join("missing.hwpx"))}),
                true,
            ),
            (
                "open_protected_document",
                json!({"path":path(&protected),"password":"schema-password"}),
                false,
            ),
            (
                "open_protected_document",
                json!({"path":path(&protected),"password":"wrong"}),
                true,
            ),
            ("close_document", doc(json!({"force":true})), false),
            ("get_text", doc(json!({})), true),
        ];
        for (name, args, expect_error) in cases {
            let (is_error, structured) = call(name, args).await?;
            assert_eq!(is_error, expect_error, "{name}: {structured}");
            if is_error {
                assert!(structured["message"].is_string(), "{name}: {structured}");
                failed.insert(name);
            } else {
                succeeded.insert(name);
            }
        }
        for tool in service.tools.iter() {
            assert!(
                succeeded.contains(tool.name.as_ref()),
                "{} never succeeded",
                tool.name
            );
        }
        assert!(failed.len() >= 10, "{failed:?}");
        Ok(())
    }

    fn built<T: DeserializeOwned>(value: Value) -> Result<T> {
        decode(value).map_err(anyhow::Error::msg)
    }

    #[test]
    fn every_built_result_names_the_server_in_meta() -> Result<()> {
        let expected = json!({"name":"hwpcraft","version":hwpcraft_engine::VERSION.trim()});
        let tool = tool_result(tools::result(json!({"pages":1}), false), None)
            .map_err(anyhow::Error::msg)?;
        let lists: Vec<ServerResult> = vec![
            tool.into(),
            ServerResult::ListToolsResult(built(
                json!({"resultType":"complete","tools":[],"ttlMs":1,"cacheScope":"public"}),
            )?),
            ServerResult::ListResourcesResult(built(sdk_catalog::list_resources())?),
            ServerResult::ListResourceTemplatesResult(built(
                sdk_catalog::list_resource_templates(),
            )?),
            ServerResult::ListPromptsResult(built(sdk_catalog::list_prompts())?),
        ];
        for result in lists {
            let value = serde_json::to_value(result)?;
            assert_eq!(value["_meta"][SERVER_INFO], expected, "{value}");
        }
        // Existing protocol metadata is kept; the server entry is only added.
        let kept: CallToolResult =
            built(json!({"resultType":"complete","content":[],"_meta":{"com.example/trace":"t"}}))?;
        let value = serde_json::to_value(kept)?;
        assert_eq!(value["_meta"]["com.example/trace"], "t");
        assert_eq!(value["_meta"][SERVER_INFO], expected);
        Ok(())
    }
}
