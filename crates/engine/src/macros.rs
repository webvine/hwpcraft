//! Owner: core area. Key macros (한글 매크로 정의 / 매크로 실행).
//!
//! While recording, every edit that reaches the session from outside (a desktop
//! key or ribbon command, an automation `run`, a caret or cell move) is appended
//! as one step in the CLI `edit` format, `{"command": id, "params": {...}}`.
//! Commands that run inside another command are not recorded again, and queries,
//! file commands and macro commands are never recorded. Consecutive plain typing
//! becomes one `text.insert` step.
//!
//! Running a macro replays its steps through `Session::run` (so each step is one
//! ordinary undo step, as in Hangul). If a step fails, the whole run is rolled
//! back, document and history, and the error names the step.
//!
//! Saved macros live in `macros.json` in the settings folder; the file is written
//! atomically through `file_access`, so automation roots and atomic batches that
//! forbid writes apply.
use super::*;
use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "macro.record.start",
        "매크로 기록 시작 (매크로 정의)",
        "Alt+Shift+H",
    ),
    (
        "macro.record.stop",
        "매크로 기록 끝 (이름을 주면 저장)",
        "Alt+Shift+H",
    ),
    ("macro.record.status", "매크로 기록 상태", ""),
    (
        "macro.run",
        "매크로 실행 (저장한 이름 또는 단계 목록)",
        "Alt+Shift+L",
    ),
    ("macro.list", "저장한 매크로 목록", ""),
    ("macro.get", "저장한 매크로 단계 보기", ""),
    ("macro.save", "매크로 저장 (단계 목록)", ""),
    ("macro.delete", "저장한 매크로 지우기", ""),
    (
        "caret.move_cell",
        "표 다음/이전 셀로 이동",
        "Tab / Shift+Tab",
    ),
];

pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "macro.record.start",
        "매개 변수 없음. 이후 편집(입력·명령·캐럿과 셀 이동)을 기록합니다. 이미 기록 중이면 실패합니다.",
        "{}",
    ),
    (
        "macro.record.stop",
        "name(선택, 1–40자)을 주면 기록을 그 이름으로 저장하며 같은 이름은 바꿉니다. CLI edit 형식의 steps를 돌려줍니다.",
        r#"{"name":"머리말 넣기"}"#,
    ),
    (
        "macro.record.status",
        "매개 변수 없음. 기록 중인지와 지금까지 단계 수.",
        "{}",
    ),
    (
        "macro.run",
        "저장한 매크로의 name, 또는 steps [{command, params}](1–1000). repeat 1–100(기본 1). 단계마다 실행 취소 한 단계이며, 한 단계라도 실패하면 실행 전체를 되돌립니다.",
        r#"{"name":"머리말 넣기","repeat":1}"#,
    ),
    (
        "macro.list",
        "매개 변수 없음. 저장한 매크로 이름과 단계 수.",
        "{}",
    ),
    (
        "macro.get",
        "name: 저장한 매크로 이름. 그 단계들을 돌려줍니다.",
        r#"{"name":"머리말 넣기"}"#,
    ),
    (
        "macro.save",
        "name과 steps [{command, params}](1–1000). 같은 이름의 매크로는 바꿉니다. 64개까지 저장합니다.",
        r#"{"name":"굵게 제목","steps":[{"command":"text.insert","params":{"text":"제목"}}]}"#,
    ),
    (
        "macro.delete",
        "name: 지울 매크로 이름.",
        r#"{"name":"머리말 넣기"}"#,
    ),
    (
        "caret.move_cell",
        "backward가 false면 다음 표 셀(Tab), true면 앞 셀(Shift+Tab)로 갑니다. 캐럿이 표 안에 있어야 합니다.",
        r#"{"backward":false}"#,
    ),
];

const MAX_STEPS: usize = 1000;
const MAX_REPEAT: u32 = 100;
const MAX_MACROS: usize = 64;
const MAX_NAME: usize = 40;
const FILE_NAME: &str = "macros.json";

/// One recorded edit, in the CLI `edit` command-file format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroStep {
    pub command: String,
    #[serde(default = "empty_object")]
    pub params: Value,
}

