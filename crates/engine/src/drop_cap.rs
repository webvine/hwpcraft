//! Owner: format area. 문단 첫 글자 장식 (Hangul 서식 > 문단 첫 글자 장식).
//!
//! As in Hangul, the paragraph's first character moves into a borderless text
//! box at the paragraph start, two or three lines tall and wrapped by the text,
//! or hung in the left margin. The box carries a description marker (kept by
//! HWP and HWPX) and the drop-cap style, so 없음 puts the character back.
use super::*;
use crate::objects::ObjectAddress;
use hwpcraft_doc::model::{
    control::Control,
    shape::DropCapStyle,
    style::{CharShapeMods, LineSpacingType, ParaShapeMods},
};
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[(
    "format.drop_cap",
    "문단 첫 글자 장식 (2줄·3줄·여백·없음)",
    "",
)];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[(
    "format.drop_cap",
    "캐럿 문단(본문)의 첫 글자를 장식합니다. style: double(2줄, 기본)·triple(3줄)·margin(여백에 3줄)·none(장식 없애고 글자 되돌림). font(선택): 장식 글자 글꼴 이름. gap_mm(선택, 0–10, 기본 1): 본문과의 간격. 첫 글자는 테두리 없는 글상자로 옮겨지며, 이미 장식된 문단은 새 모양으로 바꿉니다. 결과: address(글상자), char(장식 글자).",
    r#"{"style":"double"}"#,
)];

/// Marker on the text box; Hangul's own drop cap box is recognised by it too.
const MARK: &str = "문단 첫 글자 장식";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Style {
    #[default]
    Double,
    Triple,
    Margin,
    None,
}

impl Style {
    fn lines(self) -> u32 {
        match self {
            Style::Double => 2,
            Style::Triple | Style::Margin => 3,
            Style::None => 0,
        }
    }
    fn model(self) -> DropCapStyle {
        match self {
            Style::Double => DropCapStyle::DoubleLine,
            Style::Triple => DropCapStyle::TripleLine,
            Style::Margin => DropCapStyle::Margin,
            Style::None => DropCapStyle::None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DropCap {
    #[serde(default)]
    style: Style,
    font: Option<String>,
    gap_mm: Option<f64>,
}

/// HWPUNIT per millimetre.
const HU_PER_MM: f64 = 7200.0 / 25.4;

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_drop_cap_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        (id == "format.drop_cap").then(|| {
            serde_json::from_value::<DropCap>(params.clone())
                .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
                .and_then(|p| self.drop_cap(p))
        })
    }

    /// Run several engine commands as one undo step.
    pub(crate) fn compound<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let mut out = None;
        self.transaction(|s| {
            let before = s.generation;
            let result = f(s);
            // Each inner command pushed its own undo step; the outer one replaces them.
            let inner = usize::try_from(s.generation - before).unwrap_or(usize::MAX);
            for _ in 0..inner.min(s.undo.len()) {
                if let Some(snapshot) = s.undo.pop() {
                    s.retire_history_snapshot(snapshot.id);
                }
            }
            out = Some(result?);
            Ok(())
        })?;
        out.ok_or_else(|| anyhow!("작업 결과가 없습니다."))
    }

    /// The marked drop-cap box in a body paragraph, if any.
    fn drop_cap_box(&self, section: usize, paragraph: usize) -> Option<ObjectAddress> {
        let p = self
            .core
            .document()
            .sections
            .get(section)?
            .paragraphs
            .get(paragraph)?;
        p.controls
            .iter()
            .enumerate()
            .find_map(|(control, c)| match c {
                Control::Shape(shape) if shape.common().description == MARK => {
                    Some(ObjectAddress {
                        section,
                        paragraph,
                        control,
                    })
                }
                _ => None,
            })
    }

