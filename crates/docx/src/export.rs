//! RHWP document → WordCraft document, written as `.docx` by `wordcraft_docx::write`.
//!
//! Formatting is written directly on every paragraph and run (HWP shapes are per
//! paragraph and per run, not inherited), and the HWP styles become Word styles so the
//! style names survive. HWP keeps a font, width, spacing, size and raise for each script;
//! Word has one of each per run, so a run is split where those differ between scripts.
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::Result;
use hwpcraft_doc::DocumentCore;
use hwpcraft_doc::model::bin_data::BinDataContent;
use hwpcraft_doc::model::control::{AutoNumberType, Control, FieldType};
use hwpcraft_doc::model::document::{Document, Section};
use hwpcraft_doc::model::header_footer::HeaderFooterApply;
use hwpcraft_doc::model::image::Picture;
use hwpcraft_doc::model::page::ColumnDef;
use hwpcraft_doc::model::paragraph::{ColumnBreakType, NumberingRestart, Paragraph};
use hwpcraft_doc::model::shape::{
    Caption, CaptionDirection, CommonObjAttr, HorzAlign, HorzRelTo, ShapeObject, TextFlow,
    TextWrap, VertAlign as HwpVertAlign, VertRelTo,
};
use hwpcraft_doc::model::style::{
    Alignment, BorderFill, BorderLine, BorderLineType, CharShape, HeadType, LineSpacingType,
    ParaShape, UnderlineType,
};
use hwpcraft_doc::model::table::{Table, TablePageBreak, VerticalAlign};
use wordcraft_doc as wc;
use wordcraft_doc::numbering::{AbstractNum, Level, LevelSuffix, Num};
use wordcraft_doc::para::{Anchor, Float, FloatAlign, NoteKind, ShapeKind, Wrap, WrapText};
use wordcraft_doc::props::{
    Border, BorderStyle, Borders, CellProps, EmphasisMark, HeightRule, LineSpacing, NumRef,
    RowProps, TabAlign, TabLeader, TabStop, TableFloat, TableProps, Underline, VAlign, VMerge,
    VertAlign,
};
use wordcraft_doc::section::{Columns, HeaderSet, NumFormat, SectionStart};
use wordcraft_doc::{
    Block, CharProps, InlineObject, ParaProps, Part, PartKind, Rgb, Style, StyleKind, StyleSheet,
    TextColor,
};

use crate::units::{border_pt, hu_pt, lang_slot, para_pt, rgb};

/// Nested tables deeper than this are written as text only.
const MAX_DEPTH: usize = 16;
/// Pictures larger than this are left out.
const MAX_PICTURE_BYTES: usize = 64 << 20;

/// A WordCraft document and what DOCX could not carry, as Korean "what · count" lines.
pub struct Exported {
    pub doc: wc::Document,
    pub omitted: Vec<String>,
}

/// (section, paragraph, control) of a body-level drawing object → (page, [x, y, w, h] px).
type ShapeBoxes = BTreeMap<(usize, usize, usize), (u32, [f64; 4])>;

pub fn to_wordcraft(core: &DocumentCore) -> Result<Exported> {
    let mut x = Exporter::new(core);
    x.styles();
    x.body();
    let omitted = x
        .omitted
        .iter()
        .map(|(what, count)| format!("{what} {count}개"))
        .collect();
    Ok(Exported {
        doc: x.out,
        omitted,
    })
}

struct Exporter<'a> {
    core: &'a DocumentCore,
    doc: &'a Document,
    out: wc::Document,
    omitted: BTreeMap<&'static str, usize>,
    /// Pictures already added, by BinData id: media key.
    media: BTreeMap<u16, String>,
    chars: HashMap<(u32, usize), CharProps>,
    /// DOCX names of HWP bookmarks.
    bookmarks: BTreeMap<String, String>,
    /// Current Word list instance of each HWP numbering (by kind and id).
    lists: BTreeMap<(u8, u16), u32>,
    /// Word abstract list of each HWP numbering.
    abstracts: BTreeMap<(u8, u16), u32>,
    next_part: u32,
    next_comment: u32,
    /// (section, paragraph) of the body paragraph being written, for drawing cut-outs.
    address: Option<(usize, usize)>,
    in_body: bool,
    renderer: Option<hwpcraft_render::Renderer>,
    shape_boxes: Option<ShapeBoxes>,
    page_rasters: BTreeMap<u32, image::RgbaImage>,
    equations: BTreeMap<(String, u32, u32), String>,
    /// The outline numbering of the section being written.
    outline_numbering: u16,
    /// RHWP's resolved character styles: the faces it draws HFT fonts with.
    resolved: hwpcraft_doc::ResolvedStyleSet,
    /// Installed font family names (every localized name).
    installed: std::collections::HashSet<String>,
    /// The body height of the section being written (points): a table that fits keeps
    /// its row heights.
    body_height: f32,
    /// While a header or footer is written: its area on the page (x, y, width, height in
    /// points), which RHWP places every floating object of it in.
    hf_area: Option<[f32; 4]>,
    /// Space above the next body paragraph as Hangul laid it out (see `stored_gap`).
    space_before: Option<f32>,
    /// The same above a table that a blank paragraph holds (written as a spacer).
    table_before: Option<f32>,
}

/// Character slots: Word's run takes the values of this HWP language.
const SLOTS: usize = 7;
/// The face U+00B7 is written in (see the run loop in `paragraph`).
const MIDDLE_DOT_FACE: &str = "Times New Roman";
/// CSS generic families at the end of RHWP's render chains; not face names.
const GENERIC_FAMILIES: [&str; 5] = ["serif", "sans-serif", "monospace", "cursive", "fantasy"];

impl<'a> Exporter<'a> {
    fn new(core: &'a DocumentCore) -> Self {
        let mut out = wc::Document::new();
        out.body.clear();
        out.numbering = Arc::new(wc::Numbering::default());
        Self {
            core,
            doc: core.document(),
            out,
            omitted: BTreeMap::new(),
            media: BTreeMap::new(),
            chars: HashMap::new(),
            bookmarks: BTreeMap::new(),
            lists: BTreeMap::new(),
            abstracts: BTreeMap::new(),
            next_part: 1,
            next_comment: 1,
            address: None,
            in_body: false,
            renderer: None,
            shape_boxes: None,
            page_rasters: BTreeMap::new(),
            equations: BTreeMap::new(),
            outline_numbering: 0,
            resolved: hwpcraft_doc::resolve_styles(&core.document().doc_info, 96.0),
            installed: installed_fonts().clone(),
            body_height: f32::MAX,
            hf_area: None,
            space_before: None,
            table_before: None,
        }
    }

    fn omit(&mut self, what: &'static str) {
        *self.omitted.entry(what).or_default() += 1;
    }

    fn part(&mut self, kind: PartKind, blocks: wc::Blocks) -> u32 {
        let id = self.next_part;
        self.next_part += 1;
        self.out.parts.insert(
            id,
            Part {
                kind,
                blocks,
                ..Default::default()
            },
        );
        id
    }

    // ----- shapes -----

