//! Declared `outputSchema` for each tool's `structuredContent`.
//!
//! Schemas list the fields every result carries and leave objects open, so a
//! later field addition is not a breaking change. A tool execution error
//! (`isError: true`) carries at least `message`; the error branch keeps such
//! results conforming too.
use serde_json::{Value, json};

fn integer() -> Value {
    json!({"type":"integer","minimum":0})
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

fn cursor() -> Value {
    let cell = object(
        json!({"control":integer(),"index":integer(),"paragraph":integer()}),
        &["control", "index", "paragraph"],
    );
    object(
        json!({"section":integer(),"paragraph":integer(),"offset":integer(),
            "cell":{"anyOf":[{"type":"null"},cell]}}),
        &["section", "paragraph", "offset"],
    )
}

fn nullable(schema: Value) -> Value {
    json!({"anyOf":[{"type":"null"},schema]})
}

/// `Session::info`, returned by open, new, save and most editing commands.
fn document_summary() -> (Value, &'static [&'static str]) {
    (
        json!({"pages":nullable(integer()),"layoutError":nullable(json!({"type":"string"})),
            "sections":integer(),"paragraphs":integer(),"dirty":{"type":"boolean"},
            "path":nullable(json!({"type":"string"}))}),
        &[
            "pages",
            "layoutError",
            "sections",
            "paragraphs",
            "dirty",
            "path",
        ],
    )
}

/// Add the handle every document-scoped result carries.
fn with_document(mut schema: Value) -> Value {
    schema["properties"]["document_id"] = json!({"type":"string","minLength":1});
    if let Some(required) = schema["required"].as_array_mut() {
        required.push(json!("document_id"));
    }
    schema
}

fn summary_result() -> Value {
    let (properties, required) = document_summary();
    with_document(object(properties, required))
}

fn page_window(extra: Value, required: &[&str]) -> Value {
    let mut properties = json!({"total":integer(),"start":integer(),"nextStart":nullable(integer()),
        "scope":{"type":"string"},"offsetUnit":{"const":"unicode_scalar"}});
    for (key, value) in extra.as_object().into_iter().flatten() {
        properties[key] = value.clone();
    }
    let mut all = vec!["total", "start", "nextStart", "scope", "offsetUnit"];
    all.extend_from_slice(required);
    with_document(object(properties, &all))
}

/// The success shape of a tool's `structuredContent`, or `None` when it is not
/// declared. Parts whose shape depends on the engine command stay unconstrained.
pub(crate) fn success(tool: &str) -> Option<Value> {
    let schema = match tool {
        "list_commands" => object(
            json!({"items":{"type":"array","items":object(
                json!({"id":{"type":"string"},"label":{"type":"string"},"shortcut":{"type":"string"},
                    "paramsHelp":{"type":"string"},"paramsExample":{},
                    "hwpActions":{"type":"array","items":{"type":"string"}}}),
                &["id", "label", "shortcut", "paramsHelp", "paramsExample", "hwpActions"],
            )}}),
            &["items"],
        ),
        // The engine command decides the body; only the handle is common.
        "execute" => with_document(object(json!({}), &[])),
        "batch" => with_document(object(
            json!({"results":{"type":"array","items":object(
                json!({"command":{"type":"string"},"ok":{"type":"boolean"},"result":{},"error":{"type":"string"}}),
                &["command", "ok"],
            )},"rolledBack":{"type":"boolean"}}),
            &["results", "rolledBack"],
        )),
        "new_document"
        | "open_document"
        | "open_protected_document"
        | "save_document"
        | "type_text" => summary_result(),
        "select_text" => with_document(object(
            json!({"cursor":cursor(),"anchor":nullable(integer())}),
            &["cursor", "anchor"],
        )),
        "get_text" => page_window(json!({"text":{"type":"string"}}), &["text"]),
        "inspect_document" => {
            let (properties, required) = document_summary();
            let block = object(
                json!({"cursor":cursor(),"text":{"type":"string"},"characters":integer(),
                    "styleId":integer(),"paragraphShapeId":integer(),
                    "charShapes":{"type":"array"},"controlCount":integer()}),
                &[
                    "cursor",
                    "text",
                    "characters",
                    "styleId",
                    "paragraphShapeId",
                    "charShapes",
                    "controlCount",
                ],
            );
            page_window(
                json!({"document":object(properties, required),
                    "blocks":{"type":"array","items":block},
                    "selection":object(
                        json!({"cursor":cursor(),"anchor":nullable(integer()),"spanAnchor":nullable(cursor())}),
                        &["cursor", "anchor", "spanAnchor"],
                    ),
                    "canUndo":{"type":"boolean"},"canRedo":{"type":"boolean"}}),
                &["document", "blocks", "selection", "canUndo", "canRedo"],
            )
        }
        "hwp_action" => {
            let listed = object(
                json!({"items":{"type":"array","items":object(
                    json!({"id":{"type":"string"},"command":{"type":"string"},"defaultParams":{"type":"object"},
                        "mapping":{"type":"string"},"parameterSet":{}}),
                    &["id", "command", "defaultParams", "mapping", "parameterSet"],
                )}}),
                &["items"],
            );
            let ran = with_document(object(
                json!({"action":{"type":"string"},"command":{"type":"string"},"mapping":{"type":"string"},"result":{}}),
                &["action", "command", "mapping", "result"],
            ));
            json!({"type":"object","anyOf":[listed, ran]})
        }
        "get_selection" => with_document(object(
            json!({"cursor":cursor(),"anchor":nullable(integer()),"spanAnchor":nullable(cursor()),
                "hasSelection":{"type":"boolean"},"text":{"type":"string"}}),
            &["cursor", "anchor", "spanAnchor", "hasSelection", "text"],
        )),
        "render_page" => with_document(object(
            json!({"page":{"type":"integer","minimum":1},"pages":{"type":"integer","minimum":1},
                "width":{"type":"integer","minimum":1},"height":{"type":"integer","minimum":1}}),
            &["page", "pages", "width", "height"],
        )),
        "close_document" => with_document(object(json!({"closed":{"const":true}}), &["closed"])),
        _ => return None,
    };
    Some(schema)
}

/// The `structuredContent` of every `isError: true` result.
pub(crate) fn failure() -> Value {
    object(json!({"message":{"type":"string"}}), &["message"])
}

/// The declared `outputSchema`: the success shape or a tool execution error.
pub(crate) fn declared(tool: &str, dialect: &str) -> Option<Value> {
    success(tool)
        .map(|success| json!({"$schema":dialect,"type":"object","anyOf":[success, failure()]}))
}
