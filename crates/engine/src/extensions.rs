//! Owner: core area. Routes command ids to per-owner extension modules so
//! parallel sessions add commands without editing `protocol.rs`.
use super::*;

pub fn extension_commands()
-> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    ext_format::COMMANDS
        .iter()
        .chain(ext_table::COMMANDS)
        .chain(table_drawing::COMMANDS)
        .chain(ext_object::COMMANDS)
        .chain(ext_page::COMMANDS)
}

type Handler = fn(&mut Session, &str, &Value) -> Option<Result<Value>>;

impl Session {
    pub(crate) fn run_extension(&mut self, id: &str, params: Value) -> Result<Value> {
        let handlers: [Handler; 5] = [
            Self::run_format_ext,
            Self::run_table_ext,
            Self::run_drawing_ext,
            Self::run_object_ext,
            Self::run_page_ext,
        ];
        for handler in handlers {
            if let Some(result) = handler(self, id, &params) {
                return result;
            }
        }
        bail!("알 수 없는 편집 명령: {id}")
    }
}
