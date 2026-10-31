//! Equation macros: named equation scripts kept for reuse in the equation
//! editor and by automation.
//!
//! They live in `equation_macros.json` in the settings folder, apart from the
//! key macros of `macros.json`. Names follow the key-macro rule, and the file is
//! read and written through `file_access` (atomically), so automation roots and
//! atomic batches that forbid writes apply.
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::EquationOptions;

const FILE_NAME: &str = "equation_macros.json";
const MAX_MACROS: usize = 200;
const MAX_NAME: usize = 40;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EquationMacro {
    pub name: String,
    pub script: String,
}

#[derive(Default, Deserialize, Serialize)]
struct Store {
    schema_version: u32,
    macros: Vec<EquationMacro>,
}

fn check_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let count = name.chars().count();
    if count == 0 || count > MAX_NAME || name.chars().any(char::is_control) {
        bail!("수식 매크로 이름은 제어 문자 없이 1–{MAX_NAME}자입니다.");
    }
    Ok(name)
}

fn store_path() -> Result<PathBuf> {
    crate::autotext::config_dir()
        .map(|dir| dir.join(FILE_NAME))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못해 수식 매크로를 저장할 수 없습니다."))
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
        .with_context(|| format!("수식 매크로 파일을 읽지 못했습니다: {}", path.display()))?;
    if store.schema_version != 1 {
        bail!("지원하지 않는 수식 매크로 파일 형식입니다.");
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

/// Saved equation macros, in saving order.
pub fn equation_macros() -> Result<Vec<EquationMacro>> {
    Ok(load_store()?.macros)
}

/// Save (or replace) an equation macro; the script passes the same checks as
/// an inserted equation.
pub fn save_equation_macro(name: &str, script: &str) -> Result<()> {
    let name = check_name(name)?.to_owned();
    crate::objects::equation_size(&EquationOptions {
        script: script.to_owned(),
        size_pt: 10.0,
    })?;
    let mut store = load_store()?;
    store.macros.retain(|m| m.name != name);
    if store.macros.len() >= MAX_MACROS {
        bail!("수식 매크로는 {MAX_MACROS}개까지 저장할 수 있습니다. 쓰지 않는 매크로를 지우세요.");
    }
    store.macros.push(EquationMacro {
        name,
        script: script.to_owned(),
    });
    save_store(store)
}

/// Delete an equation macro; fails when there is none of that name.
pub fn delete_equation_macro(name: &str) -> Result<()> {
    let mut store = load_store()?;
    let before = store.macros.len();
    store.macros.retain(|m| m.name != name.trim());
    if store.macros.len() == before {
        bail!("저장한 수식 매크로가 없습니다: {name}");
    }
    save_store(store)
}

/// The script of a saved equation macro.
pub fn equation_macro(name: &str) -> Result<String> {
    equation_macros()?
        .into_iter()
        .find(|m| m.name == name.trim())
        .map(|m| m.script)
        .ok_or_else(|| anyhow!("저장한 수식 매크로가 없습니다: {name}"))
}
