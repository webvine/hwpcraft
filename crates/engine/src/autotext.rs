//! Owner: review-tools area. 상용구 (글 상용구): user text snippets kept in the settings
//! folder as `autotext.json`, inserted by name or expanded from an abbreviation (Alt+I).
//! Other areas must not edit this file; request changes through the owner.
//!
//! Only text snippets are supported (Hangul's 글 상용구). 본문 상용구 with formatting,
//! tables or objects is not stored. Automatic text such as dates and page numbers is
//! `insert.auto_text`, not this module.
use super::*;
use anyhow::Context;
use serde::{Deserialize, Serialize};

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("tools.autotext.list", "상용구 목록", "Ctrl+F3"),
    (
        "tools.autotext.add",
        "상용구 등록 (이름·준말·내용 또는 선택 영역)",
        "",
    ),
    ("tools.autotext.delete", "상용구 지우기", ""),
    ("tools.autotext.insert", "상용구 넣기 (이름)", ""),
    (
        "tools.autotext.expand",
        "상용구 바로 넣기 (커서 앞 준말·이름을 내용으로 바꿈)",
        "Alt+I",
    ),
];
/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "tools.autotext.list",
        "No parameters; entries[] with name, abbreviation, text and the store path.",
        "{}",
    ),
    (
        "tools.autotext.add",
        "name(필수, 64자 이하), abbreviation(선택, 준말: 공백 없는 32자 이하), text(선택: 생략하면 현재 선택 영역의 텍스트), overwrite(선택: 같은 이름을 덮어씀). 글 상용구만 저장하며 서식·표·개체는 저장하지 않습니다.",
        r#"{"name":"인사말","abbreviation":"인사","text":"안녕하십니까. 귀사의 무궁한 발전을 기원합니다."}"#,
    ),
    (
        "tools.autotext.delete",
        "name(필수).",
        r#"{"name":"인사말"}"#,
    ),
    (
        "tools.autotext.insert",
        "name(필수): 상용구 내용을 커서 위치에 넣습니다(선택 영역은 바뀜). 줄 바꿈은 문단 나누기가 됩니다.",
        r#"{"name":"인사말"}"#,
    ),
    (
        "tools.autotext.expand",
        "No parameters; 커서 바로 앞 낱말과 같은 준말 또는 이름의 상용구로 바꿉니다.",
        "{}",
    ),
];

const MAX_ENTRIES: usize = 1_000;
const MAX_TEXT: usize = 100_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct AutoText {
    pub name: String,
    /// 준말 typed before Alt+I.
    pub abbreviation: String,
    pub text: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Store {
    entries: Vec<AutoText>,
}

static CONFIG_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Use `dir` as the settings folder for the rest of the process (tests and embedders).
/// Returns false when a folder was already fixed.
pub fn set_config_dir(dir: PathBuf) -> bool {
    CONFIG_DIR_OVERRIDE.set(dir).is_ok()
}
/// Settings folder shared by the desktop app, CLI and MCP: `%APPDATA%\HwpCraft` on
/// Windows, `~/Library/Application Support/HwpCraft` on macOS, and
/// `$XDG_CONFIG_HOME/HwpCraft` (default `~/.config/HwpCraft`) on Linux and other
/// Unix systems. `set_config_dir` or `HWPCRAFT_CONFIG_DIR` overrides it.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = CONFIG_DIR_OVERRIDE.get() {
        return Some(dir.clone());
    }
    if let Some(dir) = std::env::var_os("HWPCRAFT_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    default_config_dir(std::env::consts::OS, &|name| std::env::var_os(name), &|p| {
        p.is_dir()
    })
}

