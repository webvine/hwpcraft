//! WordCraft document → HWPX package, which RHWP then opens like any HWPX file.
//!
//! Word's formatting is resolved first (document defaults, styles, direct formatting,
//! table styles), and every paragraph and run gets the HWP paragraph and character shape
//! of what it shows. Paragraph styles are carried over by name. Tables become tables set
//! like characters in a paragraph of their own (Word's block tables), pictures and simple
//! shapes keep their place and wrapping, and headers, footers, notes, page numbers,
//! links, bookmarks and sections map to their HWP counterparts. Line layout is left to
//! RHWP, which lays out an HWPX without stored lines on open.
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::Arc;

use anyhow::{Context, Result};
use wordcraft_doc as wc;
use wordcraft_doc::numbering::LevelSuffix;
use wordcraft_doc::para::{Anchor, Float, FloatAlign, NoteKind, ShapeKind, Wrap, WrapText};
use wordcraft_doc::props::{
    Border, BorderStyle, Borders, LineSpacing, TabAlign, TabLeader, Underline, VAlign, VMerge,
    VertAlign,
};
use wordcraft_doc::resolve::{ResolvedChar, ResolvedPara};
use wordcraft_doc::section::{NumFormat, SectionStart};
use wordcraft_doc::styles::{StyleKind, TablePart};
use wordcraft_doc::{Block, CharProps, InlineObject, ParaProps, Rgb, TextColor};

use crate::units::{BORDER_MM, border_index, hex};

/// An HWPX package and what it could not carry, as Korean "what · count" lines.
pub struct Converted {
    pub hwpx: Vec<u8>,
    pub omitted: Vec<String>,
}

/// Nested tables deeper than this are written as their text.
const MAX_DEPTH: usize = 12;
/// Word's default cell margins (top, left, bottom, right), points.
const WORD_CELL_MARGINS: [f32; 4] = [0.0, 5.4, 0.0, 5.4];

pub fn to_hwpx(doc: &wc::Document) -> Result<Converted> {
    let mut w = HwpxWriter::new(doc);
    w.styles();
    let sections = w.sections();
    let hwpx = w.package(&sections)?;
    let omitted = w
        .omitted
        .iter()
        .map(|(what, count)| format!("{what} {count}개"))
        .collect();
    Ok(Converted { hwpx, omitted })
}

fn hu(pt: f32) -> i64 {
    if pt.is_finite() {
        (f64::from(pt) * 100.0).round().clamp(-2.0e9, 2.0e9) as i64
    } else {
        0
    }
}

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// HWP languages in HWPX order.
const LANGS: [&str; 7] = [
    "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
];

const NS: &str = r##"xmlns:ha="http://www.hancom.co.kr/hwpml/2011/app" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hp10="http://www.hancom.co.kr/hwpml/2016/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:hhs="http://www.hancom.co.kr/hwpml/2011/history" xmlns:hm="http://www.hancom.co.kr/hwpml/2011/master-page" xmlns:hpf="http://www.hancom.co.kr/schema/2011/hpf" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf/" xmlns:ooxmlchart="http://www.hancom.co.kr/hwpml/2016/ooxmlchart" xmlns:hwpunitchar="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar" xmlns:epub="http://www.idpf.org/2007/ops" xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0""##;

const SUBLIST: &str = r##"textDirection="HORIZONTAL" lineWrap="BREAK" linkListIDRef="0" linkListNextIDRef="0" textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0""##;

/// A list of distinct XML items with their ids.
#[derive(Default)]
struct Pool {
    items: Vec<String>,
    index: HashMap<String, usize>,
}

impl Pool {
    fn id(&mut self, key: String) -> usize {
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = self.items.len();
        self.index.insert(key.clone(), i);
        self.items.push(key);
        i
    }
}

struct Picture {
    item: String,
    w: f32,
    h: f32,
}

struct HwpxWriter<'a> {
    doc: &'a wc::Document,
    fonts: [Pool; 7],
    char_prs: Pool,
    para_prs: Pool,
    /// Border fills; HWP ids are 1-based (index + 1).
    border_fills: Pool,
    tab_prs: Pool,
    /// Numberings and bullets; HWP ids are 1-based.
    numberings: Pool,
    bullets: Pool,
    styles: Vec<String>,
    style_ids: HashMap<String, usize>,
    /// Pictures in the package: (item id, extension, bytes).
    bin: Vec<(String, String, Arc<Vec<u8>>)>,
    media: HashMap<String, String>,
    /// Word list (num id, level) → HWP numbering or bullet.
    lists: HashMap<u32, (bool, usize)>,
    next_id: u64,
    note_number: [u32; 2],
    omitted: BTreeMap<&'static str, usize>,
    /// The note whose own text is being written (its self-reference is HWP's number).
    current_note: Option<(NoteKind, u32)>,
    /// The width text runs in where blocks are being written (points): the section's
    /// text width, or a cell's inside.
    avail: f32,
    /// While a header or footer is written: its area's left and top on the page, the
    /// page's width and height, and Word's top margin (points).
    hf_frame: Option<[f32; 5]>,
}

