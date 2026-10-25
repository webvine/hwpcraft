//! Explicit, validated editing addresses for body and arbitrarily nested cells.
//!
//! Indices are valid for one document generation. Clients can pass the returned
//! generation to refuse a write after intervening edits. The legacy UI Cursor
//! API remains compatible; commands never pretend to select an invisible cell.
//! Structural edits rebase existing visible caret/selection positions.
use super::*;
use serde::{Deserialize, Serialize};

const MAX_DEPTH: usize = 64;
const MAX_LIST: usize = 100_000;
const MAX_TEXT: usize = 1_000_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellStep {
    pub control: usize,
    pub cell: usize,
    pub paragraph: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentAddress {
    pub section: usize,
    pub paragraph: usize,
    #[serde(default)]
    pub path: Vec<CellStep>,
}

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("context.list", "본문·중첩 표 문단 주소 목록", ""),
    ("context.get", "문서 문맥의 텍스트·보호·서식 조회", ""),
    ("context.hit_test", "화면 좌표에서 중첩 문단 주소 찾기", ""),
    ("context.table.create", "문서 문맥 안에 새 표 만들기", ""),
    ("context.table.delete", "문서 문맥 안의 표 삭제", ""),
    ("context.replace", "본문·중첩 셀 텍스트 범위 교체", ""),
    ("context.split", "본문·중첩 셀 문단 나누기", ""),
    ("context.merge", "앞 문단과 합치기 (같은 문맥)", ""),
    ("context.character", "본문·중첩 셀 글자 모양", ""),
    ("context.paragraph", "본문·중첩 셀 문단 모양", ""),
];
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "context.table.create",
        "address of body/cell paragraph, rows/columns, position before|after (default after), width_mm?/height_mm?/column_widths_mm?, expected_generation?. Inserts a separate inline-table host paragraph; returns first cell address. Default width fits the containing cell.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"rows":2,"columns":2}"#,
    ),
    (
        "context.table.delete",
        "address of table-host paragraph, control index, allow_nonempty (default false), expected_generation?. Refuses any protected/locked descendant; nonempty deletion must be explicit.",
        r#"{"address":{"section":0,"paragraph":0,"path":[]},"control":2,"allow_nonempty":false}"#,
    ),
    (
        "context.hit_test",
        "page (0-based), x/y in unscaled page pixels; returns the complete validated table-cell path, Unicode scalar offset and generation.",
        r#"{"page":0,"x":120.0,"y":150.0}"#,
    ),
    (
        "context.list",
        "start>=0, limit 1–1000; body and nested-cell addresses, generation and protection. Addresses use Unicode scalar offsets and must be rediscovered after structural edits.",
        r#"{"start":0,"limit":100}"#,
    ),
    (
        "context.get",
        "address:{section,paragraph,path:[{control,cell,paragraph},...]}; path is empty for body. Returns text (up to 1 MB), lengths, format and generation.",
        r#"{"address":{"section":0,"paragraph":0,"path":[]}}"#,
    ),
    (
        "context.replace",
        "address, start/end Unicode scalar range, text (single paragraph, <=1 MB), expected_generation?; no selection change. Use split/merge for paragraph boundaries. Refuses deletion of inline control anchors.",
        r#"{"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},"start":0,"end":0,"text":"입력"}"#,
    ),
    (
        "context.split",
        "address, offset in Unicode scalars, expected_generation?; returns the next paragraph address. Existing visible caret/selection follows its text/table after structural edits.",
        r#"{"address":{"section":0,"paragraph":0,"path":[]},"offset":3}"#,
    ),
    (
        "context.merge",
        "address of the later paragraph, expected_generation?; merges with previous paragraph in the same body/cell and returns its address/end of old prefix.",
        r#"{"address":{"section":0,"paragraph":1,"path":[]}}"#,
    ),
    (
        "context.character",
        "address, start/end (defaults whole paragraph), format matching format.character, expected_generation?; targets the deepest cell.",
        r#"{"address":{"section":0,"paragraph":0,"path":[]},"format":{"bold":true}}"#,
    ),
    (
        "context.paragraph",
        "address, format matching format.paragraph, expected_generation?; updates only the addressed paragraph.",
        r#"{"address":{"section":0,"paragraph":0,"path":[]},"format":{"alignment":"center"}}"#,
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    #[serde(default)]
    start: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    100
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    address: DocumentAddress,
    #[serde(default)]
    offset: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hit {
    page: u32,
    x: f64,
    y: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replace {
    address: DocumentAddress,
    start: usize,
    end: usize,
    text: String,
    #[serde(default)]
    expected_generation: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Split {
    address: DocumentAddress,
    offset: usize,
    #[serde(default)]
    expected_generation: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Merge {
    address: DocumentAddress,
    #[serde(default)]
    expected_generation: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    address: DocumentAddress,
    #[serde(default)]
    start: Option<usize>,
    #[serde(default)]
    end: Option<usize>,
    format: Value,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTable {
    address: DocumentAddress,
    rows: u16,
    columns: u16,
    #[serde(default)]
    position: Placement,
    #[serde(default)]
    width_mm: Option<f64>,
    #[serde(default)]
    height_mm: Option<f64>,
    #[serde(default)]
    column_widths_mm: Option<Vec<f64>>,
    #[serde(default)]
    expected_generation: Option<u64>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Placement {
    Before,
    #[default]
    After,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteTable {
    address: DocumentAddress,
    control: usize,
    #[serde(default)]
    allow_nonempty: bool,
    #[serde(default)]
    expected_generation: Option<u64>,
}

impl DocumentAddress {
    /// Bridge the existing body/one-level cursor without changing its contract.
    pub fn from_cursor(cursor: Cursor) -> Self {
        Self {
            section: cursor.section,
            paragraph: cursor.paragraph,
            path: cursor
                .cell
                .map(|cell| {
                    vec![CellStep {
                        control: cell.control,
                        cell: cell.index,
                        paragraph: cell.paragraph,
                    }]
                })
                .unwrap_or_default(),
        }
    }
    /// A nested address cannot be represented by the legacy cursor; callers
    /// must keep the explicit address instead of silently dropping path steps.
    pub fn legacy_cursor(&self, offset: usize) -> Option<Cursor> {
        let cell = match self.path.as_slice() {
            [] => None,
            [step] => Some(Cell {
                control: step.control,
                index: step.cell,
                paragraph: step.paragraph,
            }),
            _ => return None,
        };
        Some(Cursor {
            section: self.section,
            paragraph: self.paragraph,
            offset,
            cell,
        })
    }
    fn native_path(&self) -> Vec<(usize, usize, usize)> {
        self.path
            .iter()
            .map(|step| (step.control, step.cell, step.paragraph))
            .collect()
    }
    fn index(&self) -> usize {
        self.path.last().map_or(self.paragraph, |p| p.paragraph)
    }
    fn with_index(&self, index: usize) -> Self {
        let mut address = self.clone();
        if let Some(last) = address.path.last_mut() {
            last.paragraph = index;
        } else {
            address.paragraph = index;
        }
        address
    }
}

/// Resolve once, validating every ancestor rather than trusting the native API's
/// permissive/clamping text functions. Protected ancestors guard their descendants.
fn resolve<'a>(
    doc: &'a hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
) -> Result<(&'a Paragraph, bool)> {
    if address.path.len() > MAX_DEPTH {
        bail!("문서 문맥은 최대 {MAX_DEPTH}단계까지 지원합니다.");
    }
    let mut para = doc
        .sections
        .get(address.section)
        .and_then(|s| s.paragraphs.get(address.paragraph))
        .ok_or_else(|| anyhow!("본문 문단 주소가 범위를 벗어났습니다."))?;
    let mut protected = false;
    for (depth, step) in address.path.iter().enumerate() {
        let Some(Control::Table(table)) = para.controls.get(step.control) else {
            bail!("문맥 경로 {depth}: 지정한 컨트롤이 표가 아닙니다.");
        };
        let cell = table
            .cells
            .get(step.cell)
            .ok_or_else(|| anyhow!("문맥 경로 {depth}: 셀이 없습니다."))?;
        protected |= table.common.locked || cell.cell_protect();
        para = cell
            .paragraphs
            .get(step.paragraph)
            .ok_or_else(|| anyhow!("문맥 경로 {depth}: 셀 문단이 없습니다."))?;
    }
    Ok((para, protected))
}

fn resolve_mut<'a>(
    doc: &'a mut hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
) -> Result<&'a mut Paragraph> {
    let mut para = doc
        .sections
        .get_mut(address.section)
        .and_then(|s| s.paragraphs.get_mut(address.paragraph))
        .ok_or_else(|| anyhow!("본문 문단 주소가 범위를 벗어났습니다."))?;
    for step in &address.path {
        let Some(Control::Table(table)) = para.controls.get_mut(step.control) else {
            bail!("문맥 컨트롤이 표가 아닙니다.");
        };
        para = table
            .cells
            .get_mut(step.cell)
            .and_then(|cell| cell.paragraphs.get_mut(step.paragraph))
            .ok_or_else(|| anyhow!("문맥 셀 문단이 없습니다."))?;
    }
    Ok(para)
}

fn container_mut<'a>(
    doc: &'a mut hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
) -> Result<&'a mut Vec<Paragraph>> {
    let section = doc
        .sections
        .get_mut(address.section)
        .ok_or_else(|| anyhow!("구역이 없습니다."))?;
    if address.path.is_empty() {
        return Ok(&mut section.paragraphs);
    }
    fn descend<'a>(para: &'a mut Paragraph, steps: &[CellStep]) -> Result<&'a mut Vec<Paragraph>> {
        let step = steps
            .first()
            .ok_or_else(|| anyhow!("셀 경로가 없습니다."))?;
        let Some(Control::Table(table)) = para.controls.get_mut(step.control) else {
            bail!("문맥 컨트롤이 표가 아닙니다.");
        };
        let cell = table
            .cells
            .get_mut(step.cell)
            .ok_or_else(|| anyhow!("셀 주소가 범위를 벗어났습니다."))?;
        if steps.len() == 1 {
            return Ok(&mut cell.paragraphs);
        }
        let child = cell
            .paragraphs
            .get_mut(step.paragraph)
            .ok_or_else(|| anyhow!("셀 문단이 없습니다."))?;
        descend(child, &steps[1..])
    }
    let para = section
        .paragraphs
        .get_mut(address.paragraph)
        .ok_or_else(|| anyhow!("본문 문단이 없습니다."))?;
    descend(para, &address.path)
}