    fn drop_cap(&mut self, p: DropCap) -> Result<Value> {
        if self.cursor.cell.is_some() {
            bail!("문단 첫 글자 장식은 본문 문단에 씁니다.");
        }
        let gap_mm = p.gap_mm.unwrap_or(1.0);
        if !(0.0..=10.0).contains(&gap_mm) {
            bail!("gap_mm은 0–10입니다.");
        }
        if let Some(font) = &p.font
            && (font.trim().is_empty() || font.len() > 100)
        {
            bail!("글꼴 이름이 올바르지 않습니다.");
        }
        let (section, paragraph) = (self.cursor.section, self.cursor.paragraph);
        let existing = self.drop_cap_box(section, paragraph);
        if p.style == Style::None && existing.is_none() {
            bail!("이 문단에는 없앨 첫 글자 장식이 없습니다.");
        }
        let at = |offset| Cursor {
            section,
            paragraph,
            offset,
            cell: None,
        };
        self.compound(|s| {
            // Put a previous drop cap's character back first.
            if let Some(a) = existing {
                let text: String = s.textbox_paragraphs(a)?.concat();
                s.delete_object(a)?;
                if !text.is_empty() {
                    s.core
                        .insert_text_native(section, paragraph, 0, &text)
                        .map_err(err)?;
                }
            }
            if p.style == Style::None {
                s.cursor = at(0);
                return Ok(json!({"address": null, "char": null}));
            }
            let first = s
                .paragraph(at(0))?
                .text
                .chars()
                .next()
                .filter(|c| !c.is_whitespace() && !c.is_control())
                .ok_or_else(|| anyhow!("첫 글자가 없는 문단은 장식할 수 없습니다."))?;
            let (size_pt, line_pt) = s.drop_cap_metrics(section, paragraph)?;
            let lines = f64::from(p.style.lines());
            // The box spans `lines` lines; its glyph is sized to fill that height.
            let height_pt = line_pt * (lines - 1.0) + size_pt;
            let glyph_pt = (height_pt * 0.95).clamp(1.0, 4096.0);
            let width_pt = glyph_pt * if first.is_ascii() { 0.75 } else { 1.05 };
            let mm = |pt: f64| pt * 25.4 / 72.0;
            s.core
                .delete_range_native(section, paragraph, 0, paragraph, 1, None)
                .map_err(err)?;
            s.cursor = at(0);
            s.clear_selection();
            let made = s.run(
                "shape.insert",
                json!({"kind":"textbox","width_mm":mm(width_pt),"height_mm":mm(height_pt),
                    "paragraphs":[first.to_string()]}),
            )?;
            let a: ObjectAddress = serde_json::from_value(made["address"].clone())?;
            // Objects are anchored behind a reserved space; the drop cap's text
            // must start right after the box, so drop that space.
            if s.paragraph(at(0))?.text.starts_with(' ') {
                s.core
                    .delete_range_native(section, paragraph, 0, paragraph, 1, None)
                    .map_err(err)?;
            }
            let gap = (gap_mm * HU_PER_MM).round() as i64;
            let width_hu = (mm(width_pt) * HU_PER_MM).round() as i64;
            let margin = p.style == Style::Margin;
            // Hangul ignores negative offsets, so a margin cap is placed from the
            // paper edge: just left of the body text, inside the left margin.
            let paper_left = {
                let page = &s.core.document().sections[section].section_def.page_def;
                (i64::from(page.margin_left) + i64::from(page.margin_gutter) - width_hu - gap)
                    .max(0)
            };
            s.run(
                "object.properties",
                json!({"address":a,"props":{
                    "treatAsChar":false,
                    "textWrap": if margin { "InFrontOfText" } else { "Square" },
                    "horzRelTo": if margin { "Paper" } else { "Para" },"horzAlign":"Left",
                    "horzOffset": if margin { paper_left } else { 0 },
                    "vertRelTo":"Para","vertAlign":"Top","vertOffset":0,
                    "outerMarginLeft":0,"outerMarginTop":0,"outerMarginBottom":0,
                    "outerMarginRight": if margin { 0 } else { gap },
                    "lineType":0,"borderWidth":0,"fillType":"none",
                    "tbMarginLeft":0,"tbMarginRight":0,"tbMarginTop":0,"tbMarginBottom":0,
                    "tbVerticalAlign":"Top","description":MARK}}),
            )?;
            s.style_drop_cap_glyph(a, glyph_pt, p.font.as_deref(), p.style)?;
            s.cursor = at(0);
            Ok(json!({"address": a, "char": first.to_string()}))
        })
    }