fn empty_object() -> Value {
    json!({})
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedMacro {
    pub name: String,
    pub steps: Vec<MacroStep>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Store {
    schema_version: u32,
    macros: Vec<SavedMacro>,
}

/// Recording state carried by a session (and across `open`).
#[derive(Default)]
pub(crate) struct MacroRecorder {
    steps: Option<Vec<MacroStep>>,
    /// Nesting of public command entry points; only depth 0 is recorded.
    depth: u32,
}

/// Commands that never become macro steps: queries, files, settings, macros.
fn recordable(id: &str) -> bool {
    !(id.starts_with("macro.")
        // Settings writes (equation macros, user table styles) and layout queries.
        || id.contains(".macro.")
        || matches!(
            id,
            "table.user_style.save"
                | "table.user_style.delete"
                | "format.style.set.save"
                | "format.style.set.delete"
                | "equation.layout"
        )
        || id.starts_with("file.")
        || id.ends_with(".get")
        || id.ends_with(".list")
        || id.ends_with(".status")
        || matches!(
            id,
            "document.text"
                | "document.inspect"
                | "document.statistics"
                | "document.outline"
                | "select.get"
                | "caret.get"
                | "edit.copy"
                | "edit.clipboard.get"
                | "edit.find_all"
                | "symbol.sets"
                | "symbol.special"
                | "symbol.search"
                | "picture.save"
                | "picture.save_all"
                | "format.style.export"
        )
        || id.starts_with("tools.autotext.")
        || id.starts_with("context."))
}

fn check_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let count = name.chars().count();
    if count == 0 || count > MAX_NAME || name.chars().any(char::is_control) {
        bail!("매크로 이름은 제어 문자 없이 1–{MAX_NAME}자입니다.");
    }
    Ok(name)
}

fn check_steps(steps: &[MacroStep]) -> Result<()> {
    if steps.is_empty() || steps.len() > MAX_STEPS {
        bail!("매크로는 1–{MAX_STEPS}개 단계입니다.");
    }
    for (i, step) in steps.iter().enumerate() {
        if !step.params.is_object() {
            bail!("매크로 {}번째 단계의 params는 JSON 객체여야 합니다.", i + 1);
        }
        if !recordable(&step.command) {
            bail!(
                "매크로 {}번째 단계 {}는 매크로에 넣을 수 없는 명령입니다(조회·파일·설정·매크로 명령).",
                i + 1,
                step.command
            );
        }
    }
    Ok(())
}

fn store_path() -> Result<PathBuf> {
    crate::autotext::config_dir()
        .map(|dir| dir.join(FILE_NAME))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못해 매크로를 저장할 수 없습니다."))
}

fn load_store() -> Result<Store> {
    let path = store_path()?;
    hwpcraft_formats::file_access::check_read(&path.to_string_lossy())
        .map_err(anyhow::Error::msg)?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(e) => return Err(e.into()),
    };
    let store: Store = serde_json::from_slice(&bytes)
        .with_context(|| format!("매크로 파일을 읽지 못했습니다: {}", path.display()))?;
    if store.schema_version != 1 {
        bail!("지원하지 않는 매크로 파일 형식입니다.");
    }
    Ok(store)
}

fn save_store(mut store: Store) -> Result<()> {
    let path = store_path()?;
    hwpcraft_formats::file_access::check_write(&path.to_string_lossy())
        .map_err(anyhow::Error::msg)?;
    store.schema_version = 1;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    hwpcraft_formats::atomic_write(&path, &serde_json::to_vec_pretty(&store)?)
}

/// Saved macros, in saving order.
pub fn saved_macros() -> Result<Vec<SavedMacro>> {
    Ok(load_store()?.macros)
}

/// Save (or replace) a macro by name.
pub fn save_macro(name: &str, steps: Vec<MacroStep>) -> Result<()> {
    let name = check_name(name)?.to_owned();
    check_steps(&steps)?;
    let mut store = load_store()?;
    store.macros.retain(|m| m.name != name);
    if store.macros.len() >= MAX_MACROS {
        bail!("매크로는 {MAX_MACROS}개까지 저장할 수 있습니다. 쓰지 않는 매크로를 지우세요.");
    }
    store.macros.push(SavedMacro { name, steps });
    save_store(store)
}

/// Delete a saved macro; fails when there is none of that name.
pub fn delete_macro(name: &str) -> Result<()> {
    let mut store = load_store()?;
    let before = store.macros.len();
    store.macros.retain(|m| m.name != name);
    if store.macros.len() == before {
        bail!("저장한 매크로가 없습니다: {name}");
    }
    save_store(store)
}

fn saved_steps(name: &str) -> Result<Vec<MacroStep>> {
    saved_macros()?
        .into_iter()
        .find(|m| m.name == name)
        .map(|m| m.steps)
        .ok_or_else(|| anyhow!("저장한 매크로가 없습니다: {name}"))
}

