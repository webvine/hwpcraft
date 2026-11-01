//! Every command is listed once, so the CLI `commands` list, MCP `list_commands`
//! and the documentation count the same commands.
use std::collections::HashSet;

#[test]
fn every_command_is_listed_once() {
    let mut seen = HashSet::new();
    let repeated: Vec<&str> = hwpcraft_engine::COMMANDS
        .iter()
        .chain(hwpcraft_engine::extension_commands())
        .map(|(id, _, _)| *id)
        .filter(|id| !seen.insert(*id))
        .collect();
    assert!(repeated.is_empty(), "listed twice: {repeated:?}");
}
