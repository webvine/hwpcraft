//! Discovery uses the engine registry, not a second list of executable IDs.
use serde_json::{Value, json};

pub(crate) fn commands(query: &str) -> Value {
    let query = query.to_lowercase();
    let mut seen = std::collections::BTreeSet::new();
    let commands: Vec<_> = hwpcraft_engine::COMMANDS.iter().chain(hwpcraft_engine::extension_commands())
        .filter(|(id,label,_)| seen.insert(*id) && (query.is_empty() || id.to_lowercase().contains(&query) || label.to_lowercase().contains(&query)))
        .map(|(id,label,shortcut)| {
            let (help,example) = hwpcraft_engine::command_help(id).unwrap_or_else(|| parameters(id));
            let actions: Vec<&str> = hwpcraft_engine::HWP_ACTIONS.iter().filter(|(_,cmd,_,_)| *cmd == *id).map(|(a,_,_,_)| *a).collect();
            json!({"id":id,"label":label,"shortcut":shortcut,"paramsHelp":help,"paramsExample":example,"hwpActions":actions})
        }).collect();
    json!(commands)
}

/// Hangul action ids with their HwpCraft command and default parameters.
pub(crate) fn actions(query: &str) -> Value {
    let query = query.to_lowercase();
    let list: Vec<Value> = hwpcraft_engine::HWP_ACTIONS
        .iter()
        .filter(|(id, cmd, _, _)| query.is_empty() || id.to_lowercase().contains(&query) || cmd.contains(&query))
        .map(|(id, cmd, params, status)| {
            json!({"id":id,"command":cmd,"defaultParams":serde_json::from_str::<Value>(params).unwrap_or(json!({})),"mapping":status,"parameterSet":hwpcraft_engine::hwp_parameter_schema(id)})
        })
        .collect();
    json!(list)
}

/// Fallback for a command whose owning module has not documented it yet.
fn parameters(_id: &str) -> (&'static str, Value) {
    (
        "Native engine parameters; see docs/mcp-commands.md. null means the command has no PARAMS entry yet, not that it is unimplemented.",
        Value::Null,
    )
}
