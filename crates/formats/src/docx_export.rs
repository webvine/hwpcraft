//! Owner: format area. DOCX (Office Open XML WordprocessingML) export.
//!
//! Writes the RHWP document model straight to a `.docx` package: paragraphs with
//! character and paragraph shapes, tables (merged cells, borders, fills), pictures,
//! headers/footers, page setup per section and footnotes. Anything DOCX cannot carry
//! from this model (drawing objects, equations, charts, form objects, numbering text,
//! …) is listed in [`DocxExport::omitted`] instead of being dropped silently.
use anyhow::{Context, Result};
use hwpcraft_doc::DocumentCore;
use hwpcraft_doc::model::bin_data::BinDataContent;
use hwpcraft_doc::model::control::{AutoNumberType, Control, FieldType, PageNumberPos};
use hwpcraft_doc::model::document::Document;
use hwpcraft_doc::model::header_footer::HeaderFooterApply;
use hwpcraft_doc::model::image::Picture;
use hwpcraft_doc::model::page::ColumnDef;
use hwpcraft_doc::model::paragraph::{NumberingMarker, Paragraph};
use hwpcraft_doc::model::shape::{Caption, CaptionDirection, TextWrap};
use hwpcraft_doc::model::style::{
    Alignment, BorderLine, BorderLineType, CharShape, HeadType, LineSpacingType, ParaShape,
    UnderlineType,
};
use hwpcraft_doc::model::table::{Table, VerticalAlign};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;

/// A finished package and what it could not carry, as Korean "what · count" lines.
pub struct DocxExport {
    pub bytes: Vec<u8>,
    pub omitted: Vec<String>,
}

/// Nested tables deeper than this are written as text only.
const MAX_DEPTH: usize = 32;
/// Pictures larger than this are left out (they would bloat the package).
const MAX_PICTURE_BYTES: usize = 64 << 20;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// HWP border widths (index 0–15) in millimetres.
const BORDER_MM: [f64; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];

/// HWPUNIT (1/7200 inch) to twips (1/1440 inch).
fn twips(hu: i64) -> i64 {
    hu / 5
}
/// Paragraph-shape lengths are stored at twice HWPUNIT.
fn para_twips(value: i32) -> i64 {
    i64::from(value) / 10
}
/// HWPUNIT to EMU (1/914400 inch).
fn emu(hu: i64) -> i64 {
    hu * 127
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 forbids most C0 controls; tabs and breaks are written as elements.
            c if (c as u32) < 0x20 => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
}
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape(text, &mut out);
    out
}

/// HWP COLORREF (0x00BBGGRR) as RRGGBB.
fn rgb(color: u32) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        color & 0xFF,
        (color >> 8) & 0xFF,
        (color >> 16) & 0xFF
    )
}
fn is_white_or_none(color: u32) -> bool {
    color & 0xFF00_0000 != 0 || color & 0x00FF_FFFF == 0x00FF_FFFF
}

/// (section, paragraph, control) of a body-level drawing object → (page, [x, y, w, h] px).
type ShapeBoxes = BTreeMap<(usize, usize, usize), (u32, [f64; 4])>;

/// Main document part name; footnotes, headers and footers are parts of their own.
const DOCUMENT_PART: &str = "document.xml";

struct Writer<'a> {
    core: &'a DocumentCore,
    doc: &'a Document,
    renderer: Option<hwpcraft_render::Renderer>,
    /// Equation pictures already written, by script, size and colour: media target.
    equations: BTreeMap<(String, u32, u32), String>,
    /// Package parts other than the main document, by part name.
    parts: Vec<(String, Vec<u8>)>,
    /// Relationships of each part (word/_rels/<part>.rels): (id, type suffix, target).
    rels: BTreeMap<String, Vec<(String, &'static str, String)>>,
    /// The part being written: relationships are recorded for it.
    current: String,
    /// Pictures already written, by BinData id: media target ("media/image1.png").
    media: BTreeMap<u16, String>,
    next_footnote: usize,
    /// (section, paragraph) of the body paragraph being written.
    address: Option<(usize, usize)>,
    /// Page and box (px) of each body-level drawing object, filled on first use.
    shape_boxes: Option<ShapeBoxes>,
    /// Rendered pages (2x) for cutting drawing objects out of.
    page_rasters: BTreeMap<u32, image::RgbaImage>,
    /// Caption paragraphs of pictures in the paragraph being written: (before it, after it).
    picture_captions: (String, String),
    extensions: BTreeMap<String, &'static str>,
    footnotes: Vec<String>,
    endnotes: Vec<String>,
    omitted: BTreeMap<&'static str, usize>,
    drawing_id: u32,
    /// DOCX names of HWP bookmarks (Word allows letters, digits and `_`, 40 at most).
    bookmarks: BTreeMap<String, String>,
    bookmark_id: u32,
}

impl<'a> Writer<'a> {
    fn new(core: &'a DocumentCore) -> Self {
        Self {
            core,
            doc: core.document(),
            renderer: None,
            equations: BTreeMap::new(),
            parts: Vec::new(),
            rels: BTreeMap::new(),
            current: DOCUMENT_PART.to_owned(),
            media: BTreeMap::new(),
            next_footnote: 0,
            address: None,
            shape_boxes: None,
            page_rasters: BTreeMap::new(),
            picture_captions: (String::new(), String::new()),
            extensions: BTreeMap::new(),
            footnotes: Vec::new(),
            endnotes: Vec::new(),
            omitted: BTreeMap::new(),
            drawing_id: 0,
            bookmarks: BTreeMap::new(),
            bookmark_id: 0,
        }
    }
    fn omit(&mut self, what: &'static str) {
        *self.omitted.entry(what).or_default() += 1;
    }
    /// Relationship of the current part to `target`, reused when it already has one.
    /// The DOCX bookmark name for an HWP bookmark, the same for its links.
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

    fn add_rel(&mut self, kind: &'static str, target: String) -> String {
        let rels = self.rels.entry(self.current.clone()).or_default();
        if let Some((id, _, _)) = rels.iter().find(|(_, k, t)| *k == kind && *t == target) {
            return id.clone();
        }
        let id = format!("rId{}", rels.len() + 10);
        rels.push((id.clone(), kind, target));
        id
    }
    /// Write `f` as part `part`: relationships made meanwhile belong to that part.
    fn in_part<T>(&mut self, part: &str, f: impl FnOnce(&mut Self) -> T) -> T {
        let previous = std::mem::replace(&mut self.current, part.to_owned());
        let result = f(self);
        self.current = previous;
        result
    }

