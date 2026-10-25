//! Formatted body snippets share one engine route across desktop, CLI and MCP.
//! The legacy plain-text snippet commands and store remain independent.
use super::*;
use crate::{body_autotext_fragment as fragment, body_autotext_store as store};
use serde::Deserialize;

pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "tools.autotext.body.source",
        "본문 상용구 등록 범위 조회",
        "",
    ),
    ("tools.autotext.body.list", "본문 상용구 목록", ""),
    (
        "tools.autotext.body.add",
        "서식·표·그림을 포함한 본문 상용구 등록",
        "",
    ),
    ("tools.autotext.body.get", "본문 상용구 정보", ""),
    (
        "tools.autotext.body.preview",
        "본문 상용구 미리 보기·삽입 검사",
        "",
    ),
    ("tools.autotext.body.delete", "본문 상용구 삭제", ""),
    ("tools.autotext.body.insert", "본문 상용구 넣기", ""),
];
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "tools.autotext.body.source",
        "No parameters. Returns generation and source {section,start,end}: zero-based body paragraphs [start,end). A whole-paragraph selection is used; without a selection, uses the current body paragraph. Partial selections and cells are refused.",
        "{}",
    ),
    (
        "tools.autotext.body.list",
        "No parameters. Returns the separate formatted snippet store's revision and bounded entry metadata, without binary payloads. Existing plain-text snippets are unchanged.",
        "{}",
    ),
    (
        "tools.autotext.body.add",
        "name: 1–64 characters; description: optional; expected_generation: required. Optional source {section,start,end} uses zero-based paragraphs [start,end); default is the current complete paragraph/selection. overwrite defaults false; expected_store_revision optionally checks the store revision. Unsupported content is refused before saving. Settings operation, not document undo.",
        r#"{"name":"안내문","description":"서식 있는 본문","expected_generation":0,"source":{"section":0,"start":0,"end":1},"expected_store_revision":0}"#,
    ),
    (
        "tools.autotext.body.get",
        "id: required; expected_store_revision: optional. Returns entry metadata, not its encoded document payload.",
        r#"{"id":"body-example"}"#,
    ),
    (
        "tools.autotext.body.preview",
        "id and expected_generation: required; expected_store_revision: optional. Validates the current body-paragraph insertion boundary and returns first-page SVG plus the strict import preview. No document or store changes.",
        r#"{"id":"body-example","expected_generation":0}"#,
    ),
    (
        "tools.autotext.body.delete",
        "id: required; expected_store_revision: optional. Removes only this formatted snippet from the separate settings store. Not document undo.",
        r#"{"id":"body-example","expected_store_revision":1}"#,
    ),
    (
        "tools.autotext.body.insert",
        "id and expected_generation: required; expected_store_revision: optional. Inserts at a collapsed body-paragraph start/end in one undo step, preserving existing paragraphs and page settings. Cells, remaining selections and mid-paragraph insertion are refused.",
        r#"{"id":"body-example","expected_generation":0}"#,
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nothing {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Add {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source: Option<fragment::SourceRange>,
    expected_generation: u64,
    #[serde(default)]
    expected_store_revision: Option<u64>,
    #[serde(default)]
    overwrite: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    id: String,
    #[serde(default)]
    expected_store_revision: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AtDestination {
    id: String,
    expected_generation: u64,
    #[serde(default)]
    expected_store_revision: Option<u64>,
}

fn metadata(entry: &store::Entry) -> Value {
    json!({"id":entry.id,"name":entry.name,"description":entry.description,
        "summary":entry.summary,"paragraphs":entry.paragraphs,"stats":entry.stats,
        "bytes":(entry.payload.len()/4*3).saturating_sub(entry.payload.as_bytes().iter().rev().take_while(|b| **b==b'=').count()),
        "createdAt":entry.created_at,"modifiedAt":entry.modified_at})
}
fn listing(snapshot: &store::Snapshot) -> Value {
    json!({"schemaVersion":1,"path":snapshot.path,"revision":snapshot.revision,
        "entries":snapshot.entries.iter().map(metadata).collect::<Vec<_>>()})
}
fn check_revision(snapshot: &store::Snapshot, expected: Option<u64>) -> Result<()> {
    if expected.is_some_and(|revision| revision != snapshot.revision) {
        bail!("본문 상용구 목록이 변경됐습니다. 새로 고침한 뒤 다시 실행하세요.");
    }
    Ok(())
}
fn entry<'a>(snapshot: &'a store::Snapshot, id: &str) -> Result<&'a store::Entry> {
    snapshot
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("본문 상용구가 없습니다: {id}"))
}