    fn char_shape(&self, id: u32) -> Option<&'a CharShape> {
        self.doc.doc_info.char_shapes.get(id as usize)
    }
    fn para_shape(&self, id: u16) -> Option<&'a ParaShape> {
        self.doc.doc_info.para_shapes.get(usize::from(id))
    }
    fn border_fill(&self, id: u16) -> Option<&'a BorderFill> {
        (id > 0)
            .then(|| self.doc.doc_info.border_fills.get(usize::from(id) - 1))
            .flatten()
    }
    /// The font of one language of a character shape, as a face Word can find: the
    /// document's own when it is installed, else the face RHWP draws it with (HFT and
    /// HCI faces only Hangul has), else an installed face of the same kind.
    fn font_name(&self, slot: usize, cs: &CharShape) -> Option<String> {
        let id = usize::from(*cs.font_ids.get(slot)?);
        let font = self.doc.doc_info.font_faces.get(slot)?.get(id)?;
        if font.name.is_empty() {
            return None;
        }
        let installed = |name: &str| self.installed.contains(name);
        let hangul_only = font.alt_type == 2 || font.name.starts_with("HCI ");
        if !hangul_only && (installed(&font.name) || self.installed.is_empty()) {
            return Some(font.name.clone());
        }
        let resolved = self
            .doc
            .doc_info
            .char_shapes
            .iter()
            .position(|c| std::ptr::eq(c, cs))
            .and_then(|index| self.resolved.char_styles.get(index))
            .map(|style| style.font_family_for_lang(slot).to_owned())
            .unwrap_or_default();
        // The face RHWP draws with: the first installed family of its render chain
        // (휴먼명조 is drawn in 바탕 there), before the file's own substitute.
        let chain = hwpcraft_doc::render_font_family_chain(if resolved.is_empty() {
            &font.name
        } else {
            &resolved
        });
        let candidates = resolved
            .split(',')
            .map(str::trim)
            .chain(
                chain
                    .split(',')
                    .map(|n| n.trim().trim_matches(|c| c == '\'' || c == '"')),
            )
            .chain(font.subst_font.as_ref().map(|s| s.face.trim()))
            .filter(|n| !n.is_empty() && !GENERIC_FAMILIES.contains(n));
        for name in candidates {
            if installed(name) {
                return Some(name.to_owned());
            }
        }
        Some(similar_face(&font.name, &self.installed).unwrap_or_else(|| font.name.clone()))
    }

    /// Run properties of a character shape for characters of one HWP language.
    fn char_props(&mut self, shape_id: u32, slot: usize) -> CharProps {
        if let Some(p) = self.chars.get(&(shape_id, slot)) {
            return p.clone();
        }
        let p = self
            .char_shape(shape_id)
            .map(|cs| self.make_char_props(cs, slot))
            .unwrap_or_default();
        self.chars.insert((shape_id, slot), p.clone());
        p
    }

    fn make_char_props(&self, cs: &CharShape, slot: usize) -> CharProps {
        let slot = slot.min(SLOTS - 1);
        let ea_slot = if matches!(slot, 0 | 2 | 3) { slot } else { 0 };
        let latin_slot = if matches!(slot, 4..=6) { slot } else { 1 };
        let mut p = CharProps {
            font: self.font_name(latin_slot, cs),
            font_ea: self.font_name(ea_slot, cs),
            ..Default::default()
        };
        let rel = f32::from(cs.relative_sizes[slot].max(1)) / 100.0;
        let size = (cs.base_size.max(100) as f32 / 100.0) * rel;
        p.size = Some((size * 2.0).round() / 2.0);
        p.size_cs = p.size;
        if cs.bold {
            p.bold = Some(true);
            p.bold_cs = Some(true);
        }
        if cs.italic {
            p.italic = Some(true);
            p.italic_cs = Some(true);
        }
        match cs.underline_type {
            UnderlineType::None => {}
            // Word has no line above the text: written as an underline.
            UnderlineType::Bottom | UnderlineType::Top => {
                p.underline = Some(underline_style(cs.underline_shape));
                p.underline_color = rgb(cs.underline_color);
            }
        }
        if cs.strikethrough {
            // Strike line shapes 7–10 are double lines (표 27).
            if (7..=10).contains(&cs.strike_shape) {
                p.double_strike = Some(true);
            } else {
                p.strike = Some(true);
            }
        }
        if let Some(c) = rgb(cs.text_color)
            && c != Rgb(0, 0, 0)
        {
            p.color = Some(TextColor::Rgb(c));
        }
        if let Some(c) = rgb(cs.shade_color)
            && c != Rgb(255, 255, 255)
        {
            p.shading = Some(c);
        }
        if cs.superscript {
            p.vert_align = Some(VertAlign::Superscript);
        } else if cs.subscript {
            p.vert_align = Some(VertAlign::Subscript);
        }
        let ratio = cs.ratios[slot];
        if ratio != 0 && ratio != 100 {
            p.scale = Some(f32::from(ratio.clamp(1, 200)));
        }
        let spacing = cs.spacings[slot];
        if spacing != 0 {
            p.spacing = Some((size * f32::from(spacing) / 100.0 * 20.0).round() / 20.0);
        }
        let raise = cs.char_offsets[slot];
        if raise != 0 {
            p.position = Some((size * f32::from(raise) / 100.0 * 2.0).round() / 2.0);
        }
        if cs.outline_type != 0 {
            p.outline = Some(true);
        }
        if cs.shadow_type != 0 {
            p.shadow = Some(true);
        }
        if cs.emboss {
            p.emboss = Some(true);
        }
        if cs.engrave {
            p.engrave = Some(true);
        }
        if cs.emphasis_dot != 0 {
            p.emphasis = Some(match cs.emphasis_dot {
                2 => EmphasisMark::Circle,
                _ => EmphasisMark::Dot,
            });
        }
        if cs.kerning {
            p.kern = Some(1.0);
        }
        if let Some(bf) = self.border_fill(cs.border_fill_id)
            && let Some(line) = bf
                .borders
                .iter()
                .find(|l| l.line_type != BorderLineType::None)
        {
            p.border = border(line, 0.0);
        }
        p
    }

    fn para_props(&mut self, para: &Paragraph, chars: &[char]) -> ParaProps {
        let mut p = ParaProps {
            style: Some(style_id(usize::from(para.style_id))),
            ..Default::default()
        };
        let Some(ps) = self.para_shape(para.para_shape_id) else {
            return p;
        };
        // The largest text in the paragraph sets the line height, as in HWP.
        let mut size = 0.0f32;
        for r in &para.char_shapes {
            if let Some(cs) = self.char_shape(r.char_shape_id) {
                size = size.max(cs.base_size as f32 / 100.0);
            }
        }
        if size <= 0.0 {
            size = 10.0;
        }
        let objects = para.controls.iter().any(|c| match c {
            Control::Picture(pic) => pic.common.treat_as_char,
            Control::Equation(_) => true,
            Control::Shape(s) => s.common().treat_as_char,
            _ => false,
        });
        self.fill_para_props(ps, size, objects, &mut p);
        // A line holding a table is as tall as the table: no line pitch from those.
        let tables = para.controls.iter().any(|c| matches!(c, Control::Table(_)));
        if !objects
            && !tables
            && let Some(pitch) = line_pitch(para)
        {
            p.line_spacing = Some(LineSpacing::Exactly(pitch));
        }
        if let Some(gap) = self.space_before.take() {
            p.space_before = Some(gap);
        }
        if para.column_type == ColumnBreakType::Page && !para.page_break_synthesized {
            p.page_break_before = Some(true);
        }
        p.numbering = self.list_ref(para, ps);
        let _ = chars;
        p
    }

    fn fill_para_props(&self, ps: &ParaShape, size: f32, objects: bool, p: &mut ParaProps) {
        p.align = Some(match ps.alignment {
            Alignment::Justify => wc::Align::Justify,
            Alignment::Left => wc::Align::Left,
            Alignment::Right => wc::Align::Right,
            Alignment::Center => wc::Align::Center,
            Alignment::Distribute | Alignment::Split => wc::Align::Distribute,
        });
        let left = para_pt(ps.margin_left);
        let indent = para_pt(ps.indent);
        // HWP: a positive indent moves the first line in; a negative one (내어쓰기) keeps
        // the first line at the margin and moves the others in.
        if indent < 0.0 {
            p.indent_left = Some(left - indent);
        } else {
            p.indent_left = Some(left);
        }
        p.indent_first = Some(indent);
        p.indent_right = Some(para_pt(ps.margin_right));
        p.space_before = Some(para_pt(ps.spacing_before).max(0.0));
        p.space_after = Some(para_pt(ps.spacing_after).max(0.0));
        // HWP's percentage is of the font size, Word's "multiple" of the font's own line
        // height: written as an exact height (at least, when pictures sit in the line).
        let fixed = |pt: f32| {
            if objects {
                LineSpacing::AtLeast(pt)
            } else {
                LineSpacing::Exactly(pt)
            }
        };
        p.line_spacing = Some(match ps.line_spacing_type {
            LineSpacingType::Percent => fixed(size * ps.line_spacing.max(1) as f32 / 100.0),
            LineSpacingType::Fixed => LineSpacing::Exactly(para_pt(ps.line_spacing).max(1.0)),
            LineSpacingType::Minimum => LineSpacing::AtLeast(para_pt(ps.line_spacing).max(1.0)),
            LineSpacingType::SpaceOnly => fixed(size + para_pt(ps.line_spacing)),
        });
        let bit = |n: u32| ps.attr1 & (1 << n) != 0;
        // HWP adds no space between Korean and Latin text or digits.
        p.auto_space_de = Some(false);
        p.auto_space_dn = Some(false);
        // 영어 줄 나눔 단위 "글자" (bits 5–6 = 2) breaks Latin words anywhere.
        if (ps.attr1 >> 5) & 0x3 == 2 {
            p.word_wrap = Some(false);
        }
        p.widow_control = Some(bit(16));
        if bit(17) {
            p.keep_next = Some(true);
        }
        if bit(18) {
            p.keep_lines = Some(true);
        }
        if bit(19) {
            p.page_break_before = Some(true);
        }
        if let Some(def) = self.doc.doc_info.tab_defs.get(usize::from(ps.tab_def_id))
            && !def.tabs.is_empty()
        {
            p.tabs = Some(
                def.tabs
                    .iter()
                    .map(|t| TabStop {
                        pos: para_pt(t.position as i32),
                        align: match t.tab_type {
                            1 => TabAlign::Right,
                            2 => TabAlign::Center,
                            3 => TabAlign::Decimal,
                            _ => TabAlign::Left,
                        },
                        leader: match t.fill_type {
                            0 => TabLeader::None,
                            1 => TabLeader::Underscore,
                            2 => TabLeader::Hyphen,
                            _ => TabLeader::Dot,
                        },
                    })
                    .collect(),
            );
        }
        if let Some(bf) = self.border_fill(ps.border_fill_id) {
            let space = |i: usize| hu_pt(i64::from(ps.border_spacing[i]));
            let b = Borders {
                left: border(&bf.borders[0], space(0)),
                right: border(&bf.borders[1], space(1)),
                top: border(&bf.borders[2], space(2)),
                bottom: border(&bf.borders[3], space(3)),
                ..Default::default()
            };
            if b.left.is_some() || b.right.is_some() || b.top.is_some() || b.bottom.is_some() {
                p.borders = Some(b);
            }
            p.shading = solid_fill(bf);
        }
        if ps.head_type == HeadType::Outline {
            p.outline_level = Some(ps.para_level.min(8));
        }
    }

    // ----- lists -----

    /// The Word list of a numbered, bulleted or outline paragraph.
    fn list_ref(&mut self, para: &Paragraph, ps: &ParaShape) -> Option<NumRef> {
        let key = match ps.head_type {
            HeadType::None => return None,
            HeadType::Bullet => (1u8, ps.numbering_id),
            HeadType::Number => (0u8, ps.numbering_id),
            HeadType::Outline => {
                let id = if ps.numbering_id > 0 {
                    ps.numbering_id
                } else {
                    self.outline_numbering
                };
                (0u8, id)
            }
        };
        if key.1 == 0 {
            return None;
        }
        let level = ps.para_level.min(6);
        let abstract_id = match self.abstracts.get(&key) {
            Some(id) => *id,
            None => {
                let levels = if key.0 == 1 {
                    self.bullet_levels(key.1)?
                } else {
                    self.number_levels(key.1)?
                };
                let numbering = Arc::make_mut(&mut self.out.numbering);
                let id = numbering.abstracts.len() as u32;
                numbering.abstracts.push(AbstractNum {
                    id,
                    levels,
                    multi_level_type: Some("multilevel".into()),
                    ..Default::default()
                });
                self.abstracts.insert(key, id);
                id
            }
        };
        let restart = match para.numbering_restart {
            Some(NumberingRestart::NewStart(n)) => Some(n),
            _ => None,
        };
        let num = match (self.lists.get(&key), restart) {
            (Some(id), None) => *id,
            _ => {
                let numbering = Arc::make_mut(&mut self.out.numbering);
                let id = numbering.nums.len() as u32 + 1;
                numbering.nums.push(Num {
                    id,
                    abstract_id,
                    start_overrides: restart.map(|n| vec![(level, n)]).unwrap_or_default(),
                    ..Default::default()
                });
                self.lists.insert(key, id);
                id
            }
        };
        Some(NumRef { num, level })
    }

    fn number_levels(&self, id: u16) -> Option<Vec<Level>> {
        let n = self.doc.doc_info.numberings.get(usize::from(id) - 1)?;
        let mut levels = Vec::new();
        for i in 0..9 {
            let j = i.min(6);
            let head = &n.heads[j];
            let start = match n.level_start_numbers[j] {
                0 => u32::from(n.start_number.max(1)),
                s => s,
            };
            levels.push(Level {
                start,
                format: num_format(head.number_format),
                text: level_text(&n.level_formats[j]),
                indent: 0.0,
                hanging: 0.0,
                suffix: LevelSuffix::Space,
                ..Default::default()
            });
        }
        Some(levels)
    }

    fn bullet_levels(&self, id: u16) -> Option<Vec<Level>> {
        let b = self.doc.doc_info.bullets.get(usize::from(id) - 1)?;
        let c = b.bullet_char;
        let text = if c.is_control() || c == '\u{FFFF}' || c == '\0' {
            "•".to_owned()
        } else {
            c.to_string()
        };
        Some(
            (0..9)
                .map(|_| Level {
                    format: NumFormat::Bullet,
                    text: text.clone(),
                    indent: 0.0,
                    hanging: 0.0,
                    suffix: LevelSuffix::Space,
                    ..Default::default()
                })
                .collect(),
        )
    }

    // ----- styles -----

    fn styles(&mut self) {
        let mut sheet = StyleSheet::empty();
        let first = self.doc.doc_info.styles.first();
        if let Some(cs) = first.and_then(|s| self.char_shape(u32::from(s.char_shape_id))) {
            sheet.default_chr = self.make_char_props(cs, 0);
        } else {
            sheet.default_chr.size = Some(10.0);
        }
        sheet.default_chr.lang = Some("ko-KR".into());
        sheet.default_chr.lang_ea = Some("ko-KR".into());
        sheet.default_para = ParaProps {
            space_after: Some(0.0),
            space_before: Some(0.0),
            line_spacing: Some(LineSpacing::Multiple(1.0)),
            ..Default::default()
        };
        sheet.default_para_present = true;
        for (i, st) in self.doc.doc_info.styles.iter().enumerate() {
            let mut para = ParaProps::default();
            let chr = self
                .char_shape(u32::from(st.char_shape_id))
                .map(|cs| self.make_char_props(cs, 0))
                .unwrap_or_default();
            if let Some(ps) = self.para_shape(st.para_shape_id) {
                let size = chr.size.unwrap_or(10.0);
                self.fill_para_props(ps, size, false, &mut para);
            }
            let name = if st.local_name.is_empty() {
                st.english_name.clone()
            } else {
                st.local_name.clone()
            };
            let char_style = st.style_type == 1;
            sheet.styles.push(Style {
                id: style_id(i),
                name: if i == 0 { "Normal".into() } else { name },
                kind: if char_style {
                    StyleKind::Character
                } else {
                    StyleKind::Paragraph
                },
                based_on: (i > 0 && !char_style).then(|| style_id(0)),
                next: (!char_style).then(|| style_id(usize::from(st.next_style_id))),
                para: if char_style {
                    ParaProps::default()
                } else {
                    para
                },
                chr,
                quick: true,
                default: i == 0,
                builtin: i == 0,
                ..Default::default()
            });
        }
        if sheet.styles.is_empty() {
            sheet.styles.push(Style {
                id: style_id(0),
                name: "Normal".into(),
                default: true,
                builtin: true,
                ..Default::default()
            });
        }
        self.out.styles = Arc::new(sheet);
    }

    // ----- body -----

    /// (section, paragraph) where each page starts, in page order.
    fn page_starts(&self) -> Vec<(usize, usize)> {
        (0..self.core.page_count())
            .filter_map(|page| {
                let raw = self.core.get_position_of_page_native(page as usize).ok()?;
                let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
                Some((v["sec"].as_u64()? as usize, v["para"].as_u64()? as usize))
            })
            .collect()
    }

    /// The body. An HWP section becomes Word sections where its columns change (each
    /// continuous) and where a page number starts again (새 번호로 시작: a new page
    /// with that start, beginning on the page that holds it). What each part hides on
    /// its first page and on all its other pages (감추기) decides its headers and footers.
    fn body(&mut self) {
        let doc = self.doc;
        let count = doc.sections.len();
        let starts = self.page_starts();
        let mut blocks: wc::Blocks = Vec::new();
        for (si, section) in doc.sections.iter().enumerate() {
            self.outline_numbering = section.section_def.outline_numbering_id;
            let pages: Vec<usize> = starts.iter().filter(|s| s.0 == si).map(|s| s.1).collect();
            let page_of = |pi: usize| pages.partition_point(|&p| p <= pi).saturating_sub(1);
            let mut hides = vec![[false; 3]; pages.len().max(1)];
            let mut cuts: BTreeMap<usize, (Option<Option<ColumnDef>>, Option<u16>)> =
                BTreeMap::new();
            cuts.entry(0).or_default();
            for (pi, para) in section.paragraphs.iter().enumerate() {
                let page = page_of(pi).min(hides.len() - 1);
                for c in &para.controls {
                    match c {
                        Control::PageHide(h) => {
                            hides[page][0] |= h.hide_header;
                            hides[page][1] |= h.hide_footer;
                            hides[page][2] |= h.hide_page_num;
                        }
                        Control::NewNumber(n) if n.number_type == AutoNumberType::Page => {
                            // Word starts the page with the section; where Hangul's page
                            // only filled up, Word's text runs on differently and a forced
                            // page would leave a gap, so the numbers just go on there.
                            let at = pages.get(page).copied().unwrap_or(pi).min(pi);
                            let breaks = at == 0
                                || section.paragraphs[at].column_type == ColumnBreakType::Page;
                            if breaks {
                                cuts.entry(at).or_default().1 = Some(n.number);
                            } else {
                                self.omit("쪽 나누기 없는 쪽의 새 쪽 번호(번호를 이어서 매김)");
                            }
                        }
                        _ => {}
                    }
                }
            }
            for (start, columns) in column_runs(section) {
                cuts.entry(start).or_default().0 = Some(columns);
            }
            let cuts: Vec<_> = cuts.into_iter().collect();
            let mut columns: Option<ColumnDef> = None;
            let mut props = wc::SectionProps::default();
            for (ci, (start, (cols, number))) in cuts.iter().enumerate() {
                let end = cuts
                    .get(ci + 1)
                    .map_or(section.paragraphs.len(), |(next, _)| *next);
                if let Some(cols) = cols {
                    columns.clone_from(cols);
                }
                if ci == 0 || number.is_some() {
                    // A new page: its own headers and footers.
                    let first = page_of(*start).min(hides.len() - 1);
                    let last = page_of(end.saturating_sub(1).max(*start)).min(hides.len() - 1);
                    let rest = &hides[(first + 1).min(last + 1)..=last];
                    let hide_rest =
                        [0, 1, 2].map(|k| !rest.is_empty() && rest.iter().all(|h| h[k]));
                    props = self.section_props(section, hides[first], hide_rest);
                    if ci > 0 {
                        props.start = SectionStart::NextPage;
                        props.page_num_start = None;
                    }
                    if let Some(number) = number {
                        props.page_num_start = Some(u32::from(*number));
                    }
                } else {
                    props.start = SectionStart::Continuous;
                }
                props.columns = columns
                    .as_ref()
                    .filter(|d| d.column_count > 1)
                    .map(columns_of)
                    .unwrap_or_default();
                let first_block = blocks.len();
                for pi in *start..end {
                    self.address = Some((si, pi));
                    let gap = pi.checked_sub(1).and_then(|prev| {
                        self.stored_gap(&section.paragraphs[prev], &section.paragraphs[pi])
                    });
                    if table_host(&section.paragraphs[pi]) {
                        self.table_before = gap;
                    } else {
                        self.space_before = gap;
                    }
                    self.in_body = true;
                    let mut out = self.paragraph(&section.paragraphs[pi], 0);
                    self.in_body = false;
                    blocks.append(&mut out);
                }
                if ci > 0 && number.is_some() {
                    // The section break already starts the page.
                    if let Some(b) = blocks.get_mut(first_block)
                        && let Block::Para(p) = Arc::make_mut(b)
                    {
                        p.props.page_break_before = None;
                    }
                }
                let last = si + 1 == count && ci + 1 == cuts.len();
                if last {
                    self.out.last_section = props.clone();
                } else {
                    // The section ends on a paragraph; a table needs one after it.
                    let ends_on_para = matches!(blocks.last().map(|b| &**b), Some(Block::Para(_)));
                    if !ends_on_para {
                        blocks.push(wc::para_block(wc::Paragraph::new()));
                    }
                    if let Some(b) = blocks.last_mut()
                        && let Block::Para(p) = Arc::make_mut(b)
                    {
                        p.section = Some(Box::new(props.clone()));
                    }
                }
            }
        }
        if blocks.is_empty() {
            blocks.push(wc::para_block(wc::Paragraph::new()));
        }
        self.out.body = blocks;
    }

    /// Page setup, headers, footers and page numbering of a Word section of `section`.
    /// `hide_first` is what its first page hides (header, footer, page number; with any,
    /// that page gets its own header and footer) and `hide_rest` what all its other pages
    /// hide.
    fn section_props(
        &mut self,
        section: &Section,
        hide_first: [bool; 3],
        hide_rest: [bool; 3],
    ) -> wc::SectionProps {
        let def = &section.section_def;
        let page = &def.page_def;
        let mut p = wc::SectionProps::default();
        let (mut w, mut h) = (i64::from(page.width), i64::from(page.height));
        if page.landscape && w < h {
            std::mem::swap(&mut w, &mut h);
        }
        p.page_w = hu_pt(w);
        p.page_h = hu_pt(h);
        p.landscape = page.landscape;
        // HWP's top margin is to the header; Word's is to the body.
        p.margin_top = hu_pt(i64::from(page.margin_top) + i64::from(page.margin_header));
        p.margin_bottom = hu_pt(i64::from(page.margin_bottom) + i64::from(page.margin_footer));
        p.margin_left = hu_pt(i64::from(page.margin_left));
        p.margin_right = hu_pt(i64::from(page.margin_right));
        p.header = hu_pt(i64::from(page.margin_top));
        p.footer = hu_pt(i64::from(page.margin_bottom));
        p.gutter = hu_pt(i64::from(page.margin_gutter));
        self.body_height = p.page_h - p.margin_top - p.margin_bottom;
        // RHWP lays a header's or footer's floating objects out in its area, whatever
        // they are set relative to (paper, page or paragraph).
        let body_w = p.page_w - p.margin_left - p.margin_right;
        let header_area = [
            p.margin_left,
            p.header,
            body_w,
            hu_pt(i64::from(page.margin_header)),
        ];
        let footer_h = hu_pt(i64::from(page.margin_footer));
        let footer_area = [
            p.margin_left,
            p.page_h - p.footer - footer_h,
            body_w,
            footer_h,
        ];
        if def.page_num > 0 {
            p.page_num_start = Some(u32::from(def.page_num));
        }
        if def.default_tab_spacing > 0 {
            self.out.settings.default_tab = hu_pt(i64::from(def.default_tab_spacing));
        }
        // Headers, footers and the page number of this section.
        let mut headers: [Option<Vec<Paragraph>>; 2] = [None, None];
        let mut footers: [Option<Vec<Paragraph>>; 2] = [None, None];
        let mut page_number = None;
        for para in &section.paragraphs {
            for c in &para.controls {
                match c {
                    Control::Header(hd) => {
                        let slot = usize::from(hd.apply_to == HeaderFooterApply::Even);
                        if headers[slot].is_none() {
                            headers[slot] = Some(hd.paragraphs.clone());
                        }
                        if hd.apply_to == HeaderFooterApply::Both && headers[1].is_none() {
                            headers[1] = Some(hd.paragraphs.clone());
                        }
                    }
                    Control::Footer(ft) => {
                        let slot = usize::from(ft.apply_to == HeaderFooterApply::Even);
                        if footers[slot].is_none() {
                            footers[slot] = Some(ft.paragraphs.clone());
                        }
                        if ft.apply_to == HeaderFooterApply::Both && footers[1].is_none() {
                            footers[1] = Some(ft.paragraphs.clone());
                        }
                    }
                    Control::PageNumberPos(pos) if page_number.is_none() && pos.position != 0 => {
                        page_number = Some(pos.clone());
                    }
                    _ => {}
                }
            }
        }
        let number = page_number.as_ref().map(|pos| {
            // 쪽 번호 위치 (HWP 표 150): 1–3 위, 4–6 아래, 7·9 바깥/안쪽 위, 8·10 아래.
            let top = matches!(pos.position, 1..=3 | 7 | 9);
            let align = match pos.position {
                1 | 4 => wc::Align::Left,
                2 | 5 => wc::Align::Center,
                _ => wc::Align::Right,
            };
            if matches!(pos.position, 7..=10) {
                self.omit("쪽 번호 바깥·안쪽 위치(오른쪽으로 씀)");
            }
            p.page_num_format = page_num_format(pos.format);
            (top, align, pos.clone())
        });
        let even_differs = headers[0] != headers[1] || footers[0] != footers[1];
        if even_differs {
            self.out.settings.even_odd_headers = true;
        }
        // What the first page hides (구역 설정, or 감추기 on that page).
        let hide = [
            hide_first[0] || def.hide_header,
            hide_first[1] || def.hide_footer,
            hide_first[2],
        ];
        if hide.iter().any(|h| *h) {
            p.title_page = true;
        }
        for (is_footer, set) in [(false, headers), (true, footers)] {
            let [odd, even] = set;
            let mut ids = HeaderSet::default();
            let wants_number = number.as_ref().is_some_and(|(top, _, _)| *top != is_footer);
            let first = (!hide[usize::from(is_footer)])
                .then(|| odd.clone())
                .flatten();
            let rest_hidden = hide_rest[usize::from(is_footer)];
            let (odd, even) = if rest_hidden {
                (None, None)
            } else {
                (odd, even)
            };
            for (slot, paras) in [(0, odd), (1, even), (2, first)] {
                if slot == 1 && !even_differs || slot == 2 && !p.title_page {
                    continue;
                }
                let number_here = wants_number
                    && !rest_hidden
                    && !(slot == 2 && hide[2])
                    && !(slot != 2 && hide_rest[2]);
                if paras.is_none() && !number_here && slot != 2 {
                    continue;
                }
                let mut blocks = Vec::new();
                self.hf_area = Some(if is_footer { footer_area } else { header_area });
                for para in paras.iter().flatten() {
                    blocks.append(&mut self.paragraph(para, 1));
                }
                self.hf_area = None;
                if number_here && let Some((_, align, pos)) = &number {
                    blocks.push(wc::para_block(page_number_para(*align, pos)));
                }
                if blocks.is_empty() {
                    blocks.push(wc::para_block(wc::Paragraph::new()));
                }
                let kind = if is_footer {
                    PartKind::Footer
                } else {
                    PartKind::Header
                };
                let id = self.part(kind, blocks);
                match slot {
                    0 => ids.default = Some(id),
                    1 => ids.even = Some(id),
                    _ => ids.first = Some(id),
                }
            }
            if is_footer {
                p.footers = ids;
            } else {
                p.headers = ids;
            }
        }
        p
    }

    /// One HWP paragraph as WordCraft blocks: tables split it, since Word tables are blocks.
    fn paragraph(&mut self, para: &Paragraph, depth: usize) -> Vec<Arc<Block>> {
        let chars: Vec<char> = para.text.chars().collect();
        let props = self.para_props(para, &chars);
        let positions = para.control_text_positions();
        let mut by_position: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (index, position) in positions.iter().enumerate() {
            by_position.entry(*position).or_default().push(index);
        }
        // Hyperlinks (start, end, target) and memos (start, end, paragraphs).
        let mut links: Vec<(usize, usize, String)> = Vec::new();
        let mut memos: Vec<(usize, usize, usize)> = Vec::new();
        for range in &para.field_ranges {
            match para.controls.get(range.control_idx) {
                Some(Control::Field(field)) if field.field_type == FieldType::Hyperlink => {
                    if let Some(target) = self.link_target(&field.command)
                        && range.end_char_idx > range.start_char_idx
                    {
                        links.push((range.start_char_idx, range.end_char_idx, target));
                    }
                }
                Some(Control::Field(field)) if field.field_type == FieldType::Memo => {
                    memos.push((range.start_char_idx, range.end_char_idx, range.control_idx));
                }
                _ => {}
            }
        }
        let mut blocks: Vec<Arc<Block>> = Vec::new();
        let mut after: Vec<Arc<Block>> = Vec::new();
        let mut cur = wc::Paragraph::new();
        cur.props = props.clone();
        if para.column_type == ColumnBreakType::Column {
            cur.text.push(wc::para::COLUMN_BREAK);
            cur.runs.push(wc::Run {
                len: wc::para::COLUMN_BREAK.len_utf8(),
                props: CharProps::default(),
            });
        }
        let mut carry = 0usize;
        // Bottom gap of a table that ends the paragraph (nothing written after it).
        let mut tail_gap: Option<f32> = None;
        let mut link: Option<String> = None;
        let mut memo_ids: BTreeMap<usize, u32> = BTreeMap::new();
        for i in 0..=chars.len() {
            for (start, end, target) in &links {
                if *end == i && link.as_deref() == Some(target.as_str()) {
                    link = None;
                }
                if *start == i {
                    link = Some(target.clone());
                }
            }
            for (start, end, ci) in &memos {
                if *end == i
                    && let Some(id) = memo_ids.get(ci)
                {
                    push_object(
                        &mut cur,
                        InlineObject::CommentEnd { id: *id },
                        CharProps::default(),
                    );
                }
                if *start == i
                    && let Some(id) = self.memo(para, *ci, depth)
                {
                    memo_ids.insert(*ci, id);
                    push_object(
                        &mut cur,
                        InlineObject::CommentStart { id },
                        CharProps::default(),
                    );
                }
            }
            if let Some(controls) = by_position.get(&i) {
                for &ci in controls {
                    let Some(control) = para.controls.get(ci) else {
                        continue;
                    };
                    let shape = para.char_shape_id_at(i).unwrap_or(0);
                    let mut run_props = self.char_props(shape, carry);
                    run_props.link = link.clone();
                    match control {
                        Control::Table(table) => {
                            // Close the paragraph so far, write the table, continue after it.
                            // An empty host line sits beside the table in HWP and takes no
                            // height of its own, so it is not written before the table.
                            // Spaces before a table share its line in HWP; alone they would
                            // make a line of their own.
                            let blank =
                                cur.objects.is_empty() && cur.text.chars().all(char::is_whitespace);
                            let gap = self.table_before.take();
                            if !blank {
                                blocks.push(wc::para_block(std::mem::take(&mut cur)));
                            } else if cur.props.page_break_before == Some(true) {
                                let mut s = spacer(1.0);
                                s.props.page_break_before = Some(true);
                                blocks.push(wc::para_block(s));
                            } else if let Some(gap) = gap.filter(|g| *g >= 1.0) {
                                // Hangul's space above the table's line.
                                blocks.push(wc::para_block(spacer(gap)));
                            }
                            cur = wc::Paragraph::new();
                            cur.props = continuation(&props);
                            let (before, after_caption) =
                                self.caption(table.caption.as_ref(), depth);
                            blocks.extend(before);
                            tail_gap = Some(self.table(table, depth, &props, &mut blocks));
                            blocks.extend(after_caption);
                            continue;
                        }
                        Control::Picture(pic) => {
                            self.picture(pic, &mut cur, run_props);
                            let (before, after_caption) = self.caption(pic.caption.as_ref(), depth);
                            blocks.extend(before);
                            after.extend(after_caption);
                        }
                        _ => self.inline_control(control, ci, depth, &mut cur, run_props),
                    }
                }
            }
            let Some(&c) = chars.get(i) else {
                break;
            };
            tail_gap = None;
            let slot = lang_slot(c).unwrap_or(carry);
            carry = slot;
            let shape = para.char_shape_id_at(i).unwrap_or(0);
            let mut run_props = self.char_props(shape, slot);
            run_props.link = link.clone();
            if c == '\u{00B7}' {
                // Word draws U+00B7 from the East Asian face whatever the run says, full
                // width in Korean faces (HY신명조: 1 em); HWP and RHWP set it about a third
                // of an em, and so does Times New Roman when it is the East Asian face too.
                for face in [
                    &mut run_props.font,
                    &mut run_props.font_ea,
                    &mut run_props.font_cs,
                ] {
                    *face = Some(MIDDLE_DOT_FACE.to_owned());
                }
            }
            let c = match c {
                // HWP's fixed-width and non-breaking spaces.
                '\u{2007}' => '\u{00A0}',
                c if (c as u32) < 0x20 && c != '\t' && c != '\n' => continue,
                c => c,
            };
            push_char(&mut cur, c, run_props);
        }
        match tail_gap {
            // The paragraph ends in a table: HWP puts the next paragraph right under the
            // table and its margin. Word needs a paragraph after a table anyway; one as
            // tall as that margin keeps the next paragraph where HWP puts it.
            Some(gap) if cur.text.is_empty() => blocks.push(wc::para_block(spacer(gap))),
            _ => {
                if cur.text.is_empty() && cur.mark.is_empty() {
                    let shape = para.char_shape_id_at(0).unwrap_or(0);
                    cur.mark = self.char_props(shape, 0);
                    cur.mark.link = None;
                }
                blocks.push(wc::para_block(cur));
            }
        }
        blocks.extend(after);
        blocks
    }

    /// The space Hangul left between two body paragraphs on one page, as Word's space
    /// before `cur`: from `prev`'s last line to `cur`'s first, less the line height
    /// Word gives that last line and `prev`'s space after. Hangul's own gap is not the
    /// paragraph shape's (with percent line spacing it is often a third of a line more,
    /// or less). `None` across a page or column, or next to tables and objects, whose
    /// lines Word does not keep.
    fn stored_gap(&self, prev: &Paragraph, cur: &Paragraph) -> Option<f32> {
        let plain = |p: &Paragraph| {
            !p.controls.iter().any(|c| match c {
                Control::Table(_) | Control::Equation(_) => true,
                Control::Picture(pic) => pic.common.treat_as_char,
                Control::Shape(s) => s.common().treat_as_char,
                _ => false,
            })
        };
        if !plain(prev)
            || !(plain(cur) || table_host(cur))
            || cur.column_type != ColumnBreakType::None
        {
            return None;
        }
        let (last, first) = (prev.line_segs.last()?, cur.line_segs.first()?);
        let rise = first.vertical_pos - last.vertical_pos;
        if rise <= 0 {
            return None;
        }
        let after = self
            .para_shape(prev.para_shape_id)
            .map_or(0.0, |ps| para_pt(ps.spacing_after).max(0.0));
        let gap = hu_pt(i64::from(rise)) - line_pitch(prev)? - after;
        (gap < 60.0).then(|| (gap.max(0.0) * 20.0).round() / 20.0)
    }

    /// Where a floating object goes: [`float_of`], except in a header or footer, where
    /// RHWP places it in that area whatever it is set relative to
    /// (`layout_header_footer_picture`); a paragraph-relative one keeps its vertical
    /// place from the paragraph.
    fn float(&self, c: &CommonObjAttr) -> Float {
        let mut f = float_of(c);
        let Some([ax, ay, aw, ah]) = self.hf_area.filter(|_| !c.treat_as_char) else {
            return f;
        };
        let (w, h) = (hu_pt(i64::from(c.width)), hu_pt(i64::from(c.height)));
        let (dx, dy) = (
            hu_pt(i64::from(c.horizontal_offset as i32)),
            hu_pt(i64::from(c.vertical_offset as i32)),
        );
        f.h_rel = Anchor::Page;
        f.h_align = None;
        f.x = match c.horz_align {
            HorzAlign::Left | HorzAlign::Inside => ax + dx,
            HorzAlign::Center => ax + (aw - w) / 2.0 + dx,
            HorzAlign::Right | HorzAlign::Outside => ax + aw - w - dx,
        };
        if c.vert_rel_to != VertRelTo::Para {
            f.v_rel = Anchor::Page;
            f.v_align = None;
            f.y = match c.vert_align {
                HwpVertAlign::Top | HwpVertAlign::Inside => ay + dy,
                HwpVertAlign::Center => ay + (ah - h) / 2.0 + dy,
                HwpVertAlign::Bottom | HwpVertAlign::Outside => ay + ah - h - dy,
            };
        }
        f
    }

    fn link_target(&mut self, command: &str) -> Option<String> {
        let uri = hwpcraft_doc::model::hyperlink::command_uri(command);
        let lower = uri.to_ascii_lowercase();
        if ["http://", "https://", "mailto:", "ftp://", "file:"]
            .iter()
            .any(|s| lower.starts_with(s))
        {
            return Some(uri);
        }
        match uri
            .strip_prefix('?')
            .filter(|n| !n.is_empty() && !n.starts_with('#'))
        {
            Some(name) => Some(format!("#{}", self.bookmark_name(name))),
            None => {
                self.omit("문서 안 하이퍼링크(글자만 씀)");
                None
            }
        }
    }

    /// The DOCX bookmark name for an HWP bookmark (letters, digits and `_`, 40 at most).
    fn bookmark_name(&mut self, name: &str) -> String {
        if let Some(known) = self.bookmarks.get(name) {
            return known.clone();
        }
        let mut base: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        if !base.chars().next().is_some_and(char::is_alphabetic) {
            base.insert(0, 'b');
        }
        let base: String = base.chars().take(36).collect();
        let mut docx = base.clone();
        let mut n = 1;
        while self.bookmarks.values().any(|v| *v == docx) {
            n += 1;
            docx = format!("{base}{n}");
        }
        self.bookmarks.insert(name.to_owned(), docx.clone());
        docx
    }

    /// A memo as a Word comment: its paragraphs go to a comment part.
    fn memo(&mut self, para: &Paragraph, control_idx: usize, depth: usize) -> Option<u32> {
        let Some(Control::Field(field)) = para.controls.get(control_idx) else {
            return None;
        };
        let mut blocks = Vec::new();
        for p in &field.memo_paragraphs {
            blocks.append(&mut self.paragraph(p, depth + 1));
        }
        if blocks.is_empty() {
            blocks.push(wc::para_block(wc::Paragraph::new()));
        }
        let part = self.part(PartKind::Comment, blocks);
        let id = self.next_comment;
        self.next_comment += 1;
        self.out.comments.insert(
            id,
            wc::Comment {
                author: "HwpCraft".into(),
                part,
                ..Default::default()
            },
        );
        Some(id)
    }

    fn inline_control(
        &mut self,
        control: &Control,
        index: usize,
        depth: usize,
        cur: &mut wc::Paragraph,
        props: CharProps,
    ) {
        match control {
            Control::Bookmark(bookmark) => {
                let name = self.bookmark_name(&bookmark.name);
                push_object(
                    cur,
                    InlineObject::BookmarkStart { name: name.clone() },
                    CharProps::default(),
                );
                push_object(
                    cur,
                    InlineObject::BookmarkEnd { name },
                    CharProps::default(),
                );
            }
            Control::Footnote(note) => {
                self.note(NoteKind::Footnote, &note.paragraphs, depth, cur, props)
            }
            Control::Endnote(note) => {
                self.note(NoteKind::Endnote, &note.paragraphs, depth, cur, props)
            }
            Control::Ruby(ruby) => {
                let size = props.size.unwrap_or(10.0);
                push_object(
                    cur,
                    InlineObject::Ruby {
                        base: ruby.main_text.clone(),
                        ruby: ruby.ruby_text.clone(),
                        align: Default::default(),
                        size: (size / 2.0).max(1.0),
                        raise: size,
                        base_size: size,
                        lang: String::new(),
                        props: Box::new(props.clone()),
                    },
                    props,
                );
            }
            Control::Shape(shape) => {
                if self.native_shape(shape, depth, cur, props.clone()) {
                    return;
                }
                let common = shape.common();
                let flows = common.treat_as_char
                    || matches!(
                        common.text_wrap,
                        TextWrap::Square
                            | TextWrap::TopAndBottom
                            | TextWrap::Tight
                            | TextWrap::Through
                    );
                if !flows {
                    self.omit("글 뒤·글 앞 도형");
                    return;
                }
                match (depth == 0 && self.in_body)
                    .then_some(self.address)
                    .flatten()
                {
                    Some((section, paragraph)) => {
                        let size = (i64::from(common.width), i64::from(common.height));
                        self.shape_picture((section, paragraph, index), size, common, cur, props);
                    }
                    None => self.omit("표·머리말·주석 안 도형"),
                }
            }
            Control::Equation(eq) => self.equation(eq, cur, props),
            Control::Form(_) => self.omit("양식 개체"),
            Control::CharOverlap(_) => self.omit("글자 겹치기"),
            Control::AutoNumber(number) => match number.number_type {
                AutoNumberType::Page => push_object(cur, field("PAGE", "1"), props),
                AutoNumberType::TotalPage => push_object(cur, field("NUMPAGES", "1"), props),
                AutoNumberType::Footnote | AutoNumberType::Endnote => {}
                _ => {
                    let n = if number.assigned_number > 0 {
                        number.assigned_number
                    } else {
                        number.number
                    };
                    for c in n.to_string().chars() {
                        push_char(cur, c, props.clone());
                    }
                }
            },
            Control::Field(field_ctl) if field_ctl.field_type == FieldType::Date => {
                push_object(cur, field("DATE", ""), props);
            }
            Control::HiddenComment(_) => self.omit("숨은 설명"),
            _ => {}
        }
    }

    fn note(
        &mut self,
        kind: NoteKind,
        paragraphs: &[Paragraph],
        depth: usize,
        cur: &mut wc::Paragraph,
        props: CharProps,
    ) {
        let id = self.next_part;
        self.next_part += 1;
        let mut blocks = Vec::new();
        for p in paragraphs {
            blocks.append(&mut self.paragraph(p, depth + 1));
        }
        if blocks.is_empty() {
            blocks.push(wc::para_block(wc::Paragraph::new()));
        }
        // The note's own number goes first in its first paragraph.
        if let Some(b) = blocks.iter_mut().find(|b| matches!(&***b, Block::Para(_)))
            && let Block::Para(p) = Arc::make_mut(b)
        {
            let mark = InlineObject::NoteRef {
                kind,
                id,
                custom: String::new(),
            };
            let mut np = wc::Paragraph::new();
            np.props = p.props.clone();
            np.mark = p.mark.clone();
            push_object(&mut np, mark, CharProps::default());
            append_paragraph(&mut np, p);
            *p = np;
        }
        let part_kind = match kind {
            NoteKind::Footnote => PartKind::Footnote,
            NoteKind::Endnote => PartKind::Endnote,
        };
        self.out.parts.insert(
            id,
            Part {
                kind: part_kind,
                blocks,
                ..Default::default()
            },
        );
        let mut p = props;
        p.vert_align = Some(VertAlign::Superscript);
        push_object(
            cur,
            InlineObject::NoteRef {
                kind,
                id,
                custom: String::new(),
            },
            p,
        );
    }

    /// Caption paragraphs of a table or picture: (before it, after it).
    fn caption(
        &mut self,
        caption: Option<&Caption>,
        depth: usize,
    ) -> (Vec<Arc<Block>>, Vec<Arc<Block>>) {
        let Some(caption) = caption else {
            return (Vec::new(), Vec::new());
        };
        let mut blocks = Vec::new();
        for p in &caption.paragraphs {
            let was = self.in_body;
            self.in_body = false;
            blocks.append(&mut self.paragraph(p, depth + 1));
            self.in_body = was;
        }
        if matches!(
            caption.direction,
            CaptionDirection::Top | CaptionDirection::Left
        ) {
            (blocks, Vec::new())
        } else {
            (Vec::new(), blocks)
        }
    }

    // ----- pictures -----

    fn bin_data(&self, id: u16) -> Option<&'a BinDataContent> {
        let list = &self.doc.bin_data_content;
        if id == 0 {
            return None;
        }
        list.get(usize::from(id) - 1)
            .filter(|b| b.id == id)
            .or_else(|| list.iter().find(|b| b.id == id))
            .or_else(|| list.get(usize::from(id) - 1))
    }

    fn picture(&mut self, pic: &Picture, cur: &mut wc::Paragraph, props: CharProps) {
        let id = pic.image_attr.bin_data_id;
        let Some(bin) = self.bin_data(id) else {
            self.omit("그림(데이터 없음)");
            return;
        };
        let ext = bin.extension.to_ascii_lowercase();
        let ext = if ext == "jpeg" { "jpg".to_owned() } else { ext };
        if !matches!(
            ext.as_str(),
            "png" | "jpg" | "gif" | "bmp" | "tif" | "tiff" | "emf" | "wmf" | "svg"
        ) {
            self.omit("그림(지원하지 않는 형식)");
            return;
        }
        let key = match self.media.get(&id) {
            Some(k) => k.clone(),
            None => {
                let Some(bytes) = bin.data.load_limited(MAX_PICTURE_BYTES) else {
                    self.omit("그림(너무 큼)");
                    return;
                };
                if bytes.is_empty() {
                    self.omit("그림(데이터 없음)");
                    return;
                }
                let k = self.out.add_media(bytes, &ext);
                self.media.insert(id, k.clone());
                k
            }
        };
        let w = hu_pt(i64::from(pic.common.width)).max(1.0);
        let h = hu_pt(i64::from(pic.common.height)).max(1.0);
        let crop = crop_fractions(pic, || bin.data.load_limited(MAX_PICTURE_BYTES));
        let mut float = self.float(&pic.common);
        float.rot = f32::from(pic.shape_attr.rotation_angle);
        float.flip_h = pic.shape_attr.horz_flip;
        float.flip_v = pic.shape_attr.vert_flip;
        push_object(
            cur,
            InlineObject::Image {
                media: key,
                w,
                h,
                alt: pic.common.description.clone(),
                float,
                crop,
                ole: None,
            },
            props,
        );
    }

    /// Rectangles, ellipses, lines and text boxes as Word shapes (their text stays text).
    /// Returns false for other drawing objects, which are pictured instead.
    fn native_shape(
        &mut self,
        shape: &ShapeObject,
        depth: usize,
        cur: &mut wc::Paragraph,
        props: CharProps,
    ) -> bool {
        let (kind, common, drawing) = match shape {
            ShapeObject::Rectangle(r) => (
                if r.round_rate > 0 {
                    ShapeKind::RoundedRectangle
                } else {
                    ShapeKind::Rectangle
                },
                &r.common,
                &r.drawing,
            ),
            ShapeObject::Ellipse(e) => (ShapeKind::Ellipse, &e.common, &e.drawing),
            ShapeObject::Line(l) => (ShapeKind::Line, &l.common, &l.drawing),
            ShapeObject::Picture(p) => {
                self.picture(p, cur, props);
                return true;
            }
            _ => return false,
        };
        let mut float = self.float(common);
        float.rot = f32::from(drawing.shape_attr.rotation_angle);
        float.flip_h = drawing.shape_attr.horz_flip;
        float.flip_v = drawing.shape_attr.vert_flip;
        if let ShapeObject::Line(l) = shape {
            // A line from bottom-left to top-right is the box's other diagonal.
            let rising = (l.end.x - l.start.x).signum() * (l.end.y - l.start.y).signum() < 0;
            if rising {
                float.flip_v = !float.flip_v;
            }
        }
        let border = &drawing.border_line;
        let (stroke, stroke_width) = if border.attr & 0x3F == 0 {
            (None, 0.0)
        } else {
            (rgb(border.color), (border.width as f32 / 100.0).max(0.35))
        };
        let story = match drawing.text_box.as_ref() {
            Some(tb) if kind != ShapeKind::Line => {
                let mut blocks = Vec::new();
                let was = self.in_body;
                self.in_body = false;
                for p in &tb.paragraphs {
                    blocks.append(&mut self.paragraph(p, depth + 1));
                }
                self.in_body = was;
                if blocks.is_empty() {
                    blocks.push(wc::para_block(wc::Paragraph::new()));
                }
                let emu = |hu: i16| u32::try_from(i32::from(hu).max(0) * 127).unwrap_or(0);
                let id = self.next_part;
                self.next_part += 1;
                self.out.parts.insert(
                    id,
                    Part {
                        kind: PartKind::TextBox,
                        blocks,
                        body: wc::TextBody {
                            anchor: match tb.vertical_align {
                                VerticalAlign::Top => VAlign::Top,
                                VerticalAlign::Center => VAlign::Center,
                                VerticalAlign::Bottom => VAlign::Bottom,
                            },
                            inset: Some([
                                emu(tb.margin_left),
                                emu(tb.margin_top),
                                emu(tb.margin_right),
                                emu(tb.margin_bottom),
                            ]),
                            ..Default::default()
                        },
                    },
                );
                Some(id)
            }
            _ => None,
        };
        push_object(
            cur,
            InlineObject::Shape {
                kind,
                w: hu_pt(i64::from(common.width)).max(0.75),
                h: hu_pt(i64::from(common.height)).max(0.75),
                fill: shape_fill(&drawing.fill),
                stroke,
                stroke_width,
                float,
                story,
                freeform: None,
                effects: Default::default(),
                extra: Default::default(),
            },
            props,
        );
        true
    }

    fn equation(
        &mut self,
        eq: &hwpcraft_doc::model::control::Equation,
        cur: &mut wc::Paragraph,
        props: CharProps,
    ) {
        let w = hu_pt(i64::from(eq.common.width));
        let h = hu_pt(i64::from(eq.common.height));
        if w <= 0.0 || h <= 0.0 {
            self.omit("수식(크기 없음)");
            return;
        }
        let key = (eq.script.clone(), eq.font_size, eq.color);
        let media = match self.equations.get(&key) {
            Some(m) => m.clone(),
            None => {
                let Some(png) = self.equation_png(eq) else {
                    self.omit("수식(그리지 못함)");
                    return;
                };
                let m = self.out.add_media(png, "png");
                self.equations.insert(key, m.clone());
                m
            }
        };
        self.omit("수식(그림으로 씀)");
        push_object(
            cur,
            InlineObject::Image {
                media,
                w,
                h,
                alt: eq.script.clone(),
                float: self.float(&eq.common),
                crop: [0.0; 4],
                ole: None,
            },
            props,
        );
    }

    fn equation_png(&mut self, eq: &hwpcraft_doc::model::control::Equation) -> Option<Vec<u8>> {
        let script = eq.script.trim();
        if script.is_empty() || script.chars().count() > 4000 {
            return None;
        }
        let size = if eq.font_size > 0 { eq.font_size } else { 1000 };
        let svg = self
            .core
            .render_equation_preview_native(script, size, eq.color)
            .ok()?;
        let renderer = self.renderer.get_or_insert_with(Default::default);
        let page = renderer.svg(&svg, 3.0).ok()?;
        if page.width == 0 || page.height == 0 {
            return None;
        }
        let image = image::RgbaImage::from_raw(page.width, page.height, page.rgba)?;
        png_bytes(image)
    }

    /// A body-level drawing object (text box, shape, chart) as a picture cut out of its
    /// rendered page, at its size and place in the document.
    fn shape_picture(
        &mut self,
        key: (usize, usize, usize),
        size: (i64, i64),
        common: &CommonObjAttr,
        cur: &mut wc::Paragraph,
        props: CharProps,
    ) {
        let Some((page, [x, y, w, h])) = self.shape_boxes().get(&key).copied() else {
            self.omit("그리기 개체(위치를 찾지 못함)");
            return;
        };
        let Some(png) = self.page_cutout(page, x, y, w, h) else {
            self.omit("그리기 개체(그리지 못함)");
            return;
        };
        self.omit("그리기 개체·글상자·차트(그림으로 씀)");
        let media = self.out.add_media(png, "png");
        // px at 96 dpi is 0.75 pt.
        let wpt = if size.0 > 0 {
            hu_pt(size.0)
        } else {
            (w * 0.75) as f32
        };
        let hpt = if size.1 > 0 {
            hu_pt(size.1)
        } else {
            (h * 0.75) as f32
        };
        push_object(
            cur,
            InlineObject::Image {
                media,
                w: wpt.max(1.0),
                h: hpt.max(1.0),
                alt: common.description.clone(),
                float: self.float(common),
                crop: [0.0; 4],
                ole: None,
            },
            props,
        );
    }

    fn shape_boxes(&mut self) -> &ShapeBoxes {
        if self.shape_boxes.is_none() {
            let mut boxes = BTreeMap::new();
            for page in 0..self.core.page_count() {
                let Ok(json) = self.core.get_page_control_layout_native(page) else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
                    continue;
                };
                for c in value["controls"].as_array().into_iter().flatten() {
                    if let Some((key, rect)) = body_drawing_box(c) {
                        boxes.entry(key).or_insert((page, rect));
                    }
                }
            }
            self.shape_boxes = Some(boxes);
        }
        self.shape_boxes.get_or_insert_with(BTreeMap::new)
    }

    fn page_cutout(&mut self, page: u32, x: f64, y: f64, w: f64, h: f64) -> Option<Vec<u8>> {
        const SCALE: f64 = 2.0;
        if !self.page_rasters.contains_key(&page) {
            let svg = self.core.render_page_svg_native(page).ok()?;
            let renderer = self.renderer.get_or_insert_with(Default::default);
            let raster = renderer.svg(&svg, SCALE as f32).ok()?;
            let image = image::RgbaImage::from_raw(raster.width, raster.height, raster.rgba)?;
            self.page_rasters.insert(page, image);
        }
        let image = self.page_rasters.get(&page)?;
        let left = (x * SCALE).floor().max(0.0) as u32;
        let top = (y * SCALE).floor().max(0.0) as u32;
        let right = ((x + w) * SCALE).ceil().min(f64::from(image.width())) as u32;
        let bottom = ((y + h) * SCALE).ceil().min(f64::from(image.height())) as u32;
        if right <= left || bottom <= top {
            return None;
        }
        png_bytes(
            image::imageops::crop_imm(image, left, top, right - left, bottom - top).to_image(),
        )
    }

    // ----- tables -----

    /// Write a table; returns the gap HWP keeps below it, in points.
    fn table(
        &mut self,
        table: &Table,
        depth: usize,
        host: &ParaProps,
        out: &mut Vec<Arc<Block>>,
    ) -> f32 {
        if depth >= MAX_DEPTH {
            self.omit("너무 깊이 중첩된 표");
            return 1.0;
        }
        let cols = usize::from(table.col_count.max(1));
        let rows = usize::from(table.row_count.max(1));
        let widths = grid_widths(table, cols);
        let total: i64 = widths.iter().sum();
        let mut props = TableProps {
            width: Some(hu_pt(total)),
            fixed: true,
            cell_margins: Some([0.0; 4]),
            ..Default::default()
        };
        let (gap_before, gap_after) = self.place_table(table, host, &mut props);
        if gap_before >= 1.0 {
            out.push(wc::para_block(spacer(gap_before)));
        }
        let mut wt = wc::Table {
            props,
            grid: widths.iter().map(|w| hu_pt(*w).max(1.0)).collect(),
            rows: Vec::new(),
            ..Default::default()
        };
        let starts: BTreeMap<(usize, usize), usize> = table
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| ((usize::from(c.row), usize::from(c.col)), i))
            .collect();
        let row_height = |r: usize| {
            table
                .cells
                .iter()
                .filter(|c| usize::from(c.row) == r && c.row_span <= 1)
                .map(|c| i64::from(c.height))
                .max()
                .or_else(|| table.row_sizes.get(r).map(|h| i64::from(*h)))
        };
        // How tall the text of a row is, from where its lines were laid out (points);
        // cells spanning rows count in their first row.
        let row_content = |r: usize| {
            table
                .cells
                .iter()
                .filter(|c| usize::from(c.row) == r)
                .map(|c| {
                    let text: i64 = c
                        .paragraphs
                        .iter()
                        .filter_map(|p| {
                            p.line_segs
                                .last()
                                .map(|l| i64::from(l.vertical_pos) + i64::from(l.line_height))
                        })
                        .max()
                        .unwrap_or(0);
                    hu_pt(text + i64::from(c.padding.top) + i64::from(c.padding.bottom))
                })
                .fold(0.0f32, f32::max)
        };
        // A one-row table that fits on a page keeps its height (a box drawn as one tall
        // cell). Otherwise a row taller than about half a page is laid out across pages,
        // and a minimum height there makes Word move the whole row to a new page first;
        // in a table of several rows Word's taller text would push it on as well.
        let total: i64 = (0..rows).filter_map(row_height).sum();
        let fits = rows == 1 && hu_pt(total) <= self.body_height;
        for r in 0..rows {
            let mut row = wc::Row::default();
            let height = row_height(r)
                .filter(|h| *h > 0)
                .map(hu_pt)
                .filter(|h| fits || *h < 360.0);
            row.props = RowProps {
                height,
                height_rule: HeightRule::AtLeast,
                header: table.repeat_header
                    && table
                        .cells
                        .iter()
                        .any(|c| c.is_header && usize::from(c.row) == r),
                // HWP still breaks a row taller than the room left; Word moves a row it
                // may not break to the next page first, so tall rows may break.
                cant_split: table.page_break != TablePageBreak::CellBreak
                    && height.is_some_and(|h| h < 150.0)
                    && row_content(r) < 150.0,
                ..Default::default()
            };
            let mut c = 0;
            while c < cols {
                if let Some(&i) = starts.get(&(r, c)) {
                    let cell = &table.cells[i];
                    let span = usize::from(cell.col_span.max(1)).min(cols - c);
                    let width: i64 = widths[c..c + span].iter().sum();
                    let mut cp = self.cell_props(table, cell.border_fill_id, hu_pt(width), span);
                    if cell.row_span > 1 {
                        cp.vmerge = VMerge::Restart;
                    }
                    let pad = if cell.apply_inner_margin {
                        &cell.padding
                    } else {
                        &table.padding
                    };
                    cp.margins = Some([
                        hu_pt(i64::from(pad.top)),
                        hu_pt(i64::from(pad.left)),
                        hu_pt(i64::from(pad.bottom)),
                        hu_pt(i64::from(pad.right)),
                    ]);
                    cp.valign = Some(match cell.vertical_align {
                        VerticalAlign::Top => VAlign::Top,
                        VerticalAlign::Center => VAlign::Center,
                        VerticalAlign::Bottom => VAlign::Bottom,
                    });
                    if cell.text_direction != 0 {
                        cp.text_direction = wc::props::TextDirection::Down;
                        cp.text_direction_set = true;
                    }
                    let mut blocks = Vec::new();
                    let was = self.in_body;
                    self.in_body = false;
                    for p in &cell.paragraphs {
                        blocks.append(&mut self.paragraph(p, depth + 1));
                    }
                    self.in_body = was;
                    // HWP sizes a cell to the last line without the gap below it.
                    if let Some(last) = cell.paragraphs.last()
                        && last.line_segs.len() == 1
                        && last.line_segs[0].line_height > 0
                        && let Some(b) = blocks.last_mut()
                        && let Block::Para(wp) = Arc::make_mut(b)
                        && matches!(wp.props.line_spacing, Some(LineSpacing::Exactly(_)))
                    {
                        wp.props.line_spacing = Some(LineSpacing::Exactly(hu_pt(i64::from(
                            last.line_segs[0].line_height,
                        ))));
                    }
                    if !matches!(blocks.last().map(|b| &**b), Some(Block::Para(_))) {
                        blocks.push(wc::para_block(wc::Paragraph::new()));
                    }
                    row.cells.push(wc::Cell {
                        props: cp,
                        blocks,
                        ..Default::default()
                    });
                    c += span;
                } else if let Some(cell) = table.cells.iter().find(|cell| {
                    usize::from(cell.col) == c
                        && usize::from(cell.row) < r
                        && r < usize::from(cell.row) + usize::from(cell.row_span)
                }) {
                    // Covered by a vertical merge from above.
                    let span = usize::from(cell.col_span.max(1)).min(cols - c);
                    let width: i64 = widths[c..c + span].iter().sum();
                    let mut cp = self.cell_props(table, cell.border_fill_id, hu_pt(width), span);
                    cp.vmerge = VMerge::Continue;
                    row.cells.push(wc::Cell {
                        props: cp,
                        blocks: vec![wc::para_block(wc::Paragraph::new())],
                        ..Default::default()
                    });
                    c += span;
                } else if table.cells.iter().any(|cell| {
                    usize::from(cell.row) <= r
                        && r < usize::from(cell.row) + usize::from(cell.row_span.max(1))
                        && usize::from(cell.col) < c
                        && c < usize::from(cell.col) + usize::from(cell.col_span.max(1))
                }) {
                    c += 1;
                } else {
                    // A hole in a damaged table: keep the grid rectangular.
                    let mut cell = wc::Cell::empty();
                    cell.props.width = Some(hu_pt(widths[c]));
                    row.cells.push(cell);
                    c += 1;
                }
            }
            wt.rows.push(row);
        }
        out.push(Arc::new(Block::Table(wt)));
        gap_after
    }

    fn cell_props(&self, table: &Table, border_fill_id: u16, width: f32, span: usize) -> CellProps {
        let mut cp = CellProps {
            width: Some(width),
            span: span as u32,
            ..Default::default()
        };
        let id = if border_fill_id > 0 {
            border_fill_id
        } else {
            table.border_fill_id
        };
        if let Some(bf) = self.border_fill(id) {
            let side = |l: &BorderLine| border(l, 0.0).or(Some(Border::default()));
            cp.borders = Some(Borders {
                left: side(&bf.borders[0]),
                right: side(&bf.borders[1]),
                top: side(&bf.borders[2]),
                bottom: side(&bf.borders[3]),
                ..Default::default()
            });
            cp.shading = solid_fill(bf);
        }
        cp
    }

    /// Inline and paragraph-anchored "자리차지" tables are Word block tables; others float.
    /// Returns the gaps (points) HWP keeps above and below a block table.
    fn place_table(
        &mut self,
        table: &Table,
        host: &ParaProps,
        props: &mut TableProps,
    ) -> (f32, f32) {
        let c = &table.common;
        let block = c.treat_as_char
            || (c.text_wrap == TextWrap::TopAndBottom && c.vert_rel_to == VertRelTo::Para);
        let margins = [
            hu_pt(i64::from(table.outer_margin_left)),
            hu_pt(i64::from(table.outer_margin_top)),
            hu_pt(i64::from(table.outer_margin_right)),
            hu_pt(i64::from(table.outer_margin_bottom)),
        ];
        if block {
            if c.treat_as_char {
                props.align = match host.align {
                    Some(wc::Align::Center) => Some(wc::Align::Center),
                    Some(wc::Align::Right) => Some(wc::Align::Right),
                    _ => None,
                };
                // A table set like a character sits where the paragraph's first line
                // starts (a hanging paragraph puts it left of the other lines).
                let first_line = host.indent_left.unwrap_or(0.0) + host.indent_first.unwrap_or(0.0);
                props.indent = Some(first_line).filter(|v| *v != 0.0);
            } else {
                props.align = match c.horz_align {
                    HorzAlign::Center => Some(wc::Align::Center),
                    HorzAlign::Right | HorzAlign::Outside => Some(wc::Align::Right),
                    _ => None,
                };
                if props.align.is_none() {
                    let x = hu_pt(i64::from(c.horizontal_offset as i32));
                    let base = match c.horz_rel_to {
                        HorzRelTo::Para => host.indent_left.unwrap_or(0.0),
                        _ => 0.0,
                    };
                    props.indent = Some(x + base).filter(|v| *v != 0.0);
                }
                let y = hu_pt(i64::from(c.vertical_offset as i32));
                // An offset larger than a page is not a gap HWP keeps (header and footer
                // objects carry their page position here).
                let y = if (0.0..=200.0).contains(&y) { y } else { 0.0 };
                return (margins[1] + y, margins[3].max(1.0));
            }
            return (0.0, 1.0);
        }
        if matches!(c.text_wrap, TextWrap::BehindText | TextWrap::InFrontOfText) {
            self.omit("글 뒤·글 앞 표(떠 있는 표로 씀)");
        }
        let f = self.float(c);
        props.float = Some(TableFloat {
            h_rel: match f.h_rel {
                Anchor::Page => Anchor::Page,
                Anchor::Margin => Anchor::Margin,
                _ => Anchor::Column,
            },
            v_rel: match f.v_rel {
                Anchor::Page => Anchor::Page,
                Anchor::Margin => Anchor::Margin,
                _ => Anchor::Paragraph,
            },
            x: f.x,
            y: f.y,
            h_align: f.h_align,
            v_align: f.v_align,
            dist: margins,
            overlap: c.allow_overlap,
        });
        (0.0, 1.0)
    }
}

