//! Owner: shell/page area. 사용자 표 마당: a table's look saved by name in the settings
//! folder and applied to other tables, the way 문서마당 keeps documents.
//!
//! A look is a 4×4 grid of cell looks (borders, fill, diagonal lines), one per role pair:
//! first, odd body, even body and last row × the same for columns. A cell of the target
//! table takes the look of its role, so a look saved from a 3×3 table fits a 10×6 one.
//! 글자 모양 is not part of a look: character shape ids belong to one document.
use super::*;
use crate::ext_table::CellRange;
use serde::Deserialize;
use serde_json::Map;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("table.user_style.list", "사용자 표 마당 목록", ""),
    (
        "table.user_style.save",
        "사용자 표 마당 저장 (현재 표 모양)",
        "",
    ),
    ("table.user_style.apply", "사용자 표 마당 적용", ""),
    ("table.user_style.delete", "사용자 표 마당 지우기", ""),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "table.user_style.list",
        "매개 변수 없음. 설정 폴더(table_styles/*.json)의 사용자 표 마당: name, modified(Unix 초), 이름순.",
        "{}",
    ),
    (
        "table.user_style.save",
        "name(1–60자, \\ / : * ? \" < > | 제외), table?(기본: 캐럿의 표), force?(같은 이름을 바꿈). 표의 테두리·채우기·대각선을 셀 역할(첫 줄, 홀수 본문, 짝수 본문, 마지막 줄/칸)별로 저장합니다. 글자 모양은 저장하지 않습니다.",
        r#"{"name":"보고서 표"}"#,
    ),
    (
        "table.user_style.apply",
        "name, table?(기본: 캐럿의 표). 모든 셀에 저장한 마당에서 그 역할의 테두리·채우기·대각선을 줍니다. 한 번 실행 취소.",
        r#"{"name":"보고서 표"}"#,
    ),
    (
        "table.user_style.delete",
        "name. 저장한 마당 파일을 지웁니다.",
        r#"{"name":"보고서 표"}"#,
    ),
];

/// Cell keys that make up a look (`set_cell_properties` names).
const LOOK_KEYS: &[&str] = &[
    "borderLeft",
    "borderRight",
    "borderTop",
    "borderBottom",
    "fillType",
    "fillColor",
    "patternColor",
    "patternType",
    "diagonalSlash",
    "diagonalBackSlash",
    "diagonalLine",
    "diagonalWidth",
    "diagonalColor",
    "centerLine",
];
const FILL_KEYS: &[&str] = &["fillType", "fillColor", "patternColor", "patternType"];
const VERSION: u64 = 1;

/// The settings folder's table style folder.
fn style_dir() -> Result<std::path::PathBuf> {
    crate::config_dir()
        .map(|d| d.join("table_styles"))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못했습니다."))
}

fn style_path(name: &str) -> Result<std::path::PathBuf> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 60
        || name.starts_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c))
    {
        bail!("표 마당 이름은 1–60자이고 \\ / : * ? \" < > | 를 쓸 수 없습니다.");
    }
    Ok(style_dir()?.join(format!("{name}.json")))
}

fn style_list() -> Result<Vec<Value>> {
    let dir = style_dir()?;
    let mut list: Vec<Value> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            let modified = entry
                .metadata()
                .ok()?
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some(json!({"name": path.file_stem()?.to_str()?, "modified": modified}))
        })
        .collect();
    list.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(list)
}

/// Role of row/column `i` of `n`: 0 first, 1 odd body, 2 even body, 3 last.
fn role(i: u16, n: u16) -> usize {
    if i == 0 {
        0
    } else if i + 1 == n {
        3
    } else if (i - 1).is_multiple_of(2) {
        1
    } else {
        2
    }
}

