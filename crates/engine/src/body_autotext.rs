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
        "매개 변수 없음. 결과: generation과 source {section,start,end}(0부터 센 본문 문단 범위 [start,end)). 문단 전체를 고른 선택을 쓰고, 선택이 없으면 지금 본문 문단을 씁니다. 문단 일부 선택과 표 셀은 거절합니다.",
        "{}",
    ),
    (
        "tools.autotext.body.list",
        "매개 변수 없음. 서식 상용구 저장소(일반 글 상용구와 따로)의 revision과 항목 정보(크기 제한, 바이너리 내용 없음)를 돌려줍니다. 기존 일반 글 상용구는 바뀌지 않습니다.",
        "{}",
    ),
    (
        "tools.autotext.body.add",
        "name: 1–64자, description: 선택, expected_generation: 필수. source {section,start,end}(선택, 0부터 센 문단 [start,end))를 생략하면 지금 문단 전체·선택을 씁니다. overwrite 기본 false, expected_store_revision(선택)으로 저장소 revision을 확인합니다. 지원하지 않는 내용은 저장 전에 거절합니다. 설정 작업이라 문서 실행 취소 대상이 아닙니다.",
        r#"{"name":"안내문","description":"서식 있는 본문","expected_generation":0,"source":{"section":0,"start":0,"end":1},"expected_store_revision":0}"#,
    ),
    (
        "tools.autotext.body.get",
        "id: 필수, expected_store_revision: 선택. 항목 정보를 돌려주며 부호화한 문서 내용은 주지 않습니다.",
        r#"{"id":"body-example"}"#,
    ),
    (
        "tools.autotext.body.preview",
        "id와 expected_generation: 필수, expected_store_revision: 선택. 지금 본문 문단의 넣을 자리를 검사하고 첫 쪽 SVG와 엄격 가져오기 미리 보기를 돌려줍니다. 문서와 저장소는 바뀌지 않습니다.",
        r#"{"id":"body-example","expected_generation":0}"#,
    ),
    (
        "tools.autotext.body.delete",
        "id: 필수, expected_store_revision: 선택. 따로 있는 설정 저장소에서 이 서식 상용구만 지웁니다. 문서 실행 취소 대상이 아닙니다.",
        r#"{"id":"body-example","expected_store_revision":1}"#,
    ),
    (
        "tools.autotext.body.insert",
        "id와 expected_generation: 필수, expected_store_revision: 선택. 선택 없는 본문 캐럿에 한 번 실행 취소로 넣으며 기존 문단과 쪽 설정은 그대로 둡니다. 문단 안에서는 표 없는 한 문단 상용구는 입력하듯 글에 이어 붙이고, 그보다 긴 것은 캐럿에서 나눈 두 반쪽 사이에 들어갑니다. 표 셀과 남은 선택은 거절합니다.",
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