    fn char_shape(&self, id: u32) -> Option<&'a CharShape> {
        self.doc.doc_info.char_shapes.get(id as usize)
    }
    fn para_shape(&self, id: u16) -> Option<&'a ParaShape> {
        self.doc.doc_info.para_shapes.get(id as usize)
    }
    fn font_name(&self, lang: usize, shape: &CharShape) -> Option<&'a str> {
        let id = *shape.font_ids.get(lang)? as usize;
        self.doc
            .doc_info
            .font_faces
            .get(lang)?
            .get(id)
            .map(|f| f.name.as_str())
            .filter(|n| !n.is_empty())
    }

    fn run_properties(&self, shape_id: u32, out: &mut String) {
        let Some(cs) = self.char_shape(shape_id) else {
            return;
        };
        out.push_str("<w:rPr>");
        let hangul = self.font_name(0, cs);
        let latin = self.font_name(1, cs).or(hangul);
        if hangul.is_some() || latin.is_some() {
            out.push_str("<w:rFonts");
            if let Some(name) = latin {
                let _ = write!(out, " w:ascii=\"{0}\" w:hAnsi=\"{0}\"", esc(name));
            }
            if let Some(name) = hangul.or(latin) {
                let _ = write!(out, " w:eastAsia=\"{}\"", esc(name));
            }
            out.push_str(" w:hint=\"eastAsia\"/>");
        }
        if cs.bold {
            out.push_str("<w:b/><w:bCs/>");
        }
        if cs.italic {
            out.push_str("<w:i/><w:iCs/>");
        }
        if cs.strikethrough {
            out.push_str("<w:strike/>");
        }
        if cs.emboss {
            out.push_str("<w:emboss/>");
        }
        if cs.engrave {
            out.push_str("<w:imprint/>");
        }
        if cs.outline_type != 0 {
            out.push_str("<w:outline/>");
        }
        if cs.shadow_type != 0 {
            out.push_str("<w:shadow/>");
        }
        if cs.text_color & 0x00FF_FFFF != 0 && cs.text_color & 0xFF00_0000 == 0 {
            let _ = write!(out, "<w:color w:val=\"{}\"/>", rgb(cs.text_color));
        }
        // Letter spacing is a percentage of the font size; ratio is the glyph width.
        let size_pt = f64::from(cs.base_size) / 100.0;
        let spacing = i64::from(cs.spacings[0]);
        if spacing != 0 {
            let tw = (size_pt * 20.0 * spacing as f64 / 100.0).round() as i64;
            let _ = write!(out, "<w:spacing w:val=\"{tw}\"/>");
        }
        let ratio = cs.ratios[0];
        if ratio != 0 && ratio != 100 {
            let _ = write!(out, "<w:w w:val=\"{}\"/>", ratio.clamp(1, 200));
        }
        if cs.base_size > 0 {
            let half_points = (cs.base_size + 25) / 50;
            let _ = write!(
                out,
                "<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/>"
            );
        }
        if !is_white_or_none(cs.shade_color) {
            let _ = write!(
                out,
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>",
                rgb(cs.shade_color)
            );
        }
        if cs.underline_type == UnderlineType::Bottom {
            out.push_str("<w:u w:val=\"single\"/>");
        }
        if cs.superscript {
            out.push_str("<w:vertAlign w:val=\"superscript\"/>");
        } else if cs.subscript {
            out.push_str("<w:vertAlign w:val=\"subscript\"/>");
        }
        out.push_str("</w:rPr>");
    }

    fn paragraph_properties(&mut self, para: &Paragraph, sect: Option<&str>, out: &mut String) {
        out.push_str("<w:pPr>");
        if let Some(ps) = self.para_shape(para.para_shape_id) {
            if matches!(ps.head_type, HeadType::Number | HeadType::Outline)
                && matches!(para.numbering_marker, NumberingMarker::Unresolved)
            {
                self.omit("문단 번호(번호 글자 빠짐)");
            }
            let before = para_twips(ps.spacing_before).max(0);
            let after = para_twips(ps.spacing_after).max(0);
            // HWP's percentage is of the font size, Word's "auto" of the font's own line
            // height (about 1.3 times larger for Korean fonts): write the HWP line height as
            // a minimum, which also lets inline pictures grow the line.
            let size = para
                .char_shape_id_at(0)
                .and_then(|id| self.char_shape(id))
                .map_or(1000, |cs| i64::from(cs.base_size.max(100)));
            let (line, rule) = match ps.line_spacing_type {
                LineSpacingType::Percent => (
                    size * 20 / 100 * i64::from(ps.line_spacing) / 100,
                    "atLeast",
                ),
                LineSpacingType::Fixed => (para_twips(ps.line_spacing), "exact"),
                LineSpacingType::Minimum => (para_twips(ps.line_spacing), "atLeast"),
                // Space between lines: Word has no such rule; written as single spacing.
                LineSpacingType::SpaceOnly => {
                    self.omit("줄 간격 '글자에 따라'(한 줄로 씀)");
                    (240, "auto")
                }
            };
            let _ = write!(
                out,
                "<w:spacing w:before=\"{before}\" w:after=\"{after}\" w:line=\"{}\" w:lineRule=\"{rule}\"/>",
                line.max(1)
            );
            let left = para_twips(ps.margin_left);
            let right = para_twips(ps.margin_right);
            let indent = para_twips(ps.indent);
            // HWP: a positive indent moves the first line in; a negative one (내어쓰기)
            // keeps the first line at the margin and moves the others in.
            if indent < 0 {
                let _ = write!(
                    out,
                    "<w:ind w:left=\"{}\" w:right=\"{right}\" w:hanging=\"{}\"/>",
                    left - indent,
                    -indent
                );
            } else {
                let _ = write!(
                    out,
                    "<w:ind w:left=\"{left}\" w:right=\"{right}\" w:firstLine=\"{indent}\"/>"
                );
            }
            let jc = match ps.alignment {
                Alignment::Justify => "both",
                Alignment::Left => "left",
                Alignment::Right => "right",
                Alignment::Center => "center",
                Alignment::Distribute | Alignment::Split => "distribute",
            };
            let _ = write!(out, "<w:jc w:val=\"{jc}\"/>");
            self.tab_stops(ps.tab_def_id, out);
        }
        if let Some(sect) = sect {
            out.push_str(sect);
        }
        out.push_str("</w:pPr>");
    }

    /// Tab stops of a paragraph shape. Positions are stored at twice HWPUNIT from the
    /// column start, as Word measures them.
    fn tab_stops(&self, tab_def_id: u16, out: &mut String) {
        let Some(def) = self.doc.doc_info.tab_defs.get(tab_def_id as usize) else {
            return;
        };
        if def.tabs.is_empty() {
            return;
        }
        out.push_str("<w:tabs>");
        for tab in &def.tabs {
            let val = match tab.tab_type {
                1 => "right",
                2 => "center",
                3 => "decimal",
                _ => "left",
            };
            let leader = match tab.fill_type {
                0 => "none",
                1 => "underscore",
                2 => "hyphen",
                _ => "dot",
            };
            let _ = write!(
                out,
                "<w:tab w:val=\"{val}\" w:leader=\"{leader}\" w:pos=\"{}\"/>",
                i64::from(tab.position) / 10
            );
        }
        out.push_str("</w:tabs>");
    }

    /// Bullet character or number text in front of a list paragraph, then the gap to the
    /// text: a tab when the paragraph hangs (Word's hanging indent is a tab stop).
    fn list_prefix(&self, para: &Paragraph) -> Option<String> {
        let ps = self.para_shape(para.para_shape_id)?;
        let marker = match ps.head_type {
            HeadType::Bullet if ps.numbering_id > 0 => {
                let bullet = self
                    .doc
                    .doc_info
                    .bullets
                    .get(ps.numbering_id as usize - 1)?;
                let c = bullet.bullet_char;
                if c.is_control() || c == '\u{FFFF}' {
                    return None;
                }
                c.to_string()
            }
            HeadType::Number | HeadType::Outline => match &para.numbering_marker {
                NumberingMarker::Text(text, _) if !text.is_empty() => text.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let gap = if ps.indent < 0 { '\t' } else { ' ' };
        Some(format!("{marker}{gap}"))
    }

    /// One HWP paragraph as DOCX blocks: tables split it, since DOCX tables are blocks.
    fn paragraph(&mut self, para: &Paragraph, sect: Option<&str>, depth: usize, out: &mut String) {
        let chars: Vec<char> = para.text.chars().collect();
        let positions = para.control_text_positions();
        let mut by_position: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (index, position) in positions.iter().enumerate() {
            by_position.entry(*position).or_default().push(index);
        }
        let mut body = String::new();
        let mut open_props = String::new();
        self.paragraph_properties(para, None, &mut open_props);
        // The last block carries the section properties when this paragraph ends a section.
        let last_props = match sect {
            Some(sect) => open_props.replacen("</w:pPr>", &format!("{sect}</w:pPr>"), 1),
            None => open_props.clone(),
        };
        if let Some(prefix) = self.list_prefix(para) {
            self.text_run(&prefix, para.char_shape_id_at(0).unwrap_or(0), &mut body);
        }
        let outer_captions = std::mem::take(&mut self.picture_captions);
        // Hyperlinks to web, mail or file addresses, or to a bookmark in the document
        // ("#" + its DOCX name): (start, end, target) in text characters.
        let links: Vec<(usize, usize, String)> = para
            .field_ranges
            .iter()
            .filter_map(|range| match para.controls.get(range.control_idx) {
                Some(Control::Field(field)) if field.field_type == FieldType::Hyperlink => {
                    let uri = hwpcraft_doc::model::hyperlink::command_uri(&field.command);
                    let external = ["http://", "https://", "mailto:", "ftp://", "file:"]
                        .iter()
                        .any(|scheme| uri.to_ascii_lowercase().starts_with(scheme));
                    let target = if external {
                        Some(uri)
                    } else {
                        uri.strip_prefix('?')
                            .filter(|name| !name.is_empty() && !name.starts_with('#'))
                            .map(|name| format!("#{}", self.bookmark_name(name)))
                    };
                    if target.is_none() {
                        self.omit("문서 안 하이퍼링크(글자만 씀)");
                    }
                    target
                        .filter(|_| range.end_char_idx > range.start_char_idx)
                        .map(|target| (range.start_char_idx, range.end_char_idx, target))
                }
                _ => None,
            })
            .collect();
        let mut link_open = false;
        let mut run = String::new();
        let mut run_shape: Option<u32> = None;
        let mut blocks: Vec<String> = Vec::new();
        for i in 0..=chars.len() {
            let ends = link_open && links.iter().any(|(_, end, _)| *end == i);
            let starts = links.iter().find(|(start, _, _)| *start == i);
            if ends || starts.is_some() {
                if let Some(shape) = run_shape.take() {
                    self.text_run(&run, shape, &mut body);
                    run.clear();
                }
                if link_open {
                    body.push_str("</w:hyperlink>");
                    link_open = false;
                }
                if let Some((_, _, uri)) = starts {
                    if let Some(anchor) = uri.strip_prefix('#') {
                        let _ = write!(
                            body,
                            "<w:hyperlink w:anchor=\"{}\" w:history=\"1\">",
                            esc(anchor)
                        );
                    } else {
                        let id = self.add_rel("/hyperlink", uri.clone());
                        let _ = write!(body, "<w:hyperlink r:id=\"{id}\" w:history=\"1\">");
                    }
                    link_open = true;
                }
            }
            if let Some(controls) = by_position.get(&i) {
                for &ci in controls {
                    let Some(control) = para.controls.get(ci) else {
                        continue;
                    };
                    if let Some(shape) = run_shape.take() {
                        self.text_run(&run, shape, &mut body);
                        run.clear();
                    }
                    if let Control::Table(table) = control {
                        // Close the paragraph so far, write the table, continue after it.
                        if link_open {
                            body.push_str("</w:hyperlink>");
                            link_open = false;
                        }
                        blocks.push(format!("<w:p>{open_props}{body}</w:p>"));
                        body.clear();
                        let (before, after) = self.caption(table.caption.as_ref(), depth);
                        let mut table_xml = before;
                        self.table(table, depth, &mut table_xml);
                        table_xml.push_str(&after);
                        blocks.push(table_xml);
                        continue;
                    }
                    self.inline_control(control, ci, depth, &mut body);
                }
            }
            let Some(&c) = chars.get(i) else {
                break;
            };
            let shape = para.char_shape_id_at(i).unwrap_or(0);
            if run_shape != Some(shape) {
                if let Some(previous) = run_shape {
                    self.text_run(&run, previous, &mut body);
                    run.clear();
                }
                run_shape = Some(shape);
            }
            run.push(c);
        }
        if let Some(shape) = run_shape {
            self.text_run(&run, shape, &mut body);
        }
        if link_open {
            body.push_str("</w:hyperlink>");
        }
        blocks.push(format!("<w:p>{last_props}{body}</w:p>"));
        let (before, after) = std::mem::replace(&mut self.picture_captions, outer_captions);
        out.push_str(&before);
        for block in blocks {
            out.push_str(&block);
        }
        out.push_str(&after);
    }

    /// Text with tabs and line breaks as their own run elements.
    fn text_run(&self, text: &str, shape: u32, out: &mut String) {
        if text.is_empty() {
            return;
        }
        out.push_str("<w:r>");
        self.run_properties(shape, out);
        let mut pending = String::new();
        let flush = |pending: &mut String, out: &mut String| {
            if !pending.is_empty() {
                out.push_str("<w:t xml:space=\"preserve\">");
                escape(pending, out);
                out.push_str("</w:t>");
                pending.clear();
            }
        };
        for c in text.chars() {
            match c {
                '\t' => {
                    flush(&mut pending, out);
                    out.push_str("<w:tab/>");
                }
                '\n' => {
                    flush(&mut pending, out);
                    out.push_str("<w:br/>");
                }
                c => pending.push(c),
            }
        }
        flush(&mut pending, out);
        out.push_str("</w:r>");
    }

    fn inline_control(&mut self, control: &Control, index: usize, depth: usize, out: &mut String) {
        match control {
            Control::Bookmark(bookmark) => {
                let name = self.bookmark_name(&bookmark.name);
                self.bookmark_id += 1;
                let id = self.bookmark_id;
                let _ = write!(
                    out,
                    "<w:bookmarkStart w:id=\"{id}\" w:name=\"{}\"/><w:bookmarkEnd w:id=\"{id}\"/>",
                    esc(&name)
                );
            }
            Control::Picture(picture) => {
                self.picture(picture, out);
                let (before, after) = self.caption(picture.caption.as_ref(), depth);
                self.picture_captions.0.push_str(&before);
                self.picture_captions.1.push_str(&after);
            }
            Control::Footnote(note) => self.note(false, &note.paragraphs, depth, out),
            Control::Endnote(note) => self.note(true, &note.paragraphs, depth, out),
            Control::Ruby(ruby) => {
                self.omit("덧말(본말만 씀)");
                let shape = 0;
                self.text_run(&ruby.main_text, shape, out);
            }
            Control::Shape(shape) => {
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
                    // Cut out of the page, body text over it would be pictured twice.
                    self.omit("글 뒤·글 앞 도형");
                    return;
                }
                let at = (depth == 0 && self.current == DOCUMENT_PART)
                    .then_some(self.address)
                    .flatten();
                match at {
                    Some((section, paragraph)) => {
                        let size = (i64::from(common.width), i64::from(common.height));
                        self.shape_picture((section, paragraph, index), size, out);
                    }
                    None => self.omit("표·머리말·주석 안 도형"),
                }
            }
            Control::Equation(equation) => self.equation(equation, out),
            Control::Form(_) => self.omit("양식 개체"),
            Control::CharOverlap(_) => self.omit("글자 겹치기"),
            Control::AutoNumber(number) => match number.number_type {
                AutoNumberType::Page => out.push_str(&field("PAGE", "1")),
                AutoNumberType::TotalPage => out.push_str(&field("NUMPAGES", "1")),
                // The note's own mark is Word's footnote reference.
                AutoNumberType::Footnote | AutoNumberType::Endnote => {}
                _ => {
                    let n = if number.assigned_number > 0 {
                        number.assigned_number
                    } else {
                        number.number
                    };
                    self.text_run(&n.to_string(), 0, out);
                }
            },
            // Written into the header or footer by `section_properties`.
            Control::PageNumberPos(_) => {}
            Control::HiddenComment(_) => self.omit("숨은 설명"),
            // Structure and markers that carry no visible content here.
            _ => {}
        }
    }

    /// An equation as a PNG picture of its rendering, at its size in the document.
    fn equation(&mut self, equation: &hwpcraft_doc::model::control::Equation, out: &mut String) {
        let width = i64::from(equation.common.width);
        let height = i64::from(equation.common.height);
        let key = (equation.script.clone(), equation.font_size, equation.color);
        let target = match self.equations.get(&key) {
            Some(target) => target.clone(),
            None => {
                let Some(png) = self.equation_png(equation) else {
                    self.omit("수식(그리지 못함)");
                    return;
                };
                let target = format!("media/equation{}.png", self.equations.len() + 1);
                self.parts.push((format!("word/{target}"), png));
                self.extensions.insert("png".to_owned(), "image/png");
                self.equations.insert(key, target.clone());
                target
            }
        };
        let rel_id = self.add_rel("/image", target);
        if width <= 0 || height <= 0 {
            self.omit("수식(크기 없음)");
            return;
        }
        self.inline_picture(&rel_id, "수식", emu(width), emu(height), out);
    }

    fn equation_png(
        &mut self,
        equation: &hwpcraft_doc::model::control::Equation,
    ) -> Option<Vec<u8>> {
        let script = equation.script.trim();
        if script.is_empty() || script.chars().count() > 4000 {
            return None;
        }
        let size = if equation.font_size > 0 {
            equation.font_size
        } else {
            1000
        };
        let svg = self
            .core
            .render_equation_preview_native(script, size, equation.color)
            .ok()?;
        let renderer = self.renderer.get_or_insert_with(Default::default);
        // Three times the screen size: sharp in print without large files.
        let page = renderer.svg(&svg, 3.0).ok()?;
        if page.width == 0 || page.height == 0 {
            return None;
        }
        let image = image::RgbaImage::from_raw(page.width, page.height, page.rgba)?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        Some(png)
    }

    /// A body-level drawing object (text box, shape, chart) as a picture cut out of its
    /// rendered page, at its size in the document.
    fn shape_picture(&mut self, key: (usize, usize, usize), size: (i64, i64), out: &mut String) {
        let Some((page, [x, y, w, h])) = self.shape_boxes().get(&key).copied() else {
            self.omit("그리기 개체(위치를 찾지 못함)");
            return;
        };
        let Some(png) = self.page_cutout(page, x, y, w, h) else {
            self.omit("그리기 개체(그리지 못함)");
            return;
        };
        self.omit("그리기 개체·글상자·차트(그림으로 씀)");
        let target = format!("media/shape{}.png", self.drawing_id + 1);
        self.parts.push((format!("word/{target}"), png));
        self.extensions.insert("png".to_owned(), "image/png");
        let rel_id = self.add_rel("/image", target);
        // px at 96 dpi is 9525 EMU.
        let width = if size.0 > 0 {
            emu(size.0)
        } else {
            (w * 9525.0) as i64
        };
        let height = if size.1 > 0 {
            emu(size.1)
        } else {
            (h * 9525.0) as i64
        };
        self.inline_picture(
            &rel_id,
            "도형",
            width.max(emu(100)),
            height.max(emu(100)),
            out,
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
                    // Body level only: cell, header/footer and note objects carry more keys.
                    if !matches!(c["type"].as_str(), Some("shape" | "ole"))
                        || c.get("cellIdx").is_some()
                        || c.get("hf").is_some()
                    {
                        continue;
                    }
                    let index = |k: &str| c[k].as_u64().map(|v| v as usize);
                    let (Some(s), Some(p), Some(i)) =
                        (index("secIdx"), index("paraIdx"), index("controlIdx"))
                    else {
                        continue;
                    };
                    let num = |k: &str| c[k].as_f64().unwrap_or(0.0);
                    boxes
                        .entry((s, p, i))
                        .or_insert((page, [num("x"), num("y"), num("w"), num("h")]));
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
        let cut =
            image::imageops::crop_imm(image, left, top, right - left, bottom - top).to_image();
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(cut)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        Some(png)
    }

    /// Caption paragraphs of a table or picture: (to write before it, after it).
    fn caption(&mut self, caption: Option<&Caption>, depth: usize) -> (String, String) {
        let Some(caption) = caption else {
            return (String::new(), String::new());
        };
        let mut xml = String::new();
        for p in &caption.paragraphs {
            self.paragraph(p, None, depth + 1, &mut xml);
        }
        if matches!(
            caption.direction,
            CaptionDirection::Top | CaptionDirection::Left
        ) {
            (xml, String::new())
        } else {
            (String::new(), xml)
        }
    }

    fn bin_data(&self, id: u16) -> Option<&'a BinDataContent> {
        let list = &self.doc.bin_data_content;
        if id == 0 {
            return None;
        }
        list.get(id as usize - 1)
            .or_else(|| list.iter().find(|b| b.id == id))
    }

    fn picture(&mut self, picture: &Picture, out: &mut String) {
        let id = picture.image_attr.bin_data_id;
        let Some(bin) = self.bin_data(id) else {
            self.omit("그림(데이터 없음)");
            return;
        };
        let ext = bin.extension.to_ascii_lowercase();
        let ext = if ext == "jpeg" { "jpg".to_owned() } else { ext };
        let content_type = match ext.as_str() {
            "png" => "image/png",
            "jpg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "tif" | "tiff" => "image/tiff",
            "emf" => "image/x-emf",
            "wmf" => "image/x-wmf",
            _ => {
                self.omit("그림(지원하지 않는 형식)");
                return;
            }
        };
        if !picture.common.treat_as_char {
            self.omit("떠 있는 그림(글자처럼 제자리에 씀)");
        }
        let target = match self.media.get(&id) {
            Some(target) => target.clone(),
            None => {
                let Some(bytes) = bin.data.load_limited(MAX_PICTURE_BYTES) else {
                    self.omit("그림(너무 큼)");
                    return;
                };
                if bytes.is_empty() {
                    self.omit("그림(데이터 없음)");
                    return;
                }
                let target = format!("media/image{}.{ext}", self.media.len() + 1);
                self.parts.push((format!("word/{target}"), bytes));
                self.extensions.insert(ext.clone(), content_type);
                self.media.insert(id, target.clone());
                target
            }
        };
        let name = target.rsplit('/').next().unwrap_or("image").to_owned();
        let rel_id = self.add_rel("/image", target);
        let width = emu(i64::from(picture.common.width)).max(emu(100));
        let height = emu(i64::from(picture.common.height)).max(emu(100));
        self.inline_picture(&rel_id, &name, width, height, out);
    }

    fn inline_picture(
        &mut self,
        rel_id: &str,
        name: &str,
        width: i64,
        height: i64,
        out: &mut String,
    ) {
        self.drawing_id += 1;
        let n = self.drawing_id;
        let _ = write!(
            out,
            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
             <wp:extent cx=\"{width}\" cy=\"{height}\"/><wp:docPr id=\"{n}\" name=\"그림 {n}\"/>\
             <wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>\
             <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
             <pic:pic><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr>\
             <pic:blipFill><a:blip r:embed=\"{rel_id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
             <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm>\
             <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
             </a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            esc(name)
        );
    }

    /// A footnote or endnote: its paragraphs go to footnotes.xml / endnotes.xml and a
    /// reference mark stays in the text. Ids are taken first, since a note may hold one.
    fn note(&mut self, end: bool, paragraphs: &[Paragraph], depth: usize, out: &mut String) {
        self.next_footnote += 1;
        let id = self.next_footnote;
        let (tag, part) = if end {
            ("endnote", "endnotes.xml")
        } else {
            ("footnote", "footnotes.xml")
        };
        let mark =
            format!("<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{tag}Ref/></w:r>");
        let mut body = String::new();
        for (i, p) in paragraphs.iter().enumerate() {
            let mut xml = String::new();
            self.in_part(part, |w| w.paragraph(p, None, depth + 1, &mut xml));
            if i == 0 {
                // The note's own number goes first in its first paragraph.
                if let Some(at) = xml.find("</w:pPr>") {
                    xml.insert_str(at + "</w:pPr>".len(), &mark);
                }
            }
            body.push_str(&xml);
        }
        if body.is_empty() {
            body = format!("<w:p>{mark}</w:p>");
        }
        let note = format!("<w:{tag} w:id=\"{id}\">{body}</w:{tag}>");
        if end {
            self.endnotes.push(note);
        } else {
            self.footnotes.push(note);
        }
        let _ = write!(
            out,
            "<w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{tag}Reference w:id=\"{id}\"/></w:r>"
        );
    }

    fn border(&self, side: &str, line: &BorderLine, out: &mut String) {
        if line.line_type == BorderLineType::None {
            let _ = write!(out, "<w:{side} w:val=\"nil\"/>");
            return;
        }
        let mm = BORDER_MM[(line.width as usize).min(BORDER_MM.len() - 1)];
        let eighths = ((mm * 72.0 / 25.4) * 8.0).round().clamp(2.0, 96.0) as i64;
        let val = match line.line_type {
            BorderLineType::Solid => "single",
            BorderLineType::Dash => "dashed",
            BorderLineType::Dot => "dotted",
            BorderLineType::DashDot => "dotDash",
            BorderLineType::DashDotDot => "dotDotDash",
            BorderLineType::Double => "double",
            _ => "single",
        };
        let color = if line.color & 0xFF00_0000 != 0 {
            "auto".to_owned()
        } else {
            rgb(line.color)
        };
        let _ = write!(
            out,
            "<w:{side} w:val=\"{val}\" w:sz=\"{eighths}\" w:space=\"0\" w:color=\"{color}\"/>"
        );
    }

    fn cell_borders_and_fill(&self, border_fill_id: u16, out: &mut String) {
        if border_fill_id == 0 {
            return;
        }
        let Some(bf) = self
            .doc
            .doc_info
            .border_fills
            .get(border_fill_id as usize - 1)
        else {
            return;
        };
        out.push_str("<w:tcBorders>");
        // HWP order: left, right, top, bottom.
        self.border("top", &bf.borders[2], out);
        self.border("left", &bf.borders[0], out);
        self.border("bottom", &bf.borders[3], out);
        self.border("right", &bf.borders[1], out);
        out.push_str("</w:tcBorders>");
        if let Some(solid) = bf.fill.solid.as_ref()
            && !is_white_or_none(solid.background_color)
        {
            let _ = write!(
                out,
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>",
                rgb(solid.background_color)
            );
        }
    }

    fn table(&mut self, table: &Table, depth: usize, out: &mut String) {
        if depth >= MAX_DEPTH {
            self.omit("너무 깊이 중첩된 표");
            return;
        }
        let cols = usize::from(table.col_count.max(1));
        let rows = usize::from(table.row_count.max(1));
        // Column widths from single-column cells, then spread spanning cells.
        let mut widths: Vec<Option<i64>> = vec![None; cols];
        for cell in table.cells.iter().filter(|c| c.col_span <= 1) {
            if let Some(w) = widths.get_mut(usize::from(cell.col)) {
                *w = Some(w.unwrap_or(0).max(i64::from(cell.width)));
            }
        }
        for cell in table.cells.iter().filter(|c| c.col_span > 1) {
            let start = usize::from(cell.col);
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
        let widths: Vec<i64> = widths.into_iter().map(|w| w.unwrap_or(fallback)).collect();
        let total: i64 = widths.iter().sum();

        out.push_str("<w:tbl><w:tblPr>");
        let _ = write!(
            out,
            "<w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/>",
            twips(total)
        );
        out.push_str("<w:tblCellMar><w:left w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar>");
        out.push_str("</w:tblPr><w:tblGrid>");
        for w in &widths {
            let _ = write!(out, "<w:gridCol w:w=\"{}\"/>", twips(*w).max(1));
        }
        out.push_str("</w:tblGrid>");

        let starts: BTreeMap<(usize, usize), usize> = table
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| ((usize::from(c.row), usize::from(c.col)), i))
            .collect();
        for r in 0..rows {
            out.push_str("<w:tr>");
            if table.repeat_header
                && table
                    .cells
                    .iter()
                    .any(|c| c.is_header && usize::from(c.row) == r)
            {
                out.push_str("<w:trPr><w:tblHeader/></w:trPr>");
            } else if let Some(h) = table
                .cells
                .iter()
                .filter(|c| usize::from(c.row) == r && c.row_span <= 1)
                .map(|c| i64::from(c.height))
                .max()
            {
                let _ = write!(
                    out,
                    "<w:trPr><w:trHeight w:val=\"{}\" w:hRule=\"atLeast\"/></w:trPr>",
                    twips(h)
                );
            }
            let mut c = 0;
            while c < cols {
                if let Some(&i) = starts.get(&(r, c)) {
                    let cell = &table.cells[i];
                    let span = usize::from(cell.col_span.max(1)).min(cols - c);
                    out.push_str("<w:tc><w:tcPr>");
                    let width: i64 = widths[c..c + span].iter().sum();
                    let _ = write!(out, "<w:tcW w:w=\"{}\" w:type=\"dxa\"/>", twips(width));
                    if span > 1 {
                        let _ = write!(out, "<w:gridSpan w:val=\"{span}\"/>");
                    }
                    if cell.row_span > 1 {
                        out.push_str("<w:vMerge w:val=\"restart\"/>");
                    }
                    self.cell_borders_and_fill(cell.border_fill_id, out);
                    let _ = write!(
                        out,
                        "<w:tcMar><w:top w:w=\"{}\" w:type=\"dxa\"/><w:left w:w=\"{}\" w:type=\"dxa\"/><w:bottom w:w=\"{}\" w:type=\"dxa\"/><w:right w:w=\"{}\" w:type=\"dxa\"/></w:tcMar>",
                        twips(i64::from(cell.padding.top)),
                        twips(i64::from(cell.padding.left)),
                        twips(i64::from(cell.padding.bottom)),
                        twips(i64::from(cell.padding.right))
                    );
                    let v = match cell.vertical_align {
                        VerticalAlign::Top => "top",
                        VerticalAlign::Center => "center",
                        VerticalAlign::Bottom => "bottom",
                    };
                    let _ = write!(out, "<w:vAlign w:val=\"{v}\"/></w:tcPr>");
                    let mut content = String::new();
                    for p in &cell.paragraphs {
                        self.paragraph(p, None, depth + 1, &mut content);
                    }
                    // A cell must end with a paragraph.
                    if !content.ends_with("</w:p>") {
                        content.push_str("<w:p/>");
                    }
                    out.push_str(&content);
                    out.push_str("</w:tc>");
                    c += span;
                } else if let Some(cell) = table.cells.iter().find(|cell| {
                    usize::from(cell.col) == c
                        && usize::from(cell.row) < r
                        && r < usize::from(cell.row) + usize::from(cell.row_span)
                }) {
                    // Covered by a vertical merge from above.
                    let span = usize::from(cell.col_span.max(1)).min(cols - c);
                    let width: i64 = widths[c..c + span].iter().sum();
                    let _ = write!(
                        out,
                        "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/>",
                        twips(width)
                    );
                    if span > 1 {
                        let _ = write!(out, "<w:gridSpan w:val=\"{span}\"/>");
                    }
                    out.push_str("<w:vMerge/>");
                    self.cell_borders_and_fill(cell.border_fill_id, out);
                    out.push_str("</w:tcPr><w:p/></w:tc>");
                    c += span;
                } else if table.cells.iter().any(|cell| {
                    usize::from(cell.row) <= r
                        && r < usize::from(cell.row) + usize::from(cell.row_span.max(1))
                        && usize::from(cell.col) < c
                        && c < usize::from(cell.col) + usize::from(cell.col_span.max(1))
                }) {
                    // Inside a horizontal span already written.
                    c += 1;
                } else {
                    // A hole in a damaged table: keep the grid rectangular.
                    let _ = write!(
                        out,
                        "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/></w:tcPr><w:p/></w:tc>",
                        twips(widths[c])
                    );
                    c += 1;
                }
            }
            out.push_str("</w:tr>");
        }
        out.push_str("</w:tbl>");
    }

    /// Header or footer part for a section, with the paragraphs it holds.
    fn header_footer(&mut self, footer: bool, paragraphs: &[Paragraph], extra: &str) -> String {
        let n = self.parts.len() + 1;
        let (tag, kind, name) = if footer {
            ("ftr", "/footer", format!("footer{n}.xml"))
        } else {
            ("hdr", "/header", format!("header{n}.xml"))
        };
        let mut body = String::new();
        self.in_part(&name, |w| {
            for p in paragraphs {
                w.paragraph(p, None, 1, &mut body);
            }
        });
        body.push_str(extra);
        if body.is_empty() {
            body.push_str("<w:p/>");
        }
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{tag} {}>{body}</w:{tag}>",
            namespaces()
        );
        self.parts.push((format!("word/{name}"), xml.into_bytes()));
        self.add_rel(kind, name)
    }

    fn section_properties(
        &mut self,
        section: &hwpcraft_doc::model::document::Section,
    ) -> SectionParts {
        let mut header = None;
        let mut footer = None;
        let mut page_number: Option<PageNumberPos> = None;
        for p in &section.paragraphs {
            for c in &p.controls {
                match c {
                    Control::PageNumberPos(pos) if page_number.is_none() && pos.position != 0 => {
                        page_number = Some(pos.clone());
                    }
                    Control::Header(h)
                        if header.is_none() && h.apply_to != HeaderFooterApply::Even =>
                    {
                        header = Some(h.paragraphs.clone());
                    }
                    Control::Footer(f)
                        if footer.is_none() && f.apply_to != HeaderFooterApply::Even =>
                    {
                        footer = Some(f.paragraphs.clone());
                    }
                    Control::Header(_) | Control::Footer(_) => self.omit("짝수 쪽 머리말·꼬리말"),
                    _ => {}
                }
            }
        }
        // 쪽 번호 위치 (HWP 표 150): 1–3 위, 4–6 아래, 7·9 바깥/안쪽 위, 8·10 아래.
        let (number_top, number_xml) = match &page_number {
            Some(pos) => {
                let jc = match pos.position {
                    1 | 4 => "left",
                    2 | 5 => "center",
                    _ => "right",
                };
                let deco = |c: char| (!c.is_control() && c != '\0').then(|| c.to_string());
                let mut xml = format!("<w:p><w:pPr><w:jc w:val=\"{jc}\"/></w:pPr>");
                if let Some(prefix) = deco(pos.prefix_char) {
                    self.text_run(&format!("{prefix} "), 0, &mut xml);
                }
                xml.push_str(&field(number_format_switch(pos.format), "1"));
                if let Some(suffix) = deco(pos.suffix_char) {
                    self.text_run(&format!(" {suffix}"), 0, &mut xml);
                }
                xml.push_str("</w:p>");
                if matches!(pos.position, 7..=10) {
                    self.omit("쪽 번호 바깥·안쪽 위치(오른쪽으로 씀)");
                }
                (matches!(pos.position, 1..=3 | 7 | 9), xml)
            }
            None => (false, String::new()),
        };
        let mut refs = String::new();
        if header.is_some() || (page_number.is_some() && number_top) {
            let extra = if number_top { number_xml.as_str() } else { "" };
            let id = self.header_footer(false, &header.unwrap_or_default(), extra);
            let _ = write!(
                refs,
                "<w:headerReference w:type=\"default\" r:id=\"{id}\"/>"
            );
        }
        if footer.is_some() || (page_number.is_some() && !number_top) {
            let extra = if number_top { "" } else { number_xml.as_str() };
            let id = self.header_footer(true, &footer.unwrap_or_default(), extra);
            let _ = write!(
                refs,
                "<w:footerReference w:type=\"default\" r:id=\"{id}\"/>"
            );
        }
        let page = &section.section_def.page_def;
        let (mut w, mut h) = (i64::from(page.width), i64::from(page.height));
        if page.landscape && w < h {
            std::mem::swap(&mut w, &mut h);
        }
        let orient = if page.landscape {
            " w:orient=\"landscape\""
        } else {
            ""
        };
        let mut size = format!(
            "<w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>",
            twips(w),
            twips(h)
        );
        // HWP's top margin is to the header; Word's is to the body.
        let _ = write!(
            size,
            "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"{}\"/>",
            twips(i64::from(page.margin_top) + i64::from(page.margin_header)),
            twips(i64::from(page.margin_right)),
            twips(i64::from(page.margin_bottom) + i64::from(page.margin_footer)),
            twips(i64::from(page.margin_left)),
            twips(i64::from(page.margin_top)),
            twips(i64::from(page.margin_bottom)),
            twips(i64::from(page.margin_gutter))
        );
        SectionParts { refs, size }
    }

    /// The paragraphs of a section split where its column definition changes
    /// (단 정의 나누기): (first paragraph, columns of that run).
    /// The paragraphs of a section split where its column definition changes
    /// (단 정의 나누기): (first paragraph, columns of that run). A paragraph holding a
    /// table wider than one column (a title spanning a two-column page) gets a
    /// one-column run of its own, as DOCX tables cannot cross columns.
    fn column_runs(
        section: &hwpcraft_doc::model::document::Section,
    ) -> Vec<(usize, Option<ColumnDef>)> {
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
                    // The definition at the start of a run is that run's own.
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
                // The columns resume after the wide table.
                if pi + 1 < section.paragraphs.len() {
                    runs.push((pi + 1, current.clone()));
                }
            }
        }
        runs
    }

    fn body(&mut self) -> String {
        let mut body = String::new();
        let sections = &self.doc.sections;
        let count = sections.len();
        let mut last_sect = String::new();
        for (si, section) in sections.iter().enumerate() {
            let parts = self.section_properties(section);
            let runs = Self::column_runs(section);
            let n = section.paragraphs.len();
            for (ri, (start, columns)) in runs.iter().enumerate() {
                let end = runs.get(ri + 1).map_or(n, |(next, _)| *next);
                // A later run starts on the same page: Word's continuous section break.
                let sect = sect_pr(&parts, columns.as_ref(), ri > 0);
                let last_run = ri + 1 == runs.len();
                for pi in *start..end {
                    let para = &section.paragraphs[pi];
                    // Each run ends in the properties of its last paragraph, except the
                    // document's very last run, whose properties close the body.
                    let ends_here = pi + 1 == end && !(last_run && si + 1 == count);
                    self.address = Some((si, pi));
                    self.paragraph(para, ends_here.then_some(sect.as_str()), 0, &mut body);
                }
                if *start >= end && !(last_run && si + 1 == count) {
                    let _ = write!(body, "<w:p><w:pPr>{sect}</w:pPr></w:p>");
                }
                if last_run && si + 1 == count {
                    last_sect = sect;
                }
            }
        }
        if last_sect.is_empty() {
            last_sect.push_str("<w:sectPr/>");
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document {}><w:body>{body}{last_sect}</w:body></w:document>",
            namespaces()
        )
    }

    fn default_fonts(&self) -> (String, String) {
        let shape = self.doc.doc_info.char_shapes.first();
        let hangul = shape
            .and_then(|s| self.font_name(0, s))
            .unwrap_or("맑은 고딕");
        let latin = shape.and_then(|s| self.font_name(1, s)).unwrap_or(hangul);
        (esc(hangul), esc(latin))
    }
}

