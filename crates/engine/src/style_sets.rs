//! Owner: format area. 스타일마당: style sets applied to the document in one step.
//!
//! A set is the `format.style.export` JSON (styles by name, any subset of their
//! character and paragraph properties). HwpCraft ships a few sets of its own design
//! (Hancom's 스타일 마당 files are not used); they change sizes, weight, spacing and
//! alignment only and keep the document's fonts. Users keep their own sets, saved
//! from a document, in the settings folder's `style_sets`. Applying goes through
//! `format.style.import`: styles are matched by name (updated) or added, one undo step.
use super::*;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("format.style.set.list", "스타일마당 목록", ""),
    ("format.style.set.apply", "스타일마당 적용", ""),
    (
        "format.style.set.save",
        "스타일마당으로 저장 (현재 문서 스타일)",
        "",
    ),
    (
        "format.style.set.delete",
        "스타일마당 지우기 (사용자 묶음)",
        "",
    ),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "format.style.set.list",
        "매개 변수 없음. sets[]: name, builtin(HwpCraft 기본 묶음이면 true), description, styles(묶음이 바꾸는 스타일 이름).",
        r#"{}"#,
    ),
    (
        "format.style.set.apply",
        "name: 스타일마당 이름. 묶음의 스타일을 이름으로 맞춰 고치거나 더합니다(format.style.import와 같음, 한 번 실행 취소). 기본 묶음은 글꼴은 그대로 두고 크기·굵기·간격·정렬만 바꿉니다. 결과: name, styles.",
        r#"{"name":"보고서"}"#,
    ),
    (
        "format.style.set.save",
        "name(1–40자): 현재 문서의 모든 스타일을 사용자 스타일마당으로 설정 폴더에 저장합니다(같은 이름은 바꿈, 기본 묶음 이름은 쓸 수 없음). 결과: name, sets.",
        r#"{"name":"우리 팀 보고서"}"#,
    ),
    (
        "format.style.set.delete",
        "name: 지울 사용자 스타일마당(기본 묶음은 지울 수 없음). 결과: sets.",
        r#"{"name":"우리 팀 보고서"}"#,
    ),
];

/// HwpCraft's own sets: (name, description, styles JSON).
const BUILTIN: &[(&str, &str, &str)] = &[
    (
        "보고서",
        "장·절 제목을 크고 굵게, 본문 줄 간격 170%",
        r#"[
 {"name":"본문","character":{"fontSize":1100},"paragraph":{"lineSpacing":170,"lineSpacingType":"Percent","spacingAfter":400}},
 {"name":"개요 1","character":{"fontSize":1600,"bold":true},"paragraph":{"spacingBefore":2400,"spacingAfter":1200,"keepWithNext":true}},
 {"name":"개요 2","character":{"fontSize":1300,"bold":true},"paragraph":{"spacingBefore":1600,"spacingAfter":800,"keepWithNext":true}},
 {"name":"개요 3","character":{"fontSize":1100,"bold":true},"paragraph":{"spacingBefore":1200,"spacingAfter":600,"keepWithNext":true}},
 {"name":"캡션","character":{"fontSize":900},"paragraph":{"alignment":"center"}},
 {"name":"차례 제목","character":{"fontSize":1600,"bold":true},"paragraph":{"alignment":"center","spacingAfter":1200}}
]"#,
    ),
    (
        "공문",
        "제목 가운데, 본문 12pt·줄 간격 160%, 개요 들여쓰기 그대로",
        r#"[
 {"name":"바탕글","character":{"fontSize":1200},"paragraph":{"lineSpacing":160,"lineSpacingType":"Percent"}},
 {"name":"본문","character":{"fontSize":1200},"paragraph":{"lineSpacing":160,"lineSpacingType":"Percent"}},
 {"name":"개요 1","character":{"fontSize":1200,"bold":true},"paragraph":{"spacingBefore":800,"spacingAfter":400}},
 {"name":"개요 2","character":{"fontSize":1200},"paragraph":{"spacingBefore":400}},
 {"name":"차례 제목","character":{"fontSize":1800,"bold":true},"paragraph":{"alignment":"center","spacingAfter":1600}}
]"#,
    ),
    (
        "간결",
        "작은 글자와 좁은 간격으로 한 쪽에 많이",
        r#"[
 {"name":"바탕글","character":{"fontSize":950},"paragraph":{"lineSpacing":135,"lineSpacingType":"Percent"}},
 {"name":"본문","character":{"fontSize":950},"paragraph":{"lineSpacing":135,"lineSpacingType":"Percent","spacingAfter":0}},
 {"name":"개요 1","character":{"fontSize":1200,"bold":true},"paragraph":{"spacingBefore":800,"spacingAfter":200}},
 {"name":"개요 2","character":{"fontSize":1050,"bold":true},"paragraph":{"spacingBefore":400,"spacingAfter":100}},
 {"name":"개요 3","character":{"fontSize":950,"bold":true},"paragraph":{"spacingBefore":200}}
]"#,
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Name {
    name: String,
}