// ----- free helpers -----

/// Installed font families, read once per process.
fn installed_fonts() -> &'static std::collections::HashSet<String> {
    static FONTS: std::sync::OnceLock<std::collections::HashSet<String>> =
        std::sync::OnceLock::new();
    FONTS.get_or_init(|| {
        hwpcraft_render::Renderer::default()
            .font_families()
            .into_iter()
            .collect()
    })
}

/// An installed face of the same kind (serif, sans, heavy) as a missing Korean font.
fn similar_face(name: &str, installed: &std::collections::HashSet<String>) -> Option<String> {
    let heavy = ["견고딕", "태고딕", "헤드라인", "굵은"];
    let serif = ["명조", "바탕", "궁서", "Myeong", "Batang", "serif"];
    let choices: &[&str] = if heavy.iter().any(|k| name.contains(k)) {
        &["HY견고딕", "HY헤드라인M", "맑은 고딕"]
    } else if name.contains("견명조") {
        &["HY견명조", "HY신명조", "바탕"]
    } else if serif.iter().any(|k| name.contains(k)) {
        &["HY신명조", "함초롬바탕", "바탕", "Batang"]
    } else if name.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)) {
        &["HY중고딕", "함초롬돋움", "돋움", "맑은 고딕"]
    } else {
        return None;
    };
    choices
        .iter()
        .find(|c| installed.contains(**c))
        .map(|c| (*c).to_owned())
}

