//! Current-protocol, stateless MCP adapter over isolated shared-engine workers.
use crate::{catalog, document_workers::DocumentWorkers, sdk_catalog, tools};
use anyhow::{Context, Result, bail};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, model::*, service::RequestContext};
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

#[derive(Clone)]
pub(crate) struct HwpService {
    subscription_shutdown: Option<CancellationToken>,
    workers: Arc<DocumentWorkers>,
    tools: Arc<Vec<Tool>>,
    validators: Arc<HashMap<String, jsonschema::Validator>>,
    capabilities: ServerCapabilities,
    completion_window: Arc<Mutex<VecDeque<Instant>>>,
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
            let independent = name == "list_commands";
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
            definition["outputSchema"] = json!({"$schema":DIALECT,"type":"object"});
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
            workers,
            tools: Arc::new(typed),
            validators: Arc::new(validators),
            capabilities: serde_json::from_value(
                json!({"tools":{},"resources":{"subscribe":true},"prompts":{},"completions":{}}),
            )?,
        })
    }

    pub(crate) fn with_subscription_shutdown(mut self, shutdown: CancellationToken) -> Self {
        self.subscription_shutdown = Some(shutdown);
        self
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
fn decode<T: DeserializeOwned>(value: Value) -> std::result::Result<T, McpError> {
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
fn tool_result(value: Value, id: Option<&str>) -> std::result::Result<CallToolResponse, McpError> {
    let is_error = value["isError"].as_bool().unwrap_or(false);
    let source_content = value.get("content").and_then(Value::as_array);
    let text = source_content.and_then(|blocks| {
        blocks
            .iter()
            .find_map(|block| block.get("text").and_then(Value::as_str))
    });
    let data = match text {
        Some(text) => serde_json::from_str(text).unwrap_or_else(|_| json!({"message":text})),
        None => value.clone(),
    };
    let mut structured = match data {
        Value::Object(map) => Value::Object(map),
        Value::Array(items) => json!({"items":items}),
        other => json!({"value":other}),
    };
    if let Some(id) = id {
        structured[DOCUMENT] = json!(id);
    }
    let mut content: Vec<Value> = source_content
        .into_iter()
        .flatten()
        .filter(|block| block["type"] != "text")
        .cloned()
        .collect();
    content.push(json!({"type":"text","text":structured.to_string()}));
    let result: CallToolResult = decode(json!({"resultType":"complete","content":content,
        "structuredContent":structured,"isError":is_error}))?;
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
            .with_server_info(Implementation::new("hwpcraft",hwpcraft_engine::VERSION.trim()))
            .with_instructions("MCP 2026-07-28 authoring server. Every request supplies protocolVersion and clientCapabilities in protocol _meta. new_document/open_document/open_protected_document return document_id; pass it on EVERY document-scoped tool call. No implicit current document. Discover all engine commands through list_commands; use execute, batch and hwp_action to author. Cursor offsets are Unicode scalars; render_page and page.layout.get pages are one-based. Query caret.get for generation before selection.range or addressed edits. Use context.v2 for supported nested regions and tools.autotext.body for formatted snippets. Atomic batches restore document edits and reject persistent file/settings writes. Save HWP/HWPX explicitly, inspect/reopen and render PNG/PDF for verification. Close documents explicitly. Unsupported operations report errors; do not infer full Hancom feature parity from command registration.")
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
        let progress = context.meta.get_progress_token();
        if let Some(token) = progress.clone().filter(|_| !context.ct.is_cancelled()) {
            let _ = context
                .peer
                .notify_progress(ProgressNotificationParam::new(token, 0.0))
                .await;
        }
        let outcome = self.dispatch(name, args, context.ct.clone()).await;
        if let Some(token) = progress.filter(|_| !context.ct.is_cancelled()) {
            let _ = context
                .peer
                .notify_progress(ProgressNotificationParam::new(token, 1.0))
                .await;
        }
        let (id, value) = outcome.unwrap_or_else(|error| {
            (
                None,
                tools::result(json!({"message":error.to_string()}), true),
            )
        });
        tool_result(value, id.as_deref())
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
                    ErrorCode(-32901),
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
}