fn set_dir() -> Result<std::path::PathBuf> {
    crate::config_dir()
        .map(|d| d.join("style_sets"))
        .ok_or_else(|| anyhow!("설정 폴더를 찾지 못했습니다."))
}

fn set_path(name: &str) -> Result<std::path::PathBuf> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 40
        || name.starts_with('.')
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
    {
        bail!("스타일마당 이름은 경로 문자 없이 1–40자로 쓰세요.");
    }
    Ok(set_dir()?.join(format!("{name}.json")))
}

/// Every set: built-in ones first, then the user's in name order.
fn sets() -> Result<Vec<(String, bool, String, Value)>> {
    let mut out = Vec::new();
    for (name, description, styles) in BUILTIN {
        out.push((
            (*name).to_owned(),
            true,
            (*description).to_owned(),
            serde_json::from_str(styles)?,
        ));
    }
    if let Ok(dir) = set_dir()
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        let mut user: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = path.file_stem()?.to_str()?.to_owned();
                (path.extension()? == "json").then_some((name, path))
            })
            .collect();
        user.sort();
        for (name, path) in user {
            if BUILTIN.iter().any(|(b, _, _)| *b == name) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(json) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            out.push((name, false, String::new(), json["styles"].clone()));
        }
    }
    Ok(out)
}

fn list() -> Result<Value> {
    Ok(json!(sets()?
        .into_iter()
        .map(|(name, builtin, description, styles)| json!({
            "name": name,
            "builtin": builtin,
            "description": description,
            "styles": styles.as_array().into_iter().flatten().filter_map(|s| s["name"].as_str()).collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>()))
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_style_sets_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        if id == "format.style.set.list" {
            return Some(if params.as_object().is_some_and(|p| !p.is_empty()) {
                Err(anyhow!("{id}에는 매개 변수가 없습니다."))
            } else {
                list().map(|sets| json!({ "sets": sets }))
            });
        }
        let name = match serde_json::from_value::<Name>(params.clone()) {
            Ok(n) => n.name.trim().to_owned(),
            Err(e) => return Some(Err(anyhow!("{id}: 매개 변수 오류 ({e})"))),
        };
        Some(match id {
            "format.style.set.apply" => self.apply_style_set(&name),
            "format.style.set.save" => self.save_style_set(&name),
            _ => delete_style_set(&name),
        })
    }

    fn apply_style_set(&mut self, name: &str) -> Result<Value> {
        let styles = sets()?
            .into_iter()
            .find(|(n, _, _, _)| n == name)
            .map(|(_, _, _, styles)| styles)
            .ok_or_else(|| anyhow!("스타일마당 [{name}]이 없습니다."))?;
        let applied = self.run("format.style.import", json!({ "styles": styles }))?;
        Ok(json!({"name": name, "styles": applied}))
    }

    fn save_style_set(&mut self, name: &str) -> Result<Value> {
        if BUILTIN.iter().any(|(b, _, _)| *b == name) {
            bail!("기본 스타일마당 이름 [{name}]은 쓸 수 없습니다.");
        }
        let path = set_path(name)?;
        let exported = self.run("format.style.export", json!({}))?;
        let set = json!({"format": exported["format"], "version": exported["version"], "styles": exported["styles"]});
        std::fs::create_dir_all(set_dir()?)?;
        // A sibling file first, so a failed save leaves an existing set intact.
        let partial = path.with_extension("json.part");
        std::fs::write(&partial, serde_json::to_vec_pretty(&set)?)?;
        if let Err(e) = std::fs::rename(&partial, &path) {
            let _ = std::fs::remove_file(&partial);
            return Err(e.into());
        }
        Ok(json!({"name": name, "sets": list()?}))
    }
}

fn delete_style_set(name: &str) -> Result<Value> {
    if BUILTIN.iter().any(|(b, _, _)| *b == name) {
        bail!("기본 스타일마당 [{name}]은 지울 수 없습니다.");
    }
    let path = set_path(name)?;
    std::fs::remove_file(&path).map_err(|_| anyhow!("사용자 스타일마당 [{name}]이 없습니다."))?;
    Ok(json!({ "sets": list()? }))
}