fn style_id(i: usize) -> String {
    if i == 0 {
        "Normal".into()
    } else {
        format!("HwpStyle{i}")
    }
}

/// Paragraph properties for the part of a paragraph after a table it holds.
fn continuation(props: &ParaProps) -> ParaProps {
    let mut p = props.clone();
    p.page_break_before = None;
    p.numbering = None;
    p.indent_first = Some(0.0);
    p.space_before = Some(0.0);
    p
}

/// The distance between the lines of a paragraph as laid out (the most common one), in
/// points. HWP measures the line from its fonts and adds the line spacing; Word's exact
/// spacing reproduces that distance.
fn line_pitch(para: &Paragraph) -> Option<f32> {
    let segs = &para.line_segs;
    let first = segs.first()?;
    let own = |s: &hwpcraft_doc::model::paragraph::LineSeg| s.line_height + s.line_spacing.max(0);
    let mut pitches: Vec<i32> = segs
        .windows(2)
        .filter_map(|w| {
            let d = w[1].vertical_pos - w[0].vertical_pos;
            // A new page or column starts over.
            (d > 0 && d <= own(&w[0]) * 2).then_some(d)
        })
        .collect();
    if pitches.is_empty() {
        pitches.push(own(first));
    }
    pitches.sort_unstable();
    let pitch = pitches[pitches.len() / 2];
    (pitch > 0 && first.line_height > 0).then(|| (hu_pt(i64::from(pitch)) * 20.0).round() / 20.0)
}