impl<'a> HwpxWriter<'a> {
    fn new(doc: &'a wc::Document) -> Self {
        let mut w = Self {
            doc,
            fonts: Default::default(),
            char_prs: Pool::default(),
            para_prs: Pool::default(),
            border_fills: Pool::default(),
            tab_prs: Pool::default(),
            numberings: Pool::default(),
            bullets: Pool::default(),
            styles: Vec::new(),
            style_ids: HashMap::new(),
            bin: Vec::new(),
            media: HashMap::new(),
            lists: HashMap::new(),
            next_id: 1_000_000,
            note_number: [0, 0],
            omitted: BTreeMap::new(),
            current_note: None,
            avail: 451.0,
            hf_frame: None,
        };
        // Border fill 1: none (pages, tables); 2: none with an empty brush (characters).
        w.border_fills
            .id(border_fill_xml(&[None, None, None, None], None));
        w.border_fills
            .id(border_fill_xml(&[None, None, None, None], Some(None)));
        w.tab_prs
            .id(r##"autoTabLeft="0" autoTabRight="0"/>"##.into());
        w
    }

    fn omit(&mut self, what: &'static str) {
        *self.omitted.entry(what).or_default() += 1;
    }

    fn uid(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn font(&mut self, lang: usize, name: &str) -> usize {
        let name = if name.trim().is_empty() {
            "함초롬바탕"
        } else {
            name.trim()
        };
        self.fonts[lang].id(name.to_owned())
    }

    // ----- character and paragraph shapes -----

    fn char_pr(&mut self, c: &ResolvedChar) -> usize {
        let size = c.size.max(1.0);
        let latin = c.font.clone();
        let ea = crate::fonts::hangul_face(&c.font_ea.clone().unwrap_or_else(|| latin.clone()));
        let ids = [
            self.font(0, &ea),
            self.font(1, &latin),
            self.font(2, &ea),
            self.font(3, &ea),
            self.font(4, &latin),
            self.font(5, &latin),
            self.font(6, &latin),
        ];
        let per_lang = |v: i64| {
            LANGS
                .iter()
                .map(|l| format!(r##" {}="{v}""##, l.to_ascii_lowercase()))
                .collect::<String>()
        };
        let font_ref: String = LANGS
            .iter()
            .zip(ids)
            .map(|(l, id)| format!(r##" {}="{id}""##, l.to_ascii_lowercase()))
            .collect();
        let ratio = (c.scale.round() as i64).clamp(50, 200);
        let spacing = ((c.spacing / size) * 100.0).round().clamp(-50.0, 50.0) as i64;
        let offset = ((c.position / size) * 100.0).round().clamp(-100.0, 100.0) as i64;
        let color = match c.color {
            TextColor::Rgb(rgb) => hex(rgb),
            TextColor::Auto => "#000000".into(),
        };
        let shade = c.shading.or(c.highlight).map_or("none".into(), hex);
        let border_fill = match c.border.filter(Border::is_visible) {
            Some(b) => {
                let side = Some(b);
                1 + self
                    .border_fills
                    .id(border_fill_xml(&[side, side, side, side], Some(None)))
            }
            None => 2,
        };
        let mut x = format!(
            r##"height="{}" textColor="{color}" shadeColor="{shade}" useFontSpace="0" useKerning="{}" symMark="{}" borderFillIDRef="{border_fill}"><hh:fontRef{font_ref}/><hh:ratio{}/><hh:spacing{}/><hh:relSz{}/><hh:offset{}/>"##,
            hu(size),
            u8::from(c.kern > 0.0),
            match c.emphasis {
                wc::props::EmphasisMark::None => "NONE",
                wc::props::EmphasisMark::Circle => "RING_ABOVE",
                wc::props::EmphasisMark::UnderDot => "DOT_BELOW",
                _ => "DOT_ABOVE",
            },
            per_lang(ratio),
            per_lang(spacing),
            per_lang(100),
            per_lang(offset),
        );
        if c.italic {
            x.push_str("<hh:italic/>");
        }
        if c.bold {
            x.push_str("<hh:bold/>");
        }
        let (utype, ushape) = match c.underline {
            Underline::None => ("NONE", "SOLID"),
            Underline::Single | Underline::Words | Underline::Thick => ("BOTTOM", "SOLID"),
            Underline::Double => ("BOTTOM", "DOUBLE_SLIM"),
            Underline::Dotted => ("BOTTOM", "DOT"),
            Underline::Dash => ("BOTTOM", "DASH"),
            Underline::DotDash => ("BOTTOM", "DASH_DOT"),
            Underline::DotDotDash => ("BOTTOM", "DASH_DOT_DOT"),
            Underline::Wave => ("BOTTOM", "WAVE"),
            Underline::DoubleWave => ("BOTTOM", "DOUBLE_WAVE"),
        };
        let ucolor = c.underline_color.map_or_else(|| color.clone(), hex);
        let _ = write!(
            x,
            r##"<hh:underline type="{utype}" shape="{ushape}" color="{ucolor}"/>"##
        );
        let strike = if c.double_strike {
            "DOUBLE_SLIM"
        } else if c.strike {
            "SOLID"
        } else {
            "NONE"
        };
        let _ = write!(x, r##"<hh:strikeout shape="{strike}" color="{color}"/>"##);
        let _ = write!(
            x,
            r##"<hh:outline type="{}"/><hh:shadow type="{}" color="#B2B2B2" offsetX="10" offsetY="10"/>"##,
            if c.outline { "SOLID" } else { "NONE" },
            if c.shadow { "DROP" } else { "NONE" }
        );
        if c.emboss {
            x.push_str("<hh:emboss/>");
        }
        if c.engrave {
            x.push_str("<hh:engrave/>");
        }
        match c.vert_align {
            VertAlign::Superscript => x.push_str("<hh:supscript/>"),
            VertAlign::Subscript => x.push_str("<hh:subscript/>"),
            VertAlign::Baseline => {}
        }
        self.char_prs.id(x)
    }

    fn tab_pr(&mut self, tabs: &[wc::props::TabStop]) -> usize {
        let tabs: Vec<_> = tabs
            .iter()
            .filter(|t| t.align != TabAlign::Clear && t.pos >= 0.0)
            .collect();
        if tabs.is_empty() {
            return 0;
        }
        let mut x = String::from(r##"autoTabLeft="0" autoTabRight="0">"##);
        for t in tabs {
            let kind = match t.align {
                TabAlign::Right => "RIGHT",
                TabAlign::Center => "CENTER",
                TabAlign::Decimal => "DECIMAL",
                _ => "LEFT",
            };
            let leader = match t.leader {
                TabLeader::None => "NONE",
                TabLeader::Underscore => "SOLID",
                TabLeader::Hyphen => "DASH",
                TabLeader::Dot | TabLeader::MiddleDot => "DOT",
            };
            let pos = hu(t.pos);
            let _ = write!(
                x,
                r##"<hp:switch><hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar"><hh:tabItem pos="{pos}" type="{kind}" leader="{leader}" unit="HWPUNIT"/></hp:case><hp:default><hh:tabItem pos="{}" type="{kind}" leader="{leader}"/></hp:default></hp:switch>"##,
                pos * 2
            );
        }
        x.push_str("</hh:tabPr>");
        self.tab_prs.id(x)
    }

    /// The paragraph shape of resolved paragraph properties. `size` is the paragraph's
    /// text size (points), which turns Word's line multiples into HWP percentages.
    fn para_pr(
        &mut self,
        p: &ResolvedPara,
        size: f32,
        font: &str,
        heading: Option<(&str, usize, u8)>,
    ) -> usize {
        let align = match p.align {
            wc::Align::Left => "LEFT",
            wc::Align::Center => "CENTER",
            wc::Align::Right => "RIGHT",
            wc::Align::Justify => "JUSTIFY",
            wc::Align::Distribute => "DISTRIBUTE",
        };
        let tab = self.tab_pr(&p.tabs);
        let border_fill = if p.borders.is_some() || p.shading.is_some() {
            let b = p.borders.unwrap_or_default();
            1 + self.border_fills.id(border_fill_xml(
                &[b.left, b.right, b.top, b.bottom],
                Some(p.shading),
            ))
        } else {
            2
        };
        let pad = |b: Option<Border>| b.map_or(0, |b| hu(b.space));
        let b = p.borders.unwrap_or_default();
        // Word hangs the first line out to the left of the others; HWP's 내어쓰기 keeps it
        // at the margin and moves the others in.
        let first = p.indent_first;
        let left = if first < 0.0 {
            p.indent_left + first
        } else {
            p.indent_left
        };
        let before = if p.before_auto { 14.0 } else { p.space_before };
        let after = if p.after_auto { 14.0 } else { p.space_after };
        let (ls_type, ls_value) = match p.line_spacing {
            LineSpacing::Multiple(m) => (
                "PERCENT",
                (m * 100.0 * line_factor(font)).round().clamp(30.0, 500.0) as i64,
            ),
            LineSpacing::Exactly(pt) => ("FIXED", hu(pt.max(1.0))),
            LineSpacing::AtLeast(pt) => {
                // HWP's minimum spacing keeps lines at least that far apart, as Word's.
                ("MINIMUM", hu(pt.max(size)))
            }
        };
        let (htype, hid, hlevel) = heading.unwrap_or(("NONE", 0, 0));
        let margin = |scale: i64| {
            format!(
                r##"<hh:margin><hc:intent value="{}" unit="HWPUNIT"/><hc:left value="{}" unit="HWPUNIT"/><hc:right value="{}" unit="HWPUNIT"/><hc:prev value="{}" unit="HWPUNIT"/><hc:next value="{}" unit="HWPUNIT"/></hh:margin>"##,
                hu(first) * scale,
                hu(left.max(-1000.0)) * scale,
                hu(p.indent_right) * scale,
                hu(before.max(0.0)) * scale,
                hu(after.max(0.0)) * scale
            )
        };
        let line = |scale: i64| {
            let v = if ls_type == "PERCENT" {
                ls_value
            } else {
                ls_value * scale
            };
            format!(r##"<hh:lineSpacing type="{ls_type}" value="{v}" unit="HWPUNIT"/>"##)
        };
        let x = format!(
            r##"tabPrIDRef="{tab}" condense="0" fontLineHeight="0" snapToGrid="0" suppressLineNumbers="{}" checked="0"><hh:align horizontal="{align}" vertical="BASELINE"/><hh:heading type="{htype}" idRef="{hid}" level="{hlevel}"/><hh:breakSetting breakLatinWord="{}" breakNonLatinWord="KEEP_WORD" widowOrphan="{}" keepWithNext="{}" keepLines="{}" pageBreakBefore="{}" lineWrap="BREAK"/><hh:autoSpacing eAsianEng="{}" eAsianNum="{}"/><hp:switch><hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar">{}{}</hp:case><hp:default>{}{}</hp:default></hp:switch><hh:border borderFillIDRef="{border_fill}" offsetLeft="{}" offsetRight="{}" offsetTop="{}" offsetBottom="{}" connect="0" ignoreMargin="0"/>"##,
            u8::from(p.suppress_line_numbers),
            if p.word_wrap {
                "KEEP_WORD"
            } else {
                "BREAK_WORD"
            },
            u8::from(p.widow_control),
            u8::from(p.keep_next),
            u8::from(p.keep_lines),
            u8::from(p.page_break_before),
            u8::from(p.auto_space_de),
            u8::from(p.auto_space_dn),
            margin(1),
            line(1),
            margin(2),
            line(2),
            pad(b.left),
            pad(b.right),
            pad(b.top),
            pad(b.bottom),
        );
        self.para_prs.id(x)
    }

    // ----- styles and lists -----

    fn styles(&mut self) {
        let sheet = self.doc.styles.clone();
        // Style 0 is HWP's 바탕글: Word's default paragraph style.
        let normal = sheet
            .styles
            .iter()
            .find(|s| s.kind == StyleKind::Paragraph && s.default)
            .or_else(|| sheet.get("Normal"))
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "Normal".into());
        let mut order: Vec<&wc::Style> = Vec::new();
        if let Some(s) = sheet.get(&normal) {
            order.push(s);
        }
        for s in &sheet.styles {
            if s.id != normal
                && matches!(s.kind, StyleKind::Paragraph | StyleKind::Character)
                && !s.hidden
            {
                order.push(s);
            }
        }
        for (i, s) in order.iter().enumerate() {
            self.style_ids.insert(s.id.clone(), i);
        }
        let mut xml = Vec::new();
        for (i, s) in order.iter().enumerate() {
            let para = ParaProps {
                style: Some(s.id.clone()),
                ..Default::default()
            };
            let rp = sheet.resolve_para(&para);
            let rc = sheet.resolve_char(Some(&s.id), &CharProps::default());
            let cpr = self.char_pr(&rc);
            let ppr = self.para_pr(&rp, rc.size, &rc.font, None);
            let next = s
                .next
                .as_ref()
                .and_then(|n| self.style_ids.get(n).copied())
                .unwrap_or(i);
            let name = if i == 0 {
                "바탕글".to_owned()
            } else {
                s.name.clone()
            };
            let eng = if i == 0 {
                "Normal".to_owned()
            } else {
                s.id.clone()
            };
            xml.push(format!(
                r##"<hh:style id="{i}" type="{}" name="{}" engName="{}" paraPrIDRef="{ppr}" charPrIDRef="{cpr}" nextStyleIDRef="{next}" langID="1042" lockForm="0"/>"##,
                if s.kind == StyleKind::Character { "CHAR" } else { "PARA" },
                esc(&name),
                esc(&eng)
            ));
        }
        if xml.is_empty() {
            let rc = sheet.resolve_char(None, &CharProps::default());
            let rp = sheet.resolve_para(&ParaProps::default());
            let cpr = self.char_pr(&rc);
            let ppr = self.para_pr(&rp, rc.size, &rc.font, None);
            xml.push(format!(
                r##"<hh:style id="0" type="PARA" name="바탕글" engName="Normal" paraPrIDRef="{ppr}" charPrIDRef="{cpr}" nextStyleIDRef="0" langID="1042" lockForm="0"/>"##
            ));
        }
        self.styles = xml;
    }

    /// Level `level` of Word list `num` (its override, else its definition's).
    fn list_level(&self, num: u32, level: u8) -> Option<wc::numbering::Level> {
        let numbering = &self.doc.numbering;
        let n = numbering.nums.iter().find(|n| n.id == num)?;
        let a = numbering.abstracts.iter().find(|a| a.id == n.abstract_id)?;
        n.level_overrides
            .iter()
            .find(|(l, _)| *l == level)
            .map(|(_, l)| l.clone())
            .or_else(|| a.levels.get(usize::from(level)).cloned())
    }

    /// The HWP numbering or bullet of a Word list: (is bullet, 1-based id).
    fn list(&mut self, num: u32) -> Option<(bool, usize)> {
        if let Some(found) = self.lists.get(&num) {
            return Some(*found);
        }
        let numbering = self.doc.numbering.clone();
        let n = numbering.nums.iter().find(|n| n.id == num)?;
        let a = numbering.abstracts.iter().find(|a| a.id == n.abstract_id)?;
        let level = |i: usize| -> Option<wc::numbering::Level> {
            n.level_overrides
                .iter()
                .find(|(l, _)| usize::from(*l) == i)
                .map(|(_, l)| l.clone())
                .or_else(|| a.levels.get(i).cloned())
        };
        let first = level(0)?;
        let found = if first.format == NumFormat::Bullet {
            let c = bullet_char(first.text.chars().next().unwrap_or('•'));
            // The bullet takes 1em in HWP; the rest of Word's hanging indent is added.
            let size = first.chr.size.unwrap_or(10.0);
            let adjust = number_area(&first, &c.to_string(), size);
            let x = format!(
                r##"char="{}" useImage="0"><hh:paraHead level="0" align="LEFT" useInstWidth="1" autoIndent="0" widthAdjust="{adjust}" textOffsetType="HWPUNIT" textOffset="0" numFormat="DIGIT" charPrIDRef="4294967295" checkable="0"/></hh:bullet>"##,
                esc(&c.to_string())
            );
            (true, self.bullets.id(x) + 1)
        } else {
            let mut x = String::from(r##"start="0">"##);
            for i in 0..10 {
                let l = level(i.min(8)).unwrap_or_default();
                let start = n
                    .start_overrides
                    .iter()
                    .find(|(lv, _)| usize::from(*lv) == i)
                    .map_or(l.start, |(_, s)| *s);
                let shown = level_sample(&l.text, start);
                let adjust = number_area(&l, &shown, l.chr.size.unwrap_or(10.0));
                let _ = write!(
                    x,
                    r##"<hh:paraHead start="{start}" level="{}" align="LEFT" useInstWidth="1" autoIndent="0" widthAdjust="{adjust}" textOffsetType="HWPUNIT" textOffset="0" numFormat="{}" charPrIDRef="4294967295" checkable="0">{}</hh:paraHead>"##,
                    i + 1,
                    hwp_num_format(l.format),
                    esc(&hwp_level_text(&l.text))
                );
            }
            x.push_str("</hh:numbering>");
            (false, self.numberings.id(x) + 1)
        };
        self.lists.insert(num, found);
        Some(found)
    }

    // ----- sections and blocks -----

    fn sections(&mut self) -> Vec<String> {
        let doc = self.doc;
        // Word keeps a section's properties on its last paragraph.
        let mut runs: Vec<(Vec<&Arc<Block>>, wc::SectionProps)> = Vec::new();
        let mut current: Vec<&Arc<Block>> = Vec::new();
        for block in &doc.body {
            current.push(block);
            if let Block::Para(p) = &**block
                && let Some(props) = &p.section
            {
                runs.push((std::mem::take(&mut current), (**props).clone()));
            }
        }
        runs.push((current, doc.last_section.clone()));
        // Word sections that start on the same page are one HWP section with a new column
        // definition; others start a new HWP section.
        let mut out = Vec::new();
        let mut xml = String::new();
        let mut first_props: Option<wc::SectionProps> = None;
        for (blocks, props) in runs {
            let continuous =
                matches!(props.start, SectionStart::Continuous) && first_props.is_some();
            if !continuous && first_props.is_some() {
                out.push(std::mem::take(&mut xml));
            }
            let columns = props.columns.count.max(1) as f32;
            self.avail = ((props.page_w
                - props.margin_left
                - props.margin_right
                - props.gutter
                - props.columns.space * (columns - 1.0))
                / columns)
                .max(36.0);
            let mut head = String::new();
            if continuous {
                let _ = write!(head, "<hp:ctrl>{}</hp:ctrl>", col_pr(&props));
            } else {
                head = self.section_head(&props);
                first_props = Some(props.clone());
            }
            let mut paras = self.blocks(&blocks, 0);
            if paras.is_empty() {
                paras.push(self.empty_paragraph());
            }
            // The section definition sits in the first run of the section's first paragraph.
            if let Some(p) = paras.first_mut() {
                insert_head(p, &head);
            }
            for p in paras {
                xml.push_str(&p);
            }
        }
        out.push(xml);
        out
    }

    fn empty_paragraph(&mut self) -> String {
        let sheet = self.doc.styles.clone();
        let rc = sheet.resolve_char(None, &CharProps::default());
        let rp = sheet.resolve_para(&ParaProps::default());
        let cpr = self.char_pr(&rc);
        let ppr = self.para_pr(&rp, rc.size, &rc.font, None);
        format!(
            r##"<hp:p id="0" paraPrIDRef="{ppr}" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0"><hp:run charPrIDRef="{cpr}"><hp:t></hp:t></hp:run></hp:p>"##
        )
    }

    /// `<hp:secPr>` with headers, footers and page numbering, as controls of the first run.
    fn section_head(&mut self, s: &wc::SectionProps) -> String {
        let landscape = s.page_w > s.page_h;
        let (pw, ph) = if landscape {
            (s.page_h, s.page_w)
        } else {
            (s.page_w, s.page_h)
        };
        // Word's top margin is to the body and its header distance to the header; HWP's
        // top margin is to the header and its header margin the header's height.
        let top = s.header.max(0.0).min(s.margin_top.abs());
        let header = (s.margin_top.abs() - top).max(0.0);
        let bottom = s.footer.max(0.0).min(s.margin_bottom.abs());
        let footer = (s.margin_bottom.abs() - bottom).max(0.0);
        let start = s.page_num_start.unwrap_or(0);
        let mut x = format!(
            r##"<hp:secPr id="" textDirection="HORIZONTAL" spaceColumns="{}" tabStop="{}" tabStopVal="{}" tabStopUnit="HWPUNIT" outlineShapeIDRef="0" memoShapeIDRef="0" textVerticalWidthHead="0" masterPageCnt="0"><hp:grid lineGrid="0" charGrid="0" wonggojiFormat="0"/><hp:startNum pageStartsOn="BOTH" page="{start}" pic="0" tbl="0" equation="0"/><hp:visibility hideFirstHeader="0" hideFirstFooter="0" hideFirstMasterPage="0" border="SHOW_ALL" fill="SHOW_ALL" hideFirstPageNum="0" hideFirstEmptyLine="0" showLineNumber="{}"/><hp:lineNumberShape restartType="0" countBy="{}" distance="{}" startNumber="{}"/><hp:pagePr landscape="{}" width="{}" height="{}" gutterType="LEFT_ONLY"><hp:margin header="{}" footer="{}" gutter="{}" left="{}" right="{}" top="{}" bottom="{}"/></hp:pagePr>"##,
            hu(s.columns.space),
            hu(self.doc.settings.default_tab.max(1.0)),
            hu(self.doc.settings.default_tab.max(1.0)) / 2,
            u8::from(s.line_numbers.is_some()),
            s.line_numbers.as_ref().map_or(0, |l| l.count_by),
            s.line_numbers.as_ref().map_or(0, |l| hu(l.distance)),
            s.line_numbers.as_ref().map_or(0, |l| l.start),
            if landscape { "NARROWLY" } else { "WIDELY" },
            hu(pw),
            hu(ph),
            hu(header),
            hu(footer),
            hu(s.gutter),
            hu(s.margin_left),
            hu(s.margin_right),
            hu(top),
            hu(bottom),
        );
        x.push_str(r##"<hp:footNotePr><hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar=")" supscript="0"/><hp:noteLine length="-1" type="SOLID" width="0.12 mm" color="#000000"/><hp:noteSpacing betweenNotes="283" belowLine="567" aboveLine="850"/><hp:numbering type="CONTINUOUS" newNum="1"/><hp:placement place="EACH_COLUMN" beneathText="0"/></hp:footNotePr><hp:endNotePr><hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar=")" supscript="0"/><hp:noteLine length="14692344" type="SOLID" width="0.12 mm" color="#000000"/><hp:noteSpacing betweenNotes="0" belowLine="567" aboveLine="850"/><hp:numbering type="CONTINUOUS" newNum="1"/><hp:placement place="END_OF_DOCUMENT" beneathText="0"/></hp:endNotePr>"##);
        for kind in ["BOTH", "EVEN", "ODD"] {
            let _ = write!(
                x,
                r##"<hp:pageBorderFill type="{kind}" borderFillIDRef="1" textBorder="PAPER" headerInside="0" footerInside="0" fillArea="PAPER"><hp:offset left="1417" right="1417" top="1417" bottom="1417"/></hp:pageBorderFill>"##
            );
        }
        x.push_str("</hp:secPr>");
        let mut ctrls = format!("<hp:ctrl>{}</hp:ctrl>", col_pr(s));
        // Headers and footers: default (both or odd pages), even pages; a first page
        // without one hides them there.
        let even = self.doc.settings.even_odd_headers;
        for (is_footer, set) in [(false, s.headers), (true, s.footers)] {
            let tag = if is_footer { "footer" } else { "header" };
            for (kind, id) in [("BOTH", set.default), ("EVEN", set.even)] {
                let Some(id) = id else { continue };
                if kind == "EVEN" && !even {
                    continue;
                }
                let kind = if kind == "BOTH" && even && set.even.is_some() {
                    "ODD"
                } else {
                    kind
                };
                let Some(part) = self.doc.parts.get(&id) else {
                    continue;
                };
                let blocks: Vec<&Arc<Block>> = part.blocks.iter().collect();
                // HWP draws a header's floating objects in the header area (as RHWP
                // does), so their page positions become positions in that area.
                let area_top = if is_footer {
                    s.page_h - s.margin_bottom.abs()
                } else {
                    top
                };
                self.hf_frame = Some([
                    s.margin_left,
                    area_top,
                    s.page_w,
                    s.page_h,
                    s.margin_top.abs(),
                ]);
                let paras = self.blocks(&blocks, 1).concat();
                self.hf_frame = None;
                let valign = if is_footer { "BOTTOM" } else { "TOP" };
                let _ = write!(
                    ctrls,
                    r##"<hp:ctrl><hp:{tag} id="{}" applyPageType="{kind}"><hp:subList id="" {SUBLIST} vertAlign="{valign}">{paras}</hp:subList></hp:{tag}></hp:ctrl>"##,
                    self.uid() % 1_000_000
                );
            }
        }
        if s.title_page {
            let empty = |id: Option<u32>| {
                id.and_then(|id| self.doc.parts.get(&id)).is_none_or(|p| {
                    p.blocks
                        .iter()
                        .all(|b| b.as_para().is_some_and(|p| p.text.trim().is_empty()))
                })
            };
            let (hide_header, hide_footer) = (empty(s.headers.first), empty(s.footers.first));
            if s.headers.first.is_some() && !hide_header
                || s.footers.first.is_some() && !hide_footer
            {
                self.omit("첫 쪽만 다른 머리말·꼬리말(기본 것으로 씀)");
            }
            let _ = write!(
                ctrls,
                r##"<hp:ctrl><hp:pageHiding hideHeader="{}" hideFooter="{}" hideMasterPage="0" hideBorder="0" hideFill="0" hidePageNum="{}"/></hp:ctrl>"##,
                u8::from(hide_header),
                u8::from(hide_footer),
                u8::from(hide_header && hide_footer)
            );
        }
        format!("{x}{ctrls}")
    }

    /// Blocks as HWP paragraphs (`<hp:p>` strings).
    fn blocks(&mut self, blocks: &[&Arc<Block>], depth: usize) -> Vec<String> {
        let mut out = Vec::new();
        for block in blocks {
            match &***block {
                Block::Para(p) => out.push(self.paragraph(p, None)),
                Block::Table(t) => {
                    if depth >= MAX_DEPTH {
                        self.omit("너무 깊이 중첩된 표(글자만 씀)");
                        for row in &t.rows {
                            for cell in &row.cells {
                                let inner: Vec<&Arc<Block>> = cell.blocks.iter().collect();
                                out.extend(self.blocks(&inner, depth));
                            }
                        }
                        continue;
                    }
                    out.push(self.table_paragraph(t, depth));
                }
            }
        }
        out
    }

    /// One Word paragraph as an HWP paragraph. `table` is the table style's formatting
    /// for a paragraph in a table cell.
    fn paragraph(&mut self, p: &wc::Paragraph, table: Option<(&ParaProps, &CharProps)>) -> String {
        let sheet = self.doc.styles.clone();
        let mut rp = sheet.resolve_para_in(&p.props, table.map(|t| t.0));
        let para_style = p.props.style.as_deref();
        if let Some(level) = rp.numbering.and_then(|n| self.list_level(n.num, n.level)) {
            // Word's list indents apply unless the paragraph (or the style giving the list)
            // sets its own.
            let own_left = p.props.indent_left.is_some()
                || p.props.indent_left_chars.is_some_and(|c| c != 0.0);
            let own_first = p.props.indent_first.is_some()
                || p.props.indent_first_chars.is_some_and(|c| c != 0.0);
            let (style_left, style_first) = if p.props.numbering.is_some() {
                (false, false)
            } else {
                sheet.indents_over_list(para_style.unwrap_or("Normal"))
            };
            if !own_left && !style_left {
                rp.indent_left = level.indent;
            }
            if !own_first && !style_first {
                rp.indent_first = -level.hanging;
            }
        }
        let mut mark = sheet.resolve_char_in(para_style, table.map(|t| t.1), &p.mark);
        // The largest text sets HWP's line height; Word's multiples follow the first font.
        let mut size = 0.0f32;
        let faces = |rc: &ResolvedChar| {
            let ea = crate::fonts::hangul_face(rc.font_ea.as_deref().unwrap_or(&rc.font));
            if line_factor(&ea) > line_factor(&rc.font) {
                ea
            } else {
                rc.font.clone()
            }
        };
        let mut font = faces(&mark);
        let mut first_run = true;
        for run in &p.runs {
            let rc = sheet.resolve_char_in(para_style, table.map(|t| t.1), &run.props);
            size = size.max(rc.size);
            if first_run {
                font = faces(&rc);
                first_run = false;
            }
        }
        if size <= 0.0 {
            size = mark.size;
        }
        // RHWP gives a fixed line at least the height of its text, so an empty line thinner
        // than its mark (a spacer, or the thin rows of a rule drawn as a table) would grow:
        // its mark gets that small instead.
        if p.text.chars().all(char::is_whitespace)
            && p.objects.is_empty()
            && let LineSpacing::Exactly(line) = rp.line_spacing
            && line < mark.size
        {
            mark.size = line.max(1.0);
            size = mark.size;
        }
        let heading = rp.numbering.and_then(|n| {
            let (bullet, id) = self.list(n.num)?;
            Some((if bullet { "BULLET" } else { "NUMBER" }, id, n.level.min(6)))
        });
        let ppr = self.para_pr(&rp, size, &font, heading);
        let style = para_style
            .and_then(|s| self.style_ids.get(s).copied())
            .unwrap_or(0);
        let mut body = String::new();
        let mut page_break = rp.page_break_before;
        let mut column_break = false;
        let mut run_open: Option<usize> = None;
        let mut text_open = false;
        let mut field_depth: Vec<bool> = Vec::new();
        let mut link_open: Option<String> = None;
        let mut link_id = 0u64;
        let mut objects = p.objects.iter();
        let mut pos = 0usize;
        let mut cur_props: Option<CharProps> = None;
        let mut cpr_mark = self.char_pr(&mark);
        for run in &p.runs {
            let text = &p.text[pos..(pos + run.len).min(p.text.len())];
            pos += run.len;
            let rc = sheet.resolve_char_in(para_style, table.map(|t| t.1), &run.props);
            // Tracked deletions are left out (the changes come in accepted).
            if rc.del.is_some() {
                for c in text.chars() {
                    if c == wc::para::OBJ {
                        objects.next();
                    }
                }
                self.omit("변경 추적(반영한 결과로 씀)");
                continue;
            }
            if rc.ins.is_some() {
                self.omit("변경 추적(반영한 결과로 씀)");
            }
            if rc.hidden {
                for c in text.chars() {
                    if c == wc::para::OBJ {
                        objects.next();
                    }
                }
                continue;
            }
            let cpr = self.char_pr(&rc);
            cpr_mark = cpr;
            // Hyperlinks open and close as fields around their runs.
            if rc.link != link_open {
                close_text(&mut body, &mut text_open);
                if link_open.take().is_some() {
                    ensure_run(&mut body, &mut run_open, cpr);
                    let _ = write!(
                        body,
                        r##"<hp:ctrl><hp:fieldEnd beginIDRef="{link_id}"/></hp:ctrl>"##
                    );
                }
                if let Some(target) = rc.link.as_ref().filter(|l| !l.is_empty()) {
                    ensure_run(&mut body, &mut run_open, cpr);
                    let command = match target.strip_prefix('#') {
                        Some(bookmark) => format!("?{bookmark}"),
                        None => target.replace(':', "\\:"),
                    };
                    link_id = self.uid() % 2_000_000_000;
                    let _ = write!(
                        body,
                        r##"<hp:ctrl><hp:fieldBegin id="{link_id}" type="HYPERLINK" name="" editable="0" dirty="0"><hp:parameters cnt="1" name=""><hp:stringParam name="Command">{};1;0;0;</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl>"##,
                        esc(&command)
                    );
                    link_open = Some(target.clone());
                }
            }
            if cur_props.as_ref() != Some(&run.props) || run_open != Some(cpr) {
                close_text(&mut body, &mut text_open);
                if run_open.is_some() {
                    body.push_str("</hp:run>");
                }
                let _ = write!(body, r##"<hp:run charPrIDRef="{cpr}">"##);
                run_open = Some(cpr);
                cur_props = Some(run.props.clone());
            }
            let hide = field_depth.iter().any(|h| *h);
            for c in text.chars() {
                match c {
                    wc::para::OBJ => {
                        let Some(obj) = objects.next() else { continue };
                        close_text(&mut body, &mut text_open);
                        self.object(obj, &mut body, &mut field_depth);
                    }
                    _ if hide || field_depth.iter().any(|h| *h) => {}
                    wc::para::PAGE_BREAK => {
                        if body.contains("<hp:t>") || body.contains("<hp:ctrl>") {
                            self.omit("문단 가운데 쪽 나누기(문단 앞으로 옮김)");
                        }
                        page_break = true;
                    }
                    wc::para::COLUMN_BREAK => column_break = true,
                    '\t' => {
                        open_text(&mut body, &mut text_open);
                        body.push_str(r##"<hp:tab width="0" leader="0" type="1"/>"##);
                    }
                    '\n' => {
                        open_text(&mut body, &mut text_open);
                        body.push_str("<hp:lineBreak/>");
                    }
                    wc::para::SOFT_HYPHEN => {}
                    wc::para::NB_HYPHEN => {
                        open_text(&mut body, &mut text_open);
                        body.push('-');
                    }
                    c => {
                        open_text(&mut body, &mut text_open);
                        body.push_str(&esc(&c.to_string()));
                    }
                }
            }
        }
        close_text(&mut body, &mut text_open);
        if link_open.is_some() {
            if run_open.is_none() {
                let _ = write!(body, r##"<hp:run charPrIDRef="{cpr_mark}">"##);
                run_open = Some(cpr_mark);
            }
            let _ = write!(
                body,
                r##"<hp:ctrl><hp:fieldEnd beginIDRef="{link_id}"/></hp:ctrl>"##
            );
        }
        if run_open.is_some() {
            body.push_str("</hp:run>");
        } else {
            let _ = write!(
                body,
                r##"<hp:run charPrIDRef="{cpr_mark}"><hp:t></hp:t></hp:run>"##
            );
        }
        format!(
            r##"<hp:p id="0" paraPrIDRef="{ppr}" styleIDRef="{style}" pageBreak="{}" columnBreak="{}" merged="0">{body}</hp:p>"##,
            u8::from(page_break),
            u8::from(column_break)
        )
    }

    /// An inline object at its place in a run.
    fn object(&mut self, obj: &InlineObject, out: &mut String, fields: &mut Vec<bool>) {
        match obj {
            InlineObject::Image {
                media,
                w,
                h,
                alt,
                float,
                crop,
                ..
            } => match self.picture(media, *w, *h) {
                Some(pic) => out.push_str(&self.pic_xml(&pic, alt, float, *crop)),
                None => self.omit("그림(읽지 못함)"),
            },
            InlineObject::Shape {
                kind,
                w,
                h,
                fill,
                stroke,
                stroke_width,
                float,
                story,
                ..
            } => {
                match self.shape_xml(
                    *kind,
                    *w,
                    *h,
                    fill.as_ref(),
                    *stroke,
                    *stroke_width,
                    float,
                    *story,
                ) {
                    Some(x) => out.push_str(&x),
                    None => self.omit("그리기 개체(지원하지 않는 모양)"),
                }
            }
            InlineObject::Graphic { .. } => self.omit("차트"),
            InlineObject::Group {
                w,
                h,
                float,
                ch_w,
                ch_h,
                children,
                ..
            } => {
                self.group(*w, *h, float, *ch_w, *ch_h, children, out, 0);
            }
            InlineObject::Field { instr, result, .. } => {
                if let Some(x) = auto_number(instr) {
                    out.push_str(&x);
                } else {
                    out.push_str(&format!("<hp:t>{}</hp:t>", esc(result)));
                }
            }
            InlineObject::FieldStart { instr, .. } => match auto_number(instr) {
                Some(x) => {
                    out.push_str(&x);
                    fields.push(true);
                }
                None => fields.push(false),
            },
            InlineObject::FieldEnd => {
                fields.pop();
            }
            InlineObject::NoteRef { kind, id, custom } => {
                if self.current_note == Some((*kind, *id)) {
                    // The note's own number: HWP numbers its notes itself.
                    let num_type = if *kind == NoteKind::Footnote {
                        "FOOTNOTE"
                    } else {
                        "ENDNOTE"
                    };
                    let n = self.note_number[usize::from(*kind == NoteKind::Endnote)];
                    let _ = write!(
                        out,
                        r##"<hp:ctrl><hp:autoNum num="{n}" numType="{num_type}"><hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar=")" supscript="0"/></hp:autoNum></hp:ctrl>"##
                    );
                    return;
                }
                if !custom.is_empty() {
                    self.omit("사용자 각주 기호(번호로 씀)");
                }
                out.push_str(&self.note_xml(*kind, *id));
            }
            InlineObject::BookmarkStart { name } => {
                let _ = write!(
                    out,
                    r##"<hp:ctrl><hp:bookmark name="{}"/></hp:ctrl>"##,
                    esc(name)
                );
            }
            InlineObject::BookmarkEnd { .. }
            | InlineObject::CommentStart { .. }
            | InlineObject::CommentEnd { .. }
            | InlineObject::PermStart { .. }
            | InlineObject::PermEnd { .. }
            | InlineObject::ControlStart { .. }
            | InlineObject::ControlEnd => {}
            InlineObject::Equation { linear, .. } => {
                self.omit("수식(글자로 씀)");
                let _ = write!(out, "<hp:t>{}</hp:t>", esc(linear));
            }
            InlineObject::Ruby { base, .. } => {
                self.omit("덧말(본말만 씀)");
                let _ = write!(out, "<hp:t>{}</hp:t>", esc(base));
            }
            InlineObject::PositionalTab { .. } => {
                out.push_str(r##"<hp:t><hp:tab width="0" leader="0" type="1"/></hp:t>"##);
            }
            other => {
                let text = other.plain_text();
                if !text.is_empty() {
                    let _ = write!(out, "<hp:t>{}</hp:t>", esc(text));
                }
            }
        }
    }

    /// A group as its members, each a drawing of its own at its place in the group. An
    /// inline group keeps its room in the line with an empty box the members sit over.
    #[allow(clippy::too_many_arguments)]
    fn group(
        &mut self,
        w: f32,
        h: f32,
        float: &Float,
        ch_w: f32,
        ch_h: f32,
        children: &[wc::para::GroupChild],
        out: &mut String,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        let sx = if ch_w > 0.0 { w / ch_w } else { 1.0 };
        let sy = if ch_h > 0.0 { h / ch_h } else { 1.0 };
        let mut base = float.clone();
        if base.wrap == Wrap::Inline {
            if let Some(x) = self.shape_xml(
                ShapeKind::Rectangle,
                w,
                h,
                None,
                None,
                0.0,
                &Float::default(),
                None,
            ) {
                out.push_str(&x);
            }
            base = Float {
                wrap: Wrap::InFrontOfText,
                h_rel: Anchor::Character,
                v_rel: Anchor::Line,
                ..Default::default()
            };
        }
        for child in children {
            let mut f = base.clone();
            f.x = base.x + child.x * sx;
            f.y = base.y + child.y * sy;
            f.h_align = None;
            f.v_align = None;
            if f.wrap != Wrap::BehindText {
                f.wrap = Wrap::InFrontOfText;
            }
            match &child.obj {
                InlineObject::Image {
                    media,
                    w: cw,
                    h: chh,
                    alt,
                    crop,
                    ..
                } => match self.picture(media, cw * sx, chh * sy) {
                    Some(pic) => out.push_str(&self.pic_xml(&pic, alt, &f, *crop)),
                    None => self.omit("그림(읽지 못함)"),
                },
                InlineObject::Shape {
                    kind,
                    w: cw,
                    h: chh,
                    fill,
                    stroke,
                    stroke_width,
                    story,
                    ..
                } => {
                    match self.shape_xml(
                        *kind,
                        cw * sx,
                        chh * sy,
                        fill.as_ref(),
                        *stroke,
                        *stroke_width,
                        &f,
                        *story,
                    ) {
                        Some(x) => out.push_str(&x),
                        None => self.omit("그리기 개체(지원하지 않는 모양)"),
                    }
                }
                InlineObject::Group {
                    w: gw,
                    h: gh,
                    ch_w: gcw,
                    ch_h: gch,
                    children: inner,
                    ..
                } => {
                    self.group(gw * sx, gh * sy, &f, *gcw, *gch, inner, out, depth + 1);
                }
                _ => self.omit("묶음 안 차트"),
            }
        }
    }

    fn note_xml(&mut self, kind: NoteKind, id: u32) -> String {
        let index = usize::from(kind == NoteKind::Endnote);
        self.note_number[index] += 1;
        let n = self.note_number[index];
        let blocks: Vec<&Arc<Block>> = self
            .doc
            .parts
            .get(&id)
            .map(|p| p.blocks.iter().collect())
            .unwrap_or_default();
        let outer = self.current_note.replace((kind, id));
        let mut paras = self.blocks(&blocks, 1);
        self.current_note = outer;
        if paras.is_empty() {
            paras.push(self.empty_paragraph());
        }
        let tag = if kind == NoteKind::Footnote {
            "footNote"
        } else {
            "endNote"
        };
        let inst = self.uid() % 1_000_000_000;
        format!(
            r##"<hp:ctrl><hp:{tag} number="{n}" suffixChar="41" instId="{inst}"><hp:subList id="" {SUBLIST} vertAlign="TOP">{}</hp:subList></hp:{tag}></hp:ctrl>"##,
            paras.concat()
        )
    }

    // ----- pictures and shapes -----

    fn picture(&mut self, media: &str, w: f32, h: f32) -> Option<Picture> {
        let item = match self.media.get(media) {
            Some(i) => i.clone(),
            None => {
                let bytes = self.doc.media.get(media)?.clone();
                let ext = media
                    .rsplit('.')
                    .next()
                    .filter(|e| e.len() <= 4 && *e != media)
                    .map(str::to_ascii_lowercase)
                    .or_else(|| sniff_ext(&bytes).map(str::to_owned))?;
                let item = format!("image{}", self.bin.len() + 1);
                self.bin.push((item.clone(), ext, bytes));
                self.media.insert(media.to_owned(), item.clone());
                item
            }
        };
        Some(Picture { item, w, h })
    }

    fn pic_xml(&mut self, pic: &Picture, alt: &str, float: &Float, crop: [f32; 4]) -> String {
        let (w, h) = (hu(pic.w).max(1), hu(pic.h).max(1));
        let bytes = self
            .bin
            .iter()
            .find(|(i, _, _)| *i == pic.item)
            .map(|(_, _, b)| b.clone());
        // Pixel size at 96 dpi is 75 HWPUNIT a pixel (imgDim).
        let (dw, dh) = bytes
            .and_then(|b| {
                image::ImageReader::new(std::io::Cursor::new(b.as_slice()))
                    .with_guessed_format()
                    .ok()?
                    .into_dimensions()
                    .ok()
            })
            .map_or((w, h), |(pw, ph)| (i64::from(pw) * 75, i64::from(ph) * 75));
        let clip = |f: f32, full: i64| ((f.clamp(0.0, 1.0) as f64) * full as f64).round() as i64;
        let (cl, ct, cr, cb) = (
            clip(crop[0], dw),
            clip(crop[1], dh),
            dw - clip(crop[2], dw),
            dh - clip(crop[3], dh),
        );
        let id = self.uid();
        let (attrs, pos) = self.placement(float, w, h, "PICTURE");
        format!(
            r##"<hp:pic id="{id}" zOrder="{}" numberingType="PICTURE" {attrs} lock="0" dropcapstyle="None" href="" groupLevel="0" instid="{}" reverse="0"><hp:offset x="0" y="0"/><hp:orgSz width="{w}" height="{h}"/><hp:curSz width="{w}" height="{h}"/><hp:flip horizontal="{}" vertical="{}"/><hp:rotationInfo angle="{}" centerX="{}" centerY="{}" rotateimage="1"/><hp:renderingInfo><hc:transMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/><hc:scaMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/><hc:rotMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/></hp:renderingInfo><hp:imgRect><hc:pt0 x="0" y="0"/><hc:pt1 x="{w}" y="0"/><hc:pt2 x="{w}" y="{h}"/><hc:pt3 x="0" y="{h}"/></hp:imgRect><hp:imgClip left="{cl}" right="{cr}" top="{ct}" bottom="{cb}"/><hp:inMargin left="0" right="0" top="0" bottom="0"/><hp:imgDim dimwidth="{dw}" dimheight="{dh}"/><hc:img binaryItemIDRef="{}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/><hp:effects></hp:effects>{pos}{}</hp:pic>"##,
            float.z,
            id % 1_000_000_000,
            u8::from(float.flip_h),
            u8::from(float.flip_v),
            float.rot.round() as i64,
            w / 2,
            h / 2,
            pic.item,
            shape_comment(alt),
        )
    }

    /// Object attributes (`textWrap`, `textFlow`) and the `sz`/`pos`/`outMargin` tail.
    fn placement(&mut self, float: &Float, w: i64, h: i64, _kind: &str) -> (String, String) {
        let inline = float.wrap == Wrap::Inline;
        let float = &self.in_header_area(float, w, h);
        let wrap = match float.wrap {
            Wrap::Inline | Wrap::TopAndBottom => "TOP_AND_BOTTOM",
            Wrap::Square => "SQUARE",
            Wrap::Tight => "TIGHT",
            Wrap::Through => "THROUGH",
            Wrap::BehindText => "BEHIND_TEXT",
            Wrap::InFrontOfText => "IN_FRONT_OF_TEXT",
        };
        let flow = match float.wrap_text {
            WrapText::BothSides => "BOTH_SIDES",
            WrapText::Left => "LEFT_ONLY",
            WrapText::Right => "RIGHT_ONLY",
            WrapText::Largest => "LARGEST_ONLY",
        };
        let (vrel, valign, voff) = match float.v_rel {
            _ if inline => ("PARA", "TOP", 0),
            Anchor::Line | Anchor::Paragraph => ("PARA", "TOP", hu(float.y)),
            Anchor::Page | Anchor::TopMargin | Anchor::BottomMargin => {
                ("PAPER", "TOP", hu(float.y))
            }
            Anchor::Margin => ("PAGE", "TOP", hu(float.y)),
            _ => ("PARA", "TOP", hu(float.y)),
        };
        let valign = match float.v_align {
            Some(FloatAlign::Center) if !inline => "CENTER",
            Some(FloatAlign::End) if !inline => "BOTTOM",
            Some(FloatAlign::Inside) if !inline => "INSIDE",
            Some(FloatAlign::Outside) if !inline => "OUTSIDE",
            _ => valign,
        };
        let (hrel, hoff) = match float.h_rel {
            _ if inline => ("PARA", 0),
            Anchor::Page | Anchor::LeftMargin | Anchor::RightMargin => ("PAPER", hu(float.x)),
            Anchor::Margin | Anchor::InsideMargin | Anchor::OutsideMargin => ("PAGE", hu(float.x)),
            _ => ("COLUMN", hu(float.x)),
        };
        let halign = match float.h_align {
            Some(FloatAlign::Center) if !inline => "CENTER",
            Some(FloatAlign::End) if !inline => "RIGHT",
            Some(FloatAlign::Inside) if !inline => "INSIDE",
            Some(FloatAlign::Outside) if !inline => "OUTSIDE",
            _ => "LEFT",
        };
        let side = hu(float.dist);
        let attrs = format!(r##"textWrap="{wrap}" textFlow="{flow}""##);
        let pos = format!(
            r##"<hp:sz width="{w}" widthRelTo="ABSOLUTE" height="{h}" heightRelTo="ABSOLUTE" protect="0"/><hp:pos treatAsChar="{}" affectLSpacing="0" flowWithText="1" allowOverlap="{}" holdAnchorAndSO="0" vertRelTo="{vrel}" horzRelTo="{hrel}" vertAlign="{valign}" horzAlign="{halign}" vertOffset="{}" horzOffset="{}"/><hp:outMargin left="{side}" right="{side}" top="{}" bottom="{}"/>"##,
            u8::from(inline),
            u8::from(!inline && matches!(float.wrap, Wrap::BehindText | Wrap::InFrontOfText)),
            voff.max(-1_000_000) as i32 as u32,
            hoff.max(-1_000_000) as i32 as u32,
            hu(float.dist_top),
            hu(float.dist_bottom)
        );
        (attrs, pos)
    }

    /// `float` placed as HWP places a header's or footer's objects (RHWP lays them out in
    /// that area whatever they are relative to; see the DOCX export): a position from the
    /// page or Word's margins becomes one from the area's corner. Elsewhere, unchanged.
    fn in_header_area(&self, float: &Float, w: i64, h: i64) -> Float {
        let mut f = float.clone();
        let Some([left, top, page_w, page_h, margin_top]) = self.hf_frame else {
            return f;
        };
        if float.wrap == Wrap::Inline {
            return f;
        }
        let (w, h) = (w as f32 / 100.0, h as f32 / 100.0);
        if matches!(
            float.h_rel,
            Anchor::Page | Anchor::LeftMargin | Anchor::RightMargin
        ) {
            let x = match float.h_align {
                Some(FloatAlign::Center) => (page_w - w) / 2.0,
                Some(FloatAlign::End | FloatAlign::Outside) => page_w - w,
                Some(FloatAlign::Start | FloatAlign::Inside) => 0.0,
                None => float.x,
            };
            f.x = x - left;
            f.h_align = None;
        }
        let v_from = match float.v_rel {
            Anchor::Page | Anchor::TopMargin | Anchor::BottomMargin => Some(0.0),
            Anchor::Margin => Some(margin_top),
            _ => None,
        };
        if let Some(origin) = v_from {
            let y = match float.v_align {
                Some(FloatAlign::Center) if origin == 0.0 => (page_h - h) / 2.0,
                Some(FloatAlign::End | FloatAlign::Outside) if origin == 0.0 => page_h - h,
                _ => origin + float.y,
            };
            f.y = y - top;
            f.v_align = None;
        }
        f
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_xml(
        &mut self,
        kind: ShapeKind,
        w: f32,
        h: f32,
        fill: Option<&wc::fill::Fill>,
        stroke: Option<Rgb>,
        stroke_width: f32,
        float: &Float,
        story: Option<u32>,
    ) -> Option<String> {
        let tag = match kind {
            ShapeKind::Rectangle | ShapeKind::RoundedRectangle | ShapeKind::TextBox => "rect",
            ShapeKind::Ellipse | ShapeKind::FlowchartOr => "ellipse",
            ShapeKind::Line
            | ShapeKind::StraightConnector
            | ShapeKind::ElbowConnector
            | ShapeKind::CurvedConnector => "line",
            // Shapes HWP has no outline for keep their box (and text).
            _ => {
                self.omit("그리기 개체(사각형으로 씀)");
                "rect"
            }
        };
        let (wi, hi) = (hu(w).max(1), hu(h).max(1));
        let id = self.uid();
        let (attrs, pos) = self.placement(float, wi, hi, "PICTURE");
        let line = match stroke {
            Some(c) => format!(
                r##"<hp:lineShape color="{}" width="{}" style="SOLID" endCap="FLAT" headStyle="NORMAL" tailStyle="NORMAL" headfill="1" tailfill="1" headSz="MEDIUM_MEDIUM" tailSz="MEDIUM_MEDIUM" outlineStyle="NORMAL" alpha="0"/>"##,
                hex(c),
                hu(stroke_width.max(0.1))
            ),
            None => r##"<hp:lineShape color="#000000" width="0" style="NONE" endCap="FLAT" headStyle="NORMAL" tailStyle="NORMAL" headfill="1" tailfill="1" headSz="MEDIUM_MEDIUM" tailSz="MEDIUM_MEDIUM" outlineStyle="NORMAL" alpha="0"/>"##.into(),
        };
        let fill_xml = match fill {
            Some(wc::fill::Fill::Solid {
                color,
                transparency,
            }) => format!(
                r##"<hc:fillBrush><hc:winBrush faceColor="{}" hatchColor="#000000" alpha="{}"/></hc:fillBrush>"##,
                hex(*color),
                (transparency.clamp(0.0, 1.0) * 255.0).round() as u8
            ),
            Some(wc::fill::Fill::Gradient(g)) => {
                let mut x = String::from(
                    r##"<hc:fillBrush><hc:gradation type="LINEAR" angle="0" centerX="50" centerY="50" step="255" stepCenter="50" alpha="0">"##,
                );
                for s in &g.stops {
                    let _ = write!(x, r##"<hc:color value="{}"/>"##, hex(s.color));
                }
                x.push_str("</hc:gradation></hc:fillBrush>");
                x
            }
            None => String::new(),
        };
        let text = match story.and_then(|s| self.doc.parts.get(&s)) {
            Some(part) if tag != "line" => {
                let blocks: Vec<&Arc<Block>> = part.blocks.iter().collect();
                let paras = self.blocks(&blocks, 1).concat();
                let valign = match part.body.anchor {
                    VAlign::Top => "TOP",
                    VAlign::Center => "CENTER",
                    VAlign::Bottom => "BOTTOM",
                };
                let inset = part.body.insets_pt();
                format!(
                    r##"<hp:drawText lastWidth="{wi}" name="" editable="0"><hp:subList id="" {SUBLIST} vertAlign="{valign}">{paras}</hp:subList><hp:textMargin left="{}" right="{}" top="{}" bottom="{}"/></hp:drawText>"##,
                    hu(inset[0]),
                    hu(inset[2]),
                    hu(inset[1]),
                    hu(inset[3])
                )
            }
            _ => String::new(),
        };
        let geometry = match tag {
            "rect" => format!(
                r##"<hc:pt0 x="0" y="0"/><hc:pt1 x="{wi}" y="0"/><hc:pt2 x="{wi}" y="{hi}"/><hc:pt3 x="0" y="{hi}"/>"##
            ),
            "ellipse" => format!(
                r##"<hc:center x="{}" y="{}"/><hc:ax1 x="{wi}" y="{}"/><hc:ax2 x="{}" y="{hi}"/><hc:start1 x="0" y="0"/><hc:end1 x="0" y="0"/><hc:start2 x="0" y="0"/><hc:end2 x="0" y="0"/>"##,
                wi / 2,
                hi / 2,
                hi / 2,
                wi / 2
            ),
            _ => {
                let (y0, y1) = if float.flip_v { (hi, 0) } else { (0, hi) };
                format!(r##"<hc:startPt x="0" y="{y0}"/><hc:endPt x="{wi}" y="{y1}"/>"##)
            }
        };
        let extra = match kind {
            ShapeKind::RoundedRectangle => r##" ratio="20""##,
            ShapeKind::Rectangle | ShapeKind::TextBox => r##" ratio="0""##,
            ShapeKind::Ellipse => r##" intervalDirty="0" hasArcPr="0" arcType="NORMAL""##,
            _ => r##" isReverseHV="0""##,
        };
        Some(format!(
            r##"<hp:{tag} id="{id}" zOrder="{}" numberingType="PICTURE" {attrs} lock="0" dropcapstyle="None" href="" groupLevel="0" instid="{}"{extra}><hp:offset x="0" y="0"/><hp:orgSz width="{wi}" height="{hi}"/><hp:curSz width="{wi}" height="{hi}"/><hp:flip horizontal="{}" vertical="0"/><hp:rotationInfo angle="{}" centerX="{}" centerY="{}" rotateimage="1"/><hp:renderingInfo><hc:transMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/><hc:scaMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/><hc:rotMatrix e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/></hp:renderingInfo>{line}{fill_xml}{text}{geometry}{pos}</hp:{tag}>"##,
            float.z,
            id % 1_000_000_000,
            u8::from(float.flip_h),
            float.rot.round() as i64,
            wi / 2,
            hi / 2,
        ))
    }

    // ----- tables -----

    /// A Word block table as an HWP paragraph holding the table like a character.
    fn table_paragraph(&mut self, t: &wc::Table, depth: usize) -> String {
        let sheet = self.doc.styles.clone();
        let style = t
            .props
            .style
            .as_deref()
            .and_then(|id| sheet.table_style(id));
        let ncols = t
            .grid
            .len()
            .max(
                t.rows
                    .iter()
                    .map(|r| r.cells.iter().map(|c| c.span()).sum::<usize>())
                    .max()
                    .unwrap_or(1),
            )
            .max(1);
        let grid = table_grid(t, ncols, self.avail);
        let margins_def = t
            .props
            .cell_margins
            .or(style.as_ref().and_then(|s| s.parts.cell_margins))
            .unwrap_or(WORD_CELL_MARGINS);
        let mut tborders = style
            .as_ref()
            .and_then(|s| s.parts.borders)
            .unwrap_or_default();
        if let Some(own) = t.props.borders {
            tborders.overlay(&own);
        }
        let nrows = t.rows.len();
        // Grid position of each cell and the rows a vertical merge spans.
        let mut placed: Vec<Vec<(usize, usize)>> = Vec::new();
        for row in &t.rows {
            let mut g = row.first_grid();
            let mut cells = Vec::new();
            for cell in &row.cells {
                cells.push((g, cell.span()));
                g += cell.span();
            }
            placed.push(cells);
        }
        let mut cells_xml = String::new();
        let mut heights = vec![0i64; nrows];
        for (ri, row) in t.rows.iter().enumerate() {
            let mut row_xml = String::new();
            let row_h = row.props.height.map_or(0, hu);
            for (ci, cell) in row.cells.iter().enumerate() {
                if cell.props.vmerge == VMerge::Continue {
                    continue;
                }
                let (g, span) = placed[ri][ci];
                let span = span.min(ncols.saturating_sub(g)).max(1);
                // Rows a vertical merge continues into.
                let mut row_span = 1;
                if cell.props.vmerge == VMerge::Restart {
                    for (rj, cells) in placed.iter().enumerate().skip(ri + 1) {
                        let continues = cells.iter().enumerate().any(|(cj, (gj, _))| {
                            *gj == g
                                && t.rows[rj]
                                    .cells
                                    .get(cj)
                                    .is_some_and(|c| c.props.vmerge == VMerge::Continue)
                        });
                        if !continues {
                            break;
                        }
                        row_span += 1;
                    }
                }
                let width: f32 = grid[g.min(ncols - 1)..(g + span).min(ncols)].iter().sum();
                // Table style regions: whole table, bands, first/last row and column.
                let region = wordcraft_doc::styles::PartSet::of_cell(
                    &t.props.look,
                    (
                        style.as_ref().and_then(|s| s.parts.band_size).unwrap_or(0),
                        style
                            .as_ref()
                            .and_then(|s| s.parts.col_band_size)
                            .unwrap_or(0),
                    ),
                    (ri, nrows),
                    (g, span, ncols),
                );
                let mut fill = None;
                let mut region_borders: Vec<(TablePart, Borders)> = Vec::new();
                let mut text_para = style.as_ref().map(|s| s.para.clone()).unwrap_or_default();
                let mut text_chr = CharProps::default();
                if let Some(st) = &style {
                    for part in region.iter() {
                        let f = st.part(part);
                        fill = f.fill.or(fill);
                        if part != TablePart::WholeTable
                            && let Some(b) = f.borders
                        {
                            region_borders.push((part, b));
                        }
                        text_chr.overlay(&f.chr);
                        if f.align.is_some() {
                            text_para.align = f.align;
                        }
                    }
                }
                let fill = cell.props.shading.or(fill);
                let (first_g, last_g) = (
                    g == row.first_grid(),
                    g + span >= ncols.saturating_sub(row.grid_after()),
                );
                let rb = wordcraft_doc::styles::part_borders(
                    &region_borders,
                    (ri, nrows),
                    (first_g, last_g),
                );
                let cb = cell.props.borders.unwrap_or_default();
                let last_row = ri + row_span >= nrows;
                let edge = |own: Option<Border>,
                            outer: bool,
                            outer_b: Option<Border>,
                            inner_b: Option<Border>| {
                    own.or(if outer { outer_b } else { inner_b })
                };
                let top = edge(cb.top.or(rb.top), ri == 0, tborders.top, tborders.between);
                let bottom = edge(
                    cb.bottom.or(rb.bottom),
                    last_row,
                    tborders.bottom,
                    tborders.between,
                );
                let left = edge(
                    cb.left.or(if first_g { rb.left } else { rb.inside_v }),
                    first_g,
                    tborders.left,
                    tborders.inside_v,
                );
                let right = edge(
                    cb.right.or(if last_g { rb.right } else { rb.inside_v }),
                    last_g,
                    tborders.right,
                    tborders.inside_v,
                );
                let bf = 1 + self
                    .border_fills
                    .id(border_fill_xml(&[left, right, top, bottom], Some(fill)));
                let m = cell.props.resolved_margins(margins_def);
                let valign = match cell.props.valign.unwrap_or(VAlign::Top) {
                    VAlign::Top => "TOP",
                    VAlign::Center => "CENTER",
                    VAlign::Bottom => "BOTTOM",
                };
                // Cell text: the table style's formatting under the paragraph's own.
                let outer_avail = self.avail;
                self.avail = (width - m[1] - m[3]).max(12.0);
                let mut paras = Vec::new();
                for b in &cell.blocks {
                    match &**b {
                        Block::Para(p) => {
                            paras.push(self.paragraph(p, Some((&text_para, &text_chr))))
                        }
                        Block::Table(nt) => {
                            if depth + 1 >= MAX_DEPTH {
                                self.omit("너무 깊이 중첩된 표(글자만 씀)");
                            } else {
                                paras.push(self.table_paragraph(nt, depth + 1));
                            }
                        }
                    }
                }
                self.avail = outer_avail;
                if paras.is_empty() {
                    paras.push(self.empty_paragraph());
                }
                // A cell is at least its margins and a line of its text tall; a paragraph
                // with an exact line height (a thin rule row) counts that height.
                let sheet = self.doc.styles.clone();
                let text_pt: f32 = cell
                    .blocks
                    .iter()
                    .map(|b| match &**b {
                        Block::Para(p) => {
                            let lines = 1.0 + p.text.matches('\n').count() as f32;
                            let resolved = sheet.resolve_para_in(&p.props, Some(&text_para));
                            let line = match resolved.line_spacing {
                                LineSpacing::Exactly(pt) => pt.max(1.0),
                                _ => 12.0 * 1.2,
                            };
                            lines * line
                        }
                        Block::Table(_) => 2.0 * 12.0 * 1.2,
                    })
                    .sum::<f32>()
                    .max(1.0);
                let text_h = hu(text_pt);
                let h = row_h.max(text_h + hu(m[0] + m[2])).max(hu(4.0));
                if row_span == 1 {
                    heights[ri] = heights[ri].max(h);
                }
                let _ = write!(
                    row_xml,
                    r##"<hp:tc name="" header="{}" hasMargin="1" protect="0" editable="0" dirty="0" borderFillIDRef="{bf}"><hp:subList id="" {SUBLIST} vertAlign="{valign}">{}</hp:subList><hp:cellAddr colAddr="{g}" rowAddr="{ri}"/><hp:cellSpan colSpan="{span}" rowSpan="{row_span}"/><hp:cellSz width="{}" height="{h}"/><hp:cellMargin left="{}" right="{}" top="{}" bottom="{}"/></hp:tc>"##,
                    u8::from(row.props.header),
                    paras.concat(),
                    hu(width),
                    hu(m[1]),
                    hu(m[3]),
                    hu(m[0]),
                    hu(m[2]),
                );
            }
            let _ = write!(cells_xml, "<hp:tr>{row_xml}</hp:tr>");
        }
        let width: f32 = grid.iter().sum();
        let height: i64 = heights.iter().map(|h| (*h).max(hu(4.0))).sum();
        let id = self.uid();
        let repeat = t.rows.first().is_some_and(|r| r.props.header);
        let (attrs, pos) = match &t.props.float {
            Some(f) => {
                let float = Float {
                    wrap: Wrap::Square,
                    h_rel: f.h_rel,
                    v_rel: f.v_rel,
                    x: f.x,
                    y: f.y,
                    h_align: f.h_align,
                    v_align: f.v_align,
                    dist: f.dist[0].max(f.dist[2]),
                    dist_top: f.dist[1],
                    dist_bottom: f.dist[3],
                    ..Default::default()
                };
                self.placement(&float, hu(width), height, "TABLE")
            }
            // A Word block table takes its place in the text (자리차지) at its paragraph,
            // so it breaks across pages inside rows as Word's does.
            None => {
                let align = match t.props.align {
                    Some(wc::Align::Center) => "CENTER",
                    Some(wc::Align::Right) => "RIGHT",
                    _ => "LEFT",
                };
                let indent = if align == "LEFT" {
                    hu(t.props.indent.unwrap_or(0.0))
                } else {
                    0
                };
                (
                    r##"textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES""##.to_owned(),
                    format!(
                        r##"<hp:sz width="{}" widthRelTo="ABSOLUTE" height="{height}" heightRelTo="ABSOLUTE" protect="0"/><hp:pos treatAsChar="0" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="COLUMN" vertAlign="TOP" horzAlign="{align}" vertOffset="0" horzOffset="{}"/><hp:outMargin left="0" right="0" top="0" bottom="0"/>"##,
                        hu(width),
                        indent.max(-1_000_000) as i32 as u32
                    ),
                )
            }
        };
        let table = format!(
            r##"<hp:tbl id="{id}" zOrder="0" numberingType="TABLE" {attrs} lock="0" dropcapstyle="None" pageBreak="CELL" repeatHeader="{}" rowCnt="{nrows}" colCnt="{ncols}" cellSpacing="0" borderFillIDRef="1" noAdjust="0">{pos}<hp:inMargin left="{}" right="{}" top="{}" bottom="{}"/>{cells_xml}</hp:tbl>"##,
            u8::from(repeat),
            hu(margins_def[1]),
            hu(margins_def[3]),
            hu(margins_def[0]),
            hu(margins_def[2]),
        );
        // The paragraph holding it: the table's alignment and indent, no line gap.
        let rp = ResolvedPara {
            align: t.props.align.unwrap_or(wc::Align::Left),
            indent_left: t.props.indent.unwrap_or(0.0),
            indent_first: 0.0,
            indent_right: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            before_auto: false,
            after_auto: false,
            line_spacing: LineSpacing::Multiple(1.0 / line_factor("")),
            ..sheet.resolve_para(&ParaProps::default())
        };
        let ppr = self.para_pr(&rp, 10.0, "", None);
        let rc = sheet.resolve_char(None, &CharProps::default());
        let cpr = self.char_pr(&rc);
        format!(
            r##"<hp:p id="0" paraPrIDRef="{ppr}" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0"><hp:run charPrIDRef="{cpr}">{table}<hp:t></hp:t></hp:run></hp:p>"##
        )
    }

    // ----- the package -----

    fn header_xml(&self, sections: usize) -> String {
        let mut x = format!(
            r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><hh:head {NS} version="1.4" secCnt="{sections}"><hh:beginNum page="1" footnote="1" endnote="1" pic="1" tbl="1" equation="1"/><hh:refList>"##
        );
        let _ = write!(x, r##"<hh:fontfaces itemCnt="7">"##);
        for (lang, pool) in LANGS.iter().zip(&self.fonts) {
            let fonts: Vec<&String> = if pool.items.is_empty() {
                vec![]
            } else {
                pool.items.iter().collect()
            };
            let _ = write!(
                x,
                r##"<hh:fontface lang="{lang}" fontCnt="{}">"##,
                fonts.len().max(1)
            );
            if fonts.is_empty() {
                x.push_str(r##"<hh:font id="0" face="함초롬바탕" type="TTF" isEmbedded="0"/>"##);
            }
            for (i, name) in fonts.iter().enumerate() {
                let _ = write!(
                    x,
                    r##"<hh:font id="{i}" face="{}" type="TTF" isEmbedded="0"/>"##,
                    esc(name)
                );
            }
            x.push_str("</hh:fontface>");
        }
        x.push_str("</hh:fontfaces>");
        let _ = write!(
            x,
            r##"<hh:borderFills itemCnt="{}">"##,
            self.border_fills.items.len()
        );
        for (i, bf) in self.border_fills.items.iter().enumerate() {
            let _ = write!(x, r##"<hh:borderFill id="{}" {bf}"##, i + 1);
        }
        x.push_str("</hh:borderFills>");
        let _ = write!(
            x,
            r##"<hh:charProperties itemCnt="{}">"##,
            self.char_prs.items.len()
        );
        for (i, c) in self.char_prs.items.iter().enumerate() {
            let _ = write!(x, r##"<hh:charPr id="{i}" {c}</hh:charPr>"##);
        }
        x.push_str("</hh:charProperties>");
        let _ = write!(
            x,
            r##"<hh:tabProperties itemCnt="{}">"##,
            self.tab_prs.items.len()
        );
        for (i, t) in self.tab_prs.items.iter().enumerate() {
            let _ = write!(x, r##"<hh:tabPr id="{i}" {t}"##);
        }
        x.push_str("</hh:tabProperties>");
        if !self.numberings.items.is_empty() {
            let _ = write!(
                x,
                r##"<hh:numberings itemCnt="{}">"##,
                self.numberings.items.len()
            );
            for (i, n) in self.numberings.items.iter().enumerate() {
                let _ = write!(x, r##"<hh:numbering id="{}" {n}"##, i + 1);
            }
            x.push_str("</hh:numberings>");
        }
        if !self.bullets.items.is_empty() {
            let _ = write!(
                x,
                r##"<hh:bullets itemCnt="{}">"##,
                self.bullets.items.len()
            );
            for (i, b) in self.bullets.items.iter().enumerate() {
                let _ = write!(x, r##"<hh:bullet id="{}" {b}"##, i + 1);
            }
            x.push_str("</hh:bullets>");
        }
        let _ = write!(
            x,
            r##"<hh:paraProperties itemCnt="{}">"##,
            self.para_prs.items.len()
        );
        for (i, p) in self.para_prs.items.iter().enumerate() {
            let _ = write!(x, r##"<hh:paraPr id="{i}" {p}</hh:paraPr>"##);
        }
        x.push_str("</hh:paraProperties>");
        let _ = write!(x, r##"<hh:styles itemCnt="{}">"##, self.styles.len());
        for s in &self.styles {
            x.push_str(s);
        }
        x.push_str("</hh:styles>");
        x.push_str(r##"</hh:refList><hh:compatibleDocument targetProgram="HWP201X"><hh:layoutCompatibility/></hh:compatibleDocument><hh:docOption><hh:linkinfo path="" pageInherit="0" footnoteInherit="0"/></hh:docOption><hh:trackchageConfig flags="56"/></hh:head>"##);
        x
    }

    fn package(&mut self, sections: &[String]) -> Result<Vec<u8>> {
        let header = self.header_xml(sections.len());
        let mut manifest = String::from(
            r##"<opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>"##,
        );
        let mut spine = String::from(r##"<opf:itemref idref="header" linear="yes"/>"##);
        for i in 0..sections.len() {
            let _ = write!(
                manifest,
                r##"<opf:item id="section{i}" href="Contents/section{i}.xml" media-type="application/xml"/>"##
            );
            let _ = write!(spine, r##"<opf:itemref idref="section{i}" linear="yes"/>"##);
        }
        manifest.push_str(
            r##"<opf:item id="settings" href="settings.xml" media-type="application/xml"/>"##,
        );
        for (item, ext, _) in &self.bin {
            let _ = write!(
                manifest,
                r##"<opf:item id="{item}" href="BinData/{item}.{ext}" media-type="{}" isEmbeded="1"/>"##,
                media_type(ext)
            );
        }
        let title = esc(&self.doc.core.title);
        let content = format!(
            r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><opf:package {NS} version="" unique-identifier="" id=""><opf:metadata><opf:title>{title}</opf:title><opf:language>ko</opf:language><opf:meta name="creator" content="text">HwpCraft</opf:meta></opf:metadata><opf:manifest>{manifest}</opf:manifest><opf:spine>{spine}</opf:spine></opf:package>"##
        );
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, bytes: &[u8], store: bool| -> Result<()> {
            zip.start_file(name, if store { stored } else { deflated })
                .context("HWPX 묶음을 만들지 못했습니다.")?;
            zip.write_all(bytes)?;
            Ok(())
        };
        put("mimetype", b"application/hwp+zip", true)?;
        put("version.xml", br##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hv:HCFVersion xmlns:hv="http://www.hancom.co.kr/hwpml/2011/version" tagetApplication="WORDPROCESSOR" major="5" minor="1" micro="0" buildNumber="1" os="1" xmlVersion="1.4" application="Hancom Office Hangul" appVersion="11, 0, 0, 3524 WIN32LEWindows_8"/>"##, false)?;
        put("META-INF/container.xml", br##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><ocf:container xmlns:ocf="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:hpf="http://www.hancom.co.kr/schema/2011/hpf"><ocf:rootfiles><ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwpml-package+xml"/></ocf:rootfiles></ocf:container>"##, false)?;
        put("Contents/content.hpf", content.as_bytes(), false)?;
        put("Contents/header.xml", header.as_bytes(), false)?;
        for (i, s) in sections.iter().enumerate() {
            let xml = format!(
                r##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec {NS}>{s}</hs:sec>"##
            );
            put(&format!("Contents/section{i}.xml"), xml.as_bytes(), false)?;
        }
        put("settings.xml", br##"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><ha:HWPApplicationSetting xmlns:ha="http://www.hancom.co.kr/hwpml/2011/app" xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0"><ha:CaretPosition listIDRef="0" paraIDRef="0" pos="0"/></ha:HWPApplicationSetting>"##, false)?;
        for (item, ext, bytes) in &self.bin {
            put(&format!("BinData/{item}.{ext}"), bytes, true)?;
        }
        Ok(zip
            .finish()
            .context("HWPX 묶음을 닫지 못했습니다.")?
            .into_inner())
    }
}

// ----- helpers -----

/// The number text a level shows for its first item ("1.", "(1)"), for its width.
fn level_sample(text: &str, start: u32) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && chars.peek().is_some_and(|d| d.is_ascii_digit()) {
            chars.next();
            out.push_str(&start.to_string());
        } else {
            out.push(c);
        }
    }
    out
}

/// HWP's number area is the number's width, a width adjustment and the distance to the
/// text; Word puts the text at the hanging indent (a tab) or right after the number. The
/// adjustment (HWPUNIT) makes HWP's area as wide as Word's.
fn number_area(level: &wc::numbering::Level, shown: &str, size: f32) -> i64 {
    // Latin digits and punctuation are about half an em, others an em.
    let glyphs: f32 = shown
        .chars()
        .map(|c| if c.is_ascii() { 0.55 } else { 1.0 })
        .sum::<f32>()
        * size;
    let area = match level.suffix {
        LevelSuffix::Tab if level.hanging > glyphs => level.hanging,
        LevelSuffix::Tab => glyphs + size,
        LevelSuffix::Space => glyphs + size * 0.3,
        LevelSuffix::Nothing => glyphs,
    };
    hu((area - glyphs).max(0.0)).min(i64::from(i16::MAX))
}

/// Word bullets in symbol fonts (private-use characters) as the Unicode characters they show.
fn bullet_char(c: char) -> char {
    match c as u32 {
        0xF0B7 | 0xF09F | 0xF06C => '●',
        0xF0A7 | 0xF06E | 0xF0A8 => '■',
        0xF06F => '○',
        0xF0D8 | 0xF0E8 => '➢',
        0xF0FC => '✓',
        0xF076 => '❖',
        0xF0A2 => '◆',
        0xF0AE => '→',
        0xF02D => '-',
        0xF000..=0xF0FF => '•',
        _ if c.is_control() => '•',
        _ => c,
    }
}

/// Column widths (points). Word's grid holds them for fixed tables; automatic ones (and
/// percentages) are laid out to the preferred width, or the room there is.
fn table_grid(t: &wc::Table, ncols: usize, avail: f32) -> Vec<f32> {
    let mut grid: Vec<f32> = t
        .grid
        .iter()
        .map(|g| if g.is_finite() { g.max(0.0) } else { 0.0 })
        .collect();
    grid.resize(ncols, 0.0);
    let target = t
        .props
        .width_pct
        .map(|p| avail * p / 100.0)
        .or(t.props.width)
        .filter(|w| w.is_finite() && *w > 1.0);
    // Column widths the cells ask for (single-column cells first).
    let mut wanted: Vec<Option<f32>> = vec![None; ncols];
    for row in &t.rows {
        let mut g = row.first_grid();
        for cell in &row.cells {
            let span = cell.span();
            let w = cell
                .props
                .width_pct
                .map(|p| target.unwrap_or(avail) * p / 100.0)
                .or(cell.props.width)
                .filter(|w| w.is_finite() && *w > 1.0);
            if span == 1
                && let (Some(w), Some(slot)) = (w, wanted.get_mut(g))
            {
                *slot = Some(slot.map_or(w, |s: f32| s.max(w)));
            }
            g += span;
        }
    }
    let sum: f32 = grid.iter().sum();
    let tiny = sum < 12.0 * ncols as f32;
    if tiny {
        for (i, g) in grid.iter_mut().enumerate() {
            *g = wanted[i].unwrap_or(0.0);
        }
        let known: f32 = grid.iter().sum();
        let unknown = grid.iter().filter(|g| **g <= 0.0).count();
        let total = target.unwrap_or(avail);
        if unknown > 0 {
            let share = ((total - known) / unknown as f32).max(18.0);
            for g in grid.iter_mut().filter(|g| **g <= 0.0) {
                *g = share;
            }
        }
    }
    let sum: f32 = grid.iter().sum();
    // A preferred width wins over a grid of another size; nothing is wider than the room.
    let goal = match target {
        Some(w) if tiny || (sum - w).abs() > 2.0 && t.props.width_pct.is_some() => Some(w),
        _ if sum > avail * 1.02 && !t.props.fixed => Some(avail),
        _ => None,
    };
    if let Some(goal) = goal
        && sum > 0.0
    {
        for g in &mut grid {
            *g *= goal / sum;
        }
    }
    for g in &mut grid {
        *g = g.max(2.0);
    }
    grid
}

/// How tall Word's single line is for a font, as a share of the font size (the font's
/// Windows ascent and descent, as Word measures it): HWP's line spacing is a percentage
/// of the size itself.
fn line_factor(font: &str) -> f32 {
    if let Some(f) = crate::fonts::line_height(font) {
        return f.clamp(0.8, 2.5);
    }
    // Faces not installed here: what the common ones measure.
    let f = font.to_ascii_lowercase();
    if font.contains("맑은") || f.contains("malgun") {
        1.73
    } else if font.contains("바탕") || font.contains("명조") || f.contains("batang") {
        1.3
    } else if f.contains("calibri") || f.contains("aptos") {
        1.22
    } else {
        1.15
    }
}

fn open_text(body: &mut String, open: &mut bool) {
    if !*open {
        body.push_str("<hp:t>");
        *open = true;
    }
}

fn close_text(body: &mut String, open: &mut bool) {
    if *open {
        body.push_str("</hp:t>");
        *open = false;
    }
}

fn ensure_run(body: &mut String, run: &mut Option<usize>, cpr: usize) {
    if run.is_none() {
        let _ = write!(body, r##"<hp:run charPrIDRef="{cpr}">"##);
        *run = Some(cpr);
    }
}

/// Put section controls at the start of a paragraph's first run.
fn insert_head(p: &mut String, head: &str) {
    if head.is_empty() {
        return;
    }
    if let Some(i) = p.find("<hp:run ")
        && let Some(j) = p[i..].find('>')
    {
        p.insert_str(i + j + 1, head);
    }
}

/// Word's page-number fields as HWP automatic numbers.
fn auto_number(instr: &str) -> Option<String> {
    let name = instr.split_whitespace().next()?.to_ascii_uppercase();
    let kind = match name.as_str() {
        "PAGE" => "PAGE",
        "NUMPAGES" | "SECTIONPAGES" => "TOTAL_PAGE",
        _ => return None,
    };
    Some(format!(
        r##"<hp:ctrl><hp:autoNum num="1" numType="{kind}"><hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar="" supscript="0"/></hp:autoNum></hp:ctrl>"##
    ))
}

fn col_pr(s: &wc::SectionProps) -> String {
    let count = s.columns.count.clamp(1, 255);
    let line = if s.columns.separator {
        r##"<hp:colLine type="SOLID" width="0.12 mm" color="#000000"/>"##
    } else {
        ""
    };
    if s.columns.widths.len() == count as usize && count > 1 {
        let mut x = format!(
            r##"<hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="{count}" sameSz="0" sameGap="0">{line}"##
        );
        for (w, gap) in &s.columns.widths {
            let _ = write!(x, r##"<hp:colSz width="{}" gap="{}"/>"##, hu(*w), hu(*gap));
        }
        x.push_str("</hp:colPr>");
        x
    } else if line.is_empty() {
        format!(
            r##"<hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="{count}" sameSz="1" sameGap="{}"/>"##,
            hu(s.columns.space)
        )
    } else {
        format!(
            r##"<hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="{count}" sameSz="1" sameGap="{}">{line}</hp:colPr>"##,
            hu(s.columns.space)
        )
    }
}

fn shape_comment(alt: &str) -> String {
    if alt.is_empty() {
        String::new()
    } else {
        format!("<hp:shapeComment>{}</hp:shapeComment>", esc(alt))
    }
}

/// A border fill's attributes and children (after `id="n"`), from left, right, top and
/// bottom borders and a fill (`Some(None)` writes an empty brush).
fn border_fill_xml(sides: &[Option<Border>; 4], fill: Option<Option<Rgb>>) -> String {
    let side = |tag: &str, b: Option<Border>| -> String {
        match b.filter(Border::is_visible) {
            Some(b) => {
                let kind = match b.style {
                    BorderStyle::None => "NONE",
                    BorderStyle::Single | BorderStyle::Thick => "SOLID",
                    BorderStyle::Double => "DOUBLE_SLIM",
                    BorderStyle::Dotted => "DOT",
                    BorderStyle::Dashed => "DASH",
                    BorderStyle::Triple => "SLIM_THICK_SLIM",
                    BorderStyle::DotDash => "DASH_DOT",
                    BorderStyle::Wave => "WAVE",
                };
                let width = BORDER_MM[border_index(b.width)];
                format!(
                    r##"<hh:{tag} type="{kind}" width="{width} mm" color="{}"/>"##,
                    b.color.map_or("#000000".into(), hex)
                )
            }
            None => format!(r##"<hh:{tag} type="NONE" width="0.1 mm" color="#000000"/>"##),
        }
    };
    let mut x = String::from(
        r##"threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0"><hh:slash type="NONE" Crooked="0" isCounter="0"/><hh:backSlash type="NONE" Crooked="0" isCounter="0"/>"##,
    );
    x.push_str(&side("leftBorder", sides[0]));
    x.push_str(&side("rightBorder", sides[1]));
    x.push_str(&side("topBorder", sides[2]));
    x.push_str(&side("bottomBorder", sides[3]));
    x.push_str(r##"<hh:diagonal type="SOLID" width="0.1 mm" color="#000000"/>"##);
    match fill {
        Some(Some(c)) => {
            let _ = write!(x, r##"<hc:fillBrush><hc:winBrush faceColor="{}" hatchColor="#999999" alpha="0"/></hc:fillBrush>"##, hex(c));
        }
        Some(None) => x.push_str(r##"<hc:fillBrush><hc:winBrush faceColor="none" hatchColor="#999999" alpha="0"/></hc:fillBrush>"##),
        None => {}
    }
    x.push_str("</hh:borderFill>");
    x
}

/// Word list format as an HWP number format.
fn hwp_num_format(f: NumFormat) -> &'static str {
    match f {
        NumFormat::UpperRoman => "ROMAN_CAPITAL",
        NumFormat::LowerRoman => "ROMAN_SMALL",
        NumFormat::UpperLetter => "LATIN_CAPITAL",
        NumFormat::LowerLetter => "LATIN_SMALL",
        NumFormat::DecimalEnclosedCircle | NumFormat::DecimalEnclosedCircleChinese => {
            "CIRCLED_DIGIT"
        }
        NumFormat::Ganada => "HANGUL_SYLLABLE",
        NumFormat::Chosung => "HANGUL_JAMO",
        NumFormat::CardinalText | NumFormat::OrdinalText => "HANGUL_PHONETIC",
        NumFormat::IdeographTraditional
        | NumFormat::IdeographDigital
        | NumFormat::ChineseCounting
        | NumFormat::TaiwaneseCounting => "IDEOGRAPH",
        NumFormat::IdeographEnclosedCircle => "CIRCLED_IDEOGRAPH",
        _ => "DIGIT",
    }
}

/// Word level text ("%1.", "(%2)") as HWP's ("^1.", "(^2)").
fn hwp_level_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%'
            && let Some(d) = chars.peek().copied().filter(|d| ('1'..='9').contains(d))
        {
            chars.next();
            out.push('^');
            out.push(if d > '7' { '7' } else { d });
        } else {
            out.push(c);
        }
    }
    out
}

fn sniff_ext(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, ..] => Some("jpg"),
        [b'G', b'I', b'F', ..] => Some("gif"),
        [b'B', b'M', ..] => Some("bmp"),
        [0x01, 0x00, 0x00, 0x00, ..] => Some("emf"),
        [0xD7, 0xCD, 0xC6, 0x9A, ..] => Some("wmf"),
        [b'I', b'I', 42, 0, ..] | [b'M', b'M', 0, 42, ..] => Some("tif"),
        _ => None,
    }
}

fn media_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_text_maps_placeholders() {
        assert_eq!(hwp_level_text("%1."), "^1.");
        assert_eq!(hwp_level_text("(%2)"), "(^2)");
        assert_eq!(crate::export::level_text("^3)"), "%3)");
    }
}