impl Session {
    pub(crate) fn run_body_autotext_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(command, _, _)| *command == id) {
            return None;
        }
        Some(self.run_body_autotext_command(id, params.clone()))
    }

    fn run_body_autotext_command(&mut self, id: &str, params: Value) -> Result<Value> {
        match id {
            "tools.autotext.body.source" => {
                let _: Nothing = serde_json::from_value(params)?;
                let source = fragment::resolve_source(self, None)?;
                let summary = self.core.document().sections[source.section].paragraphs
                    [source.start..source.end]
                    .iter()
                    .flat_map(|paragraph| paragraph.text.chars().chain(std::iter::once('\n')))
                    .take(1000)
                    .collect::<String>();
                Ok(json!({"generation":self.generation,"source":source,
                    "paragraphs":source.end-source.start,"summary":summary}))
            }
            "tools.autotext.body.list" => {
                let _: Nothing = serde_json::from_value(params)?;
                Ok(listing(&store::read()?))
            }
            "tools.autotext.body.add" => {
                let p: Add = serde_json::from_value(params)?;
                if p.name.trim().is_empty()
                    || p.name.trim().chars().count() > 64
                    || p.name.chars().any(char::is_control)
                {
                    bail!("본문 상용구 이름은 제어 문자 없는 1–64자여야 합니다.");
                }
                let captured = fragment::capture(self, p.source, p.expected_generation)?;
                let (snapshot, entry) = store::upsert(
                    store::NewEntry {
                        name: p.name,
                        description: p.description,
                        summary: captured.summary,
                        paragraphs: captured.paragraphs,
                        stats: captured.stats,
                        payload: captured.payload,
                    },
                    p.overwrite,
                    p.expected_store_revision,
                )?;
                let mut result = listing(&snapshot);
                result["entry"] = metadata(&entry);
                Ok(result)
            }
            "tools.autotext.body.get" => {
                let p: Named = serde_json::from_value(params)?;
                let snapshot = store::read()?;
                check_revision(&snapshot, p.expected_store_revision)?;
                Ok(json!({"revision":snapshot.revision,"entry":metadata(entry(&snapshot,&p.id)?)}))
            }
            "tools.autotext.body.delete" => {
                let p: Named = serde_json::from_value(params)?;
                let (snapshot, deleted) = store::delete(&p.id, p.expected_store_revision)?;
                let mut result = listing(&snapshot);
                result["deleted"] = metadata(&deleted);
                Ok(result)
            }
            "tools.autotext.body.preview" | "tools.autotext.body.insert" => {
                let p: AtDestination = serde_json::from_value(params)?;
                let snapshot = store::read()?;
                check_revision(&snapshot, p.expected_store_revision)?;
                let entry = entry(&snapshot, &p.id)?;
                let payload = entry.payload_bytes()?;
                let mut result = if id.ends_with(".preview") {
                    fragment::preview(self, &payload, p.expected_generation)?
                } else {
                    fragment::insert(self, &payload, p.expected_generation)?
                };
                if !result.is_object() {
                    result = json!({"result":result});
                }
                result["entry"] = metadata(entry);
                result["revision"] = json!(snapshot.revision);
                Ok(result)
            }
            _ => bail!("알 수 없는 본문 상용구 명령: {id}"),
        }
    }
}