/// A paragraph holding only a table (set like a character or as a block) and spaces:
/// Word writes the table without the paragraph.
fn table_host(p: &Paragraph) -> bool {
    p.text.chars().all(char::is_whitespace)
        && p.controls.iter().any(|c| matches!(c, Control::Table(_)))
        && !p.controls.iter().any(|c| {
            matches!(
                c,
                Control::Picture(_) | Control::Shape(_) | Control::Equation(_)
            )
        })
}

/// An empty paragraph exactly `height` points tall.
fn spacer(height: f32) -> wc::Paragraph {
    let mut p = wc::Paragraph::new();
    p.props.space_before = Some(0.0);
    p.props.space_after = Some(0.0);
    p.props.line_spacing = Some(LineSpacing::Exactly(height.max(1.0)));
    p.mark.size = Some(1.0);
    p
}

fn push_char(p: &mut wc::Paragraph, c: char, props: CharProps) {
    let len = c.len_utf8();
    p.text.push(c);
    match p.runs.last_mut() {
        Some(last) if last.props == props => last.len += len,
        _ => p.runs.push(wc::Run { len, props }),
    }
}

fn push_object(p: &mut wc::Paragraph, obj: InlineObject, props: CharProps) {
    p.objects.push(obj);
    push_char(p, wc::para::OBJ, props);
}

