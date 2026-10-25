//! Owner: format area. 서식: 글자·문단·스타일·번호/글머리표·탭·테두리/배경·개요.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Shortcuts follow the key bindings Hangul users expect (Alt+L, Alt+T, F6, Ctrl+1…0).
use super::*;
use hwpcraft_doc::model::header_footer::HeaderFooterApply;
use hwpcraft_doc::model::paragraph::MarkpenMark;
use hwpcraft_doc::model::style::{
    Alignment, BorderFill, BorderLine, BorderLineType, Bullet, CharShapeMods, Fill, FillType,
    HeadType, LineSpacingType, Numbering, NumberingHead, ParaShape, ParaShapeMods, SolidFill,
    Style, TabDef, TabItem, UnderlineType,
};
use serde::{Deserialize, de::DeserializeOwned};
use std::ops::Range;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("format.bold", "진하게", "Ctrl+B / Alt+Shift+B"),
    ("format.italic", "기울임", "Ctrl+I / Alt+Shift+I"),
    ("format.underline", "밑줄", "Ctrl+U / Alt+Shift+U"),
    ("format.strikethrough", "취소선", ""),
    ("format.superscript", "위 첨자", "Alt+Shift+P"),
    ("format.subscript", "아래 첨자", "Alt+Shift+S"),
    ("format.normal", "보통 모양", "Alt+Shift+C"),
    ("format.highlight", "형광펜", ""),
    ("format.highlight.get", "형광펜 범위 조회", ""),
    (
        "format.font_size.increase",
        "글자 크기 크게",
        "Alt+Shift+E / Ctrl+]",
    ),
    (
        "format.font_size.decrease",
        "글자 크기 작게",
        "Alt+Shift+R / Ctrl+[",
    ),
    ("format.ratio.increase", "장평 넓게", "Alt+Shift+K"),
    ("format.ratio.decrease", "장평 좁게", "Alt+Shift+J"),
    ("format.spacing.increase", "자간 넓게", "Alt+Shift+W"),
    ("format.spacing.decrease", "자간 좁게", "Alt+Shift+N"),
    ("format.align.justify", "양쪽 정렬", "Ctrl+Shift+M"),
    ("format.align.left", "왼쪽 정렬", "Ctrl+Shift+L"),
    ("format.align.center", "가운데 정렬", "Ctrl+Shift+C"),
    ("format.align.right", "오른쪽 정렬", "Ctrl+Shift+R"),
    ("format.align.distribute", "배분 정렬", "Ctrl+Shift+T"),
    ("format.align.split", "나눔 정렬", ""),
    (
        "format.line_spacing.increase",
        "줄 간격 넓게",
        "Alt+Shift+Z",
    ),
    (
        "format.line_spacing.decrease",
        "줄 간격 좁게",
        "Alt+Shift+A",
    ),
    ("format.indent.positive", "첫 줄 들여쓰기", "Ctrl+F6"),
    ("format.indent.negative", "첫 줄 내어쓰기", "Ctrl+F5"),
    (
        "format.margin_left.increase",
        "왼쪽 여백 늘리기",
        "Ctrl+Alt+F6",
    ),
    (
        "format.margin_left.decrease",
        "왼쪽 여백 줄이기",
        "Ctrl+Alt+F5",
    ),
    (
        "format.margin_right.increase",
        "오른쪽 여백 늘리기",
        "Ctrl+Alt+F7",
    ),
    (
        "format.margin_right.decrease",
        "오른쪽 여백 줄이기",
        "Ctrl+Alt+F8",
    ),
    ("format.margin.increase", "양쪽 여백 늘리기", "Ctrl+F7"),
    ("format.margin.decrease", "양쪽 여백 줄이기", "Ctrl+F8"),
    ("format.style.list", "스타일 목록", ""),
    ("format.style.detail", "스타일 정보", ""),
    ("format.style.create", "스타일 추가하기", ""),
    ("format.style.update", "스타일 편집하기", ""),
    ("format.style.delete", "스타일 지우기", ""),
    (
        "format.style.shortcut",
        "스타일 단축키 적용",
        "Ctrl+1 … Ctrl+0",
    ),
    (
        "format.numbering.list",
        "문단 번호 / 글머리표 모양 목록",
        "",
    ),
    ("format.numbering.create", "문단 번호 모양 만들기", ""),
    (
        "format.numbering.apply",
        "문단 번호 / 글머리표 / 개요 적용",
        "",
    ),
    (
        "format.numbering.toggle",
        "문단 번호 적용 / 해제",
        "Ctrl+Shift+Insert",
    ),
    (
        "format.bullet.toggle",
        "글머리표 적용 / 해제",
        "Ctrl+Shift+Delete",
    ),
    ("format.outline.toggle", "개요 적용 / 해제", "Ctrl+Insert"),
    ("format.level.increase", "한 수준 증가", "Ctrl+Num -"),
    ("format.level.decrease", "한 수준 감소", "Ctrl+Num +"),
    (
        "format.numbering.restart",
        "새 번호로 시작",
        "Alt+Shift+Insert",
    ),
    (
        "format.copy_shape.apply",
        "모양 복사 적용 (글자 + 문단)",
        "Alt+C",
    ),
    ("format.hf.character", "머리말 / 꼬리말 글자 모양", ""),
    (
        "format.hf.character.get",
        "머리말 / 꼬리말 글자 모양 조회",
        "",
    ),
    ("format.hf.paragraph", "머리말 / 꼬리말 문단 모양", ""),
    (
        "format.hf.paragraph.get",
        "머리말 / 꼬리말 문단 모양 조회",
        "",
    ),
    ("format.footnote.paragraph", "각주 / 미주 문단 모양", ""),
    (
        "format.footnote.paragraph.get",
        "각주 / 미주 문단 모양 조회",
        "",
    ),
];

/// Hangul's 10 style shortcuts address the first ten styles in list order.
pub fn style_shortcut(index: usize) -> Option<String> {
    (index < 10).then(|| format!("Ctrl+{}", (index + 1) % 10))
}

fn parse<T: DeserializeOwned>(id: &str, v: &Value) -> Result<T> {
    serde_json::from_value(v.clone()).map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
}
fn no_params(id: &str, v: &Value) -> Result<()> {
    if v.as_object().is_some_and(|o| !o.is_empty()) {
        bail!("{id} 명령에는 매개 변수가 없습니다.");
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Toggle {
    #[serde(default)]
    value: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleId {
    id: usize,
}

fn css(c: u32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        c & 0xff,
        (c >> 8) & 0xff,
        (c >> 16) & 0xff
    )
}
/// `#RRGGBB` → HWP COLORREF (0x00BBGGRR). Input is already validated.
fn bgr(v: &Value) -> Option<u32> {
    let rgb = u32::from_str_radix(v.as_str()?.trim_start_matches('#'), 16).ok()?;
    Some(((rgb >> 16) & 0xff) | (rgb & 0xff00) | ((rgb & 0xff) << 16))
}
fn line_type(v: u8) -> BorderLineType {
    use BorderLineType::*;
    [
        None,
        Solid,
        Dash,
        Dot,
        DashDot,
        DashDotDot,
        LongDash,
        Circle,
        Double,
        ThinThickDouble,
        ThickThinDouble,
        ThinThickThinTriple,
        Wave,
        DoubleWave,
        Thick3D,
        Thick3DReverse,
        Thin3D,
        Thin3DReverse,
    ]
    .get(v as usize)
    .copied()
    .unwrap_or(None)
}
fn alignment_name(a: Alignment) -> &'static str {
    match a {
        Alignment::Justify => "justify",
        Alignment::Left => "left",
        Alignment::Right => "right",
        Alignment::Center => "center",
        Alignment::Distribute => "distribute",
        Alignment::Split => "split",
    }
}
fn head_name(h: HeadType) -> &'static str {
    match h {
        HeadType::None => "None",
        HeadType::Outline => "Outline",
        HeadType::Number => "Number",
        HeadType::Bullet => "Bullet",
    }
}
fn arr<T: Copy + Into<i64>>(a: [T; 7]) -> Value {
    json!(a.iter().map(|x| (*x).into()).collect::<Vec<i64>>())
}
fn seven<T: TryFrom<i64>>(v: &Value) -> Option<[T; 7]> {
    let items: Vec<T> = v
        .as_array()?
        .iter()
        .filter_map(|x| x.as_i64().and_then(|n| T::try_from(n).ok()))
        .collect();
    items.try_into().ok()
}

