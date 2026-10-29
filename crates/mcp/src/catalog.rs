//! Discovery uses the engine registry, not a second list of executable IDs.
//! Both catalogs are fixed for the process, so each is built once and filtered
//! per request: building them parses every PARAMS example and parameter set.
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    sync::OnceLock,
};

fn all_commands() -> &'static [Value] {
    static COMMANDS: OnceLock<Vec<Value>> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        // One pass over the help tables; looking each id up re-parses them all.
        let mut help: HashMap<_, _> = HashMap::new();
        for (id, text, example) in hwpcraft_engine::command_help_entries() {
            help.entry(id).or_insert((text, example));
        }
        let mut seen = BTreeSet::new();
        hwpcraft_engine::COMMANDS.iter().chain(hwpcraft_engine::extension_commands())
            .filter(|(id, _, _)| seen.insert(*id))
            .map(|(id, label, shortcut)| {
                let (help, example) = help.remove(id).unwrap_or_else(|| parameters(id));
                let actions: Vec<&str> = hwpcraft_engine::HWP_ACTIONS.iter().filter(|(_,cmd,_,_)| *cmd == *id).map(|(a,_,_,_)| *a).collect();
                json!({"id":id,"label":label,"shortcut":shortcut,"paramsHelp":help,"paramsExample":example,"hwpActions":actions})
            }).collect()
    })
}

pub(crate) fn commands(query: &str) -> Value {
    let query = query.to_lowercase();
    let matches = |entry: &&Value| {
        let field = |name| entry[name].as_str().unwrap_or("").to_lowercase();
        query.is_empty() || field("id").contains(&query) || field("label").contains(&query)
    };
    Value::Array(all_commands().iter().filter(matches).cloned().collect())
}

fn all_actions() -> &'static [Value] {
    static ACTIONS: OnceLock<Vec<Value>> = OnceLock::new();
    ACTIONS.get_or_init(|| {
        hwpcraft_engine::HWP_ACTIONS
            .iter()
            .map(|(id, cmd, params, status)| {
                json!({"id":id,"command":cmd,"defaultParams":serde_json::from_str::<Value>(params).unwrap_or(json!({})),"mapping":status,"parameterSet":hwpcraft_engine::hwp_parameter_schema(id)})
            })
            .collect()
    })
}

/// Hangul action ids with their HwpCraft command and default parameters.
pub(crate) fn actions(query: &str) -> Value {
    let query = query.to_lowercase();
    let matches = |entry: &&Value| {
        let id = entry["id"].as_str().unwrap_or("");
        let command = entry["command"].as_str().unwrap_or("");
        query.is_empty() || id.to_lowercase().contains(&query) || command.contains(&query)
    };
    Value::Array(all_actions().iter().filter(matches).cloned().collect())
}

/// Fallback for a command whose owning module has not documented it yet.
fn parameters(_id: &str) -> (&'static str, Value) {
    (
        "Native engine parameters; see docs/mcp-commands.md. null means the command has no PARAMS entry yet, not that it is unimplemented.",
        Value::Null,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cached catalog must equal a direct per-command lookup.
    #[test]
    fn cached_catalog_matches_the_engine_registry() {
        let commands = commands("");
        let entries = commands.as_array().map(Vec::as_slice).unwrap_or_default();
        assert!(entries.len() > 100);
        for entry in entries {
            let id = entry["id"].as_str().unwrap_or_default();
            let (help, example) =
                hwpcraft_engine::command_help(id).unwrap_or_else(|| parameters(id));
            assert_eq!(entry["paramsHelp"], help, "{id}");
            assert_eq!(entry["paramsExample"], example, "{id}");
        }
        assert_eq!(commands, super::commands(""));
        let tables = super::commands("표");
        assert!(
            tables
                .as_array()
                .is_some_and(|t| !t.is_empty() && t.len() < entries.len())
        );
        assert_eq!(
            actions("CharShapeBold").as_array().map(Vec::len),
            Some(
                hwpcraft_engine::HWP_ACTIONS
                    .iter()
                    .filter(
                        |(id, cmd, _, _)| id.to_lowercase().contains("charshapebold")
                            || cmd.contains("charshapebold")
                    )
                    .count()
            )
        );
    }
}