/// Append `src` (text, runs and objects) to `dst`.
fn append_paragraph(dst: &mut wc::Paragraph, src: &wc::Paragraph) {
    let mut start = 0;
    for run in &src.runs {
        let text = &src.text[start..start + run.len];
        for c in text.chars() {
            push_char(dst, c, run.props.clone());
        }
        start += run.len;
    }
    dst.objects.extend(src.objects.iter().cloned());
}

fn field(instr: &str, result: &str) -> InlineObject {
    InlineObject::Field {
        instr: format!(" {instr} "),
        result: result.into(),
        locked: false,
        dirty: false,
        code: false,
        form: String::new(),
    }
}

fn page_number_para(
    align: wc::Align,
    pos: &hwpcraft_doc::model::control::PageNumberPos,
) -> wc::Paragraph {
    let mut p = wc::Paragraph::new();
    p.props.align = Some(align);
    let deco = |c: char| (!c.is_control() && c != '\0' && c != ' ').then_some(c);
    // 줄표 넣기 ("- 1 -") goes outside the prefix and suffix.
    let dash = deco(pos.dash_char);
    for c in [dash, deco(pos.prefix_char)].into_iter().flatten() {
        push_char(&mut p, c, CharProps::default());
        push_char(&mut p, ' ', CharProps::default());
    }
    push_object(&mut p, field("PAGE", "1"), CharProps::default());
    for c in [deco(pos.suffix_char), dash].into_iter().flatten() {
        push_char(&mut p, ' ', CharProps::default());
        push_char(&mut p, c, CharProps::default());
    }
    p
}