/// A simple field (`PAGE`, `NUMPAGES`, …) with its shown result.
fn field(instr: &str, shown: &str) -> String {
    format!(
        "<w:fldSimple w:instr=\" {} \"><w:r><w:t>{}</w:t></w:r></w:fldSimple>",
        esc(instr),
        esc(shown)
    )
}

/// `PAGE` with the number format of a HWP page number (표 134).
fn number_format_switch(format: u8) -> &'static str {
    match format {
        2 => "PAGE \\* ROMAN",
        3 => "PAGE \\* roman",
        4 => "PAGE \\* ALPHABETIC",
        5 => "PAGE \\* alphabetic",
        _ => "PAGE",
    }
}

/// Header/footer references and page size/margins of one HWP section.
struct SectionParts {
    refs: String,
    size: String,
}

/// `w:sectPr` for one column run of a section (child order follows the schema).
fn sect_pr(parts: &SectionParts, columns: Option<&ColumnDef>, continuous: bool) -> String {
    let mut out = format!("<w:sectPr>{}", parts.refs);
    if continuous {
        out.push_str("<w:type w:val=\"continuous\"/>");
    }
    out.push_str(&parts.size);
    if let Some(def) = columns.filter(|d| d.column_count > 1) {
        let count = def.column_count.min(45);
        if def.same_width || def.widths.len() < usize::from(count) {
            let _ = write!(
                out,
                "<w:cols w:num=\"{count}\" w:space=\"{}\"/>",
                twips(i64::from(def.spacing))
            );
        } else {
            let _ = write!(out, "<w:cols w:num=\"{count}\" w:equalWidth=\"0\">");
            for i in 0..usize::from(count) {
                let width = def.widths.get(i).copied().map_or(0, i64::from);
                let gap = def.gaps.get(i).copied().map_or(0, i64::from);
                let _ = write!(
                    out,
                    "<w:col w:w=\"{}\" w:space=\"{}\"/>",
                    twips(width),
                    twips(gap)
                );
            }
            out.push_str("</w:cols>");
        }
    }
    out.push_str("</w:sectPr>");
    out
}

