//! Owner: core area. Routes command ids to per-owner extension modules so
//! parallel sessions add commands without editing `protocol.rs`.
use super::*;

pub fn extension_commands()
-> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    caret::COMMANDS
        .iter()
        .chain(ext_format::COMMANDS)
        .chain(ext_table::COMMANDS)
        .chain(table_drawing::COMMANDS)
        .chain(ext_object::COMMANDS)
        .chain(ext_page::COMMANDS)
        .chain(hanja::COMMANDS)
        .chain(review::COMMANDS)
        .chain(autotext::COMMANDS)
}

/// Every documented command: (id, parameter help, JSON example).
pub fn command_help_entries() -> impl Iterator<Item = (&'static str, &'static str, Value)> {
    protocol::PARAMS
        .iter()
        .chain(caret::PARAMS)
        .chain(ext_format::PARAMS)
        .chain(ext_table::PARAMS)
        .chain(table_drawing::PARAMS)
        .chain(ext_object::PARAMS)
        .chain(ext_page::PARAMS)
        .chain(hanja::PARAMS)
        .chain(review::PARAMS)
        .chain(autotext::PARAMS)
        .map(|(id, help, example)| {
            (
                *id,
                *help,
                serde_json::from_str(example).unwrap_or(Value::Null),
            )
        })
}

/// Parameter help and a JSON example for a command, from the owning module's
/// `PARAMS` table. `None` when the module has not documented the command yet.
pub fn command_help(id: &str) -> Option<(&'static str, Value)> {
    command_help_entries()
        .find(|(cmd, _, _)| *cmd == id)
        .map(|(_, help, example)| (help, example))
}

type Handler = fn(&mut Session, &str, &Value) -> Option<Result<Value>>;

impl Session {
    pub(crate) fn run_extension(&mut self, id: &str, params: Value) -> Result<Value> {
        let handlers: [Handler; 9] = [
            Self::run_caret_ext,
            Self::run_format_ext,
            Self::run_table_ext,
            Self::run_drawing_ext,
            Self::run_object_ext,
            Self::run_page_ext,
            Self::run_hanja_ext,
            Self::run_review_ext,
            Self::run_autotext_ext,
        ];
        for handler in handlers {
            if let Some(result) = handler(self, id, &params) {
                return result;
            }
        }
        bail!("알 수 없는 편집 명령: {id}")
    }
}