fn page_num_format(format: u8) -> NumFormat {
    match format {
        2 => NumFormat::UpperRoman,
        3 => NumFormat::LowerRoman,
        4 => NumFormat::UpperLetter,
        5 => NumFormat::LowerLetter,
        _ => NumFormat::Decimal,
    }
}

/// HWP number format (표 43 order) as a Word list format.
pub(crate) fn num_format(code: u8) -> NumFormat {
    match code {
        1 => NumFormat::DecimalEnclosedCircle,
        2 => NumFormat::UpperRoman,
        3 => NumFormat::LowerRoman,
        4 => NumFormat::UpperLetter,
        5 => NumFormat::LowerLetter,
        8 => NumFormat::Ganada,
        10 => NumFormat::Chosung,
        12 => NumFormat::CardinalText,
        13 => NumFormat::IdeographTraditional,
        _ => NumFormat::Decimal,
    }
}

/// HWP level format ("^1.", "(^2)") as Word's ("%1.", "(%2)").
pub(crate) fn level_text(format: &str) -> String {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^'
            && let Some(d) = chars.peek().copied().filter(|d| ('1'..='7').contains(d))
        {
            chars.next();
            out.push('%');
            out.push(d);
        } else {
            out.push(c);
        }
    }
    if out.is_empty() { "%1.".into() } else { out }
}

fn underline_style(shape: u8) -> Underline {
    // HWP line shapes (표 27): 0 solid, 1 long dash, 2 dot, 3 dash-dot, 4 dash-dot-dot,
    // 5 long dash, 6 big dot, 7 double, 8 thin-thick, 9 thick-thin, 10 triple, 11 wave.
    match shape {
        1 | 5 => Underline::Dash,
        2 | 6 => Underline::Dotted,
        3 => Underline::DotDash,
        4 => Underline::DotDotDash,
        7..=10 => Underline::Double,
        11 => Underline::Wave,
        12 => Underline::DoubleWave,
        _ => Underline::Single,
    }
}

fn border(line: &BorderLine, space: f32) -> Option<Border> {
    let style = match line.line_type {
        BorderLineType::None => return None,
        BorderLineType::Solid => BorderStyle::Single,
        BorderLineType::Dash | BorderLineType::LongDash => BorderStyle::Dashed,
        BorderLineType::Dot | BorderLineType::Circle => BorderStyle::Dotted,
        BorderLineType::DashDot | BorderLineType::DashDotDot => BorderStyle::DotDash,
        BorderLineType::Double
        | BorderLineType::ThinThickDouble
        | BorderLineType::ThickThinDouble => BorderStyle::Double,
        BorderLineType::ThinThickThinTriple => BorderStyle::Triple,
        BorderLineType::Wave | BorderLineType::DoubleWave => BorderStyle::Wave,
        _ => BorderStyle::Single,
    };
    Some(Border {
        style,
        width: border_pt(line.width),
        color: rgb(line.color),
        space,
    })
}

/// The fill of a drawing object: a colour (with HWP's transparency) or a linear gradient.
fn shape_fill(fill: &hwpcraft_doc::model::style::Fill) -> Option<wordcraft_doc::fill::Fill> {
    let transparency = f32::from(fill.alpha) / 255.0;
    if let Some(g) = fill
        .gradient
        .as_ref()
        .filter(|_| fill.fill_type == hwpcraft_doc::model::style::FillType::Gradient)
    {
        let n = g.colors.len();
        let stops: Vec<wordcraft_doc::wordart::GradStop> = g
            .colors
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let pos = match g.positions.get(i) {
                    Some(p) => *p as f32 / 100.0,
                    None => i as f32 / (n.max(2) - 1) as f32,
                };
                Some(wordcraft_doc::wordart::GradStop {
                    pos: pos.clamp(0.0, 1.0),
                    color: rgb(*c)?,
                    transparency,
                })
            })
            .collect();
        if stops.len() >= 2 {
            return Some(wordcraft_doc::fill::Fill::Gradient(Arc::new(
                wordcraft_doc::fill::Gradient {
                    stops,
                    kind: wordcraft_doc::fill::GradientKind::Linear {
                        angle: f32::from(g.angle),
                        scaled: false,
                    },
                    tile: None,
                    rot_with_shape: true,
                    flip: Default::default(),
                },
            )));
        }
    }
    let solid = fill.solid.as_ref()?;
    if solid.pattern_type > 0 {
        return None;
    }
    rgb(solid.background_color).map(|color| wordcraft_doc::fill::Fill::Solid {
        color,
        transparency,
    })
}

/// The background colour of a border fill; a gradient gives its middle colour, as Word
/// shades cells and paragraphs with one colour.
fn solid_fill(bf: &BorderFill) -> Option<Rgb> {
    if let Some(solid) = bf.fill.solid.as_ref() {
        return rgb(solid.background_color).filter(|c| *c != Rgb(255, 255, 255));
    }
    let g = bf.fill.gradient.as_ref()?;
    let colors: Vec<Rgb> = g.colors.iter().filter_map(|c| rgb(*c)).collect();
    match colors.len() {
        0 => None,
        n => {
            let avg = |f: fn(&Rgb) -> u8| {
                (colors.iter().map(|c| u32::from(f(c))).sum::<u32>() / n as u32) as u8
            };
            Some(Rgb(avg(|c| c.0), avg(|c| c.1), avg(|c| c.2)))
        }
    }
}

/// Column widths from single-column cells, then spread spanning cells.
fn grid_widths(table: &Table, cols: usize) -> Vec<i64> {
    let mut widths: Vec<Option<i64>> = vec![None; cols];
    for cell in table.cells.iter().filter(|c| c.col_span <= 1) {
        if let Some(w) = widths.get_mut(usize::from(cell.col)) {
            *w = Some(w.unwrap_or(0).max(i64::from(cell.width)));
        }
    }
    for cell in table.cells.iter().filter(|c| c.col_span > 1) {
        let start = usize::from(cell.col).min(cols);
        let end = (start + usize::from(cell.col_span)).min(cols);
        let known: i64 = widths[start..end].iter().flatten().sum();
        let unknown = widths[start..end].iter().filter(|w| w.is_none()).count() as i64;
        if unknown > 0 {
            let share = ((i64::from(cell.width) - known) / unknown).max(500);
            for w in widths[start..end].iter_mut().filter(|w| w.is_none()) {
                *w = Some(share);
            }
        }
    }
    let fallback = (i64::from(table.common.width) / cols as i64).max(1000);
    widths.into_iter().map(|w| w.unwrap_or(fallback)).collect()
}