fn namespaces() -> String {
    format!(
        "xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\""
    )
}

/// The document as a `.docx` package.
pub fn export(core: &DocumentCore) -> Result<DocxExport> {
    let mut w = Writer::new(core);
    let document = w.body();
    let (hangul, latin) = w.default_fonts();
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W_NS}\">\
         <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"{latin}\" w:hAnsi=\"{latin}\" w:eastAsia=\"{hangul}\" w:cs=\"{latin}\"/>\
         <w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/><w:lang w:val=\"ko-KR\" w:eastAsia=\"ko-KR\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
         <w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:tblPr><w:tblCellMar>\
         <w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"108\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/>\
         </w:tblCellMar></w:tblPr></w:style></w:styles>"
    );
    let styles_rel = w.add_rel("/styles", "styles.xml".to_owned());
    let _ = styles_rel;
    for (tag, notes) in [
        ("footnote", std::mem::take(&mut w.footnotes)),
        ("endnote", std::mem::take(&mut w.endnotes)),
    ] {
        if notes.is_empty() {
            continue;
        }
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{tag}s {}>\
             <w:{tag} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{tag}>\
             <w:{tag} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{tag}>\
             {}</w:{tag}s>",
            namespaces(),
            notes.concat()
        );
        w.parts.push((format!("word/{tag}s.xml"), xml.into_bytes()));
        let kind = if tag == "footnote" {
            "/footnotes"
        } else {
            "/endnotes"
        };
        w.add_rel(kind, format!("{tag}s.xml"));
    }

    let mut types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>",
    );
    for (ext, content_type) in &w.extensions {
        let _ = write!(
            types,
            "<Default Extension=\"{ext}\" ContentType=\"{content_type}\"/>"
        );
    }
    types.push_str("<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>");
    types.push_str("<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>");
    for (part, _) in &w.parts {
        let content_type = if part.starts_with("word/header") {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
        } else if part.starts_with("word/footer") {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
        } else if part == "word/footnotes.xml" {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"
        } else if part == "word/endnotes.xml" {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"
        } else {
            continue;
        };
        let _ = write!(
            types,
            "<Override PartName=\"/{part}\" ContentType=\"{content_type}\"/>"
        );
    }
    types.push_str("</Types>");

    let package_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"{REL_TYPE}/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
    );
    let rels_files: Vec<(String, String)> = w
        .rels
        .iter()
        .map(|(part, rels)| {
            let mut xml = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            );
            for (id, kind, target) in rels {
                let mode = if *kind == "/hyperlink" {
                    " TargetMode=\"External\""
                } else {
                    ""
                };
                let _ = write!(
                    xml,
                    "<Relationship Id=\"{id}\" Type=\"{REL_TYPE}{kind}\" Target=\"{}\"{mode}/>",
                    esc(target)
                );
            }
            xml.push_str("</Relationships>");
            (format!("word/_rels/{part}.rels"), xml)
        })
        .collect();

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, bytes: &[u8]| -> Result<()> {
        zip.start_file(name, options)
            .context("DOCX 묶음을 만들지 못했습니다.")?;
        zip.write_all(bytes)?;
        Ok(())
    };
    put("[Content_Types].xml", types.as_bytes())?;
    put("_rels/.rels", package_rels.as_bytes())?;
    put("word/document.xml", document.as_bytes())?;
    put("word/styles.xml", styles.as_bytes())?;
    for (name, xml) in &rels_files {
        put(name, xml.as_bytes())?;
    }
    for (name, bytes) in &w.parts {
        put(name, bytes)?;
    }
    let bytes = zip
        .finish()
        .context("DOCX 묶음을 닫지 못했습니다.")?
        .into_inner();
    let omitted = w
        .omitted
        .iter()
        .map(|(what, count)| format!("{what} {count}개"))
        .collect();
    Ok(DocxExport { bytes, omitted })
}