    /// Base character size and line pitch (both in pt) of a paragraph's first character.
    fn drop_cap_metrics(&self, section: usize, paragraph: usize) -> Result<(f64, f64)> {
        let doc = self.core.document();
        let p = &doc.sections[section].paragraphs[paragraph];
        let char_shape = p.char_shape_id_at(0).unwrap_or(0);
        let size = doc
            .doc_info
            .char_shapes
            .get(char_shape as usize)
            .map_or(1000, |c| c.base_size)
            .max(100);
        let size_pt = f64::from(size) / 100.0;
        let spacing = doc
            .doc_info
            .para_shapes
            .get(usize::from(p.para_shape_id))
            .map_or(160, |s| s.line_spacing);
        // Percent line spacing (the common case); other kinds fall back to 160 %.
        let percent = if (50..=500).contains(&spacing) {
            f64::from(spacing)
        } else {
            160.0
        };
        Ok((size_pt, size_pt * percent / 100.0))
    }

    /// Enlarge the box's character and record the drop-cap style on the box.
    fn style_drop_cap_glyph(
        &mut self,
        a: ObjectAddress,
        glyph_pt: f64,
        font: Option<&str>,
        style: Style,
    ) -> Result<()> {
        let font_ids = match font {
            Some(name) => {
                let mut v = self.character_parameters(json!({"fontFamily": name}))?;
                v.get_mut("fontIds").map(Value::take)
            }
            None => None,
        };
        let doc = self.core.document_mut();
        let Some(Control::Shape(shape)) = doc.sections[a.section].paragraphs[a.paragraph]
            .controls
            .get(a.control)
        else {
            bail!("첫 글자 장식 글상자를 찾지 못했습니다.");
        };
        let first = shape
            .drawing()
            .and_then(|d| d.text_box.as_ref())
            .and_then(|t| t.paragraphs.first());
        let base = first.and_then(|p| p.char_shape_id_at(0)).unwrap_or(0);
        let base_para = first.map_or(0, |p| p.para_shape_id);
        let mut mods = CharShapeMods {
            base_size: Some((glyph_pt * 100.0).round() as i32),
            ..CharShapeMods::default()
        };
        if let Some(ids) = font_ids.as_ref().and_then(Value::as_array) {
            let ids: Vec<u16> = ids
                .iter()
                .filter_map(|v| v.as_u64())
                .map(|v| v as u16)
                .collect();
            if ids.len() == 7 {
                mods.font_ids = Some(ids.try_into().unwrap_or([0; 7]));
            }
        }
        let id = doc.find_or_create_char_shape(base, &mods);
        // One line of the enlarged glyph fills the box: 100 % line spacing.
        let para_shape = doc.find_or_create_para_shape(
            base_para,
            &ParaShapeMods {
                line_spacing: Some(100),
                line_spacing_type: Some(LineSpacingType::Percent),
                ..ParaShapeMods::default()
            },
        );
        let Some(Control::Shape(shape)) = doc.sections[a.section].paragraphs[a.paragraph]
            .controls
            .get_mut(a.control)
        else {
            bail!("첫 글자 장식 글상자를 찾지 못했습니다.");
        };
        shape.common_mut().drop_cap_style = style.model();
        let mut lengths = Vec::new();
        if let Some(text_box) = shape.drawing_mut().and_then(|d| d.text_box.as_mut()) {
            for paragraph in &mut text_box.paragraphs {
                paragraph.para_shape_id = para_shape;
                lengths.push(paragraph.text.chars().count());
            }
        }
        doc.sections[a.section].raw_stream = None;
        // Through RHWP so the box's lines are measured again for the enlarged
        // glyph (a direct model edit kept the 10 pt line height).
        for (i, len) in lengths.into_iter().enumerate() {
            self.core
                .set_char_shape_id_in_cell_native(
                    a.section,
                    a.paragraph,
                    a.control,
                    0,
                    i,
                    0,
                    len,
                    id,
                )
                .map_err(err)?;
        }
        self.refresh_layout()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(text: &str) -> Result<Session> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text": text}))?;
        s.cursor = Cursor {
            section: 0,
            paragraph: 0,
            offset: 3,
            cell: None,
        };
        Ok(s)
    }