/// The step a public `Command` stands for (the ids `Session::run` maps to it).
fn command_step(command: &Command) -> Option<MacroStep> {
    let (id, params) = match command {
        Command::Insert(text) => ("text.insert", json!({ "text": text })),
        Command::Backspace => ("text.backspace", json!({})),
        Command::Delete => ("text.delete", json!({})),
        Command::Split => ("paragraph.split", json!({})),
        Command::PageBreak => ("insert.page_break", json!({})),
        Command::Undo => ("edit.undo", json!({})),
        Command::Redo => ("edit.redo", json!({})),
        Command::Character(value) => ("format.character", value.clone()),
        Command::Paragraph(value) => ("format.paragraph", value.clone()),
        Command::Table { rows, columns } => {
            ("insert.table", json!({ "rows": rows, "columns": columns }))
        }
        Command::PageSetup(setup) => ("page.setup", serde_json::to_value(setup).ok()?),
    };
    Some(MacroStep {
        command: id.to_owned(),
        params,
    })
}

impl Session {
    /// Number of steps recorded so far, or `None` when not recording.
    pub fn macro_recording(&self) -> Option<usize> {
        self.macros.steps.as_ref().map(Vec::len)
    }

    pub fn start_macro_recording(&mut self) -> Result<()> {
        if self.macros.steps.is_some() {
            bail!("이미 매크로를 기록하고 있습니다.");
        }
        self.macros.steps = Some(Vec::new());
        Ok(())
    }

    /// Stop recording and return the steps (possibly none).
    pub fn stop_macro_recording(&mut self) -> Result<Vec<MacroStep>> {
        self.macros
            .steps
            .take()
            .ok_or_else(|| anyhow!("매크로를 기록하고 있지 않습니다."))
    }

    /// Run `steps` `repeat` times. On failure everything the run changed, document
    /// and history, is restored and the error names the failing step.
    pub fn run_macro(&mut self, steps: &[MacroStep], repeat: u32) -> Result<Value> {
        // Its steps are not recorded, even when the desktop calls this directly.
        self.recorded(|| None, |s| s.run_macro_steps(steps, repeat))
    }