/// The row/column of a table of `n` that shows `role` (a missing body role borrows the
/// nearest one, then the last row/column).
fn source(role: usize, n: u16) -> u16 {
    let last = n.saturating_sub(1);
    match role {
        0 => 0,
        3 => last,
        1 if n > 2 => 1,
        2 if n > 3 => 2,
        2 if n > 2 => 1,
        _ => last,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Name {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    name: String,
    #[serde(default)]
    table: Option<ObjectAddress>,
    #[serde(default)]
    force: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    name: String,
    #[serde(default)]
    table: Option<ObjectAddress>,
}

impl Session {
    pub(crate) fn run_table_style_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        let params = params.clone();
        Some(match id {
            "table.user_style.list" => style_list().map(|list| json!({ "styles": list })),
            "table.user_style.save" => serde_json::from_value::<Save>(params)
                .map_err(Into::into)
                .and_then(|p| self.save_table_style(&p)),
            "table.user_style.apply" => serde_json::from_value::<Apply>(params)
                .map_err(Into::into)
                .and_then(|p| self.apply_user_table_style(&p)),
            "table.user_style.delete" => serde_json::from_value::<Name>(params)
                .map_err(Into::into)
                .and_then(|p| {
                    let path = style_path(&p.name)?;
                    if std::fs::remove_file(&path).is_err() {
                        bail!("사용자 표 마당에 [{}]이 없습니다.", p.name.trim());
                    }
                    Ok(json!({ "styles": style_list()? }))
                }),
            _ => return None,
        })
    }

    /// The look (`LOOK_KEYS`) of the cell at `index` of table `a`.
    fn cell_look(&self, a: ObjectAddress, index: usize) -> Result<Map<String, Value>> {
        let v: Value = serde_json::from_str(
            &self
                .core
                .get_cell_properties_by_cell_path_native(
                    a.section,
                    a.paragraph,
                    &[(a.control, index, 0)],
                    index,
                )
                .map_err(err)?,
        )?;
        let mut look: Map<String, Value> = LOOK_KEYS
            .iter()
            .filter_map(|k| v.get(*k).map(|value| ((*k).to_owned(), value.clone())))
            .collect();
        // Gradient and picture fills are not cell-property values; such a cell keeps no fill.
        let fill = look.get("fillType").and_then(Value::as_str);
        if !matches!(fill, Some("none" | "solid")) {
            for k in FILL_KEYS {
                look.remove(*k);
            }
        }
        Ok(look)
    }

    fn save_table_style(&mut self, p: &Save) -> Result<Value> {
        let path = style_path(&p.name)?;
        if path.exists() && !p.force {
            bail!(
                "[{}] 표 마당이 이미 있습니다. 바꾸려면 force: true로 저장하세요.",
                p.name.trim()
            );
        }
        let a = self.table_target(p.table)?;
        let t = self.table_at(a)?;
        let (rows, cols) = (t.row_count, t.col_count);
        let mut grid = Vec::with_capacity(4);
        for row_role in 0..4 {
            let row = source(row_role, rows);
            let mut line = Vec::with_capacity(4);
            for col_role in 0..4 {
                let col = source(col_role, cols);
                let t = self.table_at(a)?;
                let index = t.cell_index_at(row, col).ok_or_else(|| {
                    anyhow!("표의 {}줄 {}칸을 찾지 못했습니다.", row + 1, col + 1)
                })?;
                line.push(Value::Object(self.cell_look(a, index)?));
            }
            grid.push(Value::Array(line));
        }
        let style = json!({"version": VERSION, "cells": grid});
        let dir = style_dir()?;
        std::fs::create_dir_all(&dir)?;
        // Write a sibling file first so a failed save leaves an existing style intact.
        let partial = path.with_extension("json.part");
        std::fs::write(&partial, serde_json::to_vec_pretty(&style)?)?;
        if let Err(e) = std::fs::rename(&partial, &path) {
            let _ = std::fs::remove_file(&partial);
            return Err(e.into());
        }
        Ok(json!({"name": p.name.trim(), "styles": style_list()?}))
    }

    fn apply_user_table_style(&mut self, p: &Apply) -> Result<Value> {
        let path = style_path(&p.name)?;
        let bytes = std::fs::read(&path)
            .map_err(|_| anyhow!("사용자 표 마당에 [{}]이 없습니다.", p.name.trim()))?;
        let style: Value = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow!("[{}] 표 마당 파일을 읽지 못했습니다: {e}", p.name.trim()))?;
        if style["version"].as_u64() != Some(VERSION) {
            bail!("[{}] 표 마당 파일의 판이 맞지 않습니다.", p.name.trim());
        }
        let look = |r: usize, c: usize| -> Result<Map<String, Value>> {
            style["cells"][r][c]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter(|(k, _)| LOOK_KEYS.contains(&k.as_str()))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .ok_or_else(|| anyhow!("[{}] 표 마당 파일이 손상되었습니다.", p.name.trim()))
        };
        let a = self.table_target(p.table)?;
        let t = self.table_at(a)?;
        let (rows, cols) = (t.row_count, t.col_count);
        let cells: Vec<(u16, u16, u16, u16)> = t
            .cells
            .iter()
            .map(|c| (c.row, c.col, c.row_span.max(1), c.col_span.max(1)))
            .collect();
        // A merged cell takes its right and bottom edges from the role it ends in.
        let mut plan = Vec::with_capacity(cells.len());
        for (row, col, rs, cs) in cells {
            let (r, c) = (role(row, rows), role(col, cols));
            let mut each = look(r, c)?;
            let (r_end, c_end) = (role(row + rs - 1, rows), role(col + cs - 1, cols));
            for (key, from) in [
                ("borderRight", look(r, c_end)?),
                ("borderBottom", look(r_end, c)?),
            ] {
                if let Some(v) = from.get(key) {
                    each.insert(key.into(), v.clone());
                }
            }
            let range = CellRange {
                start_row: row,
                start_col: col,
                end_row: row + rs - 1,
                end_col: col + cs - 1,
            };
            plan.push((range, Value::Object(each)));
        }
        self.transaction(|s| {
            s.deferred_layout(|s| {
                for (range, props) in &plan {
                    s.cell_properties_raw(a, *range, props, None)?;
                }
                Ok(())
            })
        })?;
        Ok(self.info())
    }
}