    fn body(s: &Session) -> String {
        s.core.document().sections[0].paragraphs[0].text.clone()
    }

    #[test]
    fn moves_the_first_character_into_a_marked_box_and_back() -> Result<()> {
        let text = "한글 문서의 첫 문단입니다. 줄이 여러 개가 되도록 충분히 긴 문장을 계속해서 적어 둡니다. 셋째 줄까지 이어지도록 조금 더 적습니다.";
        let mut s = session(text)?;
        let out = s.run("format.drop_cap", json!({"style":"triple"}))?;
        assert_eq!(out["char"], "한");
        assert_eq!(body(&s), text.chars().skip(1).collect::<String>());
        let a = s.drop_cap_box(0, 0).ok_or_else(|| anyhow!("box"))?;
        assert_eq!(s.textbox_paragraphs(a)?, ["한"]);
        let props = s.object_properties(a)?["props"].clone();
        assert_eq!(props["textWrap"], "Square");
        assert_eq!(props["treatAsChar"], false);
        // The glyph is enlarged well beyond the 10 pt body text.
        let doc = s.core.document();
        let Some(Control::Shape(shape)) = doc.sections[0].paragraphs[0].controls.get(a.control)
        else {
            bail!("shape");
        };
        assert_eq!(shape.common().drop_cap_style, DropCapStyle::TripleLine);
        let glyph = shape
            .drawing()
            .and_then(|d| d.text_box.as_ref())
            .and_then(|t| t.paragraphs[0].char_shape_id_at(0))
            .unwrap_or(0);
        assert!(doc.doc_info.char_shapes[glyph as usize].base_size >= 3000);
        // Changing the shape keeps one box; 없음 restores the text; one undo each.
        s.run("format.drop_cap", json!({"style":"double"}))?;
        assert_eq!(
            s.core.document().sections[0].paragraphs[0]
                .controls
                .iter()
                .filter(|c| matches!(c, Control::Shape(_)))
                .count(),
            1
        );
        s.run("format.drop_cap", json!({"style":"none"}))?;
        assert_eq!(body(&s), text);
        assert!(s.drop_cap_box(0, 0).is_none());
        s.run("edit.undo", json!({}))?;
        assert!(s.drop_cap_box(0, 0).is_some());
        s.run("edit.undo", json!({}))?;
        s.run("edit.undo", json!({}))?;
        assert_eq!(body(&s), text);
        assert!(s.drop_cap_box(0, 0).is_none());
        Ok(())
    }

    #[test]
    fn survives_save_and_reopen_in_both_formats() -> Result<()> {
        let mut s = session("가나다라 마바사 아자차카 타파하.")?;
        s.run("format.drop_cap", json!({"style":"margin","gap_mm":2.0}))?;
        let dir = tempfile::tempdir()?;
        for name in ["cap.hwpx", "cap.hwp"] {
            let path = dir.path().join(name);
            s.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let a = reopened
                .drop_cap_box(0, 0)
                .ok_or_else(|| anyhow!("{name}: marker lost"))?;
            assert_eq!(reopened.textbox_paragraphs(a)?, ["가"], "{name}");
            reopened.cursor = Cursor {
                section: 0,
                paragraph: 0,
                offset: 0,
                cell: None,
            };
            reopened.run("format.drop_cap", json!({"style":"none"}))?;
            assert!(body(&reopened).starts_with("가나다라"), "{name}");
        }
        Ok(())
    }

