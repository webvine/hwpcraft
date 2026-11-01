//! Dynamic page fields projected onto a separate layout document.
//!
//! A measured inline tab reserves the display width of each field while keeping
//! exactly one model scalar and the original control indices. The editable and
//! saved document is never replaced by this presentation document.
//!
//! Fields in table cells (any depth), text boxes and footnote/endnote paragraphs
//! are projected the same way. RHWP draws its own decimal value over a cell or text
//! box number placeholder (and none in notes) without reserving its width, so a
//! projected container field drops its page kind in the projection only. Headers,
//! footers and master pages are left to RHWP, which numbers them per page.
use super::*;
use crate::presentation_geometry::{
    Affinity, CaretLookup, GeometryCache, PathStep, TextPath, TextPosition,
};
use hwpcraft_doc::model::control::{AutoNumber, AutoNumberType};
use hwpcraft_doc::model::document::Document;
use hwpcraft_doc::model::paragraph::CharShapeRef;
use hwpcraft_doc::model::shape::ShapeObject;
use hwpcraft_doc::{RenderNode, RenderNodeType, SvgRenderer};
use std::collections::BTreeMap;

/// One step from a paragraph into a container held by one of its controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    /// A table cell, or a text box as cell 0, as RHWP's cell paths name them.
    Nested {
        control: usize,
        cell: usize,
        paragraph: usize,
    },
    /// A footnote or endnote paragraph of a body paragraph.
    Note { control: usize, paragraph: usize },
}

/// The paragraph a field is in: a body paragraph and the containers below it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Holder {
    section: usize,
    paragraph: usize,
    path: Vec<Step>,
}

/// What the containers around a field's paragraph imply for its drawing.
#[derive(Clone, Copy, Debug, Default)]
struct Inside {
    /// The innermost container is a text box (RHWP's `in_textbox` cell context).
    text_box: bool,
    /// A table title row RHWP draws again on each page the table continues on.
    repeated: bool,
}

struct Field {
    holder: Holder,
    offset: usize,
    /// The number's control index in its paragraph.
    control: usize,
    number: AutoNumber,
    inside: Inside,
}

/// A repeated title row's field drawn again on a later page.
#[derive(Clone)]
struct Repeat {
    field: usize,
    page: u32,
    x: f64,
    /// Top of the owning run.
    y: f64,
    width: f64,
    display: Display,
}

impl Field {
    /// The body caret position of a field written directly in body text.
    fn body_cursor(&self) -> Option<Cursor> {
        self.holder.path.is_empty().then_some(Cursor {
            section: self.holder.section,
            paragraph: self.holder.paragraph,
            offset: self.offset,
            cell: None,
        })
    }
    /// The geometry address of a field in body text, a cell or a text box.
    fn text_position(&self) -> Option<TextPosition> {
        let path = self
            .holder
            .path
            .iter()
            .map(|step| match *step {
                Step::Nested {
                    control,
                    cell,
                    paragraph,
                } => Some(PathStep {
                    control,
                    cell,
                    paragraph,
                }),
                Step::Note { .. } => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(TextPosition {
            address: TextPath {
                section: self.holder.section,
                paragraph: self.holder.paragraph,
                path,
                in_textbox: self.inside.text_box,
            },
            offset: self.offset,
        })
    }
    /// Whether `c` (body text or a first-level cell) is this field's position.
    fn is_at(&self, c: Cursor) -> bool {
        let path = c.cell.map(|cell| Step::Nested {
            control: cell.control,
            cell: cell.index,
            paragraph: cell.paragraph,
        });
        self.holder.section == c.section
            && self.holder.paragraph == c.paragraph
            && self.offset == c.offset
            && self.holder.path.as_slice() == path.as_slice()
    }
}

fn inner_paragraph(p: &Paragraph, step: Step) -> Option<&Paragraph> {
    match step {
        Step::Nested {
            control,
            cell,
            paragraph,
        } => match p.controls.get(control)? {
            Control::Table(table) => table.cells.get(cell)?.paragraphs.get(paragraph),
            Control::Shape(shape) if cell == 0 && !matches!(**shape, ShapeObject::Group(_)) => {
                shape
                    .drawing()?
                    .text_box
                    .as_ref()?
                    .paragraphs
                    .get(paragraph)
            }
            _ => None,
        },
        Step::Note { control, paragraph } => match p.controls.get(control)? {
            Control::Footnote(note) => note.paragraphs.get(paragraph),
            Control::Endnote(note) => note.paragraphs.get(paragraph),
            _ => None,
        },
    }
}

fn inner_paragraph_mut(p: &mut Paragraph, step: Step) -> Option<&mut Paragraph> {
    match step {
        Step::Nested {
            control,
            cell,
            paragraph,
        } => match p.controls.get_mut(control)? {
            Control::Table(table) => table.cells.get_mut(cell)?.paragraphs.get_mut(paragraph),
            Control::Shape(shape) if cell == 0 && !matches!(**shape, ShapeObject::Group(_)) => {
                shape
                    .drawing_mut()?
                    .text_box
                    .as_mut()?
                    .paragraphs
                    .get_mut(paragraph)
            }
            _ => None,
        },
        Step::Note { control, paragraph } => match p.controls.get_mut(control)? {
            Control::Footnote(note) => note.paragraphs.get_mut(paragraph),
            Control::Endnote(note) => note.paragraphs.get_mut(paragraph),
            _ => None,
        },
    }
}

fn holder_paragraph<'a>(doc: &'a Document, holder: &Holder) -> Option<&'a Paragraph> {
    let mut p = doc
        .sections
        .get(holder.section)?
        .paragraphs
        .get(holder.paragraph)?;
    for step in &holder.path {
        p = inner_paragraph(p, *step)?;
    }
    Some(p)
}

fn holder_paragraph_mut<'a>(doc: &'a mut Document, holder: &Holder) -> Option<&'a mut Paragraph> {
    let mut p = doc
        .sections
        .get_mut(holder.section)?
        .paragraphs
        .get_mut(holder.paragraph)?;
    for step in &holder.path {
        p = inner_paragraph_mut(p, *step)?;
    }
    Some(p)
}

/// RHWP's caret rectangle at `offset` of a field's paragraph.
fn native_caret(core: &DocumentCore, holder: &Holder, offset: usize) -> Result<Value> {
    let (s, p) = (holder.section, holder.paragraph);
    let raw = match holder.path.as_slice() {
        [] => core.get_cursor_rect_native(s, p, offset),
        [Step::Note { control, paragraph }] => {
            core.get_cursor_rect_in_note_native(s, p, *control, *paragraph, offset)
        }
        path => {
            let mut entries = Vec::new();
            for step in path {
                let Step::Nested {
                    control,
                    cell,
                    paragraph,
                } = step
                else {
                    bail!("주석 안 개체의 자동 번호 위치는 찾지 않습니다.");
                };
                entries.push(format!(
                    r#"{{"controlIndex":{control},"cellIndex":{cell},"cellParaIndex":{paragraph}}}"#
                ));
            }
            core.get_cursor_rect_by_path_native(s, p, &format!("[{}]", entries.join(",")), offset)
        }
    };
    Ok(serde_json::from_str(&raw.map_err(err)?)?)
}