/// Validated JSON → RHWP mods. Fonts are resolved to ids by `character_parameters`.
fn char_mods(v: &Value) -> CharShapeMods {
    let b = |k: &str| v[k].as_bool();
    let n = |k: &str| v[k].as_i64();
    CharShapeMods {
        bold: b("bold"),
        italic: b("italic"),
        underline: b("underline"),
        strikethrough: b("strikethrough"),
        base_size: n("fontSize").map(|x| x as i32),
        text_color: bgr(&v["textColor"]),
        shade_color: bgr(&v["shadeColor"]),
        underline_type: v["underlineType"].as_str().map(|s| match s {
            "Bottom" => UnderlineType::Bottom,
            "Top" => UnderlineType::Top,
            _ => UnderlineType::None,
        }),
        underline_color: bgr(&v["underlineColor"]),
        outline_type: n("outlineType").map(|x| x as u8),
        shadow_type: n("shadowType").map(|x| x as u8),
        shadow_color: bgr(&v["shadowColor"]),
        shadow_offset_x: n("shadowOffsetX").map(|x| x as i8),
        shadow_offset_y: n("shadowOffsetY").map(|x| x as i8),
        strike_color: bgr(&v["strikeColor"]),
        subscript: b("subscript"),
        superscript: b("superscript"),
        ratios: seven(&v["ratios"]),
        spacings: seven(&v["spacings"]),
        relative_sizes: seven(&v["relativeSizes"]),
        char_offsets: seven(&v["charOffsets"]),
        emboss: b("emboss"),
        engrave: b("engrave"),
        font_ids: seven(&v["fontIds"]),
        emphasis_dot: n("emphasisDot").map(|x| x as u8),
        underline_shape: n("underlineShape").map(|x| x as u8),
        strike_shape: n("strikeShape").map(|x| x as u8),
        kerning: b("kerning"),
        ..Default::default()
    }
}
fn para_mods(v: &Value) -> ParaShapeMods {
    let b = |k: &str| v[k].as_bool();
    let n = |k: &str| v[k].as_i64();
    ParaShapeMods {
        alignment: v["alignment"].as_str().map(|s| match s {
            "left" => Alignment::Left,
            "right" => Alignment::Right,
            "center" => Alignment::Center,
            "distribute" => Alignment::Distribute,
            "split" | "division" => Alignment::Split,
            _ => Alignment::Justify,
        }),
        line_spacing: n("lineSpacing").map(|x| x as i32),
        line_spacing_type: v["lineSpacingType"].as_str().map(|s| match s {
            "Fixed" => LineSpacingType::Fixed,
            "SpaceOnly" => LineSpacingType::SpaceOnly,
            "Minimum" => LineSpacingType::Minimum,
            _ => LineSpacingType::Percent,
        }),
        indent: n("indent").map(|x| x as i32),
        margin_left: n("marginLeft").map(|x| x as i32),
        margin_right: n("marginRight").map(|x| x as i32),
        spacing_before: n("spacingBefore").map(|x| x as i32),
        spacing_after: n("spacingAfter").map(|x| x as i32),
        head_type: v["headType"].as_str().map(|s| match s {
            "Outline" => HeadType::Outline,
            "Number" => HeadType::Number,
            "Bullet" => HeadType::Bullet,
            _ => HeadType::None,
        }),
        para_level: n("paraLevel").map(|x| x as u8),
        widow_orphan: b("widowOrphan"),
        keep_with_next: b("keepWithNext"),
        keep_lines: b("keepLines"),
        page_break_before: b("pageBreakBefore"),
        font_line_height: b("fontLineHeight"),
        single_line: b("singleLine"),
        auto_space_kr_en: b("autoSpaceKrEn"),
        auto_space_kr_num: b("autoSpaceKrNum"),
        vertical_align: n("verticalAlign").map(|x| x as u8),
        english_break_unit: n("englishBreakUnit").map(|x| x as u8),
        korean_break_unit: n("koreanBreakUnit").map(|x| x as u8),
        numbering_id: n("numberingId").map(|x| x as u16),
        border_spacing: seven::<i16>(&json!([
            v["borderSpacing"][0],
            v["borderSpacing"][1],
            v["borderSpacing"][2],
            v["borderSpacing"][3],
            0,
            0,
            0
        ]))
        .filter(|_| v["borderSpacing"].is_array())
        .map(|a| [a[0], a[1], a[2], a[3]]),
        border_connect: b("borderConnect"),
        border_ignore_margin: b("borderIgnoreMargin"),
        ..Default::default()
    }
}
/// A complete (already completed) border/fill patch → BorderFill, starting from `base`.
fn border_fill(v: &Value, base: Option<&BorderFill>) -> BorderFill {
    let mut bf = base.cloned().unwrap_or_else(|| BorderFill {
        borders: [BorderLine {
            line_type: BorderLineType::None,
            width: 0,
            color: 0,
        }; 4],
        ..Default::default()
    });
    bf.raw_data = None;
    for (i, key) in ["borderLeft", "borderRight", "borderTop", "borderBottom"]
        .iter()
        .enumerate()
    {
        if let Some(o) = v.get(*key) {
            bf.borders[i] = BorderLine {
                line_type: line_type(o["type"].as_u64().unwrap_or(0) as u8),
                width: o["width"].as_u64().unwrap_or(0) as u8,
                color: bgr(&o["color"]).unwrap_or(0),
            };
        }
    }
    if let Some(kind) = v["fillType"].as_str() {
        bf.fill = if kind == "solid" {
            Fill {
                fill_type: FillType::Solid,
                solid: Some(SolidFill {
                    background_color: bgr(&v["fillColor"]).unwrap_or(0xffffff),
                    pattern_color: bgr(&v["patternColor"]).unwrap_or(0),
                    pattern_type: v["patternType"].as_i64().unwrap_or(0) as i32,
                }),
                ..Default::default()
            }
        } else {
            Fill::default()
        };
    }
    bf
}
fn border_fill_json(bf: Option<&BorderFill>, id: u16) -> Value {
    let mut out = json!({"borderFillId": if bf.is_some() { id } else { 0 }});
    for (i, key) in ["borderLeft", "borderRight", "borderTop", "borderBottom"]
        .iter()
        .enumerate()
    {
        let line = bf.map(|b| b.borders[i]);
        out[*key] = json!({
            "type": line.map(|l| l.line_type as u8).map(|_| line_type_index(line.map(|l| l.line_type))).unwrap_or(0),
            "width": line.map(|l| l.width).unwrap_or(0),
            "color": css(line.map(|l| l.color).unwrap_or(0)),
        });
    }
    let solid = bf.and_then(|b| (b.fill.fill_type == FillType::Solid).then_some(b.fill.solid?));
    out["fillType"] = json!(if solid.is_some() { "solid" } else { "none" });
    out["fillColor"] = json!(css(solid.map(|s| s.background_color).unwrap_or(0xffffff)));
    out["patternColor"] = json!(css(solid.map(|s| s.pattern_color).unwrap_or(0)));
    out["patternType"] = json!(solid.map(|s| s.pattern_type).unwrap_or(0));
    out
}
fn line_type_index(t: Option<BorderLineType>) -> u8 {
    (0..18u8).find(|i| Some(line_type(*i)) == t).unwrap_or(0)
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_format_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        Some(match id {
            "format.bold"
            | "format.italic"
            | "format.underline"
            | "format.strikethrough"
            | "format.superscript"
            | "format.subscript" => self.toggle_character(id, params),
            "format.normal" => no_params(id, params).and_then(|_| {
                self.format_character(json!({"bold":false,"italic":false,"underline":false,
                    "underlineType":"None","strikethrough":false,"superscript":false,
                    "subscript":false,"outlineType":0,"shadowType":0,"emboss":false,
                    "engrave":false}))
            }),
            "format.highlight" => self.highlight(id, params),
            "format.highlight.get" => no_params(id, params).and_then(|_| self.highlight_get()),
            "format.font_size.increase"
            | "format.font_size.decrease"
            | "format.ratio.increase"
            | "format.ratio.decrease"
            | "format.spacing.increase"
            | "format.spacing.decrease" => {
                no_params(id, params).and_then(|_| self.step_character(id))
            }
            "format.align.justify"
            | "format.align.left"
            | "format.align.center"
            | "format.align.right"
            | "format.align.distribute"
            | "format.align.split" => no_params(id, params).and_then(|_| {
                self.format_paragraph(json!({"alignment": &id["format.align.".len()..]}))
            }),
            "format.line_spacing.increase"
            | "format.line_spacing.decrease"
            | "format.indent.positive"
            | "format.indent.negative"
            | "format.margin_left.increase"
            | "format.margin_left.decrease"
            | "format.margin_right.increase"
            | "format.margin_right.decrease"
            | "format.margin.increase"
            | "format.margin.decrease" => {
                no_params(id, params).and_then(|_| self.step_paragraph(id))
            }
            "format.style.list" => no_params(id, params).map(|_| self.style_list()),
            "format.style.detail" => {
                parse::<StyleId>(id, params).and_then(|p| self.style_detail(p.id))
            }
            "format.style.create" => parse(id, params).and_then(|p| self.create_style(p)),
            "format.style.update" => parse(id, params).and_then(|p| self.update_style(p)),
            "format.style.delete" => parse(id, params).and_then(|p| self.delete_style(p)),
            "format.style.shortcut" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Shortcut {
                    number: usize,
                }
                parse::<Shortcut>(id, params).and_then(|p| self.style_shortcut(p.number))
            }
            "format.numbering.list" => no_params(id, params).map(|_| self.numbering_list()),
            "format.numbering.create" => {
                parse(id, params).and_then(|p| self.transaction_value(|s| s.create_numbering(p)))
            }
            "format.numbering.apply" => parse(id, params).and_then(|p| self.apply_numbering(p)),
            "format.numbering.toggle" | "format.bullet.toggle" | "format.outline.toggle" => {
                no_params(id, params).and_then(|_| {
                    let kind = match id {
                        "format.numbering.toggle" => HeadKind::Number,
                        "format.bullet.toggle" => HeadKind::Bullet,
                        _ => HeadKind::Outline,
                    };
                    self.toggle_numbering(kind)
                })
            }
            "format.level.increase" | "format.level.decrease" => {
                no_params(id, params).and_then(|_| self.step_level(id == "format.level.increase"))
            }
            "format.numbering.restart" => parse(id, params).and_then(|p| self.restart_numbering(p)),
            "format.paragraph.apply" => parse(id, params).and_then(|p| self.paragraph_dialog(p)),
            "format.outline.shape" => parse(id, params).and_then(|p| self.outline_shape(p)),
            "format.copy_shape.apply" => parse(id, params).and_then(|p| self.copy_shape_apply(p)),
            "format.hf.character" => parse(id, params).and_then(|p| self.fmt_hf_character(p)),
            "format.hf.character.get" => {
                parse(id, params).and_then(|p| self.fmt_hf_character_get(&p))
            }
            "format.hf.paragraph" => parse(id, params).and_then(|p| self.fmt_hf_paragraph(p)),
            "format.hf.paragraph.get" => {
                parse(id, params).and_then(|p| self.fmt_hf_paragraph_get(&p))
            }
            "format.footnote.paragraph" => {
                parse(id, params).and_then(|p| self.fmt_footnote_paragraph(p))
            }
            "format.footnote.paragraph.get" => {
                parse(id, params).and_then(|p| self.fmt_footnote_paragraph_get(&p))
            }
            _ => return None,
        })
    }

    // ---- shared application ----

    fn format_character(&mut self, v: Value) -> Result<Value> {
        self.execute(Command::Character(v))?;
        Ok(self.info())
    }
    fn format_paragraph(&mut self, v: Value) -> Result<Value> {
        self.execute(Command::Paragraph(v))?;
        Ok(self.info())
    }
    /// Paragraph shape of one paragraph, without a transaction.
    fn apply_paragraph_at(&mut self, c: Cursor, v: Value) -> Result<()> {
        let props = self.paragraph_parameters(v)?.to_string();
        if let Some(t) = c.cell {
            self.core.apply_para_format_in_cell_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
                &props,
            )
        } else {
            self.core
                .apply_para_format_native(c.section, c.paragraph, &props)
        }
        .map_err(err)?;
        Ok(())
    }
    fn para_shape_of(&self, c: Cursor) -> Result<ParaShape> {
        let id = self.paragraph(c)?.para_shape_id;
        self.core
            .document()
            .doc_info
            .para_shapes
            .get(id as usize)
            .cloned()
            .ok_or_else(|| anyhow!("문단 서식을 찾을 수 없습니다."))
    }
    fn transaction_value(&mut self, f: impl FnOnce(&mut Self) -> Result<Value>) -> Result<Value> {
        let mut out = Value::Null;
        self.transaction(|s| {
            out = f(s)?;
            Ok(())
        })?;
        Ok(out)
    }
    /// Rebuild derived layout state after direct model edits through public RHWP API:
    /// restoring a snapshot of the current document runs RHWP's full derived-state rebuild.
    fn refresh_layout(&mut self) -> Result<()> {
        let id = self.core.save_snapshot_native();
        let r = self.core.restore_snapshot_native(id).map_err(err);
        self.core.discard_snapshot_native(id);
        r.map(|_| ())
    }

    // ---- character tools ----

    fn toggle_character(&mut self, id: &str, params: &Value) -> Result<Value> {
        let key = &id["format.".len()..];
        let p: Toggle = parse::<Toggle>(id, params)?;
        let value = match p.value {
            Some(v) => v,
            None => !self.character_properties()?[key].as_bool().unwrap_or(false),
        };
        let mut patch = json!({ key: value });
        if key == "underline" {
            patch["underlineType"] = json!(if value { "Bottom" } else { "None" });
        }
        self.format_character(patch)
    }
    /// 형광펜 through RHWP's `Paragraph::markpen_marks`, which HWP saves as PARA_RANGE_TAG
    /// kind 2 and HWPX as `<hp:markpenBegin color=…/>`…`<hp:markpenEnd/>`. No `color`
    /// (or null) removes the highlight. RHWP's renderer does not draw markpen marks yet,
    /// so the canvas shows no change; the file keeps it and Hangul shows it.
    fn highlight(&mut self, id: &str, params: &Value) -> Result<Value> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Highlight {
            #[serde(default)]
            color: Option<String>,
        }
        let p: Highlight = parse::<Highlight>(id, params)?;
        let color = p.color.as_deref().map(hex_color).transpose()?;
        let parts = self.selected_paragraphs()?;
        // No selection marks the whole caret paragraph, like `format.character`.
        let whole = parts.len() == 1 && parts[0].1.is_empty();
        self.transaction(|s| {
            for (c, r) in parts {
                let len = s.paragraph(c)?.text.chars().count();
                let r = if whole {
                    0..len
                } else {
                    r.start.min(len)..r.end.min(len)
                };
                if !r.is_empty() {
                    s.set_markpen(c, r, color.as_deref())?;
                }
            }
            Ok(())
        })?;
        Ok(self.info())
    }
    /// Highlight ranges of the caret paragraph and the color under the caret.
    fn highlight_get(&self) -> Result<Value> {
        let para = self.paragraph(self.cursor)?;
        let ranges = markpen_ranges(para);
        // At the paragraph end the preceding character's highlight counts, like 글자 모양.
        let offset = self
            .selection()
            .start
            .min(para.text.chars().count().saturating_sub(1));
        let current = ranges
            .iter()
            .find(|(a, b, _)| (*a..*b).contains(&offset))
            .map(|(_, _, c)| c.clone());
        Ok(json!({
            "color": current,
            "ranges": ranges
                .iter()
                .map(|(a, b, c)| json!({"start": a, "end": b, "color": c}))
                .collect::<Vec<_>>(),
        }))
    }
    /// Replace the highlight of `r` in one paragraph; other ranges are trimmed around it.
    fn set_markpen(&mut self, c: Cursor, r: Range<usize>, color: Option<&str>) -> Result<()> {
        let para = self.paragraph_mut(c)?;
        let mut ranges = Vec::new();
        for (a, b, col) in markpen_ranges(para) {
            if b <= r.start || a >= r.end {
                ranges.push((a, b, col));
                continue;
            }
            if a < r.start {
                ranges.push((a, r.start, col.clone()));
            }
            if b > r.end {
                ranges.push((r.end, b, col));
            }
        }
        if let Some(col) = color {
            ranges.push((r.start, r.end, col.to_owned()));
        }
        ranges.sort_by_key(|x| x.0);
        let mut merged: Vec<(usize, usize, String)> = Vec::new();
        for (a, b, col) in ranges {
            if let Some(last) = merged.last_mut()
                && last.1 == a
                && last.2 == col
            {
                last.1 = b;
            } else {
                merged.push((a, b, col));
            }
        }
        para.markpen_marks = merged
            .into_iter()
            .flat_map(|(a, b, col)| {
                [
                    MarkpenMark {
                        char_idx: a,
                        color: Some(col.to_ascii_uppercase()),
                        utf16_pos: None,
                    },
                    MarkpenMark {
                        char_idx: b,
                        color: None,
                        utf16_pos: None,
                    },
                ]
            })
            .collect();
        // With no mark left, the HWP serializer falls back to the stored range tags;
        // drop the stored markpen tags so a removed highlight stays removed.
        para.range_tags.retain(|t| t.tag >> 24 != 2);
        self.core.document_mut().sections[c.section].raw_stream = None;
        Ok(())
    }
    fn paragraph_mut(&mut self, c: Cursor) -> Result<&mut Paragraph> {
        let p = self
            .core
            .document_mut()
            .sections
            .get_mut(c.section)
            .and_then(|s| s.paragraphs.get_mut(c.paragraph))
            .ok_or_else(|| anyhow!("문단이 없습니다."))?;
        if let Some(cell) = c.cell {
            if let Some(Control::Table(t)) = p.controls.get_mut(cell.control) {
                return t
                    .cells
                    .get_mut(cell.index)
                    .and_then(|v| v.paragraphs.get_mut(cell.paragraph))
                    .ok_or_else(|| anyhow!("표의 셀 문단이 없습니다."));
            }
            bail!("이 개체의 편집은 아직 지원하지 않습니다.");
        }
        Ok(p)
    }

    // ---- 모양 복사 ----

    /// 모양 복사 (Alt+C) applies the copied 글자 모양 and 문단 모양 in one undo step.
    fn copy_shape_apply(&mut self, p: CopyShape) -> Result<Value> {
        if p.character.is_none() && p.paragraph.is_none() {
            bail!("복사한 글자 모양이나 문단 모양이 필요합니다.");
        }
        if let Some(v) = &p.character {
            validate_format(v, true)?;
        }
        if let Some(v) = &p.paragraph {
            validate_format(v, false)?;
        }
        self.transaction(|s| {
            if let Some(v) = p.character {
                s.apply_character_now(v)?;
            }
            if let Some(v) = p.paragraph {
                for (c, _) in s.selected_paragraphs()? {
                    s.apply_paragraph_at(c, v.clone())?;
                }
            }
            Ok(())
        })?;
        Ok(self.info())
    }
    /// Character shape of the selection (whole caret paragraph without one), no transaction.
    fn apply_character_now(&mut self, v: Value) -> Result<()> {
        let props = self.character_parameters(v)?.to_string();
        let parts = self.selected_paragraphs()?;
        let whole = parts.len() == 1 && parts[0].1.is_empty();
        for (c, r) in parts {
            let r = if whole {
                0..self.paragraph(c)?.text.chars().count()
            } else {
                r
            };
            if let Some(t) = c.cell {
                self.core.apply_char_format_in_cell_native(
                    c.section,
                    c.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    r.start,
                    r.end,
                    &props,
                )
            } else {
                self.core
                    .apply_char_format_native(c.section, c.paragraph, r.start, r.end, &props)
            }
            .map_err(err)?;
        }
        Ok(())
    }

    // ---- 머리말 / 꼬리말 · 각주 안 서식 ----
    // The caret model has no header/footer or footnote position (core area), so these
    // commands take the target in their parameters; the section is the caret's.

    /// The addressed header/footer paragraph of the caret section, from the model.
    fn fmt_hf_paragraph_ref(&self, header: bool, apply: u8, index: usize) -> Result<&Paragraph> {
        let want = match apply {
            1 => HeaderFooterApply::Even,
            2 => HeaderFooterApply::Odd,
            _ => HeaderFooterApply::Both,
        };
        let section = self
            .core
            .document()
            .sections
            .get(self.cursor.section)
            .ok_or_else(|| anyhow!("구역이 없습니다."))?;
        let name = if header { "머리말" } else { "꼬리말" };
        let paragraphs = section
            .paragraphs
            .iter()
            .flat_map(|p| p.controls.iter())
            .find_map(|c| match c {
                Control::Header(h) if header && h.apply_to == want => Some(&h.paragraphs),
                Control::Footer(f) if !header && f.apply_to == want => Some(&f.paragraphs),
                _ => None,
            })
            .ok_or_else(|| anyhow!("이 구역에 {name}이 없습니다."))?;
        paragraphs
            .get(index)
            .ok_or_else(|| anyhow!("{name} 문단 {index}이 없습니다."))
    }
    fn fmt_hf_character(&mut self, p: HfCharacter) -> Result<Value> {
        let apply = hf_apply_code(p.apply_to.as_deref())?;
        let len = self
            .fmt_hf_paragraph_ref(p.header, apply, p.paragraph)?
            .text
            .chars()
            .count();
        let (start, end) = (p.start.unwrap_or(0), p.end.unwrap_or(len));
        if start > end || end > len {
            bail!("글자 범위 {start}–{end}가 문단({len}자)을 벗어났습니다.");
        }
        if start == end {
            bail!("적용할 글자가 없습니다.");
        }
        validate_format(&p.format, true)?;
        let section = self.cursor.section;
        let query = HfQuery {
            header: p.header,
            apply_to: p.apply_to.clone(),
            paragraph: p.paragraph,
            offset: start,
        };
        let current = self.fmt_hf_character_get(&query)?;
        let props = self
            .character_parameters_from(p.format, Some(current))?
            .to_string();
        self.transaction(|s| {
            s.core
                .apply_char_format_in_header_footer_native(
                    section,
                    p.header,
                    apply,
                    p.paragraph,
                    start,
                    p.paragraph,
                    end,
                    &props,
                )
                .map_err(err)?;
            Ok(())
        })?;
        self.fmt_hf_character_get(&query)
    }
    fn fmt_hf_character_get(&self, q: &HfQuery) -> Result<Value> {
        let apply = hf_apply_code(q.apply_to.as_deref())?;
        let len = self
            .fmt_hf_paragraph_ref(q.header, apply, q.paragraph)?
            .text
            .chars()
            .count();
        let raw = self
            .core
            .get_char_properties_in_header_footer_native(
                self.cursor.section,
                q.header,
                apply,
                q.paragraph,
                q.offset.min(len.saturating_sub(1)),
            )
            .map_err(err)?;
        Ok(serde_json::from_str(&raw)?)
    }
    fn fmt_hf_paragraph(&mut self, p: HfParagraph) -> Result<Value> {
        let apply = hf_apply_code(p.apply_to.as_deref())?;
        self.fmt_hf_paragraph_ref(p.header, apply, p.paragraph)?;
        validate_format(&p.format, false)?;
        let section = self.cursor.section;
        let query = HfQuery {
            header: p.header,
            apply_to: p.apply_to.clone(),
            paragraph: p.paragraph,
            offset: 0,
        };
        let props = self
            .paragraph_parameters_from(p.format, || self.fmt_hf_paragraph_get(&query))?
            .to_string();
        self.transaction(|s| {
            s.core
                .apply_para_format_in_hf_native(section, p.header, apply, p.paragraph, &props)
                .map_err(err)?;
            Ok(())
        })?;
        self.fmt_hf_paragraph_get(&query)
    }
    fn fmt_hf_paragraph_get(&self, q: &HfQuery) -> Result<Value> {
        let apply = hf_apply_code(q.apply_to.as_deref())?;
        let shape = self
            .fmt_hf_paragraph_ref(q.header, apply, q.paragraph)?
            .para_shape_id;
        let raw = self
            .core
            .get_para_properties_in_hf_native(self.cursor.section, q.header, apply, q.paragraph)
            .map_err(err)?;
        self.with_raw_units(serde_json::from_str(&raw)?, shape)
    }
    /// Like `paragraph_format`: margins and spacing in raw units, as `format.paragraph` writes.
    fn with_raw_units(&self, mut value: Value, shape: u16) -> Result<Value> {
        let shape = self
            .core
            .document()
            .doc_info
            .para_shapes
            .get(shape as usize)
            .ok_or_else(|| anyhow!("문단 서식을 찾을 수 없습니다."))?;
        for (key, raw) in [
            ("marginLeft", shape.margin_left),
            ("marginRight", shape.margin_right),
            ("indent", shape.indent),
            ("spacingBefore", shape.spacing_before),
            ("spacingAfter", shape.spacing_after),
            ("lineSpacing", shape.line_spacing),
        ] {
            value[key] = json!(raw);
        }
        Ok(value)
    }
    /// The addressed footnote/endnote paragraph; the host paragraph is the caret's by default.
    fn fmt_footnote_paragraph_ref(
        &self,
        paragraph: Option<usize>,
        control: usize,
        index: usize,
    ) -> Result<(usize, &Paragraph)> {
        let host = Cursor {
            section: self.cursor.section,
            paragraph: paragraph.unwrap_or(self.cursor.paragraph),
            ..Default::default()
        };
        let paragraphs = match self.paragraph(host)?.controls.get(control) {
            Some(Control::Footnote(n)) => &n.paragraphs,
            Some(Control::Endnote(n)) => &n.paragraphs,
            _ => bail!("개체 {control}은 각주 / 미주가 아닙니다."),
        };
        let p = paragraphs
            .get(index)
            .ok_or_else(|| anyhow!("각주 / 미주 문단 {index}이 없습니다."))?;
        Ok((host.paragraph, p))
    }
    fn fmt_footnote_paragraph(&mut self, p: FootnoteParagraph) -> Result<Value> {
        let (host, _) =
            self.fmt_footnote_paragraph_ref(p.paragraph, p.control, p.footnote_paragraph)?;
        validate_format(&p.format, false)?;
        let section = self.cursor.section;
        let query = FootnoteQuery {
            paragraph: Some(host),
            control: p.control,
            footnote_paragraph: p.footnote_paragraph,
        };
        let props = self
            .paragraph_parameters_from(p.format, || self.fmt_footnote_paragraph_get(&query))?
            .to_string();
        self.transaction(|s| {
            s.core
                .apply_para_format_in_footnote_native(
                    section,
                    host,
                    p.control,
                    p.footnote_paragraph,
                    &props,
                )
                .map_err(err)?;
            Ok(())
        })?;
        self.fmt_footnote_paragraph_get(&query)
    }
    fn fmt_footnote_paragraph_get(&self, q: &FootnoteQuery) -> Result<Value> {
        let (host, para) =
            self.fmt_footnote_paragraph_ref(q.paragraph, q.control, q.footnote_paragraph)?;
        let shape = para.para_shape_id;
        let raw = self
            .core
            .get_para_properties_in_footnote_native(
                self.cursor.section,
                host,
                q.control,
                q.footnote_paragraph,
            )
            .map_err(err)?;
        self.with_raw_units(serde_json::from_str(&raw)?, shape)
    }
    fn step_character(&mut self, id: &str) -> Result<Value> {
        let p = self.character_properties()?;
        let up = id.ends_with(".increase");
        let delta = if up { 1 } else { -1 };
        let patch = if id.starts_with("format.font_size") {
            let size = p["fontSize"].as_i64().unwrap_or(1000);
            json!({"fontSize": (size + delta * 100).clamp(100, 409600)})
        } else {
            let (key, min, max) = if id.starts_with("format.ratio") {
                ("ratios", 50, 200)
            } else {
                ("spacings", -50, 50)
            };
            let default = if key == "ratios" { 100 } else { 0 };
            let values: Vec<i64> = (0..7)
                .map(|i| p[key][i].as_i64().unwrap_or(default))
                .map(|x| (x + delta).clamp(min, max))
                .collect();
            json!({ key: values })
        };
        self.format_character(patch)
    }

    // ---- paragraph tools ----

    fn step_paragraph(&mut self, id: &str) -> Result<Value> {
        let p = self.paragraph_format()?;
        let up = id.ends_with(".increase") || id.ends_with(".positive");
        let sign = if up { 1 } else { -1 };
        let raw = |k: &str| p[k].as_f64().unwrap_or(0.0).round() as i64;
        // Hangul 2024 moves indents and margins 1 pt per action (200 raw units),
        // measured through its ParagraphShapeIndentPositive / IncreaseLeftMargin actions.
        let step = 200;
        let patch = match id {
            "format.line_spacing.increase" | "format.line_spacing.decrease" => {
                if p["lineSpacingType"] == "Percent" {
                    json!({"lineSpacing": (raw("lineSpacing") + sign * 10).clamp(50, 500)})
                } else {
                    json!({"lineSpacing": (raw("lineSpacing") + sign * 100).clamp(100, 200000)})
                }
            }
            "format.indent.positive" | "format.indent.negative" => {
                json!({"indent": (raw("indent") + sign * step).clamp(-100000, 100000)})
            }
            "format.margin_left.increase" | "format.margin_left.decrease" => {
                json!({"marginLeft": (raw("marginLeft") + sign * step).clamp(0, 100000)})
            }
            "format.margin_right.increase" | "format.margin_right.decrease" => {
                json!({"marginRight": (raw("marginRight") + sign * step).clamp(0, 100000)})
            }
            _ => json!({
                "marginLeft": (raw("marginLeft") + sign * step).clamp(0, 100000),
                "marginRight": (raw("marginRight") + sign * step).clamp(0, 100000),
            }),
        };
        self.format_paragraph(patch)
    }

    // ---- styles ----

    fn style_list(&self) -> Value {
        let doc = self.core.document();
        let current = self
            .paragraph(self.cursor)
            .map(|p| p.style_id as usize)
            .ok();
        json!(
            doc.doc_info
                .styles
                .iter()
                .enumerate()
                .map(|(id, s)| json!({
                    "id": id,
                    "name": s.local_name,
                    "englishName": s.english_name,
                    "type": if s.style_type == 1 { "character" } else { "paragraph" },
                    "nextStyleId": s.next_style_id,
                    "shortcut": style_shortcut(id),
                    "current": current == Some(id),
                }))
                .collect::<Vec<_>>()
        )
    }
    fn char_shape_json(&self, id: u16) -> Value {
        let doc = self.core.document();
        let Some(cs) = doc.doc_info.char_shapes.get(id as usize) else {
            return json!({});
        };
        let fonts: Vec<String> = (0..7)
            .map(|lang| {
                doc.doc_info
                    .font_faces
                    .get(lang)
                    .and_then(|f| f.get(cs.font_ids[lang] as usize))
                    .map(|f| f.name.clone())
                    .unwrap_or_default()
            })
            .collect();
        let bf = cs
            .border_fill_id
            .checked_sub(1)
            .and_then(|i| doc.doc_info.border_fills.get(i as usize));
        let mut v = json!({
            "fontFamily": fonts[0],
            "fontFamilies": fonts,
            "fontSize": cs.base_size,
            "bold": cs.bold,
            "italic": cs.italic,
            "underline": cs.underline_type != UnderlineType::None,
            "underlineType": match cs.underline_type {
                UnderlineType::None => "None",
                UnderlineType::Bottom => "Bottom",
                UnderlineType::Top => "Top",
            },
            "strikethrough": cs.strikethrough,
            "superscript": cs.superscript,
            "subscript": cs.subscript,
            "emboss": cs.emboss,
            "engrave": cs.engrave,
            "kerning": cs.kerning,
            "outlineType": cs.outline_type,
            "shadowType": cs.shadow_type,
            "shadowOffsetX": cs.shadow_offset_x,
            "shadowOffsetY": cs.shadow_offset_y,
            "emphasisDot": cs.emphasis_dot,
            "underlineShape": cs.underline_shape,
            "strikeShape": cs.strike_shape,
            "textColor": css(cs.text_color),
            "shadeColor": css(cs.shade_color),
            "underlineColor": css(cs.underline_color),
            "strikeColor": css(cs.strike_color),
            "shadowColor": css(cs.shadow_color),
            "ratios": arr(cs.ratios),
            "spacings": arr(cs.spacings),
            "relativeSizes": arr(cs.relative_sizes),
            "charOffsets": arr(cs.char_offsets),
        });
        if let (Some(o), Value::Object(b)) =
            (v.as_object_mut(), border_fill_json(bf, cs.border_fill_id))
        {
            o.extend(b);
        }
        v
    }
    fn para_shape_json(&self, id: u16) -> Value {
        let doc = self.core.document();
        let Some(ps) = doc.doc_info.para_shapes.get(id as usize) else {
            return json!({});
        };
        let tabs = doc.doc_info.tab_defs.get(ps.tab_def_id as usize);
        let bit = |n: u32| (ps.attr1 >> n) & 1 != 0;
        let bf = ps
            .border_fill_id
            .checked_sub(1)
            .and_then(|i| doc.doc_info.border_fills.get(i as usize));
        let mut v = json!({
            "alignment": alignment_name(ps.alignment),
            "marginLeft": ps.margin_left,
            "marginRight": ps.margin_right,
            "indent": ps.indent,
            "spacingBefore": ps.spacing_before,
            "spacingAfter": ps.spacing_after,
            "lineSpacing": ps.line_spacing,
            "lineSpacingType": format!("{:?}", ps.line_spacing_type),
            "headType": head_name(ps.head_type),
            "paraLevel": ps.para_level,
            "numberingId": ps.numbering_id,
            "widowOrphan": bit(16),
            "keepWithNext": bit(17),
            "keepLines": bit(18),
            "pageBreakBefore": bit(19),
            "fontLineHeight": bit(22),
            "singleLine": ps.attr2 & 0x03 != 0,
            "autoSpaceKrEn": (ps.attr2 >> 4) & 1 != 0,
            "autoSpaceKrNum": (ps.attr2 >> 5) & 1 != 0,
            "verticalAlign": (ps.attr1 >> 20) & 0x03,
            "englishBreakUnit": (ps.attr1 >> 5) & 0x03,
            "koreanBreakUnit": (ps.attr1 >> 7) & 0x01,
            "tabAutoLeft": tabs.is_some_and(|t| t.auto_tab_left),
            "tabAutoRight": tabs.is_some_and(|t| t.auto_tab_right),
            "tabStops": tabs.map(|t| t.tabs.iter().map(|s| json!({
                "position": s.position, "type": s.tab_type, "fill": s.fill_type,
            })).collect::<Vec<_>>()).unwrap_or_default(),
            "borderSpacing": ps.border_spacing,
            "borderConnect": bit(28),
            "borderIgnoreMargin": bit(29),
        });
        if let (Some(o), Value::Object(b)) =
            (v.as_object_mut(), border_fill_json(bf, ps.border_fill_id))
        {
            o.extend(b);
        }
        v
    }
    fn style_detail(&self, id: usize) -> Result<Value> {
        let s = self
            .core
            .document()
            .doc_info
            .styles
            .get(id)
            .ok_or_else(|| anyhow!("스타일을 찾지 못했습니다."))?;
        Ok(json!({
            "id": id,
            "name": s.local_name,
            "englishName": s.english_name,
            "type": if s.style_type == 1 { "character" } else { "paragraph" },
            "nextStyleId": s.next_style_id,
            "shortcut": style_shortcut(id),
            "character": self.char_shape_json(s.char_shape_id),
            "paragraph": self.para_shape_json(s.para_shape_id),
        }))
    }
    fn validate_style_name(&self, name: &str, english: &str, except: Option<usize>) -> Result<()> {
        if name.trim().is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control)
        {
            bail!("스타일 이름은 1–64자이며 제어 문자를 쓸 수 없습니다.");
        }
        if english.chars().count() > 64 || english.chars().any(char::is_control) {
            bail!("영문 이름은 64자 이하이며 제어 문자를 쓸 수 없습니다.");
        }
        if self
            .core
            .document()
            .doc_info
            .styles
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != except && s.local_name == name.trim())
        {
            bail!("같은 이름의 스타일이 이미 있습니다: {name}");
        }
        Ok(())
    }
    fn create_style(&mut self, p: CreateStyle) -> Result<Value> {
        let english = p.english_name.unwrap_or_default();
        self.validate_style_name(&p.name, &english, None)?;
        let count = self.core.document().doc_info.styles.len();
        if count >= 255 {
            bail!("스타일은 255개까지 만들 수 있습니다.");
        }
        let character = match p.kind.as_deref().unwrap_or("paragraph") {
            "paragraph" => false,
            "character" => true,
            other => bail!("스타일 종류는 paragraph 또는 character입니다: {other}"),
        };
        if p.next_style_id.is_some_and(|n| n > count) {
            bail!("다음 스타일을 찾지 못했습니다.");
        }
        if let Some(v) = &p.character {
            validate_format(v, true)?;
        }
        if let Some(v) = &p.paragraph {
            validate_format(v, false)?;
        }
        // Hangul's 스타일 추가하기 takes the caret's current paragraph and character shapes.
        let para = self.paragraph(self.cursor)?;
        let offset = self.selection().start.min(self.len().saturating_sub(1));
        let char_shape_id = para.char_shape_id_at(offset).unwrap_or(0) as u16;
        let para_shape_id = if character {
            self.core
                .document()
                .doc_info
                .styles
                .first()
                .map(|s| s.para_shape_id)
                .unwrap_or(0)
        } else {
            para.para_shape_id
        };
        let name = p.name.trim().to_owned();
        self.transaction_value(|s| {
            let doc = s.core.document_mut();
            doc.doc_info.styles.push(Style {
                raw_data: None,
                local_name: name,
                english_name: english,
                style_type: u8::from(character),
                next_style_id: p.next_style_id.unwrap_or(count) as u8,
                lang_id: 1042,
                para_shape_id,
                char_shape_id,
                lock_form: false,
            });
            doc.doc_info.raw_stream_dirty = true;
            s.refresh_layout()?;
            if p.character.is_some() || p.paragraph.is_some() {
                let style = s.core.document().doc_info.styles[count].clone();
                let (name, english) = (style.local_name.clone(), style.english_name.clone());
                let update = UpdateStyle {
                    id: count,
                    name: None,
                    english_name: None,
                    next_style_id: None,
                    character: p.character,
                    paragraph: p.paragraph,
                };
                s.update_style_now(update, style, name, english)?;
            }
            Ok(json!({"id": count}))
        })
    }
    fn update_style(&mut self, p: UpdateStyle) -> Result<Value> {
        let style = self
            .core
            .document()
            .doc_info
            .styles
            .get(p.id)
            .cloned()
            .ok_or_else(|| anyhow!("스타일을 찾지 못했습니다."))?;
        let name = p.name.clone().unwrap_or_else(|| style.local_name.clone());
        let english = p
            .english_name
            .clone()
            .unwrap_or_else(|| style.english_name.clone());
        self.validate_style_name(&name, &english, Some(p.id))?;
        if p.next_style_id
            .is_some_and(|n| n >= self.core.document().doc_info.styles.len())
        {
            bail!("다음 스타일을 찾지 못했습니다.");
        }
        if let Some(v) = &p.character {
            validate_format(v, true)?;
        }
        if let Some(v) = &p.paragraph {
            validate_format(v, false)?;
        }
        self.transaction_value(|s| s.update_style_now(p, style, name, english))
    }
    fn update_style_now(
        &mut self,
        p: UpdateStyle,
        style: Style,
        name: String,
        english: String,
    ) -> Result<Value> {
        let s = self;
        {
            let old_cs = style.char_shape_id;
            let old_ps = style.para_shape_id;
            let mut new_cs = old_cs;
            let mut new_ps = old_ps;
            if let Some(v) = p.character {
                let mut v = s.character_parameters(v)?;
                complete_from(&mut v, &s.char_shape_json(old_cs));
                let mut mods = char_mods(&v);
                mods.border_fill_id = s.border_fill_for(&v, old_cs, true);
                new_cs = s
                    .core
                    .document_mut()
                    .find_or_create_char_shape(u32::from(old_cs), &mods)
                    as u16;
            }
            if let Some(v) = p.paragraph {
                let mut v = s.paragraph_parameters(v)?;
                complete_from(&mut v, &s.para_shape_json(old_ps));
                let mut mods = para_mods(&v);
                mods.border_fill_id = s.border_fill_for(&v, old_ps, false);
                if v.get("tabStops").is_some()
                    || v.get("tabAutoLeft").is_some()
                    || v.get("tabAutoRight").is_some()
                {
                    let doc = s.core.document_mut();
                    let base = doc
                        .doc_info
                        .para_shapes
                        .get(old_ps as usize)
                        .and_then(|ps| doc.doc_info.tab_defs.get(ps.tab_def_id as usize))
                        .cloned()
                        .unwrap_or_default();
                    mods.tab_def_id = Some(doc.find_or_create_tab_def(tab_def(&v, &base)));
                }
                new_ps = s
                    .core
                    .document_mut()
                    .find_or_create_para_shape(old_ps, &mods);
            }
            let doc = s.core.document_mut();
            let target = &mut doc.doc_info.styles[p.id];
            target.local_name = name.trim().to_owned();
            target.english_name = english;
            if let Some(n) = p.next_style_id {
                target.next_style_id = n as u8;
            }
            target.char_shape_id = new_cs;
            target.para_shape_id = new_ps;
            target.raw_data = None;
            doc.doc_info.raw_stream_dirty = true;
            // Propagate like Hangul: paragraphs keep direct formatting; runs that used the
            // style's character shape follow the edited style.
            let paragraph_style = style.style_type == 0;
            let mut touched = Vec::new();
            for (si, section) in doc.sections.iter_mut().enumerate() {
                section.raw_stream = None;
                for (pi, para) in section.paragraphs.iter_mut().enumerate() {
                    let mut visit = |para: &mut hwpcraft_doc::model::paragraph::Paragraph,
                                     cell: Option<Cell>| {
                        if para.style_id as usize == p.id {
                            if paragraph_style && para.para_shape_id == old_ps {
                                para.para_shape_id = new_ps;
                            }
                            para.replace_style_char_shape_preserving_overrides(
                                u32::from(old_cs),
                                u32::from(new_cs),
                            );
                            touched.push(Cursor {
                                section: si,
                                paragraph: pi,
                                offset: 0,
                                cell,
                            });
                        }
                    };
                    for (ci, control) in para.controls.iter_mut().enumerate() {
                        if let Control::Table(t) = control {
                            for (ti, cell) in t.cells.iter_mut().enumerate() {
                                for (k, cp) in cell.paragraphs.iter_mut().enumerate() {
                                    visit(
                                        cp,
                                        Some(Cell {
                                            control: ci,
                                            index: ti,
                                            paragraph: k,
                                        }),
                                    );
                                }
                            }
                        }
                    }
                    visit(para, None);
                }
            }
            s.refresh_layout()?;
            for c in touched {
                s.reflow(c)?;
            }
            Ok(json!({"id": p.id}))
        }
    }
    /// Re-run RHWP's line breaking for one paragraph by re-applying its own left margin.
    fn reflow(&mut self, c: Cursor) -> Result<()> {
        let margin = {
            let para = self.paragraph(c)?;
            self.core
                .document()
                .doc_info
                .para_shapes
                .get(para.para_shape_id as usize)
                .map(|p| p.margin_left)
                .unwrap_or(0)
        };
        let props = json!({"marginLeft": margin}).to_string();
        if let Some(t) = c.cell {
            self.core.apply_para_format_in_cell_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
                &props,
            )
        } else {
            self.core
                .apply_para_format_native(c.section, c.paragraph, &props)
        }
        .map_err(err)?;
        Ok(())
    }
    /// BorderFill id (1-based) for a style shape edit, reusing an identical entry.
    fn border_fill_for(&mut self, v: &Value, shape: u16, character: bool) -> Option<u16> {
        formatting::BORDER_FILL_KEYS
            .iter()
            .any(|k| v.get(*k).is_some())
            .then(|| {
                let doc = self.core.document_mut();
                let current = if character {
                    doc.doc_info
                        .char_shapes
                        .get(shape as usize)
                        .map(|c| c.border_fill_id)
                } else {
                    doc.doc_info
                        .para_shapes
                        .get(shape as usize)
                        .map(|p| p.border_fill_id)
                }
                .unwrap_or(0);
                let base = current
                    .checked_sub(1)
                    .and_then(|i| doc.doc_info.border_fills.get(i as usize));
                let bf = border_fill(v, base);
                let none = bf
                    .borders
                    .iter()
                    .all(|b| b.line_type == BorderLineType::None)
                    && bf.fill.fill_type == FillType::None;
                if none {
                    return 0;
                }
                if let Some(i) = doc.doc_info.border_fills.iter().position(|b| *b == bf) {
                    return (i + 1) as u16;
                }
                doc.doc_info.border_fills.push(bf);
                doc.doc_info.raw_stream_dirty = true;
                doc.doc_info.border_fills.len() as u16
            })
    }
    fn delete_style(&mut self, p: DeleteStyle) -> Result<Value> {
        let styles = &self.core.document().doc_info.styles;
        if p.id == 0 {
            bail!("바탕글 스타일은 지울 수 없습니다.");
        }
        if p.id >= styles.len() {
            bail!("스타일을 찾지 못했습니다.");
        }
        let replacement = p.replacement.unwrap_or(0);
        if replacement == p.id || replacement >= styles.len() {
            bail!("대치할 스타일을 찾지 못했습니다.");
        }
        let users: Vec<Cursor> = self
            .entries()
            .into_iter()
            .map(|(c, _)| c)
            .filter(|c| {
                self.paragraph(*c)
                    .is_ok_and(|para| para.style_id as usize == p.id)
            })
            .collect();
        self.transaction_value(|s| {
            // Hangul gives the paragraphs the replacement style's formatting first.
            for c in &users {
                if let Some(t) = c.cell {
                    s.core.apply_cell_style_native(
                        c.section,
                        c.paragraph,
                        t.control,
                        t.index,
                        t.paragraph,
                        replacement,
                    )
                } else {
                    s.core
                        .apply_style_native(c.section, c.paragraph, replacement)
                }
                .map_err(err)?;
            }
            let removed = p.id as u8;
            let doc = s.core.document_mut();
            doc.doc_info.styles.remove(p.id);
            for style in &mut doc.doc_info.styles {
                if style.next_style_id == removed {
                    style.next_style_id = 0;
                } else if style.next_style_id > removed {
                    style.next_style_id -= 1;
                }
            }
            let shift = |para: &mut hwpcraft_doc::model::paragraph::Paragraph| {
                if para.style_id > removed {
                    para.style_id -= 1;
                }
            };
            for section in &mut doc.sections {
                section.raw_stream = None;
                for para in &mut section.paragraphs {
                    for control in &mut para.controls {
                        if let Control::Table(t) = control {
                            for cell in &mut t.cells {
                                cell.paragraphs.iter_mut().for_each(shift);
                            }
                        }
                    }
                    shift(para);
                }
            }
            doc.doc_info.raw_stream_dirty = true;
            s.refresh_layout()?;
            Ok(json!({"replaced": users.len()}))
        })
    }
    /// `number` is the digit key: 1 … 9 and 0 (= tenth style).
    fn style_shortcut(&mut self, number: usize) -> Result<Value> {
        if number > 9 {
            bail!("스타일 단축키는 Ctrl+1 … Ctrl+0입니다.");
        }
        let index = (number + 9) % 10;
        if index >= self.core.document().doc_info.styles.len() {
            bail!("Ctrl+{number}에 해당하는 스타일이 없습니다.");
        }
        self.apply_style(index)?;
        Ok(self.info())
    }

    // ---- numbering / bullets / outline ----

    fn numbering_list(&self) -> Value {
        let previous = self.previous_lists().unwrap_or_default();
        let doc = self.core.document();
        let outline = doc
            .sections
            .get(self.cursor.section)
            .map(|s| s.section_def.outline_numbering_id)
            .unwrap_or(0);
        let info = &doc.doc_info;
        json!({
            "previous": previous,
            "outlineId": outline,
            "numberings": info.numberings.iter().enumerate().map(|(i, n)| json!({
                "id": i + 1,
                "levelFormats": n.level_formats,
                "numberFormats": n.heads.iter().map(|h| h.number_format).collect::<Vec<_>>(),
                "startNumber": n.start_number,
                "levelStartNumbers": n.level_start_numbers,
            })).collect::<Vec<_>>(),
            "bullets": info.bullets.iter().enumerate().map(|(i, b)| json!({
                "id": i + 1,
                "char": b.bullet_char.to_string(),
            })).collect::<Vec<_>>(),
        })
    }
    /// 개요 번호 모양: the current section's outline numbering (SectionDef outlineShapeIDRef).
    fn outline_shape(&mut self, p: CreateNumbering) -> Result<Value> {
        let section = self.cursor.section;
        if self.core.document().sections.get(section).is_none() {
            bail!("구역이 없습니다.");
        }
        self.transaction_value(|s| {
            let id = s.find_or_create_numbering(p)?;
            let doc = s.core.document_mut();
            let target = &mut doc.sections[section];
            target.section_def.outline_numbering_id = id;
            // HWP saving writes the first paragraph's SectionDef control, so keep both in step.
            for control in target
                .paragraphs
                .iter_mut()
                .flat_map(|p| p.controls.iter_mut())
            {
                if let Control::SectionDef(def) = control {
                    def.outline_numbering_id = id;
                }
            }
            target.raw_stream = None;
            s.refresh_layout()?;
            Ok(json!({ "id": id }))
        })
    }
    /// 문단 모양 창: shape values and 문단 종류 in one undo step.
    fn paragraph_dialog(&mut self, p: ParagraphDialog) -> Result<Value> {
        if p.format.get("headType").is_some() || p.format.get("numberingId").is_some() {
            bail!("문단 종류는 head로 지정합니다.");
        }
        validate_format(&p.format, false)?;
        let head = match &p.head {
            None => None,
            Some(h) => {
                if h.level.is_some_and(|l| l > 6) {
                    bail!("수준은 1–7(0–6)입니다.");
                }
                Some((
                    match h.kind.as_str() {
                        "none" => HeadKind::None,
                        "outline" => HeadKind::Outline,
                        "number" => HeadKind::Number,
                        "bullet" => HeadKind::Bullet,
                        other => bail!("문단 종류는 none/outline/number/bullet입니다: {other}"),
                    },
                    h.level,
                ))
            }
        };
        let empty = p.format.as_object().is_none_or(|o| o.is_empty());
        self.transaction(|s| {
            if !empty {
                for (c, _) in s.selected_paragraphs()? {
                    s.apply_paragraph_at(c, p.format.clone())?;
                }
            }
            if let Some((kind, level)) = head {
                // Keep an existing definition of the same kind; otherwise use the default.
                let current = s.para_shape_of(s.cursor)?;
                let id = (current.head_type == kind.head() && current.numbering_id > 0)
                    .then_some(current.numbering_id)
                    .filter(|_| matches!(kind, HeadKind::Number | HeadKind::Bullet));
                s.set_head(kind, id, None, level)?;
            }
            Ok(())
        })?;
        Ok(self.info())
    }
    fn find_or_create_numbering(&mut self, p: CreateNumbering) -> Result<u16> {
        let start = p.start_number.unwrap_or(1);
        if let Some(i) = self
            .core
            .document()
            .doc_info
            .numberings
            .iter()
            .position(|n| {
                n.level_formats.as_slice() == p.level_formats.as_slice()
                    && n.heads
                        .iter()
                        .map(|h| h.number_format)
                        .eq(p.number_formats.iter().copied())
                    && n.start_number == start
            })
        {
            return Ok((i + 1) as u16);
        }
        Ok(self.create_numbering(p)?["id"].as_u64().unwrap_or(1) as u16)
    }
    fn create_numbering(&mut self, p: CreateNumbering) -> Result<Value> {
        if p.level_formats.len() != 7 || p.number_formats.len() != 7 {
            bail!("수준 1–7의 번호 형식 7개가 필요합니다.");
        }
        for f in &p.level_formats {
            if f.chars().count() > 32 || f.chars().any(char::is_control) {
                bail!("번호 형식은 32자 이하이며 제어 문자를 쓸 수 없습니다.");
            }
        }
        if p.number_formats.iter().any(|n| *n > 16) {
            bail!("번호 모양 코드는 0–16입니다.");
        }
        let start = p.start_number.unwrap_or(1);
        if start == 0 {
            bail!("시작 번호는 1 이상입니다.");
        }
        let mut n = Numbering {
            start_number: start,
            level_start_numbers: [u32::from(start); 7],
            ..Default::default()
        };
        for level in 0..7 {
            n.level_formats[level] = p.level_formats[level].clone();
            // Hangul's default head: automatic hanging indent (attr bit 3).
            n.heads[level] = NumberingHead {
                number_format: p.number_formats[level],
                attr: 1 << 3,
                ..Default::default()
            };
        }
        let doc = self.core.document_mut();
        doc.doc_info.numberings.push(n);
        doc.doc_info.raw_stream_dirty = true;
        Ok(json!({"id": doc.doc_info.numberings.len()}))
    }
    /// Hangul's default 문단 번호 모양: 1. 가) (1) (가) ① ㄱ) a).
    fn default_numbering(&mut self) -> Result<u16> {
        if !self.core.document().doc_info.numberings.is_empty() {
            return Ok(1);
        }
        let v = self.create_numbering(CreateNumbering {
            level_formats: ["^1.", "^2)", "(^3)", "(^4)", "^5", "^6)", "^7)"]
                .map(String::from)
                .to_vec(),
            number_formats: vec![0, 8, 0, 8, 1, 10, 5],
            start_number: Some(1),
        })?;
        Ok(v["id"].as_u64().unwrap_or(1) as u16)
    }
    fn bullet(&mut self, ch: char) -> u16 {
        let doc = self.core.document_mut();
        if let Some(i) = doc
            .doc_info
            .bullets
            .iter()
            .position(|b| b.bullet_char == ch)
        {
            return (i + 1) as u16;
        }
        doc.doc_info.bullets.push(Bullet {
            bullet_char: ch,
            attr: 1 << 3,
            text_distance: 50,
            ..Default::default()
        });
        doc.doc_info.raw_stream_dirty = true;
        doc.doc_info.bullets.len() as u16
    }
    fn apply_numbering(&mut self, p: ApplyNumbering) -> Result<Value> {
        let kind = match p.kind.as_str() {
            "number" => HeadKind::Number,
            "bullet" => HeadKind::Bullet,
            "outline" => HeadKind::Outline,
            "none" => HeadKind::None,
            other => bail!("번호 종류는 number/bullet/outline/none입니다: {other}"),
        };
        if p.level.is_some_and(|l| l > 6) {
            bail!("수준은 1–7(0–6)입니다.");
        }
        let info = &self.core.document().doc_info;
        if let Some(id) = p.id {
            let count = match kind {
                HeadKind::Number => info.numberings.len(),
                HeadKind::Bullet => info.bullets.len(),
                _ => bail!("개요와 해제에는 id를 지정하지 않습니다."),
            };
            if id == 0 || usize::from(id) > count {
                bail!("번호 / 글머리표 모양 {id}을 찾지 못했습니다.");
            }
        }
        let bullet_char = match p.char.as_deref() {
            None => None,
            Some(s) if kind == HeadKind::Bullet && s.chars().count() == 1 => s.chars().next(),
            Some(_) => bail!("글머리표 문자는 한 글자입니다."),
        };
        let inline = match (p.level_formats, p.number_formats) {
            (None, None) => None,
            (Some(level_formats), Some(number_formats))
                if kind == HeadKind::Number && p.id.is_none() =>
            {
                Some(CreateNumbering {
                    level_formats,
                    number_formats,
                    start_number: p.start_number,
                })
            }
            _ => bail!(
                "번호 모양은 levelFormats와 numberFormats를 함께 주고 id 없이 number에만 씁니다."
            ),
        };
        let restart = p.restart.map(restart_value).transpose()?;
        self.transaction(|s| {
            let id = match inline {
                Some(n) => Some(s.find_or_create_numbering(n)?),
                None => p.id,
            };
            s.set_head(kind, id, bullet_char, p.level)?;
            if let Some(r) = restart {
                s.set_restart_now(r)?;
            }
            Ok(())
        })?;
        Ok(self.info())
    }
    /// Applies to every selected paragraph; each keeps its own level unless one is given.
    fn set_head(
        &mut self,
        kind: HeadKind,
        id: Option<u16>,
        bullet_char: Option<char>,
        level: Option<u8>,
    ) -> Result<()> {
        let (head, id) = match kind {
            HeadKind::None => ("None", None),
            HeadKind::Outline => ("Outline", Some(0)),
            HeadKind::Number => (
                "Number",
                Some(match id {
                    Some(id) => id,
                    None => self.default_numbering()?,
                }),
            ),
            HeadKind::Bullet => (
                "Bullet",
                Some(match id {
                    Some(id) => id,
                    None => self.bullet(bullet_char.unwrap_or('●')),
                }),
            ),
        };
        for (c, _) in self.selected_paragraphs()? {
            let mut patch = json!({ "headType": head });
            if let Some(id) = id {
                let level = level.unwrap_or(self.para_shape_of(c)?.para_level.min(6));
                patch["numberingId"] = json!(id);
                patch["paraLevel"] = json!(level);
            }
            self.apply_paragraph_at(c, patch)?;
        }
        Ok(())
    }
    /// The caret paragraph decides: on when it has this kind of head, otherwise off.
    fn toggle_numbering(&mut self, kind: HeadKind) -> Result<Value> {
        let on = self.paragraph_format()?["headType"] == kind.name();
        self.transaction(|s| s.set_head(if on { HeadKind::None } else { kind }, None, None, None))?;
        Ok(self.info())
    }
    /// "한 수준 증가" goes one level deeper (1 → 2). Paragraphs without a head, or
    /// already at level 1 / 7, are left unchanged.
    fn step_level(&mut self, deeper: bool) -> Result<Value> {
        let mut changes = Vec::new();
        for (c, _) in self.selected_paragraphs()? {
            let shape = self.para_shape_of(c)?;
            let next = i64::from(shape.para_level) + if deeper { 1 } else { -1 };
            if shape.head_type != HeadType::None && (0..=6).contains(&next) {
                changes.push((c, next));
            }
        }
        if changes.is_empty() {
            return Ok(self.info());
        }
        self.transaction(|s| {
            for (c, next) in changes {
                s.apply_paragraph_at(c, json!({ "paraLevel": next }))?;
            }
            Ok(())
        })?;
        Ok(self.info())
    }
    fn restart_numbering(&mut self, p: RestartParams) -> Result<Value> {
        let restart = restart_value(p)?;
        self.paragraph(self.cursor)?;
        self.transaction(|s| s.set_restart_now(restart))?;
        Ok(self.info())
    }
    /// HWP saves no per-paragraph restart flag (RHWP keeps `numbering_restart` in memory
    /// only). Hangul expresses 시작 번호 방식 through the numbering id: the same id as the
    /// preceding numbered paragraph continues it, an earlier list's id resumes that list,
    /// and a new definition with its own start number begins a new list. The caret
    /// paragraph and the rest of its list in the same container move to the chosen id.
    /// Numbering ids of 번호 문단 before the caret in its container, most recent last,
    /// each listed once (at its latest position).
    fn previous_lists(&self) -> Result<Vec<u16>> {
        let c = self.cursor;
        let position = |a: &Cursor| a.cell.map(|t| t.paragraph).unwrap_or(a.paragraph);
        let mut out: Vec<u16> = Vec::new();
        for (p, _) in self.entries() {
            let same = p.section == c.section
                && match (p.cell, c.cell) {
                    (None, None) => true,
                    (Some(x), Some(y)) => {
                        p.paragraph == c.paragraph && x.control == y.control && x.index == y.index
                    }
                    _ => false,
                };
            if !same || position(&p) >= position(&c) {
                continue;
            }
            let ps = self.para_shape_of(p)?;
            if ps.head_type == HeadType::Number && ps.numbering_id > 0 {
                out.retain(|id| *id != ps.numbering_id);
                out.push(ps.numbering_id);
            }
        }
        Ok(out)
    }
    fn set_restart_now(&mut self, restart: Restart) -> Result<()> {
        let c = self.cursor;
        let shape = self.para_shape_of(c)?;
        if shape.head_type != HeadType::Number || shape.numbering_id == 0 {
            bail!("시작 번호 방식은 문단 번호가 있는 문단에서만 바꿀 수 있습니다.");
        }
        let old = shape.numbering_id;
        let same_container = |a: &Cursor| {
            a.section == c.section
                && match (a.cell, c.cell) {
                    (None, None) => true,
                    (Some(x), Some(y)) => {
                        a.paragraph == c.paragraph && x.control == y.control && x.index == y.index
                    }
                    _ => false,
                }
        };
        let position = |a: &Cursor| a.cell.map(|t| t.paragraph).unwrap_or(a.paragraph);
        let here = position(&c);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for (p, _) in self.entries() {
            if !same_container(&p) {
                continue;
            }
            let ps = self.para_shape_of(p)?;
            if ps.head_type != HeadType::Number {
                continue;
            }
            if position(&p) < here {
                before.push(ps.numbering_id);
            } else if position(&p) > here && ps.numbering_id == old {
                after.push(p);
            }
        }
        let target = match restart {
            Restart::Continue => *before
                .last()
                .ok_or_else(|| anyhow!("이어 갈 앞 번호 목록이 없습니다."))?,
            Restart::Previous => {
                let last = before.last().copied();
                *before
                    .iter()
                    .rev()
                    .find(|id| Some(**id) != last)
                    .ok_or_else(|| anyhow!("이어 갈 이전 번호 목록이 없습니다."))?
            }
            Restart::New(start) => {
                let doc = self.core.document_mut();
                let mut n = doc
                    .doc_info
                    .numberings
                    .get(usize::from(old) - 1)
                    .cloned()
                    .ok_or_else(|| anyhow!("문단 번호 모양을 찾지 못했습니다."))?;
                n.raw_data = None;
                n.level_start_numbers[usize::from(shape.para_level.min(6))] = start;
                if shape.para_level == 0 {
                    n.start_number = u16::try_from(start).unwrap_or(u16::MAX);
                }
                doc.doc_info.numberings.push(n);
                doc.doc_info.raw_stream_dirty = true;
                doc.doc_info.numberings.len() as u16
            }
        };
        if target == old && !matches!(restart, Restart::New(_)) {
            return Ok(());
        }
        self.apply_paragraph_at(c, json!({"numberingId": target}))?;
        for p in after {
            self.apply_paragraph_at(p, json!({"numberingId": target}))?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyShape {
    #[serde(default)]
    character: Option<Value>,
    #[serde(default)]
    paragraph: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HfCharacter {
    header: bool,
    #[serde(default)]
    apply_to: Option<String>,
    #[serde(default)]
    paragraph: usize,
    #[serde(default)]
    start: Option<usize>,
    #[serde(default)]
    end: Option<usize>,
    format: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HfParagraph {
    header: bool,
    #[serde(default)]
    apply_to: Option<String>,
    #[serde(default)]
    paragraph: usize,
    format: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HfQuery {
    header: bool,
    #[serde(default)]
    apply_to: Option<String>,
    #[serde(default)]
    paragraph: usize,
    #[serde(default)]
    offset: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FootnoteParagraph {
    #[serde(default)]
    paragraph: Option<usize>,
    control: usize,
    #[serde(default)]
    footnote_paragraph: usize,
    format: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FootnoteQuery {
    #[serde(default)]
    paragraph: Option<usize>,
    control: usize,
    #[serde(default)]
    footnote_paragraph: usize,
}
fn hf_apply_code(apply_to: Option<&str>) -> Result<u8> {
    Ok(match apply_to.unwrap_or("both") {
        "both" => 0,
        "even" => 1,
        "odd" => 2,
        other => bail!("applyTo는 both, odd, even 중 하나입니다: {other}"),
    })
}
fn hex_color(value: &str) -> Result<String> {
    let hex = value.strip_prefix('#').unwrap_or("");
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("색은 #rrggbb 형식이어야 합니다: {value}");
    }
    Ok(value.to_ascii_lowercase())
}
/// `(start, end, "#rrggbb")` highlight ranges of a paragraph, in text order.
fn markpen_ranges(para: &Paragraph) -> Vec<(usize, usize, String)> {
    let len = para.text.chars().count();
    let mut marks: Vec<&MarkpenMark> = para.markpen_marks.iter().collect();
    marks.sort_by_key(|m| m.char_idx);
    let mut open: Vec<(usize, String)> = Vec::new();
    let mut out = Vec::new();
    for m in marks {
        let idx = m.char_idx.min(len);
        match &m.color {
            Some(c) => open.push((idx, c.to_ascii_lowercase())),
            None => {
                if let Some((start, c)) = open.pop()
                    && idx > start
                {
                    out.push((start, idx, c));
                }
            }
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeadKind {
    None,
    Outline,
    Number,
    Bullet,
}
impl HeadKind {
    fn head(self) -> HeadType {
        match self {
            HeadKind::None => HeadType::None,
            HeadKind::Outline => HeadType::Outline,
            HeadKind::Number => HeadType::Number,
            HeadKind::Bullet => HeadType::Bullet,
        }
    }
    fn name(self) -> &'static str {
        match self {
            HeadKind::None => "None",
            HeadKind::Outline => "Outline",
            HeadKind::Number => "Number",
            HeadKind::Bullet => "Bullet",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CreateStyle {
    name: String,
    #[serde(default)]
    english_name: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    next_style_id: Option<usize>,
    #[serde(default)]
    character: Option<Value>,
    #[serde(default)]
    paragraph: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct UpdateStyle {
    id: usize,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    english_name: Option<String>,
    #[serde(default)]
    next_style_id: Option<usize>,
    #[serde(default)]
    character: Option<Value>,
    #[serde(default)]
    paragraph: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteStyle {
    id: usize,
    #[serde(default)]
    replacement: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CreateNumbering {
    level_formats: Vec<String>,
    number_formats: Vec<u8>,
    #[serde(default)]
    start_number: Option<u16>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ApplyNumbering {
    kind: String,
    #[serde(default)]
    id: Option<u16>,
    #[serde(default)]
    char: Option<String>,
    #[serde(default)]
    level: Option<u8>,
    /// Inline 문단 번호 모양: reuses an identical definition or creates one.
    #[serde(default)]
    level_formats: Option<Vec<String>>,
    #[serde(default)]
    number_formats: Option<Vec<u8>>,
    #[serde(default)]
    start_number: Option<u16>,
    /// 시작 번호 방식 for the caret paragraph, in the same undo step.
    #[serde(default)]
    restart: Option<RestartParams>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParagraphDialog {
    #[serde(default = "empty_object")]
    format: Value,
    #[serde(default)]
    head: Option<HeadParams>,
}
fn empty_object() -> Value {
    json!({})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeadParams {
    kind: String,
    #[serde(default)]
    level: Option<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RestartParams {
    mode: String,
    #[serde(default)]
    start: Option<u32>,
}

#[derive(Clone, Copy)]
enum Restart {
    Continue,
    Previous,
    New(u32),
}
fn restart_value(p: RestartParams) -> Result<Restart> {
    Ok(match (p.mode.as_str(), p.start) {
        ("continue", None) => Restart::Continue,
        ("previous", None) => Restart::Previous,
        ("new", start) => {
            let start = start.unwrap_or(1);
            if !(1..=65535).contains(&start) {
                bail!("시작 번호는 1–65535입니다.");
            }
            Restart::New(start)
        }
        _ => bail!("mode는 continue/previous/new이며 start는 new에서만 씁니다."),
    })
}
/// Fill missing border/fill keys of a style patch from the style's own shape.
fn complete_from(v: &mut Value, current: &Value) {
    if formatting::BORDER_FILL_KEYS
        .iter()
        .any(|k| v.get(*k).is_some())
    {
        for key in formatting::BORDER_FILL_KEYS {
            if v.get(key).is_none() {
                v[key] = current[key].clone();
            }
        }
    }
}
fn tab_def(v: &Value, base: &TabDef) -> TabDef {
    let auto_left = v["tabAutoLeft"].as_bool().unwrap_or(base.auto_tab_left);
    let auto_right = v["tabAutoRight"].as_bool().unwrap_or(base.auto_tab_right);
    let tabs = v["tabStops"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| TabItem {
                    position: t["position"].as_u64().unwrap_or(0) as u32,
                    tab_type: t["type"].as_u64().unwrap_or(0) as u8,
                    fill_type: t["fill"].as_u64().unwrap_or(0) as u8,
                })
                .collect()
        })
        .unwrap_or_else(|| base.tabs.clone());
    TabDef {
        raw_data: None,
        attr: u32::from(auto_left) | (u32::from(auto_right) << 1),
        tabs,
        auto_tab_left: auto_left,
        auto_tab_right: auto_right,
    }
}