    #[test]
    fn refuses_cells_empty_paragraphs_and_bad_options() -> Result<()> {
        let mut s = session(" 공백으로 시작")?;
        assert!(s.run("format.drop_cap", json!({})).is_err());
        assert!(s.run("format.drop_cap", json!({"style":"none"})).is_err());
        let mut s = session("가나다")?;
        assert!(s.run("format.drop_cap", json!({"gap_mm":11})).is_err());
        assert!(s.run("format.drop_cap", json!({"style":"four"})).is_err());
        assert_eq!(body(&s), "가나다");
        Ok(())
    }

    /// Text of every page, from the glyph runs of the page SVGs.
    fn page_text(s: &Session) -> Result<String> {
        let mut text = String::new();
        for page in 0..s.page_count()? {
            let svg = s.render_page_svg(page)?;
            for run in svg.split("<text").skip(1) {
                let body = run.split_once('>').map_or("", |(_, rest)| rest);
                text.push_str(body.split_once("</text>").map_or("", |(t, _)| t));
            }
        }
        Ok(text)
    }

    /// A 3-line drop cap on a one-line paragraph reaches into what follows. The
    /// following paragraph, table, picture or page must still be drawn: the side
    /// flow band once made the next paragraph vanish from pages and PDF.
    #[test]
    fn drop_cap_band_never_hides_what_follows() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let picture = dir.path().join("dot.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 30, 255])).save(&picture)?;
        // The case that vanished: a one-line host and a two-line follower whose
        // rows both fall inside the band.
        let first =
            "첫 문단은 짧게 한 줄 반쯤만 씁니다. 장식 글자가 둘째 문단까지 내려가야 합니다.";
        let after = "둘째 문단도 이어 씁니다. 장식 글자 옆에서 시작해 넓은 줄로 돌아가는지 봅니다. 문장을 더 붙여 줄을 늘립니다.";
        let unique: Vec<char> = after
            .chars()
            .filter(|c| !first.contains(*c) && !c.is_whitespace())
            .collect();
        assert!(unique.len() >= 5);
        for case in ["paragraph", "spaced", "table", "picture", "page_break"] {
            let mut s = Session::new()?;
            s.run("text.insert", json!({"text": first}))?;
            match case {
                "paragraph" => {
                    s.run("text.insert", json!({"text": format!("\n{after}")}))?;
                }
                "spaced" => {
                    s.run("text.insert", json!({"text": format!("\n{after}")}))?;
                    s.run("select.all", json!({}))?;
                    s.run(
                        "format.paragraph",
                        json!({"spacingBefore": 400, "spacingAfter": 800}),
                    )?;
                }
                "table" => {
                    s.run("text.insert", json!({"text": "\n"}))?;
                    s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
                    s.run("caret.move", json!({"motion": "document_end"}))?;
                    s.run("text.insert", json!({"text": format!("\n{after}")}))?;
                }
                "picture" => {
                    s.run("text.insert", json!({"text": "\n"}))?;
                    s.run("picture.insert", json!({"path": picture, "width_mm": 30.0}))?;
                    s.run("text.insert", json!({"text": after}))?;
                }
                _ => {
                    s.run("insert.page_break", json!({}))?;
                    s.run("text.insert", json!({"text": after}))?;
                }
            }
            s.cursor = Cursor {
                section: 0,
                paragraph: 0,
                offset: 0,
                cell: None,
            };
            s.clear_selection();
            s.run("format.drop_cap", json!({"style": "triple"}))?;
            let mut shown = vec![("live", page_text(&s)?)];
            for ext in ["hwp", "hwpx"] {
                let path = dir.path().join(format!("{case}.{ext}"));
                s.save(&path)?;
                let mut reopened = Session::new()?;
                reopened.open(&path)?;
                shown.push((ext, page_text(&reopened)?));
            }
            for (state, text) in shown {
                for &c in &unique {
                    assert!(
                        text.contains(c),
                        "{case} {state}: '{c}' is missing from the pages"
                    );
                }
                assert!(
                    text.contains('짧'),
                    "{case} {state}: the drop cap paragraph is missing"
                );
            }
        }
        Ok(())
    }
}