#[derive(Clone)]
struct Display {
    value: String,
    width: f64,
    origin_x: f64,
    origin_y: f64,
    caret_y_offset: f64,
    svg: String,
}

struct FieldRect {
    page: u32,
    x: f64,
    /// Top of the owning run, or of RHWP's caret when `caret_top`.
    y: f64,
    width: f64,
    caret_top: bool,
}

fn body_run(node: &RenderNode, inside: bool, c: Cursor) -> Option<&RenderNode> {
    if matches!(
        node.node_type,
        RenderNodeType::Header
            | RenderNodeType::Footer
            | RenderNodeType::MasterPage
            | RenderNodeType::Table(_)
            | RenderNodeType::TextBox
            | RenderNodeType::FootnoteArea
    ) {
        return None;
    }
    let inside = inside || matches!(node.node_type, RenderNodeType::Body { .. });
    if inside
        && let RenderNodeType::TextRun(run) = &node.node_type
        && run.cell_context.is_none()
        && run.section_index == Some(c.section)
        && run.para_index == Some(c.paragraph)
        && let Some(start) = run.char_start
        && c.offset >= start
        && c.offset < start + run.text.chars().count()
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| body_run(child, inside, c))
}

fn field_rect(
    core: &DocumentCore,
    geometry: &GeometryCache,
    revision: u64,
    field: &Field,
) -> Result<FieldRect> {
    let Some(c) = field.body_cursor() else {
        // Container text has no body run of its own; RHWP's caret in the cell,
        // text box or note paragraph gives the reserved spacer's start and end.
        let a = native_caret(core, &field.holder, field.offset)?;
        let b = native_caret(core, &field.holder, field.offset + 1)?;
        let same_line = a["pageIndex"] == b["pageIndex"]
            && (a["y"].as_f64().unwrap_or(0.0) - b["y"].as_f64().unwrap_or(0.0)).abs() < 1.0;
        let x = a["x"].as_f64().unwrap_or(0.0);
        return Ok(FieldRect {
            page: a["pageIndex"].as_u64().unwrap_or(0) as u32,
            x,
            y: a["y"].as_f64().unwrap_or(0.0),
            width: if same_line {
                (b["x"].as_f64().unwrap_or(x) - x).max(0.0)
            } else {
                0.0
            },
            caret_top: true,
        });
    };
    let native: Value = serde_json::from_str(
        &core
            .get_cursor_rect_native(c.section, c.paragraph, c.offset)
            .map_err(err)?,
    )?;
    let first = native["pageIndex"].as_u64().unwrap_or(0) as u32;
    let position = TextPosition::from(c);
    let mut fallback = None;
    for page in first
        ..=first
            .saturating_add(1)
            .min(core.page_count().saturating_sub(1))
    {
        for at in geometry.scalar_occurrences_on_page(core, revision, page, &position)? {
            let rect = FieldRect {
                page,
                x: at.x,
                y: at.run_bounds.y,
                width: at.advance.max(0.0),
                caret_top: false,
            };
            if at.fully_in_flow && at.visible {
                return Ok(rect);
            }
            if fallback.is_none() {
                fallback = Some(rect);
            }
        }
    }
    fallback.ok_or_else(|| anyhow!("본문 자동 번호를 소유한 줄을 찾지 못했습니다."))
}

pub(crate) struct PageFieldLayout {
    pub core: DocumentCore,
    fields: Vec<(Field, Display)>,
    repeats: Vec<Repeat>,
    view_marks: crate::ext_page::ViewMarks,
    pub(crate) geometry: GeometryCache,
    pub(crate) geometry_revision: u64,
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn alphabet(mut n: u32, letters: &[char]) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(letters[(n % letters.len() as u32) as usize]);
        n /= letters.len() as u32;
    }
    out.into_iter().rev().collect()
}

fn hanja(mut n: u32) -> String {
    if n == 0 {
        return "〇".into();
    }
    let digits: Vec<char> = "〇一二三四五六七八九".chars().collect();
    let mut out = String::new();
    if n >= 10_000 {
        out.push_str(&hanja(n / 10_000));
        out.push('萬');
        n %= 10_000;
    }
    for (unit, symbol) in [(1000, '千'), (100, '百'), (10, '十')] {
        let d = n / unit;
        if d > 0 {
            if d != 1 {
                out.push(digits[d as usize]);
            }
            out.push(symbol);
            n %= unit;
        }
    }
    if n > 0 {
        out.push(digits[n as usize]);
    }
    out
}

fn hangul_number(mut n: u32) -> String {
    if n == 0 {
        return "영".into();
    }
    let digits: Vec<char> = "영일이삼사오육칠팔구".chars().collect();
    let mut out = String::new();
    if n >= 10_000 {
        out.push_str(&hangul_number(n / 10_000));
        out.push('만');
        n %= 10_000;
    }
    for (unit, symbol) in [(1000, '천'), (100, '백'), (10, '십')] {
        let digit = n / unit;
        if digit > 0 {
            if digit != 1 {
                out.push(digits[digit as usize]);
            }
            out.push(symbol);
            n %= unit;
        }
    }
    if n > 0 {
        out.push(digits[n as usize]);
    }
    out
}