fn containing_width(
    doc: &hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
) -> Result<Option<f64>> {
    let mut para = doc
        .sections
        .get(address.section)
        .and_then(|section| section.paragraphs.get(address.paragraph))
        .ok_or_else(|| anyhow!("본문 문단이 없습니다."))?;
    let mut available = None;
    for step in &address.path {
        let Some(Control::Table(table)) = para.controls.get(step.control) else {
            bail!("문맥 컨트롤이 표가 아닙니다.");
        };
        let cell = table
            .cells
            .get(step.cell)
            .ok_or_else(|| anyhow!("셀이 없습니다."))?;
        let padding = cell.effective_padding(&table.padding);
        let width = (i64::from(cell.width)
            - i64::from(padding.left.max(0))
            - i64::from(padding.right.max(0)))
        .max(0);
        available = Some(width as f64 * 25.4 / 7200.0);
        para = cell
            .paragraphs
            .get(step.paragraph)
            .ok_or_else(|| anyhow!("셀 문단이 없습니다."))?;
    }
    Ok(available)
}

fn table_has_content(table: &hwpcraft_doc::model::table::Table) -> bool {
    table.caption.as_ref().is_some_and(|caption| {
        caption
            .paragraphs
            .iter()
            .any(|para| !para.text.is_empty() || !para.controls.is_empty())
    }) || table
        .cells
        .iter()
        .flat_map(|cell| &cell.paragraphs)
        .any(|para| !para.text.is_empty() || !para.controls.is_empty())
}

