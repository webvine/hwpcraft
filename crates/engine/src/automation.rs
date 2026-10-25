//! Document inspection and Unicode-safe selection shared by CLI and MCP.
use crate::Session;
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Window {
    start: usize,
    limit: usize,
}
impl Default for Window {
    fn default() -> Self {
        Self {
            start: 0,
            limit: 100,
        }
    }
}

impl Session {
    pub(crate) fn run_automation(&mut self, id: &str, params: Value) -> Result<Value> {
        if id == "select.text" {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Select {
                text: String,
                #[serde(default = "one")]
                occurrence: usize,
            }
            fn one() -> usize {
                1
            }
            let p: Select = serde_json::from_value(params)?;
            if p.text.is_empty() || p.text.contains(['\r', '\n']) || p.occurrence == 0 {
                bail!("text는 비어 있지 않은 한 문단 문자열, occurrence는 1 이상이어야 합니다.");
            }
            let mut remaining = p.occurrence;
            for mut cursor in self.cursors() {
                let text = &self.paragraph(cursor)?.text;
                for (byte, _) in text.match_indices(&p.text) {
                    remaining -= 1;
                    if remaining == 0 {
                        let anchor = text[..byte].chars().count();
                        cursor.offset = anchor + p.text.chars().count();
                        return self.run("selection.set", json!({"cursor":cursor,"anchor":anchor}));
                    }
                }
            }
            bail!(
                "요청한 문자열의 {}번째 항목을 찾지 못했습니다.",
                p.occurrence
            );
        }
        let window: Window = serde_json::from_value(params)?;
        if !(1..=1000).contains(&window.limit) {
            bail!("limit는 1–1000이어야 합니다.");
        }
        let entries = self.cursors();
        if window.start > entries.len() {
            bail!("start가 문단 수를 벗어났습니다.");
        }
        let end = window.start.saturating_add(window.limit).min(entries.len());
        let mut blocks = Vec::new();
        let mut texts = Vec::new();
        for cursor in entries.iter().skip(window.start).take(window.limit) {
            let p = self.paragraph(*cursor)?;
            texts.push(p.text.as_str());
            if id == "document.inspect" {
                blocks.push(json!({"cursor":cursor,"text":p.text,
                    "characters":p.text.chars().count(),"styleId":p.style_id,
                    "paragraphShapeId":p.para_shape_id,"charShapes":p.char_shapes,
                    "controlCount":p.controls.len()}));
            }
        }
        let mut result = json!({"total":entries.len(),"start":window.start,
            "nextStart":if end < entries.len() {Some(end)} else {None},
            "scope":"body and first-level table cells","offsetUnit":"unicode_scalar"});
        if id == "document.text" {
            result["text"] = json!(texts.join("\n"));
        } else {
            result["document"] = self.info();
            result["blocks"] = json!(blocks);
            result["selection"] =
                json!({"cursor":self.cursor,"anchor":self.anchor,"spanAnchor":self.span_anchor});
            result["canUndo"] = json!(self.can_undo());
            result["canRedo"] = json!(self.can_redo());
        }
        Ok(result)
    }
}