fn circle_letters(text: String, first: char, base: u32) -> String {
    text.chars()
        .map(|ch| {
            if ch >= first && u32::from(ch) < u32::from(first) + 26 {
                char::from_u32(base + u32::from(ch) - u32::from(first)).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

fn number_text(value: u32, number: &AutoNumber) -> Result<String> {
    let text = match number.format {
        0 => value.to_string(),
        1 if value == 0 => "⓪".into(),
        1 if value <= 20 => char::from_u32(0x245f + value)
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        1 => format!("{}\u{20dd}", value),
        2 | 3 => {
            let mut n = value;
            let mut text = String::new();
            for (amount, symbol) in [
                (1000, "M"),
                (900, "CM"),
                (500, "D"),
                (400, "CD"),
                (100, "C"),
                (90, "XC"),
                (50, "L"),
                (40, "XL"),
                (10, "X"),
                (9, "IX"),
                (5, "V"),
                (4, "IV"),
                (1, "I"),
            ] {
                while n >= amount {
                    text.push_str(symbol);
                    n -= amount;
                }
            }
            if number.format == 3 {
                text.make_ascii_lowercase();
            }
            if text.is_empty() { "0".into() } else { text }
        }
        4 => alphabet(
            value,
            &"ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect::<Vec<_>>(),
        ),
        5 => alphabet(
            value,
            &"abcdefghijklmnopqrstuvwxyz".chars().collect::<Vec<_>>(),
        ),
        6 => circle_letters(
            alphabet(
                value,
                &"ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect::<Vec<_>>(),
            ),
            'A',
            0x24b6,
        ),
        7 => circle_letters(
            alphabet(
                value,
                &"abcdefghijklmnopqrstuvwxyz".chars().collect::<Vec<_>>(),
            ),
            'a',
            0x24d0,
        ),
        8 => alphabet(
            value,
            &"가나다라마바사아자차카타파하".chars().collect::<Vec<_>>(),
        ),
        9 if (1..=14).contains(&value) => char::from_u32(0x326d + value)
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        9 => format!(
            "{}\u{20dd}",
            alphabet(
                value,
                &"가나다라마바사아자차카타파하".chars().collect::<Vec<_>>()
            )
        ),
        10 => alphabet(
            value,
            &"ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎ".chars().collect::<Vec<_>>(),
        ),
        11 if (1..=14).contains(&value) => char::from_u32(0x325f + value)
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        11 => format!(
            "{}\u{20dd}",
            alphabet(
                value,
                &"ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎ".chars().collect::<Vec<_>>()
            )
        ),
        12 => hangul_number(value),
        13 => hanja(value),
        14 if (1..=10).contains(&value) => char::from_u32(0x327f + value)
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        14 => format!("{}\u{20dd}", hanja(value)),
        18 | 0x81 => {
            if number.user_symbol == '\0' {
                String::new()
            } else {
                number.user_symbol.to_string()
            }
        }
        format => bail!("본문 자동 번호 표시 형식 {format}은 아직 지원하지 않습니다."),
    };
    let mut out = String::new();
    if number.prefix_char != '\0' {
        out.push(number.prefix_char);
    }
    out.push_str(&text);
    if number.suffix_char != '\0' {
        out.push(number.suffix_char);
    }
    Ok(out)
}

fn text_only(node: &mut RenderNode) {
    node.children.retain(|child| {
        !matches!(
            child.node_type,
            RenderNodeType::PageBackground(_)
                | RenderNodeType::Header
                | RenderNodeType::Footer
                | RenderNodeType::MasterPage
        )
    });
    if let RenderNodeType::Body { clip_rect } = &mut node.node_type {
        *clip_rect = None;
    }
    for child in &mut node.children {
        text_only(child);
    }
}

fn measure(source: &DocumentCore, field: &Field, value: String, dpi: f64) -> Result<Display> {
    let shape_id = holder_paragraph(source.document(), &field.holder)
        .and_then(|p| p.char_shape_id_at(field.offset))
        .unwrap_or(0);
    let original = source.document();
    let mut doc = hwpcraft_doc::model::document::Document {
        header: original.header.clone(),
        doc_properties: original.doc_properties.clone(),
        doc_info: original.doc_info.clone(),
        bin_data_content: original.bin_data_content.clone(),
        is_hwp3_variant: original.is_hwp3_variant,
        is_hwpx_variant: original.is_hwpx_variant,
        provenance: original.provenance,
        ..Default::default()
    };
    let mut p = Paragraph {
        text: value.clone(),
        has_para_text: true,
        ..Default::default()
    };
    let mut position = 0;
    for ch in p.text.chars() {
        p.char_offsets.push(position);
        position += ch.len_utf16() as u32;
    }
    p.char_count = position + 1;
    let shape = if field.number.superscript {
        let mut shape = doc
            .doc_info
            .char_shapes
            .get(shape_id as usize)
            .cloned()
            .ok_or_else(|| anyhow!("번호 글자 모양이 없습니다."))?;
        shape.superscript = true;
        shape.raw_data = None;
        let id = doc.doc_info.char_shapes.len() as u32;
        doc.doc_info.char_shapes.push(shape);
        id
    } else {
        shape_id
    };
    p.char_shapes = vec![CharShapeRef {
        start_pos: 0,
        char_shape_id: shape,
    }];
    doc.sections = vec![hwpcraft_doc::model::document::Section {
        section_def: original.sections[0].section_def.clone(),
        paragraphs: vec![p],
        raw_stream: None,
        raw_provenance: None,
    }];
    doc.sections[0].section_def.page_def.width = 1_000_000;
    let mut core = blank()?;
    core.set_dpi(dpi);
    core.set_font_environment(source.font_environment().cloned())
        .map_err(err)?;
    core.set_document(doc);
    core.reflow_linesegs_on_demand();
    let a: Value = serde_json::from_str(&core.get_cursor_rect_native(0, 0, 0).map_err(err)?)?;
    let b: Value = serde_json::from_str(
        &core
            .get_cursor_rect_native(0, 0, value.chars().count())
            .map_err(err)?,
    )?;
    if a["pageIndex"] != b["pageIndex"]
        || (a["y"].as_f64().unwrap_or(0.0) - b["y"].as_f64().unwrap_or(0.0)).abs() > 1.0
    {
        bail!("번호 표시 문자열이 측정 가능한 한 줄 범위를 넘었습니다.");
    }
    let mut tree = core.build_page_render_tree(0).map_err(err)?;
    text_only(&mut tree.root);
    let origin =
        body_run(&tree.root, false, Cursor::default()).map(|node| (node.bbox.x, node.bbox.y));
    let mut renderer = SvgRenderer::new();
    renderer.render_tree(&tree);
    let svg = renderer.output();
    let start = svg
        .find('>')
        .ok_or_else(|| anyhow!("번호 표시 SVG를 읽지 못했습니다."))?
        + 1;
    let end = svg
        .rfind("</svg>")
        .ok_or_else(|| anyhow!("번호 표시 SVG를 읽지 못했습니다."))?;
    Ok(Display {
        value,
        width: (b["x"].as_f64().unwrap_or(0.0) - a["x"].as_f64().unwrap_or(0.0)).max(0.0),
        origin_x: origin.map_or(a["x"].as_f64().unwrap_or(0.0), |v| v.0),
        origin_y: origin.map_or(a["y"].as_f64().unwrap_or(0.0), |v| v.1),
        caret_y_offset: a["y"].as_f64().unwrap_or(0.0)
            - origin.map_or(a["y"].as_f64().unwrap_or(0.0), |v| v.1),
        svg: svg[start..end].to_owned(),
    })
}

fn tab(width: f64) -> [u16; 7] {
    // Native inline-tab geometry reads ext[0] at a fixed 96/7200 scale,
    // even when page and glyph geometry use another DPI. These transient
    // spacers target that public renderer contract, not saved HWP units.
    let units = (width * 75.0).ceil().clamp(1.0, u32::MAX as f64) as u32;
    [
        (units & 0xffff) as u16,
        (units >> 16) as u16,
        0x100,
        0,
        0,
        0,
        9,
    ]
}

/// Page-number / total-pages fields of `source` in body text, table cells, text
/// boxes and notes, in document order.
fn collect_fields(source: &DocumentCore) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    for (section, s) in source.document().sections.iter().enumerate() {
        for (paragraph, p) in s.paragraphs.iter().enumerate() {
            let mut holder = Holder {
                section,
                paragraph,
                path: Vec::new(),
            };
            collect_in(p, &mut holder, Inside::default(), &mut fields)?;
        }
    }
    Ok(fields)
}

fn collect_in(
    p: &Paragraph,
    holder: &mut Holder,
    inside: Inside,
    fields: &mut Vec<Field>,
) -> Result<()> {
    // Most paragraphs hold no number; their positions are not worth computing.
    let mut positions = None;
    for (control, item) in p.controls.iter().enumerate() {
        let Control::AutoNumber(number) = item else {
            continue;
        };
        if !matches!(
            number.number_type,
            AutoNumberType::Page | AutoNumberType::TotalPage
        ) {
            continue;
        }
        let mut offset = positions
            .get_or_insert_with(|| p.control_text_positions())
            .get(control)
            .copied()
            .ok_or_else(|| anyhow!("본문 자동 번호 위치가 없습니다."))?;
        // A trailing placeholder has no following stream sentinel.
        if offset == p.text.chars().count()
            && offset > 0
            && p.text.chars().last().is_some_and(char::is_whitespace)
            && !fields
                .iter()
                .any(|f: &Field| f.holder == *holder && f.offset == offset - 1)
        {
            offset -= 1;
        }
        if !p
            .text
            .chars()
            .nth(offset)
            .is_some_and(|ch| ch.is_whitespace() || ch == '\u{0015}')
        {
            // Unknown stored layout: leave this field to RHWP's own
            // rendering. Failing here would refuse to open the document.
            continue;
        }
        fields.push(Field {
            holder: holder.clone(),
            offset,
            control,
            number: number.clone(),
            inside,
        });
    }
    // RHWP gives no caret position to containers inside notes.
    if holder
        .path
        .iter()
        .any(|step| matches!(step, Step::Note { .. }))
    {
        return Ok(());
    }
    for (control, item) in p.controls.iter().enumerate() {
        match item {
            Control::Table(table) => {
                for (index, cell) in table.cells.iter().enumerate() {
                    // Vertical cell text keeps RHWP's own number drawing.
                    if cell.text_direction == 0 {
                        let step = |paragraph| Step::Nested {
                            control,
                            cell: index,
                            paragraph,
                        };
                        let inside = Inside {
                            text_box: false,
                            repeated: inside.repeated || table.repeat_header && cell.is_header,
                        };
                        collect_inside(&cell.paragraphs, holder, step, inside, fields)?;
                    }
                }
            }
            Control::Shape(shape) if !matches!(**shape, ShapeObject::Group(_)) => {
                if let Some(text_box) = shape.drawing().and_then(|d| d.text_box.as_ref())
                    && text_box.list_attr & 0x07 == 0
                {
                    let step = |paragraph| Step::Nested {
                        control,
                        cell: 0,
                        paragraph,
                    };
                    let inside = Inside {
                        text_box: true,
                        ..inside
                    };
                    collect_inside(&text_box.paragraphs, holder, step, inside, fields)?;
                }
            }
            Control::Footnote(note) if holder.path.is_empty() => {
                let step = |paragraph| Step::Note { control, paragraph };
                collect_inside(&note.paragraphs, holder, step, inside, fields)?;
            }
            Control::Endnote(note) if holder.path.is_empty() => {
                let step = |paragraph| Step::Note { control, paragraph };
                collect_inside(&note.paragraphs, holder, step, inside, fields)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn collect_inside(
    paragraphs: &[Paragraph],
    holder: &mut Holder,
    step: impl Fn(usize) -> Step,
    inside: Inside,
    fields: &mut Vec<Field>,
) -> Result<()> {
    for (index, p) in paragraphs.iter().enumerate() {
        holder.path.push(step(index));
        let collected = collect_in(p, holder, inside, fields);
        holder.path.pop();
        collected?;
    }
    Ok(())
}

/// The fields RHWP can place in `source`: a container field without a caret
/// position (a hidden or clipped cell, ...) keeps RHWP's own drawing.
fn placed_fields(source: &DocumentCore) -> Result<Vec<Field>> {
    let mut fields = collect_fields(source)?;
    fields
        .retain(|f| f.body_cursor().is_some() || native_caret(source, &f.holder, f.offset).is_ok());
    Ok(fields)
}

type Measurements = BTreeMap<(u32, bool, String), Display>;
/// (page, displayed value) of each field.
type FieldStates = Vec<(u32, String)>;

/// Page and display of every field as laid out in `current` (the source or a projection).
#[allow(clippy::too_many_arguments)]
fn measure_fields(
    source: &DocumentCore,
    current: &DocumentCore,
    geometry: &GeometryCache,
    revision: u64,
    fields: &[Field],
    measurements: &mut Measurements,
    dpi: f64,
) -> Result<(FieldStates, Vec<Display>)> {
    let mut state = Vec::new();
    let mut displays = Vec::new();
    for field in fields {
        let page = field_rect(current, geometry, revision, field)?.page;
        let display = display_on(
            source,
            current,
            geometry,
            revision,
            field,
            page,
            measurements,
            dpi,
        )?;
        state.push((page, display.value.clone()));
        displays.push(display);
    }
    Ok((state, displays))
}

/// The display of `field` drawn on `page` of `current`.
#[allow(clippy::too_many_arguments)]
fn display_on(
    source: &DocumentCore,
    current: &DocumentCore,
    geometry: &GeometryCache,
    revision: u64,
    field: &Field,
    page: u32,
    measurements: &mut Measurements,
    dpi: f64,
) -> Result<Display> {
    let page_geometry = geometry.page(current, revision, page)?;
    let info = &page_geometry.page_info;
    let number = if field.number.number_type == AutoNumberType::TotalPage {
        current.page_count()
    } else {
        info["pageNumber"].as_u64().unwrap_or(u64::from(page + 1)) as u32
    };
    let value = number_text(number, &field.number)?;
    let shape = holder_paragraph(source.document(), &field.holder)
        .and_then(|p| p.char_shape_id_at(field.offset))
        .unwrap_or(0);
    let key = (shape, field.number.superscript, value.clone());
    if let Some(display) = measurements.get(&key) {
        return Ok(display.clone());
    }
    let display = measure(source, field, value, dpi)?;
    measurements.insert(key, display.clone());
    Ok(display)
}

/// Where a field in a repeated table title row is drawn again after its first page,
/// with the value of each of those pages. The spacer keeps the first page's width.
#[allow(clippy::too_many_arguments)]
fn repeated(
    source: &DocumentCore,
    current: &DocumentCore,
    geometry: &GeometryCache,
    revision: u64,
    fields: &[Field],
    measurements: &mut Measurements,
    dpi: f64,
) -> Result<Vec<Repeat>> {
    let mut repeats = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let Some(position) = field.text_position().filter(|_| field.inside.repeated) else {
            continue;
        };
        let first = field_rect(current, geometry, revision, field)?.page;
        for page in first + 1..current.page_count() {
            let Some(at) = geometry
                .scalar_occurrences_on_page(current, revision, page, &position)?
                .into_iter()
                .next()
            else {
                break;
            };
            let display = display_on(
                source,
                current,
                geometry,
                revision,
                field,
                page,
                measurements,
                dpi,
            )?;
            repeats.push(Repeat {
                field: index,
                page,
                x: at.x,
                y: at.run_bounds.y,
                width: at.advance.max(0.0),
                display,
            });
        }
    }
    Ok(repeats)
}

/// Whether the widths measured on a projection equal those it was built with.
fn same_widths(built: Option<&[f64]>, measured: &[f64]) -> bool {
    built.is_some_and(|w| {
        w.len() == measured.len() && w.iter().zip(measured).all(|(a, b)| (a - b).abs() < 0.01)
    })
}

/// An empty core with the source's DPI and font environment, for the projection.
fn projection_core(source: &DocumentCore, dpi: f64) -> Result<DocumentCore> {
    let mut core = DocumentCore::new_empty();
    core.set_dpi(dpi);
    core.set_font_environment(source.font_environment().cloned())
        .map_err(|e| anyhow!("{e}"))?;
    Ok(core)
}

impl PageFieldLayout {
    pub(crate) fn from_core(core: DocumentCore) -> Self {
        Self {
            core,
            fields: Vec::new(),
            repeats: Vec::new(),
            view_marks: Default::default(),
            geometry: GeometryCache::default(),
            geometry_revision: 0,
        }
    }
    pub(crate) fn field_count(&self) -> usize {
        self.fields.len()
    }
    pub fn first_position(&self, page: u32) -> Result<Option<Cursor>> {
        Ok(self
            .geometry
            .first_body_position(&self.core, self.geometry_revision, page)?
            .and_then(|position| position.legacy_cursor()))
    }

    pub fn build(source: &DocumentCore) -> Result<Option<Self>> {
        let fields = placed_fields(source)?;
        if fields.is_empty() {
            return Ok(None);
        }
        if fields.iter().all(|f| f.body_cursor().is_some()) {
            return Self::build_with(source, fields);
        }
        match Self::build_with(source, fields) {
            Ok(layout) => Ok(layout),
            // Never lose the body fields over a container this projection cannot
            // place; those keep RHWP's own drawing.
            Err(_) => {
                let body = placed_fields(source)?
                    .into_iter()
                    .filter(|f| f.body_cursor().is_some())
                    .collect::<Vec<_>>();
                if body.is_empty() {
                    Ok(None)
                } else {
                    Self::build_with(source, body)
                }
            }
        }
    }

    fn build_with(source: &DocumentCore, fields: Vec<Field>) -> Result<Option<Self>> {
        let dpi = crate::nested_presentation::source_dpi(source)?;
        // The first pass measures the source itself: it is already laid out with the
        // same DPI and fonts, and an unmodified clone would only repaginate the same
        // document (a full layout of every page on each edit). The projection core is
        // created once the field widths are known.
        let mut projected: Option<DocumentCore> = None;
        let mut last = Vec::new();
        // Reserved widths the current projection was built with.
        let mut built: Option<Vec<f64>> = None;
        // Only one source generation owns this cache. A changed style/font
        // document always starts a new build and measures again.
        let mut measurements = Measurements::new();
        let geometry = GeometryCache::default();
        for geometry_revision in 0..8 {
            let current = projected.as_ref().unwrap_or(source);
            let (state, displays) = measure_fields(
                source,
                current,
                &geometry,
                geometry_revision,
                &fields,
                &mut measurements,
                dpi,
            )?;
            // The projection depends only on the reserved widths. When the widths
            // measured on it equal the widths it was built with, rebuilding would lay
            // out the same document again: it is final, with this pass's values
            // (e.g. 380 → 381 total pages keeps the same three-digit width).
            let widths: Vec<f64> = displays.iter().map(|d| d.width).collect();
            let fixed = same_widths(built.as_deref(), &widths);
            if (fixed || state == last)
                && let Some(core) = projected.take()
            {
                let repeats = repeated(
                    source,
                    &core,
                    &geometry,
                    geometry_revision,
                    &fields,
                    &mut measurements,
                    dpi,
                )?;
                return Ok(Some(Self {
                    core,
                    fields: fields.into_iter().zip(displays).collect(),
                    repeats,
                    view_marks: Default::default(),
                    geometry,
                    geometry_revision,
                }));
            }
            last = state;
            let mut doc = source.document().clone();
            // Spacer widths by paragraph and offset; a container field also gives
            // the control whose page kind the projection drops.
            type Spacers = BTreeMap<usize, ([u16; 7], Option<usize>)>;
            let mut by_paragraph: BTreeMap<&Holder, Spacers> = BTreeMap::new();
            for (field, display) in fields.iter().zip(&displays) {
                let control = (!field.holder.path.is_empty()).then_some(field.control);
                by_paragraph
                    .entry(&field.holder)
                    .or_default()
                    .insert(field.offset, (tab(display.width), control));
            }
            for (holder, widths) in by_paragraph {
                let para = holder_paragraph_mut(&mut doc, holder)
                    .ok_or_else(|| anyhow!("자동 번호 문단을 찾지 못했습니다."))?;
                let mut tabs = Vec::new();
                let mut old_tab = 0;
                let chars: Vec<char> = para
                    .text
                    .chars()
                    .enumerate()
                    .map(|(i, ch)| {
                        if let Some((width, _)) = widths.get(&i) {
                            tabs.push(*width);
                            if ch == '\t' {
                                old_tab += 1;
                            }
                            '\t'
                        } else {
                            if ch == '\t' {
                                tabs.push(
                                    para.tab_extended
                                        .get(old_tab)
                                        .copied()
                                        .unwrap_or([0, 0, 0, 0, 0, 0, 9]),
                                );
                                old_tab += 1;
                            }
                            ch
                        }
                    })
                    .collect();
                para.text = chars.into_iter().collect();
                para.tab_extended = tabs;
                para.line_segs.clear();
                para.layout_space_metrics.clear();
                para.stored_text_partition_dirty = true;
                // RHWP would draw its own decimal value over a cell or text box
                // spacer; an equation number is never drawn outside captions.
                for (_, control) in widths.values() {
                    if let Some(Control::AutoNumber(number)) =
                        control.and_then(|c| para.controls.get_mut(c))
                    {
                        number.number_type = AutoNumberType::Equation;
                    }
                }
                let section = &mut doc.sections[holder.section];
                section.raw_stream = None;
                section.raw_provenance = None;
            }
            let mut core = match projected.take() {
                Some(core) => core,
                None => projection_core(source, dpi)?,
            };
            core.set_document(doc);
            geometry.clear();
            core.reflow_linesegs_on_demand();
            projected = Some(core);
            built = Some(widths);
        }
        bail!("본문 자동 번호의 쪽 배치가 8회 안에 수렴하지 않았습니다.")
    }

    /// Bring this projection up to date after a plain-text edit of one body paragraph
    /// of `source`, using RHWP's incremental delete/insert instead of laying out the
    /// whole document again. `Ok(None)` means the edit is not one this path handles
    /// (fields or tabs touched, controls or character shapes changed, a field width
    /// changed, ...); the caller then rebuilds. Debug builds always compare the result
    /// with a fresh build and use the fresh one when they disagree.
    pub(crate) fn replay_paragraph(
        mut self,
        source: &DocumentCore,
        section: usize,
        paragraph: usize,
    ) -> Result<Option<Self>> {
        let doc = source.document();
        let proj = self.core.document();
        if doc.sections.len() != proj.sections.len()
            || doc
                .sections
                .iter()
                .zip(&proj.sections)
                .any(|(a, b)| a.paragraphs.len() != b.paragraphs.len())
        {
            return Ok(None);
        }
        let (Some(src), Some(old)) = (
            doc.sections
                .get(section)
                .and_then(|s| s.paragraphs.get(paragraph)),
            proj.sections
                .get(section)
                .and_then(|s| s.paragraphs.get(paragraph)),
        ) else {
            return Ok(None);
        };
        if src.controls.len() != old.controls.len() {
            return Ok(None);
        }
        let field_at: std::collections::BTreeSet<usize> = self
            .fields
            .iter()
            .filter_map(|(f, _)| f.body_cursor())
            .filter(|c| c.section == section && c.paragraph == paragraph)
            .map(|c| c.offset)
            .collect();
        let p: Vec<char> = old.text.chars().collect();
        let n: Vec<char> = src.text.chars().collect();
        // A projected field is a tab standing for the source placeholder.
        let same = |i: usize, j: usize| {
            p[i] == n[j] || (field_at.contains(&i) && (n[j].is_whitespace() || n[j] == '\u{0015}'))
        };
        let mut prefix = 0;
        while prefix < p.len() && prefix < n.len() && same(prefix, prefix) {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < p.len().min(n.len()) - prefix
            && same(p.len() - 1 - suffix, n.len() - 1 - suffix)
        {
            suffix += 1;
        }
        let removed = prefix..p.len() - suffix;
        let added: String = n[prefix..n.len() - suffix].iter().collect();
        if removed.is_empty() && added.is_empty() {
            return Ok(None);
        }
        if removed
            .clone()
            .any(|i| field_at.contains(&i) || p[i] == '\t')
            || added.chars().any(|c| c == '\t' || c.is_control())
        {
            return Ok(None);
        }
        if !removed.is_empty() {
            self.core
                .delete_text_native(section, paragraph, prefix, removed.len())
                .map_err(err)?;
        }
        if !added.is_empty() {
            self.core
                .insert_text_native(section, paragraph, prefix, &added)
                .map_err(err)?;
        }
        // Character shapes must match the source exactly (typing after a shape change
        // may have applied one the plain insert does not know about).
        let shapes = |p: &hwpcraft_doc::model::paragraph::Paragraph| -> Vec<(u32, u32)> {
            p.char_shapes
                .iter()
                .map(|r| (r.start_pos, r.char_shape_id))
                .collect()
        };
        let now = &self.core.document().sections[section].paragraphs[paragraph];
        if shapes(now) != shapes(src) || now.text.chars().count() != n.len() {
            return Ok(None);
        }
        let mut fields = collect_fields(source)?;
        // A body text edit moves no container field: keep the ones this
        // projection placed instead of asking RHWP for each again.
        fields.retain(|f| {
            f.body_cursor().is_some()
                || self
                    .fields
                    .iter()
                    .any(|(placed, _)| placed.holder == f.holder && placed.offset == f.offset)
        });
        if fields.len() != self.fields.len() {
            return Ok(None);
        }
        self.geometry.clear();
        self.geometry_revision += 1;
        let dpi = crate::nested_presentation::source_dpi(source)?;
        let mut measurements = Measurements::new();
        let (_, displays) = measure_fields(
            source,
            &self.core,
            &self.geometry,
            self.geometry_revision,
            &fields,
            &mut measurements,
            dpi,
        )?;
        let built: Vec<f64> = self.fields.iter().map(|(_, d)| d.width).collect();
        let measured: Vec<f64> = displays.iter().map(|d| d.width).collect();
        if !same_widths(Some(&built), &measured) {
            // A value gained or lost a digit: the reserved tabs must change.
            return Ok(None);
        }
        self.repeats = repeated(
            source,
            &self.core,
            &self.geometry,
            self.geometry_revision,
            &fields,
            &mut measurements,
            dpi,
        )?;
        self.fields = fields.into_iter().zip(displays).collect();
        if cfg!(debug_assertions) {
            let fresh = Self::build(source)?;
            if !fresh.as_ref().is_some_and(|fresh| self.agrees_with(fresh)) {
                eprintln!("page-field replay disagreed with a full rebuild; using the rebuild");
                return Ok(fresh.map(|mut fresh| {
                    fresh.view_marks = self.view_marks;
                    fresh
                }));
            }
        }
        Ok(Some(self))
    }

    /// Same pages, field positions and values, and paragraph text as `other`.
    pub(crate) fn agrees_with(&self, other: &Self) -> bool {
        let text = |l: &Self| -> Vec<String> {
            l.core
                .document()
                .sections
                .iter()
                .flat_map(|s| s.paragraphs.iter().map(|p| p.text.clone()))
                .collect()
        };
        let fields = |l: &Self| -> Vec<(Holder, usize, String)> {
            l.fields
                .iter()
                .map(|(f, d)| (f.holder.clone(), f.offset, d.value.clone()))
                .collect()
        };
        let repeats = |l: &Self| -> Vec<(usize, u32, String)> {
            l.repeats
                .iter()
                .map(|r| (r.field, r.page, r.display.value.clone()))
                .collect()
        };
        self.core.page_count() == other.core.page_count()
            && fields(self) == fields(other)
            && repeats(self) == repeats(other)
            && text(self) == text(other)
    }

    pub fn render_svg(&self, page: u32, profile: hwpcraft_doc::RenderProfile) -> Result<String> {
        let mut svg = self
            .core
            .render_page_svg_layer_with_profile_native(page, profile)
            .map_err(err)?;
        if profile.shows_editor_visuals()
            && (self.view_marks.paragraph_marks || self.view_marks.control_codes)
        {
            self.suppress_field_tab_marks(&mut svg, page)?;
        }
        let mut overlay = String::new();
        let id_pattern = regex::Regex::new(r#"\bid="([^"]+)""#)?;
        let mut draw = |index: usize, field: &Field, display: &Display, at: FieldRect| {
            let run_top = if at.caret_top {
                at.y - display.caret_y_offset
            } else {
                at.y
            };
            let x = at.x - display.origin_x;
            let y = run_top - display.origin_y;
            // Each standalone native fragment gets its own definition namespace.
            let mut fragment = display.svg.clone();
            let ids: Vec<String> = id_pattern
                .captures_iter(&fragment)
                .map(|c| c[1].to_owned())
                .collect();
            for id in ids {
                let scoped = format!("hwpcraft-field-{index}-{id}");
                fragment = fragment
                    .replace(&format!("id=\"{id}\""), &format!("id=\"{scoped}\""))
                    .replace(&format!("url(#{id})"), &format!("url(#{scoped})"));
            }
            overlay.push_str(&format!("<g data-hwpcraft-page-field=\"{}\" data-value=\"{}\" data-x=\"{}\" data-width=\"{}\" data-reserved-width=\"{}\" transform=\"translate({x},{y})\">{fragment}</g>",if field.number.number_type==AutoNumberType::TotalPage {"total"}else{"page"},xml(&display.value),at.x,display.width,at.width));
        };
        for (index, (field, display)) in self.fields.iter().enumerate() {
            let at = field_rect(&self.core, &self.geometry, self.geometry_revision, field)?;
            if at.page == page {
                draw(index, field, display, at);
            }
        }
        for repeat in self.repeats.iter().filter(|r| r.page == page) {
            let at = FieldRect {
                page,
                x: repeat.x,
                y: repeat.y,
                width: repeat.width,
                caret_top: false,
            };
            draw(
                repeat.field,
                &self.fields[repeat.field].0,
                &repeat.display,
                at,
            );
        }
        let end = svg
            .rfind("</svg>")
            .ok_or_else(|| anyhow!("번호 표시 SVG를 읽지 못했습니다."))?;
        svg.insert_str(end, &overlay);
        Ok(svg)
    }

    pub fn set_view_marks(&mut self, marks: crate::ext_page::ViewMarks) {
        let mut wrapper = hwpcraft_doc::HwpDocument::create_empty();
        std::mem::swap(&mut *wrapper, &mut self.core);
        wrapper.set_show_paragraph_marks(marks.paragraph_marks);
        wrapper.set_show_control_codes(marks.control_codes);
        wrapper.set_show_transparent_borders(marks.transparent_borders);
        std::mem::swap(&mut *wrapper, &mut self.core);
        self.view_marks = marks;
        self.geometry.clear();
    }

    pub fn caret_rect(&self, c: Cursor) -> Result<Value> {
        let mut native: Value = serde_json::from_str(
            &self
                .core
                .get_cursor_rect_native(c.section, c.paragraph, c.offset)
                .map_err(err)?,
        )?;
        if let Some((field, display)) = self
            .fields
            .iter()
            .find(|(field, _)| field.body_cursor() == Some(c))
        {
            let at = field_rect(&self.core, &self.geometry, self.geometry_revision, field)?;
            native["pageIndex"] = json!(at.page);
            native["x"] = json!(at.x);
            native["y"] = json!(at.y + display.caret_y_offset);
        }
        Ok(native)
    }

    pub(crate) fn caret_with_affinity(
        &self,
        source: &DocumentCore,
        c: Cursor,
        affinity: Affinity,
        page_hint: Option<u32>,
    ) -> Result<Value> {
        let raw = if let Some(t) = c.cell {
            self.core.get_cursor_rect_in_cell_native(
                c.section,
                c.paragraph,
                t.control,
                t.index,
                t.paragraph,
                c.offset,
            )
        } else {
            self.core
                .get_cursor_rect_native(c.section, c.paragraph, c.offset)
        };
        let mut native: Value = serde_json::from_str(&raw.map_err(err)?)?;
        if let CaretLookup::Text(at) = self.geometry.caret_from_source(
            &self.core,
            source,
            self.geometry_revision,
            &c.into(),
            affinity,
            page_hint,
        )? {
            native["pageIndex"] = json!(at.page);
            native["x"] = json!(at.x);
            native["y"] = json!(at.y);
            native["height"] = json!(at.height);
            if at.boundary == crate::presentation_geometry::Boundary::Scalar
                && let Some((_, display)) = self.fields.iter().find(|(field, _)| field.is_at(c))
            {
                native["y"] = json!(at.run_bounds.y + display.caret_y_offset);
            }
        }
        Ok(native)
    }

    fn suppress_field_tab_marks(&self, svg: &mut String, page: u32) -> Result<()> {
        let marker = regex::Regex::new(r#"<text\s+[^>]*>→</text>\s*"#)?;
        let x_attr = regex::Regex::new(r#"\bx="([^"]+)""#)?;
        let y_attr = regex::Regex::new(r#"\by="([^"]+)""#)?;
        let mut bounds = Vec::new();
        for (field, _) in &self.fields {
            let caret = match field.body_cursor() {
                Some(c) => self.caret_rect(c)?,
                None => native_caret(&self.core, &field.holder, field.offset)?,
            };
            if caret["pageIndex"].as_u64() == Some(u64::from(page)) {
                bounds.push((
                    caret["x"].as_f64().unwrap_or(0.0),
                    caret["y"].as_f64().unwrap_or(0.0),
                    caret["height"].as_f64().unwrap_or(0.0),
                ));
            }
        }
        let mut remove = Vec::new();
        for m in marker.find_iter(svg) {
            let text = m.as_str();
            // Native editor arrows have no font-family; ordinary document text
            // and field glyphs do, including a user-chosen blue arrow symbol.
            if text.contains("font-family=") || !text.contains("fill=\"#0066FF\"") {
                continue;
            }
            let x = x_attr.captures(text).and_then(|c| c[1].parse::<f64>().ok());
            let y = y_attr.captures(text).and_then(|c| c[1].parse::<f64>().ok());
            if let (Some(x), Some(y)) = (x, y)
                && let Some(index) = bounds.iter().position(|(bx, by, h)| {
                    (x - bx).abs() < 0.25 && y >= by - 1.0 && y <= by + h + 1.0
                })
            {
                remove.push(m.range());
                bounds.remove(index);
            }
        }
        for range in remove.into_iter().rev() {
            svg.replace_range(range, "");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_field_session() -> Result<crate::Session> {
        let mut s = crate::Session::new()?;
        for i in 0..60 {
            s.execute(crate::Command::Insert(format!(
                "{i} 번째 문단입니다. 쪽 번호 투영을 시험합니다."
            )))?;
            s.execute(crate::Command::Split)?;
        }
        s.select(
            Cursor {
                paragraph: 3,
                offset: 2,
                ..Default::default()
            },
            false,
        )?;
        s.run(
            "insert.auto_text",
            serde_json::json!({"kind": "page_of_total"}),
        )?;
        Ok(s)
    }

    /// Typing and deleting replayed into the projection gives what a full rebuild gives.
    #[test]
    fn replayed_typing_and_deleting_match_a_full_rebuild() -> Result<()> {
        let mut s = long_field_session()?;
        let mut layout = PageFieldLayout::build(&s.core)?.ok_or_else(|| anyhow!("no fields"))?;
        let edits: [(usize, usize, Option<&str>); 4] = [
            (3, 0, Some("앞")),      // same paragraph, before the field
            (3, 1, None),            // delete it again
            (40, 5, Some("가나다")), // later paragraph
            (40, 5, None),
        ];
        for (paragraph, offset, text) in edits {
            s.select(
                Cursor {
                    paragraph,
                    offset,
                    ..Default::default()
                },
                false,
            )?;
            match text {
                Some(t) => s.execute(crate::Command::Insert(t.into()))?,
                None => s.execute(crate::Command::Backspace)?,
            }
            let revision = layout.geometry_revision;
            layout = layout
                .replay_paragraph(&s.core, 0, paragraph)?
                .ok_or_else(|| anyhow!("plain edit at {paragraph}:{offset} not replayed"))?;
            assert!(layout.geometry_revision > revision, "replayed, not rebuilt");
            let fresh = PageFieldLayout::build(&s.core)?.ok_or_else(|| anyhow!("no fields"))?;
            assert!(layout.agrees_with(&fresh), "edit at {paragraph}:{offset}");
        }
        Ok(())
    }

    /// Edits the replay does not understand are declined, never applied half-way.
    #[test]
    fn replay_declines_field_and_structure_edits() -> Result<()> {
        let mut s = long_field_session()?;
        let layout = PageFieldLayout::build(&s.core)?.ok_or_else(|| anyhow!("no fields"))?;
        // Deleting the page-number placeholder itself.
        let field = layout.fields[0]
            .0
            .body_cursor()
            .ok_or_else(|| anyhow!("body field"))?;
        s.select(
            Cursor {
                offset: field.offset + 1,
                ..field
            },
            false,
        )?;
        s.execute(crate::Command::Backspace)?;
        assert!(
            layout
                .replay_paragraph(&s.core, 0, field.paragraph)?
                .is_none()
        );
        // A paragraph split changes the paragraph count.
        let mut s = long_field_session()?;
        let layout = PageFieldLayout::build(&s.core)?.ok_or_else(|| anyhow!("no fields"))?;
        s.select(
            Cursor {
                paragraph: 10,
                offset: 1,
                ..Default::default()
            },
            false,
        )?;
        s.execute(crate::Command::Split)?;
        assert!(layout.replay_paragraph(&s.core, 0, 10)?.is_none());
        Ok(())
    }

    #[test]
    fn projection_is_final_once_its_widths_repeat() {
        assert!(!same_widths(None, &[9.2]));
        assert!(same_widths(Some(&[9.2, 27.5]), &[9.2, 27.500_000_1]));
        assert!(!same_widths(Some(&[9.2]), &[18.0]));
        assert!(!same_widths(Some(&[9.2]), &[9.2, 9.2]));
    }

    #[test]
    fn fields_settle_after_one_projection_with_projected_totals() -> Result<()> {
        let mut s = crate::Session::new()?;
        for i in 0..40 {
            s.execute(crate::Command::Insert(format!("{i} 번째 문단입니다.")))?;
            s.execute(crate::Command::Split)?;
        }
        s.run(
            "insert.auto_text",
            serde_json::json!({"kind": "page_of_total"}),
        )?;
        let layout = PageFieldLayout::build(&s.core)?.ok_or_else(|| anyhow!("no fields"))?;
        // One projection: the source pass, then a single re-layout with the widths.
        assert_eq!(layout.geometry_revision, 1);
        let total = layout.core.page_count().to_string();
        assert!(
            layout
                .fields
                .iter()
                .any(|(f, d)| f.number.number_type == AutoNumberType::TotalPage && d.value == total),
            "total pages must come from the projection"
        );
        Ok(())
    }
    #[test]
    fn public_number_shapes_are_not_the_renderer_legacy_enum() -> Result<()> {
        for (format, expected) in [
            (0, "2"),
            (1, "②"),
            (2, "II"),
            (3, "ii"),
            (4, "B"),
            (5, "b"),
            (6, "Ⓑ"),
            (7, "ⓑ"),
            (8, "나"),
            (9, "㉯"),
            (10, "ㄴ"),
            (11, "㉡"),
            (12, "이"),
            (13, "二"),
            (14, "㊁"),
        ] {
            assert_eq!(
                number_text(
                    2,
                    &AutoNumber {
                        format,
                        ..Default::default()
                    }
                )?,
                expected,
                "format={format}"
            );
        }
        for format in [18, 0x81] {
            assert_eq!(
                number_text(
                    100,
                    &AutoNumber {
                        format,
                        user_symbol: '★',
                        prefix_char: '[',
                        suffix_char: ']',
                        ..Default::default()
                    }
                )?,
                "[★]"
            );
        }
        assert!(
            number_text(
                1,
                &AutoNumber {
                    format: 0x80,
                    ..Default::default()
                }
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn field_wrap_uses_its_owning_run_instead_of_previous_line_caret_affinity() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("가".repeat(43)))?;
        s.run(
            "insert.auto_text",
            serde_json::json!({"kind":"page_number"}),
        )?;
        s.execute(Command::Insert("끝".into()))?;
        s.run("page.new_number", serde_json::json!({"start":123}))?;
        let layout =
            PageFieldLayout::build(&s.core)?.ok_or_else(|| anyhow!("expected field layout"))?;
        let c = layout.fields[0]
            .0
            .body_cursor()
            .ok_or_else(|| anyhow!("body field"))?;
        let at = field_rect(
            &layout.core,
            &layout.geometry,
            layout.geometry_revision,
            &layout.fields[0].0,
        )?;
        assert!(at.x < 200.0, "field must start on the next line");
        assert!(at.y > 140.0);
        let caret = layout.caret_rect(c)?;
        assert!(caret["x"].as_f64().is_some_and(|x| x < 200.0));
        assert!(caret["y"].as_f64().is_some_and(|y| y > 140.0));
        Ok(())
    }
}
