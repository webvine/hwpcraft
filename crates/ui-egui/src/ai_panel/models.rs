//! The models and reasoning levels the panel offers per CLI. The newest model
//! comes first and is the default.
//!
//! Claude Code takes family aliases that it resolves to that family's newest
//! model, so the list needs no update when a model ships. Codex publishes its
//! own list in `<CODEX_HOME>/models_cache.json` (read only, never written); when
//! that file is missing or unreadable the panel offers the CLI's default only.
use super::stream::Engine;
use serde_json::Value;
use std::path::Path;

/// One choice in the model menu.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Model {
    /// What the CLI is given; `None` leaves the CLI's own default.
    pub id: Option<String>,
    pub name: String,
    /// Tooltip.
    pub note: String,
    /// Reasoning levels it takes, lightest first.
    pub efforts: Vec<String>,
    /// The level it uses when none is chosen.
    pub default_effort: Option<String>,
}

const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// A cache bigger than this is not read (it is a few hundred KB).
const CACHE_LIMIT: u64 = 8 * 1024 * 1024;

fn claude_model(alias: &str, name: &str, note: &str) -> Model {
    Model {
        id: Some(alias.to_owned()),
        name: name.to_owned(),
        note: note.to_owned(),
        efforts: CLAUDE_EFFORTS.iter().map(|e| e.to_string()).collect(),
        default_effort: None,
    }
}

/// The CLI's default model, when the panel knows no list.
fn cli_default() -> Model {
    Model {
        id: None,
        name: "CLI 기본 모델".to_owned(),
        note: "CLI 설정의 기본 모델을 씁니다.".to_owned(),
        efforts: Vec::new(),
        default_effort: None,
    }
}

/// Models for `engine`, newest first. `codex_home` is the user's Codex folder.
pub(crate) fn list(engine: Engine, codex_home: Option<&Path>) -> Vec<Model> {
    match engine {
        Engine::Claude => vec![
            claude_model("fable", "Fable (최신)", "가장 최신의 최상위 모델"),
            claude_model("opus", "Opus", "Opus 계열의 최신 모델"),
            claude_model("sonnet", "Sonnet", "Sonnet 계열의 최신 모델 (균형)"),
            claude_model("haiku", "Haiku", "Haiku 계열의 최신 모델 (빠름)"),
        ],
        Engine::Codex => codex_home
            .map(|h| h.join("models_cache.json"))
            .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= CACHE_LIMIT))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| parse_codex(&text))
            .unwrap_or_else(|| vec![cli_default()]),
    }
}

/// Codex's `models_cache.json`: listed models by priority (1 = newest).
/// `None` when the text is not that file or lists nothing usable.
pub(crate) fn parse_codex(text: &str) -> Option<Vec<Model>> {
    let v: Value = serde_json::from_str(text).ok()?;
    let mut models: Vec<(u64, Model)> = v["models"]
        .as_array()?
        .iter()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| {
            let slug = m["slug"].as_str().filter(|s| !s.is_empty())?;
            let efforts = m["supported_reasoning_levels"]
                .as_array()
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(|l| l["effort"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            Some((
                m["priority"].as_u64().unwrap_or(u64::MAX),
                Model {
                    id: Some(slug.to_owned()),
                    name: m["display_name"].as_str().unwrap_or(slug).to_owned(),
                    note: m["description"].as_str().unwrap_or_default().to_owned(),
                    efforts,
                    default_effort: m["default_reasoning_level"].as_str().map(str::to_owned),
                },
            ))
        })
        .collect();
    models.sort_by_key(|(priority, _)| *priority);
    let models: Vec<Model> = models.into_iter().map(|(_, m)| m).collect();
    (!models.is_empty()).then_some(models)
}

/// A reasoning level as the menu shows it.
pub(crate) fn effort_label(effort: &str) -> String {
    let korean = match effort {
        "minimal" => "가장 빠르게",
        "low" => "빠르게",
        "medium" => "보통",
        "high" => "깊게",
        "xhigh" => "더 깊게",
        "max" => "최대",
        "ultra" => "최고",
        _ => return effort.to_owned(),
    };
    format!("{korean} ({effort})")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CACHE: &str = r#"{"fetched_at":"2026-11-01","models":[
        {"slug":"old-model","display_name":"Old","visibility":"list","priority":6,
         "default_reasoning_level":"medium",
         "supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"}]},
        {"slug":"hidden","display_name":"Hidden","visibility":"hide","priority":0},
        {"slug":"new-model","display_name":"New","visibility":"list","priority":1,
         "description":"Latest","default_reasoning_level":"low",
         "supported_reasoning_levels":[{"effort":"low"},{"effort":"high"},{"effort":"ultra"}]}
    ]}"#;

    #[test]
    fn codex_lists_its_visible_models_newest_first() {
        let m = parse_codex(CACHE).unwrap();
        let ids: Vec<_> = m.iter().map(|x| x.id.as_deref().unwrap()).collect();
        assert_eq!(ids, ["new-model", "old-model"]);
        assert_eq!(m[0].name, "New");
        assert_eq!(m[0].efforts, ["low", "high", "ultra"]);
        assert_eq!(m[0].default_effort.as_deref(), Some("low"));
    }

    #[test]
    fn a_missing_or_odd_codex_cache_leaves_the_cli_default() {
        for odd in [
            "",
            "not json",
            "{}",
            r#"{"models":7}"#,
            r#"{"models":[{"slug":"x"}]}"#,
        ] {
            assert_eq!(parse_codex(odd), None, "{odd}");
        }
        let dir = std::env::temp_dir().join(format!("ai-models-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // No file at all, then a broken one: one entry, the CLI's own default.
        assert_eq!(list(Engine::Codex, Some(&dir)), [cli_default()]);
        std::fs::write(dir.join("models_cache.json"), "{broken").unwrap();
        assert_eq!(list(Engine::Codex, Some(&dir)), [cli_default()]);
        assert_eq!(list(Engine::Codex, None), [cli_default()]);
        // A real cache is read, and left as it was.
        std::fs::write(dir.join("models_cache.json"), CACHE).unwrap();
        assert_eq!(
            list(Engine::Codex, Some(&dir))[0].id.as_deref(),
            Some("new-model")
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("models_cache.json")).unwrap(),
            CACHE
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn claude_offers_family_aliases_with_the_newest_first() {
        let m = list(Engine::Claude, None);
        assert_eq!(m[0].id.as_deref(), Some("fable"));
        assert!(m.iter().all(|x| x.efforts == CLAUDE_EFFORTS));
        assert_eq!(effort_label("xhigh"), "더 깊게 (xhigh)");
        assert_eq!(effort_label("turbo"), "turbo");
    }
}
