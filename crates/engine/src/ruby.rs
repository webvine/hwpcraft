//! Owner: format area. 덧말 넣기·고치기·지우기 (Hangul 서식 > 덧말 넣기).
//!
//! A 덧말 is one control holding its base text and annotation (HWP 'tdut',
//! HWPX `hp:dutmal`); the base text leaves the paragraph text when it moves
//! into the control, and comes back when the 덧말 is removed.
use super::*;
use crate::control_slots::insert_control;
use crate::ext_format::CHAR_OVERLAP_CTRL_CHAR;
use hwpcraft_doc::model::control::Ruby;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "format.ruby",
        "덧말 넣기 (선택한 글자 위나 아래에 작은 글)",
        "",
    ),
    ("format.ruby.list", "덧말 목록 (캐럿 문단)", ""),
    ("format.ruby.update", "덧말 고치기", ""),
    ("format.ruby.delete", "덧말 지우기 (기준 글자는 남김)", ""),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "format.ruby",
        "본문 한 문단 안에서 선택한 글자(기준 글, 1–100자)에 덧말을 답니다. text(필수, 1–100자): 덧말. position: top(기본)·bottom. align: center(기본)·left·right. size(10–100, 기본 50): 덧말 크기 비율(%). 결과: control(문단 안 컨트롤 번호).",
        r#"{"text":"한국어","position":"top"}"#,
    ),
    (
        "format.ruby.list",
        "캐럿 문단의 덧말: control, position(글자 위치), base(기준 글), text(덧말), position_type, align, size.",
        r#"{}"#,
    ),
    (
        "format.ruby.update",
        "control(필수): format.ruby.list의 번호. text·position·align·size 가운데 바꿀 것만 줍니다.",
        r#"{"control":2,"text":"고친 덧말"}"#,
    ),
    (
        "format.ruby.delete",
        "control(필수): 덧말을 지우고 기준 글은 그 자리에 본문 글자로 되돌립니다.",
        r#"{"control":2}"#,
    ),
];

const MAX_CHARS: usize = 100;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RubyOptions {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    align: Option<String>,
    #[serde(default)]
    size: Option<u8>,
    #[serde(default)]
    control: Option<usize>,
}

impl RubyOptions {
    /// Apply the given fields onto `ruby`.
    fn apply(&self, ruby: &mut Ruby) -> Result<()> {
        if let Some(text) = &self.text {
            ruby.ruby_text = checked_text(text, "덧말")?;
        }
        if let Some(position) = &self.position {
            ruby.pos_type = match position.as_str() {
                "top" => 0,
                "bottom" => 1,
                other => bail!("덧말 위치는 top 또는 bottom입니다: {other}"),
            };
        }
        if let Some(align) = &self.align {
            // HWP/HWPX: 0 왼쪽 · 1 오른쪽 · 2 가운데.
            ruby.align = match align.as_str() {
                "left" => 0,
                "right" => 1,
                "center" => 2,
                other => bail!("덧말 정렬은 center·left·right입니다: {other}"),
            };
        }
        if let Some(size) = self.size {
            if !(10..=100).contains(&size) {
                bail!("덧말 크기 비율은 10–100%입니다.");
            }
            ruby.sz_ratio = size;
        }
        Ok(())
    }
}