fn rebase_inserted_paragraph(cursor: &mut Cursor, address: &DocumentAddress, index: usize) {
    if cursor.section != address.section {
        return;
    }
    match address.path.as_slice() {
        [] if cursor.paragraph >= index => cursor.paragraph += 1,
        [step] if cursor.paragraph == address.paragraph => {
            if let Some(cell) = &mut cursor.cell
                && cell.control == step.control
                && cell.index == step.cell
                && cell.paragraph >= index
            {
                cell.paragraph += 1;
            }
        }
        _ => {}
    }
}

fn rebase_deleted_table(
    cursor: &mut Cursor,
    host: &DocumentAddress,
    control: usize,
    offset: usize,
) -> bool {
    if !host.path.is_empty() || cursor.section != host.section || cursor.paragraph != host.paragraph
    {
        return false;
    }
    if let Some(cell) = &mut cursor.cell {
        if cell.control == control {
            cursor.cell = None;
            cursor.offset = offset;
            return true;
        }
        if cell.control > control {
            cell.control -= 1;
        }
    }
    false
}

#[derive(Clone, Copy)]
enum ParagraphChange {
    Split { offset: usize },
    Merge { prefix: usize },
}

/// A legacy cell caret must follow its actual table if splitting/merging a
/// body paragraph relocates that table or changes its control index.
#[derive(Clone, Copy)]
struct ContextCaret {
    cursor: Cursor,
    table_instance: Option<u32>,
}

impl ContextCaret {
    fn capture(session: &Session, cursor: Cursor) -> Result<Self> {
        checked_range(session.paragraph(cursor)?, cursor.offset, cursor.offset)?;
        let table_instance = if let Some(cell) = cursor.cell {
            let Some(Control::Table(table)) = session
                .core
                .document()
                .sections
                .get(cursor.section)
                .and_then(|section| section.paragraphs.get(cursor.paragraph))
                .and_then(|para| para.controls.get(cell.control))
            else {
                bail!("현재 선택의 표 주소가 올바르지 않습니다.");
            };
            Some(table.common.instance_id)
        } else {
            None
        };
        Ok(Self {
            cursor,
            table_instance,
        })
    }

    fn rebase(
        mut self,
        session: &Session,
        address: &DocumentAddress,
        change: ParagraphChange,
    ) -> Result<Cursor> {
        if self.cursor.section != address.section {
            return Ok(self.cursor);
        }
        let current = DocumentAddress::from_cursor(self.cursor);
        let same_cell = match (current.path.as_slice(), address.path.as_slice()) {
            ([a], [b]) => {
                current.paragraph == address.paragraph && a.control == b.control && a.cell == b.cell
            }
            _ => false,
        };
        if !address.path.is_empty() && !same_cell {
            return Ok(self.cursor);
        }
        let index = address.index();
        let current_index = if same_cell {
            current.index()
        } else {
            self.cursor.paragraph
        };
        let table_moved = self.cursor.cell.is_some()
            && address.path.is_empty()
            && match change {
                ParagraphChange::Split { .. } => current_index == index,
                ParagraphChange::Merge { .. } => {
                    current_index == index || current_index + 1 == index
                }
            };
        if table_moved {
            let range = match change {
                ParagraphChange::Split { .. } => index..=index + 1,
                ParagraphChange::Merge { .. } => index - 1..=index - 1,
            };
            let instance = self
                .table_instance
                .ok_or_else(|| anyhow!("현재 선택의 표 식별자가 없습니다."))?;
            let mut matches = Vec::new();
            for paragraph in range {
                for (control, node) in session.core.document().sections[address.section].paragraphs
                    [paragraph]
                    .controls
                    .iter()
                    .enumerate()
                {
                    if matches!(node, Control::Table(table) if table.common.instance_id == instance)
                    {
                        matches.push((paragraph, control));
                    }
                }
            }
            let [(paragraph, control)] = matches.as_slice() else {
                bail!("이동한 표의 선택을 고유하게 복원할 수 없습니다.");
            };
            self.cursor.paragraph = *paragraph;
            if let Some(cell) = &mut self.cursor.cell {
                cell.control = *control;
            }
            return Ok(self.cursor);
        }
        let (next_index, next_offset) = match change {
            ParagraphChange::Split { offset } if current_index == index => {
                if self.cursor.offset >= offset {
                    (index + 1, self.cursor.offset - offset)
                } else {
                    (index, self.cursor.offset)
                }
            }
            ParagraphChange::Split { .. } if current_index > index => {
                (current_index + 1, self.cursor.offset)
            }
            ParagraphChange::Merge { prefix } if current_index == index => {
                (index - 1, prefix + self.cursor.offset)
            }
            ParagraphChange::Merge { .. } if current_index > index => {
                (current_index - 1, self.cursor.offset)
            }
            _ => (current_index, self.cursor.offset),
        };
        if same_cell {
            if let Some(cell) = &mut self.cursor.cell {
                cell.paragraph = next_index;
            }
        } else {
            self.cursor.paragraph = next_index;
        }
        self.cursor.offset = next_offset;
        Ok(self.cursor)
    }
}