    fn run_macro_steps(&mut self, steps: &[MacroStep], repeat: u32) -> Result<Value> {
        check_steps(steps)?;
        if !(1..=MAX_REPEAT).contains(&repeat) {
            bail!("repeat는 1–{MAX_REPEAT}입니다.");
        }
        // Inside an atomic MCP batch the batch already owns the only full scope
        // and rolls back for us; then run without one of our own.
        let checkpoint = self.full_checkpoint().ok();
        let mut failure = None;
        'runs: for round in 0..repeat {
            for (i, step) in steps.iter().enumerate() {
                if let Err(e) = self.run(&step.command, step.params.clone()) {
                    failure = Some(anyhow!(
                        "매크로 {}번째 단계 {} 실패{}: {e:#}",
                        i + 1,
                        step.command,
                        if repeat > 1 {
                            format!(" ({}번째 반복)", round + 1)
                        } else {
                            String::new()
                        }
                    ));
                    break 'runs;
                }
            }
        }
        match (failure, checkpoint) {
            (Some(e), Some(checkpoint)) => {
                self.rollback_full(checkpoint)?;
                Err(e.context("매크로 실행 전 상태로 되돌렸습니다."))
            }
            (Some(e), None) => Err(e),
            (None, checkpoint) => {
                if let Some(checkpoint) = checkpoint {
                    self.release_full(checkpoint)?;
                }
                Ok(json!({"steps": steps.len(), "repeat": repeat, "info": self.info()}))
            }
        }
    }

    /// True when an edit entering through a public entry point now is recorded.
    fn recording_top(&self) -> bool {
        self.macros.steps.is_some() && self.macros.depth == 0
    }

    fn push_step(&mut self, step: MacroStep) {
        let Some(steps) = self.macros.steps.as_mut() else {
            return;
        };
        // Plain typing joins the previous typing step.
        if step.command == "text.insert"
            && let Some(last) = steps.last_mut()
            && last.command == "text.insert"
            && last.params.as_object().is_some_and(|o| o.len() == 1)
            && step.params.as_object().is_some_and(|o| o.len() == 1)
            && let (Some(a), Some(b)) = (last.params["text"].as_str(), step.params["text"].as_str())
            && !a.contains(['\n', '\r'])
            && !b.contains(['\n', '\r'])
        {
            last.params = json!({ "text": format!("{a}{b}") });
            return;
        }
        if steps.len() < MAX_STEPS {
            steps.push(step);
        }
    }

    /// Run `f` as a public command entry point: commands it runs internally are not
    /// recorded, and `step` is recorded when `f` succeeds at the top level.
    pub(crate) fn recorded<T>(
        &mut self,
        step: impl FnOnce() -> Option<MacroStep>,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let step = self.recording_top().then(step).flatten();
        self.macros.depth += 1;
        let result = f(self);
        self.macros.depth -= 1;
        if result.is_ok()
            && let Some(step) = step
        {
            self.push_step(step);
        }
        result
    }

    pub(crate) fn record_run(&mut self, id: &str, params: &Value) -> Option<MacroStep> {
        (self.recording_top() && recordable(id)).then(|| MacroStep {
            command: id.to_owned(),
            params: params.clone(),
        })
    }

    pub(crate) fn record_command(&self, command: &Command) -> Option<MacroStep> {
        if self.recording_top() {
            command_step(command)
        } else {
            None
        }
    }

    pub(crate) fn take_macro_recorder(&mut self) -> MacroRecorder {
        std::mem::take(&mut self.macros)
    }

    pub(crate) fn run_macro_ext(&mut self, id: &str, p: &Value) -> Option<Result<Value>> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Name {
            name: String,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Stop {
            #[serde(default)]
            name: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Run {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            steps: Option<Vec<MacroStep>>,
            #[serde(default = "one")]
            repeat: u32,
        }
        fn one() -> u32 {
            1
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Save {
            name: String,
            steps: Vec<MacroStep>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Cell {
            #[serde(default)]
            backward: bool,
        }
        fn none(id: &str, p: &Value) -> Result<()> {
            if p.as_object().is_some_and(|o| o.is_empty()) {
                Ok(())
            } else {
                bail!("{id} 명령에는 매개 변수가 없습니다.")
            }
        }
        let parse = |p: &Value| p.clone();
        Some(match id {
            "macro.record.start" => none(id, p).and_then(|()| {
                self.start_macro_recording()?;
                Ok(json!({"recording": true}))
            }),
            "macro.record.stop" => serde_json::from_value::<Stop>(parse(p))
                .map_err(Into::into)
                .and_then(|s| {
                    if let Some(name) = &s.name {
                        check_name(name)?;
                    }
                    let steps = self.stop_macro_recording()?;
                    let saved = match s.name {
                        Some(name) if !steps.is_empty() => {
                            save_macro(&name, steps.clone())?;
                            Some(name.trim().to_owned())
                        }
                        _ => None,
                    };
                    Ok(json!({"recording": false, "steps": steps, "saved": saved}))
                }),
            "macro.record.status" => none(id, p).map(|()| {
                json!({"recording": self.macro_recording().is_some(),
                       "steps": self.macro_recording().unwrap_or(0)})
            }),
            "macro.run" => serde_json::from_value::<Run>(parse(p))
                .map_err(Into::into)
                .and_then(|r| {
                    let steps = match (r.name, r.steps) {
                        (Some(name), None) => saved_steps(&name)?,
                        (None, Some(steps)) => steps,
                        _ => bail!("macro.run에는 name 또는 steps 중 하나만 주세요."),
                    };
                    self.run_macro(&steps, r.repeat)
                }),
            "macro.list" => none(id, p).and_then(|()| {
                Ok(json!({"macros": saved_macros()?
                    .iter()
                    .map(|m| json!({"name": m.name, "steps": m.steps.len()}))
                    .collect::<Vec<_>>()}))
            }),
            "macro.get" => serde_json::from_value::<Name>(parse(p))
                .map_err(Into::into)
                .and_then(|n| Ok(json!({"name": n.name, "steps": saved_steps(&n.name)?}))),
            "macro.save" => serde_json::from_value::<Save>(parse(p))
                .map_err(Into::into)
                .and_then(|s| {
                    let count = s.steps.len();
                    save_macro(&s.name, s.steps)?;
                    Ok(json!({"saved": s.name.trim(), "steps": count}))
                }),
            "macro.delete" => serde_json::from_value::<Name>(parse(p))
                .map_err(Into::into)
                .and_then(|n| {
                    delete_macro(&n.name)?;
                    Ok(json!({"deleted": n.name}))
                }),
            "caret.move_cell" => serde_json::from_value::<Cell>(parse(p))
                .map_err(Into::into)
                .and_then(|c| {
                    self.move_cell(c.backward)?;
                    Ok(json!({"cursor": self.cursor}))
                }),
            _ => return None,
        })
    }
}