/// Word placement of an HWP object.
fn float_of(c: &CommonObjAttr) -> Float {
    if c.treat_as_char {
        return Float::default();
    }
    let mut f = Float {
        wrap: match c.text_wrap {
            TextWrap::Square => Wrap::Square,
            TextWrap::Tight => Wrap::Tight,
            TextWrap::Through => Wrap::Through,
            TextWrap::TopAndBottom => Wrap::TopAndBottom,
            TextWrap::BehindText => Wrap::BehindText,
            TextWrap::InFrontOfText => Wrap::InFrontOfText,
        },
        wrap_text: match c.text_flow {
            TextFlow::BothSides => WrapText::BothSides,
            TextFlow::LeftOnly => WrapText::Left,
            TextFlow::RightOnly => WrapText::Right,
            TextFlow::LargestOnly => WrapText::Largest,
        },
        h_rel: match c.horz_rel_to {
            HorzRelTo::Paper => Anchor::Page,
            HorzRelTo::Page => Anchor::Margin,
            HorzRelTo::Column | HorzRelTo::Para => Anchor::Column,
        },
        v_rel: match c.vert_rel_to {
            VertRelTo::Paper => Anchor::Page,
            VertRelTo::Page => Anchor::Margin,
            VertRelTo::Para => Anchor::Paragraph,
        },
        x: hu_pt(i64::from(c.horizontal_offset as i32)),
        y: hu_pt(i64::from(c.vertical_offset as i32)),
        dist: hu_pt(i64::from(c.margin.left.max(c.margin.right))),
        dist_top: hu_pt(i64::from(c.margin.top)),
        dist_bottom: hu_pt(i64::from(c.margin.bottom)),
        z: u32::try_from(c.z_order.max(0)).unwrap_or(0),
        ..Default::default()
    };
    f.h_align = match c.horz_align {
        HorzAlign::Left => None,
        HorzAlign::Center => Some(FloatAlign::Center),
        HorzAlign::Right => Some(FloatAlign::End),
        HorzAlign::Inside => Some(FloatAlign::Inside),
        HorzAlign::Outside => Some(FloatAlign::Outside),
    };
    f.v_align = match c.vert_align {
        HwpVertAlign::Top => None,
        HwpVertAlign::Center => Some(FloatAlign::Center),
        HwpVertAlign::Bottom => Some(FloatAlign::End),
        HwpVertAlign::Inside => Some(FloatAlign::Inside),
        HwpVertAlign::Outside => Some(FloatAlign::Outside),
    };
    // A paragraph-relative object is placed from the paragraph top in both.
    if c.vert_rel_to == VertRelTo::Para {
        f.v_align = None;
    }
    f
}

/// Crop (left, top, right, bottom) as fractions of the picture. HWP crop coordinates
/// span the full picture as `imgDim` says; without it an axis whose start is 0 has its
/// end at the full range, and otherwise 75 HWPUNIT is one pixel (RHWP's reading).
fn crop_fractions(pic: &Picture, bytes: impl FnOnce() -> Option<Vec<u8>>) -> [f32; 4] {
    let c = &pic.crop;
    if c.right <= c.left || c.bottom <= c.top {
        return [0.0; 4];
    }
    let (full_w, full_h) = match pic.crop_reference_size() {
        Some((w, h)) => (w as f32, h as f32),
        None => {
            if c.left == 0 && c.top == 0 {
                return [0.0; 4];
            }
            let Some((pw, ph)) = bytes().and_then(|b| {
                image::ImageReader::new(std::io::Cursor::new(b))
                    .with_guessed_format()
                    .ok()?
                    .into_dimensions()
                    .ok()
            }) else {
                return [0.0; 4];
            };
            let scale_x = (c.left == 0).then(|| c.right as f32 / pw as f32);
            let scale_y = (c.top == 0).then(|| c.bottom as f32 / ph as f32);
            let (sx, sy) = match (scale_x, scale_y) {
                (Some(x), Some(y)) => (x, y),
                (Some(s), None) | (None, Some(s)) => (s, s),
                (None, None) => (75.0, 75.0),
            };
            (pw as f32 * sx, ph as f32 * sy)
        }
    };
    if full_w <= 0.0 || full_h <= 0.0 {
        return [0.0; 4];
    }
    let f = |v: f32| if v < 0.001 { 0.0 } else { v.min(0.99) };
    [
        f(c.left as f32 / full_w),
        f(c.top as f32 / full_h),
        f(1.0 - c.right as f32 / full_w),
        f(1.0 - c.bottom as f32 / full_h),
    ]
}

fn columns_of(def: &ColumnDef) -> Columns {
    let count = u32::from(def.column_count.clamp(1, 45));
    let widths = if def.same_width || def.widths.len() < count as usize {
        Vec::new()
    } else {
        (0..count as usize)
            .map(|i| {
                (
                    hu_pt(def.widths.get(i).copied().map_or(0, i64::from)),
                    hu_pt(def.gaps.get(i).copied().map_or(0, i64::from)),
                )
            })
            .collect()
    };
    Columns {
        count,
        space: hu_pt(i64::from(def.spacing)),
        separator: def.separator_type != 0,
        widths,
    }
}

/// The paragraphs of a section split where its column definition changes
/// (단 정의 나누기): (first paragraph, columns of that run). A paragraph holding a
/// table wider than one column gets a one-column run of its own, as Word tables
/// cannot cross columns.
fn column_runs(section: &Section) -> Vec<(usize, Option<ColumnDef>)> {
    let page = &section.section_def.page_def;
    let body_width = i64::from(page.width)
        - i64::from(page.margin_left)
        - i64::from(page.margin_right)
        - i64::from(page.margin_gutter);
    let column_width = |def: &ColumnDef| {
        let n = i64::from(def.column_count.max(1));
        (body_width - i64::from(def.spacing) * (n - 1)) / n
    };
    let mut runs: Vec<(usize, Option<ColumnDef>)> = vec![(0, None)];
    let mut current: Option<ColumnDef> = None;
    for (pi, para) in section.paragraphs.iter().enumerate() {
        if let Some(def) = para.controls.iter().find_map(|c| match c {
            Control::ColumnDef(def) => Some(def.clone()),
            _ => None,
        }) {
            current = Some(def.clone());
            match runs.last_mut() {
                Some((start, columns)) if *start == pi => *columns = Some(def),
                _ => runs.push((pi, Some(def))),
            }
        }
        let wide = current.as_ref().is_some_and(|def| {
            def.column_count > 1
                && para.controls.iter().any(|c| {
                    matches!(c, Control::Table(t) if i64::from(t.common.width) > column_width(def) + 500)
                })
        });
        if wide {
            match runs.last_mut() {
                Some((start, columns)) if *start == pi => *columns = None,
                _ => runs.push((pi, None)),
            }
            if pi + 1 < section.paragraphs.len() {
                runs.push((pi + 1, current.clone()));
            }
        }
    }
    runs
}

fn png_bytes(image: image::RgbaImage) -> Option<Vec<u8>> {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

/// A body-level drawing object (shape or OLE) in RHWP's page control layout: its
/// (section, paragraph, control) and box. Objects in cells, headers and footers, and
/// notes are left out: their paragraph and control numbers count inside the cell,
/// header or note, so they would take the box of a body object with the same numbers.
fn body_drawing_box(c: &serde_json::Value) -> Option<((usize, usize, usize), [f64; 4])> {
    if !matches!(c["type"].as_str(), Some("shape" | "ole"))
        || ["cellIdx", "headerFooter", "noteRef"]
            .iter()
            .any(|k| c.get(k).is_some())
    {
        return None;
    }
    let index = |k: &str| c[k].as_u64().map(|v| v as usize);
    let num = |k: &str| c[k].as_f64().unwrap_or(0.0);
    Some((
        (index("secIdx")?, index("paraIdx")?, index("controlIdx")?),
        [num("x"), num("y"), num("w"), num("h")],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Result<DocumentCore> {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(core)
    }

    /// RHWP lays a header's floating objects out in the header area whatever they are
    /// set relative to: a paper-relative logo 399.9 pt from the left and 0.77 pt up sits
    /// that far from the area's corner (56.7 pt, 42.5 pt), so its right edge meets the
    /// right margin as drawn. Outside a header the paper position stays.
    #[test]
    fn header_objects_are_placed_in_the_header_area() -> Result<()> {
        let core = blank()?;
        let mut exporter = Exporter::new(&core);
        let logo = CommonObjAttr {
            horz_rel_to: HorzRelTo::Paper,
            vert_rel_to: VertRelTo::Paper,
            horizontal_offset: 39994,
            vertical_offset: (-77i32) as u32,
            width: 8196,
            height: 3382,
            ..Default::default()
        };
        let body = exporter.float(&logo);
        assert_eq!((body.h_rel, body.v_rel), (Anchor::Page, Anchor::Page));
        assert!((body.x - 399.94).abs() < 0.01 && (body.y + 0.77).abs() < 0.01);
        exporter.hf_area = Some([56.7, 42.51, 481.9, 42.51]);
        let header = exporter.float(&logo);
        assert_eq!((header.h_rel, header.v_rel), (Anchor::Page, Anchor::Page));
        assert!((header.x - 456.64).abs() < 0.01, "{}", header.x);
        assert!((header.y - 41.74).abs() < 0.01, "{}", header.y);
        // Right-aligned: from the area's right edge; a paragraph-relative object keeps
        // its vertical place from the paragraph.
        let right = CommonObjAttr {
            horz_align: HorzAlign::Right,
            vert_rel_to: VertRelTo::Para,
            vertical_offset: 100,
            width: 8196,
            ..Default::default()
        };
        let placed = exporter.float(&right);
        assert!(
            (placed.x - (56.7 + 481.9 - 81.96)).abs() < 0.01,
            "{}",
            placed.x
        );
        assert_eq!(placed.v_rel, Anchor::Paragraph);
        assert!((placed.y - 1.0).abs() < 0.01);
        Ok(())
    }

    fn line(vertical_pos: i32) -> hwpcraft_doc::model::paragraph::LineSeg {
        hwpcraft_doc::model::paragraph::LineSeg {
            vertical_pos,
            line_height: 1500,
            text_height: 1500,
            line_spacing: 824,
            ..Default::default()
        }
    }

    /// The space Hangul left between two paragraphs on a page becomes Word's space
    /// before: here 752 HWPUNIT (7.52 pt) beyond the 23.24 pt line pitch. Across a page
    /// (the next paragraph starts higher) the paragraph shape's spacing stays.
    #[test]
    fn space_between_paragraphs_follows_hangul_lines() -> Result<()> {
        let core = blank()?;
        let exporter = Exporter::new(&core);
        let prev = Paragraph {
            line_segs: vec![line(14884), line(17208)],
            ..Default::default()
        };
        let next = |at| Paragraph {
            line_segs: vec![line(at)],
            ..Default::default()
        };
        assert_eq!(exporter.stored_gap(&prev, &next(20284)), Some(7.5));
        assert_eq!(exporter.stored_gap(&prev, &next(19532)), Some(0.0));
        assert_eq!(exporter.stored_gap(&prev, &next(0)), None);
        Ok(())
    }

    /// A header shape numbered like a body shape must not lend it its box (RHWP marks
    /// header and footer objects with `headerFooter`, notes with `noteRef`).
    #[test]
    fn header_footer_and_note_shapes_do_not_take_a_body_shapes_box() {
        let layout = serde_json::json!({"controls": [
            {"type": "shape", "secIdx": 0, "paraIdx": 1, "controlIdx": 0,
             "x": 10.0, "y": 20.0, "w": 30.0, "h": 40.0,
             "headerFooter": {"kind": "header", "outerParaIdx": 0, "outerControlIdx": 2}},
            {"type": "shape", "secIdx": 0, "paraIdx": 1, "controlIdx": 0,
             "x": 1.0, "y": 2.0, "w": 3.0, "h": 4.0,
             "noteRef": {"kind": "footnote", "sectionIdx": 0, "paraIdx": 3, "controlIdx": 1,
                         "noteParaIdx": 1, "innerControlIdx": 0}},
            {"type": "shape", "secIdx": 0, "paraIdx": 1, "controlIdx": 0, "cellIdx": 0,
             "x": 5.0, "y": 6.0, "w": 7.0, "h": 8.0},
            {"type": "image", "secIdx": 0, "paraIdx": 1, "controlIdx": 0,
             "x": 9.0, "y": 9.0, "w": 9.0, "h": 9.0},
            {"type": "shape", "secIdx": 0, "paraIdx": 1, "controlIdx": 0,
             "x": 100.0, "y": 200.0, "w": 50.0, "h": 60.0},
        ]});
        let found: Vec<_> = layout["controls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(body_drawing_box)
            .collect();
        assert_eq!(found, [((0, 1, 0), [100.0, 200.0, 50.0, 60.0])]);
    }
}