fn table_contains_protection(table: &hwpcraft_doc::model::table::Table) -> bool {
    use hwpcraft_doc::model::shape::{Caption, ShapeObject};
    fn paragraphs(paras: &[Paragraph], depth: usize) -> bool {
        depth > MAX_DEPTH
            || paras
                .iter()
                .flat_map(|para| &para.controls)
                .any(|control| protected(control, depth + 1))
    }
    fn caption(value: Option<&Caption>, depth: usize) -> bool {
        value.is_some_and(|value| paragraphs(&value.paragraphs, depth + 1))
    }
    fn shape(value: &ShapeObject, depth: usize) -> bool {
        if depth > MAX_DEPTH || value.common().locked {
            return true;
        }
        if let Some(drawing) = value.drawing()
            && (drawing
                .text_box
                .as_ref()
                .is_some_and(|text| paragraphs(&text.paragraphs, depth + 1))
                || caption(drawing.caption.as_ref(), depth))
        {
            return true;
        }
        match value {
            ShapeObject::Group(group) => {
                caption(group.caption.as_ref(), depth)
                    || group.children.iter().any(|child| shape(child, depth + 1))
            }
            ShapeObject::Picture(picture) => caption(picture.caption.as_ref(), depth),
            ShapeObject::Chart(chart) => caption(chart.caption.as_ref(), depth),
            ShapeObject::Ole(ole) => caption(ole.caption.as_ref(), depth),
            _ => false,
        }
    }
    fn protected(control: &Control, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return true;
        }
        match control {
            Control::Table(table) => visit(table, depth),
            Control::Shape(value) => shape(value, depth),
            Control::Picture(picture) => {
                picture.common.locked || caption(picture.caption.as_ref(), depth)
            }
            Control::Equation(equation) => equation.common.locked,
            Control::Form(form) => form.common.locked,
            Control::Header(header) => paragraphs(&header.paragraphs, depth + 1),
            Control::Footer(footer) => paragraphs(&footer.paragraphs, depth + 1),
            Control::Footnote(note) => paragraphs(&note.paragraphs, depth + 1),
            Control::Endnote(note) => paragraphs(&note.paragraphs, depth + 1),
            Control::HiddenComment(comment) => paragraphs(&comment.paragraphs, depth + 1),
            Control::Field(field) => paragraphs(&field.memo_paragraphs, depth + 1),
            _ => false,
        }
    }
    fn visit(table: &hwpcraft_doc::model::table::Table, depth: usize) -> bool {
        depth > MAX_DEPTH
            || table.common.locked
            || caption(table.caption.as_ref(), depth)
            || table
                .cells
                .iter()
                .any(|cell| cell.cell_protect() || paragraphs(&cell.paragraphs, depth + 1))
    }
    visit(table, 0)
}

pub(super) fn checked_range(para: &Paragraph, start: usize, end: usize) -> Result<()> {
    let length = para.text.chars().count();
    if start > end || end > length {
        bail!("문자 범위 {start}–{end}가 문단({length}자)을 벗어났습니다.");
    }
    Ok(())
}

pub(super) fn guard_inline_deletion(para: &Paragraph, start: usize, end: usize) -> Result<()> {
    if para
        .controls
        .iter()
        .zip(para.control_text_positions())
        .any(|(control, position)| {
            (control.is_logical_inline()
                || matches!(control, Control::AutoNumber(_) | Control::CharOverlap(_)))
                && start <= position
                && position < end
        })
    {
        bail!(
            "인라인 개체 자리까지 지우는 문맥 텍스트 편집은 지원하지 않습니다. 개체 삭제 명령을 사용하세요."
        );
    }
    Ok(())
}

pub(super) fn logical_split_offset(para: &Paragraph, offset: usize) -> usize {
    offset
        + para
            .controls
            .iter()
            .zip(para.control_text_positions())
            .filter(|(control, position)| {
                *position < offset
                    && matches!(
                        control,
                        Control::Shape(_)
                            | Control::Table(_)
                            | Control::Picture(_)
                            | Control::Equation(_)
                            | Control::Footnote(_)
                            | Control::Endnote(_)
                            | Control::AutoNumber(_)
                            | Control::CharOverlap(_)
                    )
            })
            .count()
}

impl Session {
    /// A nested insertion changes the minimum content height of every ancestor
    /// cell. RHWP intentionally trusts stored inline-table frames, so rebuilding
    /// caches alone cannot make an old short frame contain a new child table.
    /// Grow only affected rows, never shrink user-declared minima. All callers
    /// run this inside the same source transaction as the initiating edit.
    pub(crate) fn context_reflow_ancestors(&mut self, address: &DocumentAddress) -> Result<()> {
        if address.path.is_empty() {
            return Ok(());
        }
        for depth in (1..=address.path.len()).rev() {
            let mut cell_address = address.clone();
            cell_address.path.truncate(depth);
            let step = &address.path[depth - 1];
            let mut parent = address.clone();
            parent.path.truncate(depth - 1);
            let count = {
                let para = resolve(self.core.document(), &parent)?.0;
                let Some(Control::Table(table)) = para.controls.get(step.control) else {
                    bail!("조상 표 주소가 올바르지 않습니다.");
                };
                table
                    .cells
                    .get(step.cell)
                    .ok_or_else(|| anyhow!("조상 셀이 없습니다."))?
                    .paragraphs
                    .len()
            };
            // Empty native insertion changes no text/field/control content but
            // publishes line metrics and a fresh vpos ladder at the real width.
            for paragraph in 0..count {
                cell_address.path[depth - 1].paragraph = paragraph;
                self.core
                    .insert_text_in_cell_by_path(
                        cell_address.section,
                        cell_address.paragraph,
                        &cell_address.native_path(),
                        0,
                        "",
                    )
                    .map_err(err)?;
            }
            let width_mm = containing_width(self.core.document(), &cell_address)?
                .ok_or_else(|| anyhow!("조상 셀의 내용 폭을 찾을 수 없습니다."))?;
            let width = width_mm * 7200.0 / 25.4;
            if !width.is_finite() || width <= 0.0 {
                bail!("내용 폭이 없는 셀의 중첩 편집은 지원하지 않습니다.");
            }
            let mut document = self.core.document().clone();
            let mut paragraphs = container_mut(&mut document, &cell_address)?.clone();
            for paragraph in &mut paragraphs {
                paragraph.invalidate_layout_inputs();
            }
            let count = paragraphs.len();
            let section = &mut document.sections[cell_address.section];
            section.paragraphs = paragraphs;
            section.raw_stream = None;
            section.section_def.page_def.width = width.ceil() as u32;
            section.section_def.page_def.margin_left = 0;
            section.section_def.page_def.margin_right = 0;
            section.section_def.page_def.margin_gutter = 0;
            section.section_def.page_def.landscape = false;
            let mut measurement = DocumentCore::new_empty();
            measurement.set_dpi(96.0);
            measurement.set_document(document);
            let mut content_height = 0.0;
            for paragraph in 0..count {
                let height = measurement
                    .production_paragraph_height(
                        cell_address.section,
                        paragraph,
                        Some(width / 75.0),
                    )
                    .ok_or_else(|| anyhow!("중첩 문단의 실제 높이를 측정하지 못했습니다."))?;
                if !height.is_finite() || height < 0.0 {
                    bail!("중첩 문단 높이 측정 결과가 올바르지 않습니다.");
                }
                content_height += height * 75.0;
            }
            let para = resolve_mut(self.core.document_mut(), &parent)?;
            let Some(Control::Table(table)) = para.controls.get_mut(step.control) else {
                bail!("조상 표 주소가 올바르지 않습니다.");
            };
            let target = table
                .cells
                .get(step.cell)
                .ok_or_else(|| anyhow!("조상 셀이 없습니다."))?;
            let padding = target.effective_padding(&table.padding);
            let required = content_height.ceil()
                + f64::from(padding.top.max(0))
                + f64::from(padding.bottom.max(0));
            if required > f64::from(u32::MAX) {
                bail!("중첩 셀의 필요한 높이가 문서 형식의 한도를 넘습니다.");
            }
            // Negative/automatic source sizes use the renderer's own content
            // sizing contract; do not replace their sentinel with a new value.
            if target.height >= 0x8000_0000 || required <= f64::from(target.height) {
                continue;
            }
            if table.common.size_protect {
                bail!("크기가 보호된 조상 표에는 새 내용을 담을 높이가 부족합니다.");
            }
            let delta = (required as u32).saturating_sub(target.height);
            let row = usize::from(target.row)
                .checked_add(usize::from(target.row_span).max(1) - 1)
                .ok_or_else(|| anyhow!("조상 셀의 행 범위가 올바르지 않습니다."))?;
            if row >= usize::from(table.row_count) {
                bail!("조상 셀의 행 범위가 표를 벗어났습니다.");
            }
            // A physical row is shared by all covering cells, including merged
            // spans. Preserve their existing heights and add the same growth.
            for cell in &mut table.cells {
                let start = usize::from(cell.row);
                let end = start.saturating_add(usize::from(cell.row_span).max(1));
                if start <= row && row < end && cell.height < 0x8000_0000 {
                    cell.height = cell
                        .height
                        .checked_add(delta)
                        .ok_or_else(|| anyhow!("조상 셀 높이 한도 초과"))?;
                }
            }
            table.common.height = table
                .common
                .height
                .checked_add(delta)
                .ok_or_else(|| anyhow!("조상 표 높이 한도 초과"))?;
            if table.raw_ctrl_data.len() >= 20 {
                table.raw_ctrl_data[16..20].copy_from_slice(&table.common.height.to_le_bytes());
            }
            para.invalidate_layout_inputs();
        }
        self.core.document_mut().sections[address.section].raw_stream = None;
        self.core.set_document(self.core.document().clone());
        Ok(())
    }