/// The platform settings folder for `os` (`std::env::consts::OS`), reading the
/// environment through `var` so every platform's rule is testable anywhere.
fn default_config_dir(
    os: &str,
    var: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    is_dir: &dyn Fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    let non_empty = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = non_empty("HOME");
    let base = match os {
        "windows" => non_empty("APPDATA")
            .or_else(|| non_empty("USERPROFILE").map(|h| h.join("AppData").join("Roaming")))?,
        "macos" => {
            let home = home?;
            // Builds before 0.17 used ~/.config on macOS too; keep using it if it exists.
            let legacy = home.join(".config").join("HwpCraft");
            if is_dir(&legacy) {
                return Some(legacy);
            }
            home.join("Library").join("Application Support")
        }
        _ => non_empty("XDG_CONFIG_HOME").or_else(|| home.map(|h| h.join(".config")))?,
    };
    Some(base.join("HwpCraft"))
}
fn store_path() -> Result<PathBuf> {
    config_dir()
        .map(|d| d.join("autotext.json"))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못해 상용구를 저장할 수 없습니다."))
}
fn load() -> Result<(PathBuf, Vec<AutoText>)> {
    load_path(store_path()?)
}
/// Updates need both authorities before even reading an existing store.
fn load_for_update() -> Result<(PathBuf, Vec<AutoText>)> {
    let path = store_path()?;
    check_store_write(&path)?;
    load_path(path)
}
fn load_path(path: PathBuf) -> Result<(PathBuf, Vec<AutoText>)> {
    hwpcraft_formats::file_access::check_read(&path.to_string_lossy())
        .map_err(anyhow::Error::msg)?;
    let entries = match std::fs::read(&path) {
        Ok(bytes) => {
            serde_json::from_slice::<Store>(&bytes)
                .with_context(|| format!("상용구 파일을 읽지 못했습니다: {}", path.display()))?
                .entries
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => bail!("상용구 파일을 읽지 못했습니다: {} ({e})", path.display()),
    };
    Ok((path, entries))
}
fn check_store_write(path: &Path) -> Result<()> {
    hwpcraft_formats::file_access::check_write(&path.to_string_lossy())
        .map_err(anyhow::Error::msg)?;
    // An existing temporary-file link must not redirect the write outside the root.
    hwpcraft_formats::file_access::check_write(&path.with_extension("json.tmp").to_string_lossy())
        .map_err(anyhow::Error::msg)
}
/// Write through a temporary file so a failed save never truncates the store.
fn save(path: &Path, entries: &[AutoText]) -> Result<()> {
    check_store_write(path)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("설정 폴더를 만들지 못했습니다: {}", dir.display()))?;
    }
    let json = serde_json::to_vec_pretty(&Store {
        entries: entries.to_vec(),
    })?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, json)
        .with_context(|| format!("상용구 파일을 쓰지 못했습니다: {}", temp.display()))?;
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        bail!("상용구 파일을 바꾸지 못했습니다: {} ({e})", path.display());
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    let n = name.chars().count();
    if name.trim().is_empty() || n > 64 || name.chars().any(char::is_control) {
        bail!("상용구 이름은 1–64자이며 줄 바꿈을 넣을 수 없습니다.");
    }
    Ok(())
}
fn validate_abbreviation(abbr: &str) -> Result<()> {
    if abbr.chars().count() > 32 || abbr.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("준말은 공백 없이 32자 이하여야 합니다.");
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Add {
    name: String,
    #[serde(default)]
    abbreviation: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    overwrite: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nothing {}

fn entries_json(path: &Path, entries: &[AutoText]) -> Value {
    json!({
        "path": path,
        "entries": entries.iter().map(|e| json!({
            "name": e.name, "abbreviation": e.abbreviation, "text": e.text,
        })).collect::<Vec<_>>(),
    })
}

/// The word (letters, digits, Hangul, hanja, `_`) ending at `offset`.
fn word_before(text: &str, offset: usize) -> Range<usize> {
    let chars: Vec<char> = text.chars().collect();
    let end = offset.min(chars.len());
    let mut start = end;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    start..end
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_autotext_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_autotext_command(id, params.clone()))
    }

    fn insert_autotext(&mut self, text: &str) -> Result<Value> {
        self.guard_protected()?;
        let text = text.to_owned();
        self.transaction(|s| s.insert(&text))?;
        Ok(json!({"inserted": text, "cursor": self.cursor}))
    }

    fn run_autotext_command(&mut self, id: &str, params: Value) -> Result<Value> {
        match id {
            "tools.autotext.list" => {
                let _: Nothing = serde_json::from_value(params)?;
                let (path, entries) = load()?;
                Ok(entries_json(&path, &entries))
            }
            "tools.autotext.add" => {
                let p: Add = serde_json::from_value(params)?;
                validate_name(&p.name)?;
                validate_abbreviation(&p.abbreviation)?;
                let text = match p.text {
                    Some(t) => t,
                    None => self.selected_text(),
                };
                if text.is_empty() {
                    bail!("상용구로 등록할 내용이 없습니다. 글을 선택하거나 text를 지정하세요.");
                }
                if text.len() > MAX_TEXT {
                    bail!("상용구 내용은 100 KB 이하입니다.");
                }
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                let (path, mut entries) = load_for_update()?;
                let name = p.name.trim().to_owned();
                // One 준말 per entry, also when an overwrite changes it: Alt+I would
                // otherwise silently pick whichever entry comes first.
                if !p.abbreviation.is_empty()
                    && entries
                        .iter()
                        .any(|e| e.name != name && e.abbreviation == p.abbreviation)
                {
                    bail!("이미 쓰고 있는 준말입니다: {}", p.abbreviation);
                }
                if let Some(e) = entries.iter_mut().find(|e| e.name == name) {
                    if !p.overwrite {
                        bail!(
                            "이미 있는 상용구 이름입니다: {name}. overwrite로 덮어쓸 수 있습니다."
                        );
                    }
                    e.abbreviation = p.abbreviation;
                    e.text = text;
                } else {
                    if entries.len() >= MAX_ENTRIES {
                        bail!("상용구는 {MAX_ENTRIES}개까지 등록할 수 있습니다.");
                    }
                    entries.push(AutoText {
                        name,
                        abbreviation: p.abbreviation,
                        text,
                    });
                }
                save(&path, &entries)?;
                Ok(entries_json(&path, &entries))
            }
            "tools.autotext.delete" => {
                let p: Named = serde_json::from_value(params)?;
                let (path, mut entries) = load_for_update()?;
                let before = entries.len();
                entries.retain(|e| e.name != p.name);
                if entries.len() == before {
                    bail!("상용구가 없습니다: {}", p.name);
                }
                save(&path, &entries)?;
                Ok(entries_json(&path, &entries))
            }
            "tools.autotext.insert" => {
                let p: Named = serde_json::from_value(params)?;
                let (_, entries) = load()?;
                let Some(e) = entries.iter().find(|e| e.name == p.name) else {
                    bail!("상용구가 없습니다: {}", p.name);
                };
                let text = e.text.clone();
                self.insert_autotext(&text)
            }
            "tools.autotext.expand" => {
                let _: Nothing = serde_json::from_value(params)?;
                if self.span_anchor.is_some() || !self.selection().is_empty() {
                    bail!(
                        "상용구 바로 넣기는 선택 영역 없이 준말 바로 뒤에 커서를 두고 실행합니다."
                    );
                }
                let range = word_before(self.text(), self.cursor.offset);
                let key: String = self
                    .text()
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect();
                if key.is_empty() {
                    bail!("커서 앞에 준말이 없습니다.");
                }
                let (_, entries) = load()?;
                let Some(e) = entries
                    .iter()
                    .find(|e| e.abbreviation == key)
                    .or_else(|| entries.iter().find(|e| e.name == key))
                else {
                    bail!("준말 또는 이름이 '{key}'인 상용구가 없습니다.");
                };
                let text = e.text.clone();
                self.guard_protected()?;
                self.transaction(|s| {
                    s.clear_selection();
                    s.cursor.offset = range.end;
                    s.anchor = Some(range.start);
                    s.insert(&text)
                })?;
                Ok(json!({"expanded": key, "inserted": text, "cursor": self.cursor}))
            }
            _ => bail!("알 수 없는 상용구 명령: {id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_folder_follows_each_platform_convention() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| std::ffi::OsString::from(*v))
            }
        };
        let none = |_: &std::path::Path| false;
        let p = |s: &str| Some(PathBuf::from(s));
        let win = env(&[("APPDATA", "C:/Users/u/AppData/Roaming"), ("HOME", "/h")]);
        assert_eq!(
            default_config_dir("windows", &win, &none),
            p("C:/Users/u/AppData/Roaming/HwpCraft")
        );
        let win_profile = env(&[("USERPROFILE", "C:/Users/u")]);
        assert_eq!(
            default_config_dir("windows", &win_profile, &none),
            p("C:/Users/u/AppData/Roaming/HwpCraft")
        );
        let mac = env(&[("HOME", "/Users/u"), ("XDG_CONFIG_HOME", "/x")]);
        assert_eq!(
            default_config_dir("macos", &mac, &none),
            p("/Users/u/Library/Application Support/HwpCraft")
        );
        let legacy = |d: &std::path::Path| d == std::path::Path::new("/Users/u/.config/HwpCraft");
        assert_eq!(
            default_config_dir("macos", &mac, &legacy),
            p("/Users/u/.config/HwpCraft"),
            "an existing pre-0.17 macOS folder keeps being used"
        );
        let linux = env(&[("HOME", "/home/u"), ("XDG_CONFIG_HOME", "/home/u/.cfg")]);
        assert_eq!(
            default_config_dir("linux", &linux, &none),
            p("/home/u/.cfg/HwpCraft")
        );
        let linux_home = env(&[("HOME", "/home/u"), ("XDG_CONFIG_HOME", "")]);
        assert_eq!(
            default_config_dir("linux", &linux_home, &none),
            p("/home/u/.config/HwpCraft")
        );
        assert_eq!(default_config_dir("linux", &env(&[]), &none), None);
        assert_eq!(default_config_dir("macos", &env(&[]), &none), None);
    }

    #[test]
    fn word_before_stops_at_spaces_and_punctuation() {
        assert_eq!(word_before("안녕 인사", 5), 3..5);
        assert_eq!(word_before("abc_1 ", 5), 0..5);
        assert_eq!(word_before("abc. ", 5), 5..5);
        assert_eq!(word_before("", 0), 0..0);
    }

    #[test]
    fn names_and_abbreviations_are_validated() {
        assert!(validate_name(" ").is_err());
        assert!(validate_name("a\nb").is_err());
        assert!(validate_name(&"가".repeat(65)).is_err());
        assert!(validate_name("인사말").is_ok());
        assert!(validate_abbreviation("인 사").is_err());
        assert!(validate_abbreviation("").is_ok());
    }
}