fn checked_text(text: &str, what: &str) -> Result<String> {
    let count = text.chars().count();
    if count == 0 || count > MAX_CHARS || text.chars().any(char::is_control) {
        bail!("{what}은(는) 제어 문자 없이 1–{MAX_CHARS}자입니다.");
    }
    Ok(text.to_owned())
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_ruby_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        let parsed = serde_json::from_value::<RubyOptions>(params.clone())
            .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"));
        Some(parsed.and_then(|p| match id {
            "format.ruby" => self.insert_ruby(p),
            "format.ruby.list" => self.rubies(),
            "format.ruby.update" => self.update_ruby(p),
            _ => self.delete_ruby(p),
        }))
    }

    fn ruby_paragraph(&self) -> Result<Cursor> {
        let c = self.cursor;
        if c.cell.is_some() {
            bail!("덧말은 본문 문단에 넣습니다. 표 안에서는 아직 지원하지 않습니다.");
        }
        self.paragraph(c)?;
        Ok(c)
    }

    fn insert_ruby(&mut self, p: RubyOptions) -> Result<Value> {
        let c = self.ruby_paragraph()?;
        if p.control.is_some() {
            bail!("format.ruby에는 control을 주지 않습니다. 고치려면 format.ruby.update를 쓰세요.");
        }
        if p.text.is_none() {
            bail!("덧말 text가 필요합니다.");
        }
        if self.span_anchor.is_some() || !self.has_selection() {
            bail!("한 문단 안에서 덧말을 달 글자를 선택하세요.");
        }
        let range = self.selection();
        let base: String = self
            .paragraph(c)?
            .text
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect();
        let base = checked_text(&base, "기준 글")?;
        let mut ruby = Ruby {
            main_text: base,
            align: 2,
            sz_ratio: 50,
            ..Default::default()
        };
        p.apply(&mut ruby)?;
        let mut control = 0;
        self.transaction(|s| {
            s.core
                .delete_range_native(
                    c.section,
                    c.paragraph,
                    range.start,
                    c.paragraph,
                    range.end,
                    None,
                )
                .map_err(err)?;
            let section = &mut s.core.document_mut().sections[c.section];
            section.raw_stream = None;
            let para = &mut section.paragraphs[c.paragraph];
            // CTRL_DATA records and field ranges move with it (see control_slots).
            control = insert_control(para, range.start, Control::Ruby(ruby), None);
            para.control_mask |= 1 << CHAR_OVERLAP_CTRL_CHAR;
            s.refresh_layout()?;
            s.clear_selection();
            s.cursor = Cursor {
                offset: range.start,
                ..c
            };
            Ok(())
        })?;
        Ok(json!({"control": control, "position": range.start}))
    }

    fn rubies(&self) -> Result<Value> {
        let c = self.ruby_paragraph()?;
        let para = self.paragraph(c)?;
        let positions = para.control_text_positions();
        let list: Vec<Value> = para
            .controls
            .iter()
            .enumerate()
            .filter_map(|(i, control)| match control {
                Control::Ruby(r) => Some(json!({
                    "control": i,
                    "position": positions.get(i).copied().unwrap_or(0),
                    "base": r.main_text,
                    "text": r.ruby_text,
                    "position_type": if r.pos_type == 1 { "bottom" } else { "top" },
                    "align": match r.align { 0 => "left", 1 => "right", _ => "center" },
                    "size": r.sz_ratio,
                })),
                _ => None,
            })
            .collect();
        Ok(json!(list))
    }

    fn ruby_at(&self, c: Cursor, control: usize) -> Result<Ruby> {
        match self.paragraph(c)?.controls.get(control) {
            Some(Control::Ruby(r)) => Ok(r.clone()),
            _ => {
                bail!("{control}번 컨트롤은 덧말이 아닙니다. format.ruby.list로 번호를 확인하세요.")
            }
        }
    }

    fn update_ruby(&mut self, p: RubyOptions) -> Result<Value> {
        let c = self.ruby_paragraph()?;
        let control = p
            .control
            .ok_or_else(|| anyhow!("고칠 덧말 control이 필요합니다."))?;
        let mut ruby = self.ruby_at(c, control)?;
        p.apply(&mut ruby)?;
        self.transaction(|s| {
            let section = &mut s.core.document_mut().sections[c.section];
            section.raw_stream = None;
            section.paragraphs[c.paragraph].controls[control] = Control::Ruby(ruby);
            s.refresh_layout()
        })?;
        Ok(self.info())
    }

    fn delete_ruby(&mut self, p: RubyOptions) -> Result<Value> {
        let c = self.ruby_paragraph()?;
        let control = p
            .control
            .ok_or_else(|| anyhow!("지울 덧말 control이 필요합니다."))?;
        let ruby = self.ruby_at(c, control)?;
        let position = self
            .paragraph(c)?
            .control_text_positions()
            .get(control)
            .copied()
            .unwrap_or(0);
        self.transaction(|s| {
            s.delete_control_keeping_fields(c.section, c.paragraph, control, false)?;
            s.core
                .insert_text_native(c.section, c.paragraph, position, &ruby.main_text)
                .map_err(err)?;
            s.clear_selection();
            s.cursor = Cursor {
                offset: position + ruby.main_text.chars().count(),
                ..c
            };
            Ok(())
        })?;
        Ok(self.info())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn select(s: &mut Session, start: usize, end: usize) {
        s.clear_selection();
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: end,
            cell: None,
        };
        s.anchor = Some(start);
    }

    fn body(s: &Session) -> String {
        s.core.document().sections[0].paragraphs[0].text.clone()
    }

    #[test]
    fn base_text_moves_into_the_control_and_back() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"앞 한국어 뒤"}))?;
        select(&mut s, 2, 5);
        let out = s.run("format.ruby", json!({"text":"Korean","size":40}))?;
        let control = out["control"].as_u64().ok_or_else(|| anyhow!("control"))? as usize;
        assert_eq!(body(&s), "앞  뒤");
        let list = s.run("format.ruby.list", json!({}))?;
        assert_eq!(list[0]["base"], "한국어");
        assert_eq!(list[0]["text"], "Korean");
        assert_eq!(list[0]["size"], 40);
        assert_eq!(list[0]["align"], "center");
        s.run(
            "format.ruby.update",
            json!({"control":control,"text":"KO","position":"bottom"}),
        )?;
        let list = s.run("format.ruby.list", json!({}))?;
        assert_eq!(list[0]["text"], "KO");
        assert_eq!(list[0]["position_type"], "bottom");
        // Saved and reopened in both formats, the 덧말 is intact.
        let dir = tempfile::tempdir()?;
        for name in ["r.hwpx", "r.hwp"] {
            let path = dir.path().join(name);
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            reopened.cursor = Cursor {
                section: 0,
                paragraph: 0,
                offset: 0,
                cell: None,
            };
            let list = reopened.run("format.ruby.list", json!({}))?;
            assert_eq!(list[0]["base"], "한국어", "{name}");
            assert_eq!(list[0]["text"], "KO", "{name}");
            assert_eq!(list[0]["position_type"], "bottom", "{name}");
        }
        s.run("format.ruby.delete", json!({"control":control}))?;
        assert_eq!(body(&s), "앞 한국어 뒤");
        // Each step is one undo.
        s.run("edit.undo", json!({}))?;
        assert_eq!(body(&s), "앞  뒤");
        s.run("edit.undo", json!({}))?;
        s.run("edit.undo", json!({}))?;
        assert_eq!(body(&s), "앞 한국어 뒤");
        Ok(())
    }

    #[test]
    fn refuses_empty_selection_bad_options_and_non_ruby_controls() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"기준 글"}))?;
        assert!(s.run("format.ruby", json!({"text":"덧"})).is_err());
        select(&mut s, 0, 2);
        assert!(s.run("format.ruby", json!({})).is_err());
        assert!(s.run("format.ruby", json!({"text":""})).is_err());
        assert!(s.run("format.ruby", json!({"text":"덧","size":5})).is_err());
        assert!(
            s.run("format.ruby", json!({"text":"덧","position":"left"}))
                .is_err()
        );
        assert!(s.run("format.ruby.delete", json!({"control":0})).is_err());
        assert_eq!(body(&s), "기준 글");
        Ok(())
    }
}