    fn context_selection(&self) -> Result<(ContextCaret, Option<ContextCaret>)> {
        Ok((
            ContextCaret::capture(self, self.cursor)?,
            self.anchor_cursor()
                .map(|cursor| ContextCaret::capture(self, cursor))
                .transpose()?,
        ))
    }

    fn context_rebase_selection(
        &mut self,
        selection: (ContextCaret, Option<ContextCaret>),
        address: &DocumentAddress,
        change: ParagraphChange,
    ) -> Result<()> {
        let cursor = selection.0.rebase(self, address, change)?;
        let anchor = selection
            .1
            .map(|anchor| anchor.rebase(self, address, change))
            .transpose()?;
        self.clear_selection();
        if let Some(anchor) = anchor {
            self.select(anchor, false)?;
            self.select(cursor, true)?;
        } else {
            self.select(cursor, false)?;
        }
        Ok(())
    }

    pub(crate) fn run_context_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(command, _, _)| *command == id) {
            return None;
        }
        Some((|| match id {
            "context.list" => {
                let p: List = serde_json::from_value(params.clone())?;
                self.context_list(p.start, p.limit)
            }
            "context.get" => {
                let p: Query = serde_json::from_value(params.clone())?;
                self.context_info_at(&p.address, p.offset)
            }
            "context.hit_test" => {
                let p: Hit = serde_json::from_value(params.clone())?;
                self.context_hit_test(p.page, p.x, p.y)
            }
            "context.table.create" => {
                let p: CreateTable = serde_json::from_value(params.clone())?;
                self.context_create_table(p)
            }
            "context.table.delete" => {
                let p: DeleteTable = serde_json::from_value(params.clone())?;
                self.context_delete_table(p)
            }
            "context.replace" => {
                let p: Replace = serde_json::from_value(params.clone())?;
                self.context_replace(p)
            }
            "context.split" => {
                let p: Split = serde_json::from_value(params.clone())?;
                self.context_split(p)
            }
            "context.merge" => {
                let p: Merge = serde_json::from_value(params.clone())?;
                self.context_merge(p)
            }
            "context.character" | "context.paragraph" => {
                let p: Format = serde_json::from_value(params.clone())?;
                self.context_format(p, id == "context.character")
            }
            _ => bail!("알 수 없는 문맥 명령"),
        })())
    }

    pub fn context_paragraph(&self, address: &DocumentAddress) -> Result<&Paragraph> {
        Ok(resolve(self.core.document(), address)?.0)
    }

    pub fn context_hit_test(&mut self, page: u32, x: f64, y: f64) -> Result<Value> {
        if !x.is_finite() || !y.is_finite() {
            bail!("문맥 좌표는 유한한 수여야 합니다.");
        }
        let layout = self.page_field_layout()?;
        let core = layout.as_ref().map_or(&self.core, |layout| &layout.core);
        if page >= core.page_count() {
            bail!("문맥 좌표의 쪽이 범위를 벗어났습니다.");
        }
        let hit: Value = serde_json::from_str(&core.hit_test_native(page, x, y).map_err(err)?)?;
        if hit["isTextBox"] == true {
            bail!("글상자 문맥은 아직 이 편집 주소에서 지원하지 않습니다.");
        }
        let number = |value: &Value, key: &str| {
            value[key]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| anyhow!("문맥 좌표 응답에 {key}이 없습니다."))
        };
        let mut path = Vec::new();
        if let Some(steps) = hit["cellPath"].as_array() {
            for step in steps {
                path.push(CellStep {
                    control: number(step, "controlIndex")?,
                    cell: number(step, "cellIndex")?,
                    paragraph: number(step, "cellParaIndex")?,
                });
            }
        } else if hit.get("cellIndex").is_some() {
            path.push(CellStep {
                control: number(&hit, "controlIndex")?,
                cell: number(&hit, "cellIndex")?,
                paragraph: number(&hit, "cellParaIndex")?,
            });
        }
        let address = DocumentAddress {
            section: number(&hit, "sectionIndex")?,
            paragraph: number(
                &hit,
                if path.is_empty() {
                    "paragraphIndex"
                } else {
                    "parentParaIndex"
                },
            )?,
            path,
        };
        self.context_info_at(&address, number(&hit, "charOffset")?)
    }

    fn context_writable(
        &self,
        address: &DocumentAddress,
        generation: Option<u64>,
    ) -> Result<&Paragraph> {
        if generation.is_some_and(|expected| expected != self.generation) {
            bail!("문서가 변경돼 문맥 주소를 다시 조회해야 합니다.");
        }
        let (para, protected) = resolve(self.core.document(), address)?;
        if protected {
            bail!("보호된 표/셀 안의 문단은 고칠 수 없습니다.");
        }
        Ok(para)
    }

    fn context_create_table(&mut self, p: CreateTable) -> Result<Value> {
        let target = self.context_writable(&p.address, p.expected_generation)?;
        if p.address.path.len() >= MAX_DEPTH {
            bail!("더 깊은 중첩 표는 만들 수 없습니다.");
        }
        let available = containing_width(self.core.document(), &p.address)?;
        let requested = p
            .column_widths_mm
            .as_ref()
            .map(|widths| widths.iter().sum::<f64>())
            .or(p.width_mm)
            // The shared inline table factory reserves 283 HWP units on each
            // side. Keep that outer box inside the containing cell as well.
            .or(available.map(|width| width - 566.0 * 25.4 / 7200.0));
        if let (Some(width), Some(available)) = (requested, available)
            && (!width.is_finite() || width > available + 0.01)
        {
            bail!("새 표 너비는 현재 셀의 본문 폭 {available:.2} mm 이하여야 합니다.");
        }
        // Generate using the established table command against a temporary
        // complete document. Its instance allocator sees every existing object;
        // style IDs refer to the same tables. No user clipboard is overwritten.
        let empty = Paragraph::new_empty_like(target);
        let mut document = self.core.document().clone();
        let temporary = document.sections[p.address.section].paragraphs.len();
        document.sections[p.address.section].paragraphs.push(empty);
        document.sections[p.address.section].raw_stream = None;
        let mut generator = DocumentCore::new_empty();
        generator.set_document(document);
        let mut generator = Self::with_core(generator, None);
        generator.cursor = Cursor {
            section: p.address.section,
            paragraph: temporary,
            offset: 0,
            cell: None,
        };
        let mut options = json!({"rows":p.rows,"columns":p.columns,"treat_as_char":true});
        if let Some(width) = requested {
            options["width_mm"] = json!(width);
        }
        if let Some(height) = p.height_mm {
            options["height_mm"] = json!(height);
        }
        if let Some(widths) = p.column_widths_mm {
            options["column_widths_mm"] = json!(widths);
        }
        let made = generator.run("table.create", options)?;
        let made: crate::ObjectAddress = serde_json::from_value(made["table"].clone())?;
        let host =
            generator.core.document().sections[made.section].paragraphs[made.paragraph].clone();
        let Some(Control::Table(table)) = host.controls.get(made.control) else {
            bail!("생성된 표를 찾지 못했습니다.");
        };
        if let Some(available) = available {
            let outer_width = (f64::from(table.common.width)
                + f64::from(table.common.margin.left.max(0))
                + f64::from(table.common.margin.right.max(0)))
                * 25.4
                / 7200.0;
            if outer_width > available + 0.01 {
                bail!(
                    "새 표의 바깥 여백을 포함한 너비는 현재 셀의 본문 폭 {available:.2} mm 이하여야 합니다."
                );
            }
        }
        if host.controls.len() != 1 {
            bail!("생성된 표 문단에 예상하지 못한 개체가 있습니다.");
        }
        let (rows, columns, instance) =
            (table.row_count, table.col_count, table.common.instance_id);
        let doc_info = generator.core.document().doc_info.clone();
        drop(generator);
        let index = p.address.index() + usize::from(matches!(p.position, Placement::After));
        let host_address = p.address.with_index(index);
        self.transaction(|s| {
            container_mut(s.core.document_mut(), &p.address)?.insert(index, host);
            s.core.document_mut().doc_info = doc_info;
            s.core.document_mut().sections[p.address.section].raw_stream = None;
            s.core.set_document(s.core.document().clone());
            s.context_reflow_ancestors(&host_address)?;
            rebase_inserted_paragraph(&mut s.cursor, &p.address, index);
            if let Some(anchor) = &mut s.span_anchor {
                rebase_inserted_paragraph(anchor, &p.address, index);
            }
            Ok(())
        })?;
        let mut cell = host_address.clone();
        cell.path.push(CellStep {
            control: made.control,
            cell: 0,
            paragraph: 0,
        });
        let mut result = self.context_info(&cell)?;
        result["table"] = json!({"address":host_address,"control":made.control,"rows":rows,"columns":columns,"instanceId":instance});
        Ok(result)
    }

    fn context_delete_table(&mut self, p: DeleteTable) -> Result<Value> {
        let host = self.context_writable(&p.address, p.expected_generation)?;
        let Some(Control::Table(table)) = host.controls.get(p.control) else {
            bail!("지정한 문맥 개체가 표가 아닙니다.");
        };
        if table_contains_protection(table) {
            bail!("보호된 표/셀/개체 또는 지원 깊이를 넘는 문맥을 포함한 표는 지울 수 없습니다.");
        }
        if !p.allow_nonempty && table_has_content(table) {
            bail!("내용이 있는 표입니다. 전체 내용을 지우려면 allow_nonempty:true를 명시하세요.");
        }
        let offset = host.control_text_positions()[p.control];
        let path=p.address.path.iter().map(|step|json!({"controlIndex":step.control,"cellIndex":step.cell,"cellParaIndex":step.paragraph})).collect::<Vec<_>>();
        self.transaction(|s| {
            if path.is_empty() {
                s.core
                    .delete_table_control_native(p.address.section, p.address.paragraph, p.control)
                    .map_err(err)?;
            } else {
                s.core
                    .delete_cell_table_control_by_path_native(
                        p.address.section,
                        p.address.paragraph,
                        &serde_json::to_string(&path)?,
                        p.control,
                    )
                    .map_err(err)?;
            }
            let selection_deleted =
                rebase_deleted_table(&mut s.cursor, &p.address, p.control, offset);
            if let Some(anchor) = &mut s.span_anchor {
                rebase_deleted_table(anchor, &p.address, p.control, offset);
            }
            if selection_deleted {
                s.clear_selection();
            }
            s.context_reflow_ancestors(&p.address)?;
            Ok(())
        })?;
        let mut value = self.context_info(&p.address)?;
        value["deleted"] = json!(true);
        Ok(value)
    }

    pub fn context_info(&mut self, address: &DocumentAddress) -> Result<Value> {
        self.context_info_at(address, 0)
    }

    fn context_info_at(&mut self, address: &DocumentAddress, offset: usize) -> Result<Value> {
        let (para, protected) = resolve(self.core.document(), address)?;
        checked_range(para, offset, offset)?;
        let length = para.text.chars().count();
        let shape = self
            .core
            .document()
            .doc_info
            .para_shapes
            .get(usize::from(para.para_shape_id));
        let paragraph = shape.map(|shape| json!({
            "alignment":format!("{:?}",shape.alignment).to_ascii_lowercase(),
            "lineSpacing":shape.line_spacing,"lineSpacingType":format!("{:?}",shape.line_spacing_type),
            "marginLeft":shape.margin_left,"marginRight":shape.margin_right,"indent":shape.indent,
            "spacingBefore":shape.spacing_before,"spacingAfter":shape.spacing_after,
        }));
        let mut value = json!({"address":address,"text":para.text.chars().take(MAX_TEXT).collect::<String>(),
            "length":length,"textTruncated":length > MAX_TEXT,"protected":protected,"controls":para.controls.len(),
            "paragraphShapeId":para.para_shape_id,"generation":self.generation,"offset":offset,"offsetUnit":"unicode_scalar",
            "paragraph":paragraph});
        let path = address.native_path();
        let raw = if path.is_empty() {
            self.core
                .get_char_properties_at_native(address.section, address.paragraph, offset)
        } else {
            self.core.get_cell_char_properties_at_by_path(
                address.section,
                address.paragraph,
                &path,
                offset,
            )
        }
        .map_err(err)?;
        value["character"] = serde_json::from_str(&raw)?;
        Ok(value)
    }

    fn context_list(&self, start: usize, limit: usize) -> Result<Value> {
        if !(1..=1000).contains(&limit) {
            bail!("문맥 목록 limit은 1–1000이어야 합니다.");
        }
        fn visit(
            para: &Paragraph,
            address: DocumentAddress,
            protected: bool,
            rows: &mut Vec<Value>,
            truncated: &mut bool,
        ) {
            if rows.len() >= MAX_LIST {
                *truncated = true;
                return;
            }
            rows.push(json!({"address":address,"length":para.text.chars().count(),"text":para.text.chars().take(160).collect::<String>(),"protected":protected,"depth":address.path.len()}));
            if address.path.len() >= MAX_DEPTH {
                *truncated |= para.controls.iter().any(|c| matches!(c, Control::Table(_)));
                return;
            }
            for (control, child) in para.controls.iter().enumerate() {
                if let Control::Table(table) = child {
                    for (cell_index, cell) in table.cells.iter().enumerate() {
                        for (paragraph, para) in cell.paragraphs.iter().enumerate() {
                            let mut child_address = address.clone();
                            child_address.path.push(CellStep {
                                control,
                                cell: cell_index,
                                paragraph,
                            });
                            visit(
                                para,
                                child_address,
                                protected || table.common.locked || cell.cell_protect(),
                                rows,
                                truncated,
                            );
                            if rows.len() >= MAX_LIST {
                                *truncated = true;
                                return;
                            }
                        }
                    }
                }
            }
        }
        let mut rows = Vec::new();
        let mut truncated = false;
        for (section, sec) in self.core.document().sections.iter().enumerate() {
            for (paragraph, para) in sec.paragraphs.iter().enumerate() {
                visit(
                    para,
                    DocumentAddress {
                        section,
                        paragraph,
                        path: vec![],
                    },
                    false,
                    &mut rows,
                    &mut truncated,
                );
                if rows.len() >= MAX_LIST {
                    break;
                }
            }
            if rows.len() >= MAX_LIST {
                break;
            }
        }
        let total = rows.len();
        let end = start.saturating_add(limit).min(total);
        let items = if start < total {
            rows.drain(start..end).collect::<Vec<_>>()
        } else {
            vec![]
        };
        Ok(
            json!({"items":items,"start":start,"total":total,"nextStart":(end < total).then_some(end),"truncated":truncated,"generation":self.generation,"offsetUnit":"unicode_scalar"}),
        )
    }

    fn context_replace(&mut self, p: Replace) -> Result<Value> {
        let para = self.context_writable(&p.address, p.expected_generation)?;
        checked_range(para, p.start, p.end)?;
        guard_inline_deletion(para, p.start, p.end)?;
        if p.text.len() > MAX_TEXT
            || p.text.contains(['\r', '\n'])
            || p.text.chars().any(|c| c.is_control() && c != '\t')
        {
            bail!(
                "문맥 교체 텍스트는 줄바꿈/제어 문자 없는 1 MB 이하 문자열이어야 합니다. 문단은 split/merge로 편집하세요."
            );
        }
        let before: String = para
            .text
            .chars()
            .skip(p.start)
            .take(p.end - p.start)
            .collect();
        if before == p.text {
            return self.context_info(&p.address);
        }
        let path = p.address.native_path();
        self.transaction(|s| {
            if p.start != p.end {
                if path.is_empty() {
                    s.core
                        .delete_range_native(
                            p.address.section,
                            p.address.paragraph,
                            p.start,
                            p.address.paragraph,
                            p.end,
                            None,
                        )
                        .map_err(err)?;
                } else {
                    s.core
                        .delete_text_in_cell_by_path(
                            p.address.section,
                            p.address.paragraph,
                            &path,
                            p.start,
                            p.end - p.start,
                        )
                        .map_err(err)?;
                }
            }
            if !p.text.is_empty() {
                if path.is_empty() {
                    s.core
                        .insert_text_native(
                            p.address.section,
                            p.address.paragraph,
                            p.start,
                            &p.text,
                        )
                        .map_err(err)?;
                } else {
                    s.core
                        .insert_text_in_cell_by_path(
                            p.address.section,
                            p.address.paragraph,
                            &path,
                            p.start,
                            &p.text,
                        )
                        .map_err(err)?;
                }
            }
            s.context_reflow_ancestors(&p.address)?;
            Ok(())
        })?;
        let mut value = self.context_info(&p.address)?;
        value["offset"] = json!(p.start + p.text.chars().count());
        Ok(value)
    }

    fn context_split(&mut self, p: Split) -> Result<Value> {
        let para = self.context_writable(&p.address, p.expected_generation)?;
        checked_range(para, p.offset, p.offset)?;
        let offset = logical_split_offset(para, p.offset);
        let path = p.address.native_path();
        let selection = self.context_selection()?;
        self.transaction(|s| {
            if path.is_empty() {
                s.core
                    .split_paragraph_native(p.address.section, p.address.paragraph, offset, None)
                    .map_err(err)?;
            } else {
                s.core
                    .split_paragraph_in_cell_by_path(
                        p.address.section,
                        p.address.paragraph,
                        &path,
                        offset,
                        None,
                    )
                    .map_err(err)?;
            }
            s.context_rebase_selection(
                selection,
                &p.address,
                ParagraphChange::Split { offset: p.offset },
            )?;
            s.context_reflow_ancestors(&p.address)?;
            Ok(())
        })?;
        self.context_info(&p.address.with_index(p.address.index() + 1))
    }

    fn context_merge(&mut self, p: Merge) -> Result<Value> {
        self.context_writable(&p.address, p.expected_generation)?;
        let index = p.address.index();
        if index == 0 {
            bail!("문맥의 첫 문단은 앞 문단과 합칠 수 없습니다.");
        }
        let previous = p.address.with_index(index - 1);
        let offset = self
            .context_writable(&previous, p.expected_generation)?
            .text
            .chars()
            .count();
        let path = p.address.native_path();
        let selection = self.context_selection()?;
        self.transaction(|s| {
            if path.is_empty() {
                s.core
                    .merge_paragraph_native(p.address.section, p.address.paragraph)
                    .map_err(err)?;
            } else {
                s.core
                    .merge_paragraph_in_cell_by_path(p.address.section, p.address.paragraph, &path)
                    .map_err(err)?;
            }
            s.context_rebase_selection(
                selection,
                &p.address,
                ParagraphChange::Merge { prefix: offset },
            )?;
            s.context_reflow_ancestors(&previous)?;
            Ok(())
        })?;
        let mut value = self.context_info(&previous)?;
        value["offset"] = json!(offset);
        Ok(value)
    }

    fn context_format(&mut self, p: Format, character: bool) -> Result<Value> {
        let para = self.context_writable(&p.address, p.expected_generation)?;
        let (start, end) = (
            p.start.unwrap_or(0),
            p.end.unwrap_or_else(|| para.text.chars().count()),
        );
        checked_range(para, start, end)?;
        if !character && (p.start.is_some() || p.end.is_some()) {
            bail!("문단 모양에는 문자 범위를 지정할 수 없습니다.");
        }
        validate_format(&p.format, character)?;
        let path = p.address.native_path();
        self.transaction(|s| {
            if character {
                let raw = if path.is_empty() {
                    s.core.get_char_properties_at_native(
                        p.address.section,
                        p.address.paragraph,
                        start,
                    )
                } else {
                    s.core.get_cell_char_properties_at_by_path(
                        p.address.section,
                        p.address.paragraph,
                        &path,
                        start,
                    )
                }
                .map_err(err)?;
                let props = s
                    .character_parameters_from(p.format.clone(), Some(serde_json::from_str(&raw)?))?
                    .to_string();
                if path.is_empty() {
                    s.core
                        .apply_char_format_native(
                            p.address.section,
                            p.address.paragraph,
                            start,
                            end,
                            &props,
                        )
                        .map_err(err)?;
                } else {
                    s.core
                        .apply_char_format_in_cell_by_path(
                            p.address.section,
                            p.address.paragraph,
                            &path,
                            start,
                            end,
                            &props,
                        )
                        .map_err(err)?;
                }
            } else {
                // Reuse the existing public formatter against a temporary body
                // paragraph, then move only its model and new style tables back.
                // All object references remain in the original document.
                let target = s.context_paragraph(&p.address)?.clone();
                let mut document = s.core.document().clone();
                document.sections[p.address.section].paragraphs = vec![target];
                document.sections[p.address.section].raw_stream = None;
                let mut formatter = DocumentCore::new_empty();
                formatter.set_document(document);
                let current = serde_json::from_str(
                    &formatter
                        .get_para_properties_at_native(p.address.section, 0)
                        .map_err(err)?,
                )?;
                let props = s
                    .paragraph_parameters_from(p.format.clone(), || Ok(current))?
                    .to_string();
                formatter
                    .apply_para_format_native(p.address.section, 0, &props)
                    .map_err(err)?;
                let formatted =
                    formatter.document().sections[p.address.section].paragraphs[0].clone();
                let doc_info = formatter.document().doc_info.clone();
                *resolve_mut(s.core.document_mut(), &p.address)? = formatted;
                s.core.document_mut().doc_info = doc_info;
                s.core.document_mut().sections[p.address.section].raw_stream = None;
                s.core.set_document(s.core.document().clone());
            }
            s.context_reflow_ancestors(&p.address)?;
            Ok(())
        })?;
        let mut value = self.context_info(&p.address)?;
        let raw = if path.is_empty() {
            self.core
                .get_char_properties_at_native(p.address.section, p.address.paragraph, start)
        } else {
            self.core.get_cell_char_properties_at_by_path(
                p.address.section,
                p.address.paragraph,
                &path,
                start,
            )
        }
        .map_err(err)?;
        value["character"] = serde_json::from_str(&raw)?;
        Ok(value)
    }
}
