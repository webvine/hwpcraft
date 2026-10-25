//! Per-layout geometry, shared by page fields and ordinary text navigation.
//!
//! Callers must change `generation` whenever the supplied core is mutated,
//! including each intermediate field-layout iteration and view/font changes.
//! A cache belongs to one caller; no process-global document/font state is kept.
use anyhow::{Result, anyhow, ensure};
use hwpcraft_doc::{DocumentCore, RenderNode, RenderNodeType};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    ops::Range,
    rc::Rc,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathStep {
    pub control: usize,
    pub cell: usize,
    pub paragraph: usize,
}

/// Exact native source identity. A path never drops an outer table level.
/// Textbox context uses the native path/sentinel and is distinguished explicitly.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextPath {
    pub section: usize,
    pub paragraph: usize,
    pub path: Vec<PathStep>,
    pub in_textbox: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextPosition {
    pub address: TextPath,
    pub offset: usize,
}

impl From<crate::Cursor> for TextPosition {
    fn from(c: crate::Cursor) -> Self {
        Self {
            address: TextPath {
                section: c.section,
                paragraph: c.paragraph,
                path: c
                    .cell
                    .map(|cell| {
                        vec![PathStep {
                            control: cell.control,
                            cell: cell.index,
                            paragraph: cell.paragraph,
                        }]
                    })
                    .unwrap_or_default(),
                in_textbox: false,
            },
            offset: c.offset,
        }
    }
}

impl TextPosition {
    /// A deep path cannot be represented by the legacy, single-cell cursor.
    pub fn legacy_cursor(&self) -> Option<crate::Cursor> {
        if self.address.in_textbox || self.address.path.len() > 1 {
            return None;
        }
        Some(crate::Cursor {
            section: self.address.section,
            paragraph: self.address.paragraph,
            offset: self.offset,
            cell: self.address.path.first().map(|p| crate::Cell {
                control: p.control,
                index: p.cell,
                paragraph: p.paragraph,
            }),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn finite(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }
    fn contains(self, other: Self) -> bool {
        other.x >= self.x - 0.5
            && other.y >= self.y - 0.5
            && other.x + other.width <= self.x + self.width + 0.5
            && other.y + other.height <= self.y + self.height + 0.5
    }
    fn intersects(self, other: Self) -> bool {
        other.x + other.width >= self.x
            && other.y + other.height >= self.y
            && other.x <= self.x + self.width
            && other.y <= self.y + self.height
    }
    fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width).min(other.x + other.width).max(x) - x,
            height: (self.y + self.height).min(other.y + other.height).max(y) - y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    Scalar,
    RunEnd,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaretGeometry {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub advance: f64,
    pub run_bounds: Rect,
    pub clip: Option<Rect>,
    pub fully_in_flow: bool,
    pub visible: bool,
    pub boundary: Boundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeReason {
    NoTextRun,
    ControlMarker,
    InlineControlBoundary,
    VerticalOrRotatedText,
    SearchBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CaretLookup {
    Text(CaretGeometry),
    Native(NativeReason),
}

impl CaretLookup {
    pub fn text(self) -> Option<CaretGeometry> {
        match self {
            Self::Text(c) => Some(c),
            Self::Native(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunGeometry {
    pub address: TextPath,
    pub scalar_range: Range<usize>,
    pub bounds: Rect,
    pub text: String,
    pub positions: Vec<f64>,
    pub baseline: f64,
    pub font_size: f64,
    pub vertical: bool,
    pub rotation: f64,
    pub transformed: bool,
    pub clip: Option<Rect>,
    pub fully_in_flow: bool,
    pub visible: bool,
}

impl RunGeometry {
    fn candidate(
        &self,
        page: u32,
        p: &TextPosition,
        affinity: Affinity,
    ) -> Option<(u8, CaretLookup)> {
        if self.address != p.address {
            return None;
        }
        let start = self.scalar_range.start;
        let end = self.scalar_range.end;
        let (rank, boundary) = match affinity {
            Affinity::Downstream if p.offset >= start && p.offset < end => (0, Boundary::Scalar),
            Affinity::Downstream if p.offset == end => (1, Boundary::RunEnd),
            Affinity::Upstream if p.offset > start && p.offset <= end => (
                0,
                if p.offset == end {
                    Boundary::RunEnd
                } else {
                    Boundary::Scalar
                },
            ),
            Affinity::Upstream if p.offset == 0 && start == 0 => (0, Boundary::Scalar),
            Affinity::Upstream if p.offset == start => (1, Boundary::Scalar),
            _ => return None,
        };
        if self.vertical || self.rotation.abs() > 0.01 || self.transformed {
            return Some((
                rank,
                CaretLookup::Native(NativeReason::VerticalOrRotatedText),
            ));
        }
        let local = p.offset.checked_sub(start)?;
        if boundary == Boundary::Scalar
            && let Some(ch) = self.text.chars().nth(local)
            && (ch == '\u{fffc}' || (ch.is_control() && ch != '\t'))
        {
            return Some((rank, CaretLookup::Native(NativeReason::ControlMarker)));
        }
        let x = *self.positions.get(local)?;
        let advance = self.positions.get(local + 1).map_or(0.0, |next| next - x);
        Some((
            rank,
            CaretLookup::Text(CaretGeometry {
                page,
                x: self.bounds.x + x,
                y: self.bounds.y + self.baseline - self.font_size * 0.8,
                height: self.font_size,
                advance,
                run_bounds: self.bounds,
                clip: self.clip,
                fully_in_flow: self.fully_in_flow,
                visible: self.visible,
                boundary,
            }),
        ))
    }
}

/// Public geometry nodes only. The native tree's opaque shaping frame is
/// dropped; this type deliberately cannot be passed to a native paint replayer.
pub struct GeometryTree {
    pub root: RenderNode,
}

pub struct PageGeometry {
    pub page: u32,
    pub tree: GeometryTree,
    pub text_layout: Value,
    pub page_info: Value,
    pub paper: Rect,
    pub flow: Rect,
    pub runs: Vec<RunGeometry>,
    estimated_bytes: usize,
}

impl PageGeometry {
    /// Actual body text, excluding cells, textboxes, headers, footers and notes.
    /// Empty/object-only pages return None so the caller can preserve its native
    /// empty-paragraph/structural position rather than inventing a text scalar.
    pub fn first_body_position(&self) -> Option<TextPosition> {
        let eligible = |r: &&RunGeometry| {
            r.address.path.is_empty()
                && !r.address.in_textbox
                && !r.scalar_range.is_empty()
                && r.visible
        };
        let run = self
            .runs
            .iter()
            .filter(eligible)
            .find(|r| r.fully_in_flow)
            .or_else(|| self.runs.iter().find(eligible))?;
        Some(TextPosition {
            address: run.address.clone(),
            offset: run.scalar_range.start,
        })
    }

    fn caret(&self, p: &TextPosition, affinity: Affinity) -> CaretLookup {
        let candidates = self
            .runs
            .iter()
            .filter_map(|r| r.candidate(self.page, p, affinity))
            .collect();
        choose(candidates).unwrap_or(CaretLookup::Native(NativeReason::NoTextRun))
    }

    /// Every occurrence of one source scalar on this page (e.g. repeated rows).
    /// Run-end fallbacks are excluded: they do not own the field's scalar.
    pub fn scalar_occurrences(&self, p: &TextPosition) -> Vec<CaretGeometry> {
        self.runs
            .iter()
            .filter_map(|r| r.candidate(self.page, p, Affinity::Downstream))
            .filter_map(|(_, c)| match c {
                CaretLookup::Text(c) if c.boundary == Boundary::Scalar => Some(c),
                _ => None,
            })
            .collect()
    }
}

fn choose(mut candidates: Vec<(u8, CaretLookup)>) -> Option<CaretLookup> {
    // Scalar ownership wins over an end boundary. A complete, visible glyph in
    // the true flow area wins over its previous-page overflow duplicate.
    candidates.sort_by_key(|(rank, c)| {
        (
            *rank,
            match c {
                CaretLookup::Text(c) => (!c.visible, !c.fully_in_flow),
                CaretLookup::Native(_) => (false, false),
            },
        )
    });
    candidates.into_iter().next().map(|(_, c)| c)
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub pages: usize,
    pub estimated_bytes: usize,
    pub scan_pages: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            pages: 12,
            estimated_bytes: 64 * 1024 * 1024,
            scan_pages: 512,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub tree_builds: u64,
    pub layout_builds: u64,
    pub hits: u64,
    pub evictions: u64,
    pub invalidations: u64,
    pub uncached_oversize: u64,
    pub resident_pages: usize,
    pub estimated_bytes: usize,
}

struct Entry {
    page: Rc<PageGeometry>,
    used: u64,
}
#[derive(Default)]
struct State {
    epoch: Option<(usize, u64)>,
    pages: BTreeMap<u32, Entry>,
    clock: u64,
    stats: CacheStats,
}

impl State {
    fn clear(&mut self) {
        self.pages.clear();
        self.stats.resident_pages = 0;
        self.stats.estimated_bytes = 0;
        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
    }
    fn epoch(&mut self, core: usize, generation: u64) {
        if self.epoch != Some((core, generation)) {
            if self.epoch.is_some() {
                self.clear();
            }
            self.epoch = Some((core, generation));
        }
    }
    fn insert(&mut self, page: Rc<PageGeometry>, limits: Limits) {
        if limits.pages == 0 || page.estimated_bytes > limits.estimated_bytes {
            self.stats.uncached_oversize = self.stats.uncached_oversize.saturating_add(1);
            return;
        }
        while self.pages.len() >= limits.pages
            || self
                .stats
                .estimated_bytes
                .saturating_add(page.estimated_bytes)
                > limits.estimated_bytes
        {
            let Some(old) = self
                .pages
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(p, _)| *p)
            else {
                break;
            };
            if let Some(entry) = self.pages.remove(&old) {
                self.stats.estimated_bytes = self
                    .stats
                    .estimated_bytes
                    .saturating_sub(entry.page.estimated_bytes);
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
        }
        self.clock = self.clock.saturating_add(1);
        self.stats.estimated_bytes = self
            .stats
            .estimated_bytes
            .saturating_add(page.estimated_bytes);
        self.pages.insert(
            page.page,
            Entry {
                page,
                used: self.clock,
            },
        );
        self.stats.resident_pages = self.pages.len();
    }
}

/// Bounded LRU owned by one presentation generation. Bytes are a conservative
/// estimate of public payloads and copied run/index metadata, not process RSS.
/// Native opaque frames are dropped. Page count is also a hard limit; oversize
/// pages are returned to the caller but never retained in the cache itself.
pub struct GeometryCache {
    limits: Limits,
    state: RefCell<State>,
}

impl Default for GeometryCache {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl GeometryCache {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            state: RefCell::new(State::default()),
        }
    }
    pub fn clear(&self) {
        let mut s = self.state.borrow_mut();
        s.clear();
        s.epoch = None;
    }
    pub fn stats(&self) -> CacheStats {
        self.state.borrow().stats
    }
    fn synchronize(&self, core: &DocumentCore, generation: u64) {
        self.state
            .borrow_mut()
            .epoch(std::ptr::from_ref(core) as usize, generation);
    }

    pub fn page(
        &self,
        core: &DocumentCore,
        generation: u64,
        page: u32,
    ) -> Result<Rc<PageGeometry>> {
        self.synchronize(core, generation);
        ensure!(
            page < core.page_count(),
            "기하를 요청한 쪽이 문서 범위를 벗어났습니다."
        );
        {
            let mut s = self.state.borrow_mut();
            s.clock = s.clock.saturating_add(1);
            let clock = s.clock;
            if let Some(entry) = s.pages.get_mut(&page) {
                entry.used = clock;
                let result = Rc::clone(&entry.page);
                s.stats.hits = s.stats.hits.saturating_add(1);
                return Ok(result);
            }
        }
        let result = Rc::new(build(core, page, self.limits.estimated_bytes, |step| {
            let mut state = self.state.borrow_mut();
            match step {
                BuildStep::Tree => {
                    state.stats.tree_builds = state.stats.tree_builds.saturating_add(1)
                }
                BuildStep::TextLayout => {
                    state.stats.layout_builds = state.stats.layout_builds.saturating_add(1)
                }
            }
        })?);
        self.state
            .borrow_mut()
            .insert(Rc::clone(&result), self.limits);
        Ok(result)
    }

    pub fn first_body_position(
        &self,
        core: &DocumentCore,
        generation: u64,
        page: u32,
    ) -> Result<Option<TextPosition>> {
        Ok(self.page(core, generation, page)?.first_body_position())
    }
    pub fn caret_on_page(
        &self,
        core: &DocumentCore,
        generation: u64,
        page: u32,
        p: &TextPosition,
        affinity: Affinity,
    ) -> Result<CaretLookup> {
        self.caret_on_page_from_source(core, core, generation, page, p, affinity)
    }
    /// A projection changes its spacer text while preserving source indices.
    /// The original core owns inline-control and paragraph-end decisions.
    pub fn caret_on_page_from_source(
        &self,
        core: &DocumentCore,
        source: &DocumentCore,
        generation: u64,
        page: u32,
        p: &TextPosition,
        affinity: Affinity,
    ) -> Result<CaretLookup> {
        self.synchronize(core, generation);
        ensure!(
            page < core.page_count(),
            "기하를 요청한 쪽이 문서 범위를 벗어났습니다."
        );
        if let Some(reason) = source_native_reason(source, p) {
            return Ok(CaretLookup::Native(reason));
        }
        Ok(self.page(core, generation, page)?.caret(p, affinity))
    }
    pub fn scalar_occurrences_on_page(
        &self,
        core: &DocumentCore,
        generation: u64,
        page: u32,
        p: &TextPosition,
    ) -> Result<Vec<CaretGeometry>> {
        Ok(self.page(core, generation, page)?.scalar_occurrences(p))
    }

    /// Prefer a supplied/current native page and its neighbors. Only if none
    /// owns a complete scalar do we scan, under the explicit search budget.
    pub fn caret(
        &self,
        core: &DocumentCore,
        generation: u64,
        p: &TextPosition,
        affinity: Affinity,
        page_hint: Option<u32>,
    ) -> Result<CaretLookup> {
        self.caret_from_source(core, core, generation, p, affinity, page_hint)
    }
    pub fn caret_from_source(
        &self,
        core: &DocumentCore,
        source: &DocumentCore,
        generation: u64,
        p: &TextPosition,
        affinity: Affinity,
        page_hint: Option<u32>,
    ) -> Result<CaretLookup> {
        self.synchronize(core, generation);
        if let Some(reason) = source_native_reason(source, p) {
            return Ok(CaretLookup::Native(reason));
        }
        let count = core.page_count();
        if count == 0 {
            return Ok(CaretLookup::Native(NativeReason::NoTextRun));
        }
        let paragraph_end =
            paragraph_text(source, &p.address).is_some_and(|text| p.offset == text.chars().count());
        let hint = page_hint.or_else(|| native_hint(core, p));
        let mut pages = Vec::new();
        if let Some(page) = hint.filter(|p| *p < count) {
            pages.push(page);
            if page + 1 < count {
                pages.push(page + 1);
            }
            if page > 0 {
                pages.push(page - 1);
            }
        }
        let mut seen = BTreeSet::new();
        let mut candidates = Vec::new();
        for page in pages {
            seen.insert(page);
            let page = self.page(core, generation, page)?;
            candidates.extend(
                page.runs
                    .iter()
                    .filter_map(|r| r.candidate(page.page, p, affinity)),
            );
        }
        if let Some(found) = choose(candidates.clone()) {
            match &found {
                CaretLookup::Text(c)
                    if c.visible
                        && c.fully_in_flow
                        && (c.boundary == Boundary::Scalar
                            || affinity == Affinity::Upstream
                            || paragraph_end) =>
                {
                    return Ok(found);
                }
                CaretLookup::Native(
                    NativeReason::ControlMarker | NativeReason::VerticalOrRotatedText,
                ) => return Ok(found),
                _ => {}
            }
        }
        if count > self.limits.scan_pages {
            return Ok(
                choose(candidates).unwrap_or(CaretLookup::Native(NativeReason::SearchBudget))
            );
        }
        for page in 0..count {
            if seen.contains(&page) {
                continue;
            }
            let page = self.page(core, generation, page)?;
            candidates.extend(
                page.runs
                    .iter()
                    .filter_map(|r| r.candidate(page.page, p, affinity)),
            );
        }
        Ok(choose(candidates).unwrap_or(CaretLookup::Native(NativeReason::NoTextRun)))
    }
}

fn native_hint(core: &DocumentCore, p: &TextPosition) -> Option<u32> {
    if p.address.in_textbox {
        return None;
    }
    let raw = if let Some(cell) = p.address.path.first() {
        if p.address.path.len() != 1 {
            return None;
        }
        core.get_cursor_rect_in_cell_native(
            p.address.section,
            p.address.paragraph,
            cell.control,
            cell.cell,
            cell.paragraph,
            p.offset,
        )
    } else {
        core.get_cursor_rect_native(p.address.section, p.address.paragraph, p.offset)
    }
    .ok()?;
    serde_json::from_str::<Value>(&raw).ok()?["pageIndex"]
        .as_u64()
        .and_then(|p| p.try_into().ok())
}

fn paragraph_ref<'a>(
    core: &'a DocumentCore,
    address: &TextPath,
) -> Option<&'a hwpcraft_doc::model::paragraph::Paragraph> {
    use hwpcraft_doc::model::control::Control;
    let mut paragraph = core
        .document()
        .sections
        .get(address.section)?
        .paragraphs
        .get(address.paragraph)?;
    for step in &address.path {
        paragraph = match paragraph.controls.get(step.control)? {
            Control::Table(table) => table.cells.get(step.cell)?.paragraphs.get(step.paragraph)?,
            Control::Shape(shape) if step.cell == 0 => shape
                .drawing()?
                .text_box
                .as_ref()?
                .paragraphs
                .get(step.paragraph)?,
            _ => return None,
        };
    }
    Some(paragraph)
}
fn paragraph_text<'a>(core: &'a DocumentCore, address: &TextPath) -> Option<&'a str> {
    Some(&paragraph_ref(core, address)?.text)
}

/// Call this against the editable source, not a TAB-spacer projection. Inline
/// objects can own an anchor immediately before a literal scalar at the same
/// source offset; a following TextRun alone cannot distinguish that affinity.
pub fn source_native_reason(core: &DocumentCore, position: &TextPosition) -> Option<NativeReason> {
    use hwpcraft_doc::model::control::{AutoNumberType, Control};
    let Some(paragraph) = paragraph_ref(core, &position.address) else {
        return Some(NativeReason::NoTextRun);
    };
    let len = paragraph.text.chars().count();
    if len == 0 || position.offset > len {
        return Some(NativeReason::NoTextRun);
    }
    if let Some(ch) = paragraph.text.chars().nth(position.offset)
        && (ch == '\u{fffc}' || (ch.is_control() && ch != '\t'))
    {
        return Some(NativeReason::ControlMarker);
    }
    for (control, at) in paragraph
        .controls
        .iter()
        .zip(paragraph.control_text_positions())
    {
        let caption_number = matches!(control,Control::AutoNumber(n) if !matches!(n.number_type,AutoNumberType::Page|AutoNumberType::TotalPage));
        let at = if caption_number && at == len && len > 0 && paragraph.text.ends_with(' ') {
            len - 1
        } else {
            at
        };
        if at == position.offset
            && (control.is_logical_inline()
                || matches!(control, Control::Form(_))
                || caption_number)
        {
            return Some(NativeReason::InlineControlBoundary);
        }
    }
    None
}

fn json_number(v: &Value, name: &str) -> Result<f64> {
    v[name]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| anyhow!("페이지 기하의 {name} 값이 올바르지 않습니다."))
}

enum BuildStep {
    Tree,
    TextLayout,
}

fn build(
    core: &DocumentCore,
    page: u32,
    budget: usize,
    mut observe: impl FnMut(BuildStep),
) -> Result<PageGeometry> {
    observe(BuildStep::Tree);
    let tree = {
        let native = core
            .build_page_render_tree(page)
            .map_err(|e| anyhow!("{e}"))?;
        GeometryTree { root: native.root }
    };
    observe(BuildStep::TextLayout);
    let layout_raw = core
        .get_page_text_layout_native(page)
        .map_err(|e| anyhow!("{e}"))?;
    let info_raw = core
        .get_page_info_native(page)
        .map_err(|e| anyhow!("{e}"))?;
    let text_layout: Value = serde_json::from_str(&layout_raw)?;
    let page_info: Value = serde_json::from_str(&info_raw)?;
    let paper = Rect {
        x: 0.0,
        y: 0.0,
        width: json_number(&page_info, "width")?,
        height: json_number(&page_info, "height")?,
    };
    let x = json_number(&page_info, "bodyLeft")?;
    let right = json_number(&page_info, "bodyRight")?;
    let y = json_number(&page_info["headerArea"], "y")?
        + json_number(&page_info["headerArea"], "height")?;
    let bottom = json_number(&page_info["footerArea"], "y")?;
    let flow = Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    };
    ensure!(
        paper.finite() && flow.finite(),
        "페이지 본문 기하가 올바르지 않습니다."
    );
    let runs = {
        let mut context = Collect {
            paper,
            flow,
            layout: &text_layout,
            runs: Vec::new(),
        };
        collect(&tree.root, Walk::default(), &mut context)?;
        context.runs
    };
    let overhead = retained_overhead(&tree.root);
    let mut counter = Count {
        bytes: 0,
        limit: budget.saturating_div(8).saturating_add(1),
        exceeded: false,
    };
    let serialized = serde_json::to_writer(&mut counter, &tree.root);
    if let Err(error) = serialized
        && !counter.exceeded
    {
        return Err(error.into());
    }
    let estimated_bytes = if counter.exceeded {
        usize::MAX
    } else {
        counter
            .bytes
            .saturating_add(layout_raw.len())
            .saturating_add(info_raw.len())
            .saturating_mul(8)
            .saturating_add(overhead)
    };
    Ok(PageGeometry {
        page,
        tree,
        text_layout,
        page_info,
        paper,
        flow,
        runs,
        estimated_bytes,
    })
}

struct Count {
    bytes: usize,
    limit: usize,
    exceeded: bool,
}
impl Write for Count {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buf.len());
        if self.bytes > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("geometry cache budget"));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct Walk {
    body: bool,
    nested: bool,
    transformed: bool,
    clip: Option<Rect>,
}
struct Collect<'a> {
    paper: Rect,
    flow: Rect,
    layout: &'a Value,
    runs: Vec<RunGeometry>,
}

fn retained_overhead(node: &RenderNode) -> usize {
    let mut bytes = std::mem::size_of::<RenderNode>().saturating_mul(2);
    if let RenderNodeType::Image(image) = &node.node_type
        && let Some(data) = &image.data
    {
        bytes = bytes.saturating_add(data.capacity());
    }
    if let RenderNodeType::TextRun(run) = &node.node_type {
        bytes = bytes.saturating_add(run.text.chars().count().saturating_mul(128));
    }
    for child in &node.children {
        bytes = bytes.saturating_add(retained_overhead(child));
    }
    bytes
}

fn collect(node: &RenderNode, mut walk: Walk, context: &mut Collect<'_>) -> Result<()> {
    if !node.visible
        || matches!(
            node.node_type,
            RenderNodeType::Header
                | RenderNodeType::Footer
                | RenderNodeType::MasterPage
                | RenderNodeType::FootnoteArea
        )
        || matches!(&node.node_type,RenderNodeType::TextLine(line) if line.caption_owner.is_some())
    {
        return Ok(());
    }
    walk.body |= matches!(node.node_type, RenderNodeType::Body { .. });
    walk.nested |= matches!(
        node.node_type,
        RenderNodeType::Table(_) | RenderNodeType::TextBox
    );
    walk.transformed |= match &node.node_type {
        RenderNodeType::Rectangle(n) => n.transform.has_transform(),
        RenderNodeType::Ellipse(n) => n.transform.has_transform(),
        RenderNodeType::Line(n) => n.transform.has_transform(),
        RenderNodeType::Path(n) => n.transform.has_transform(),
        RenderNodeType::TableCell(n) => n.text_direction != 0,
        _ => false,
    };
    let bounds = Rect {
        x: node.bbox.x,
        y: node.bbox.y,
        width: node.bbox.width,
        height: node.bbox.height,
    };
    if matches!(&node.node_type,RenderNodeType::TableCell(cell) if cell.clip) {
        walk.clip = Some(walk.clip.map_or(bounds, |c| c.intersection(bounds)));
    }
    if walk.body
        && let RenderNodeType::TextRun(run) = &node.node_type
        && !run.text.is_empty()
        && let (Some(section), Some(paragraph), Some(start)) =
            (run.section_index, run.para_index, run.char_start)
    {
        let address = if let Some(ctx) = &run.cell_context {
            if ctx.path.is_empty() {
                None
            } else {
                Some(TextPath {
                    section,
                    paragraph: ctx.parent_para_index,
                    path: ctx
                        .path
                        .iter()
                        .map(|p| PathStep {
                            control: p.control_index,
                            cell: p.cell_index,
                            paragraph: p.cell_para_index,
                        })
                        .collect(),
                    in_textbox: ctx.in_textbox,
                })
            }
        } else if walk.nested {
            None
        } else {
            Some(TextPath {
                section,
                paragraph,
                ..Default::default()
            })
        };
        if let Some(address) = address {
            let count = run.text.chars().count();
            let end = start
                .checked_add(count)
                .ok_or_else(|| anyhow!("문자 기하 범위가 너무 큽니다."))?;
            let native_only = walk.transformed || run.is_vertical || run.rotation.abs() > 0.01;
            let positions = if count == 1 && run.char_overlap.is_some() {
                vec![0.0, bounds.width]
            } else if native_only {
                Vec::new()
            } else if let Some(positions) = &run.layout_positions {
                positions.clone()
            } else {
                positions(
                    context.layout,
                    &address,
                    start,
                    &run.text,
                    bounds,
                    RunIdentity {
                        font: &run.style.font_family,
                        size: run.style.font_size,
                        char_shape: run.char_shape_id,
                        para_shape: run.para_shape_id,
                    },
                )?
            };
            ensure!(
                native_only
                    || positions.len() == count + 1 && positions.iter().all(|x| x.is_finite()),
                "문자 경계 기하와 원본 스칼라 수가 다릅니다."
            );
            ensure!(
                bounds.finite()
                    && run.baseline.is_finite()
                    && run.style.font_size.is_finite()
                    && run.style.font_size >= 0.0,
                "문자 런 기하가 올바르지 않습니다."
            );
            let visible = context.paper.intersects(bounds)
                && walk
                    .clip
                    .is_none_or(|c| c.width > 0.0 && c.height > 0.0 && c.intersects(bounds));
            // Stored line boxes can fit while their baseline-adjusted glyph
            // band extends into the next page. Native partial paragraphs may
            // repeat that source scalar on both pages; the complete displayed
            // occurrence must win without depending on a caller's page hint.
            // Use the renderer's public script metrics so a superscript or
            // subscript is tested at its actual drawn size and baseline.
            let (draw_size, draw_baseline) = run
                .style
                .script_draw_metrics(run.style.font_size, bounds.y + run.baseline);
            let glyph_band = Rect {
                x: bounds.x,
                y: draw_baseline - draw_size * 0.8,
                width: bounds.width,
                height: draw_size,
            };
            let fully_in_flow = context.flow.contains(bounds)
                && context.flow.contains(glyph_band)
                && walk
                    .clip
                    .is_none_or(|c| c.contains(bounds) && c.contains(glyph_band));
            context.runs.push(RunGeometry {
                address,
                scalar_range: start..end,
                bounds,
                text: run.text.clone(),
                positions,
                baseline: run.baseline,
                font_size: run.style.font_size,
                vertical: run.is_vertical,
                rotation: run.rotation,
                transformed: walk.transformed,
                clip: walk.clip,
                fully_in_flow,
                visible,
            });
        }
    }
    for child in &node.children {
        collect(child, walk, context)?;
    }
    Ok(())
}

struct RunIdentity<'a> {
    font: &'a str,
    size: f64,
    char_shape: Option<u32>,
    para_shape: Option<u16>,
}

fn positions(
    layout: &Value,
    address: &TextPath,
    start: usize,
    text: &str,
    bounds: Rect,
    identity: RunIdentity<'_>,
) -> Result<Vec<f64>> {
    let rows = layout["runs"]
        .as_array()
        .ok_or_else(|| anyhow!("페이지 문자 기하 목록이 없습니다."))?;
    let mut matching = rows.iter().filter(|v| {
        if v["secIdx"].as_u64() != Some(address.section as u64)
            || v["charStart"].as_u64() != Some(start as u64)
            || v["text"].as_str() != Some(text)
        {
            return false;
        }
        if (v["x"].as_f64().unwrap_or(f64::INFINITY) - bounds.x).abs() > 0.15
            || (v["y"].as_f64().unwrap_or(f64::INFINITY) - bounds.y).abs() > 0.15
            || (v["w"].as_f64().unwrap_or(f64::INFINITY) - bounds.width).abs() > 0.15
            || (v["h"].as_f64().unwrap_or(f64::INFINITY) - bounds.height).abs() > 0.15
            || v["fontFamily"].as_str() != Some(identity.font)
            || (v["fontSize"].as_f64().unwrap_or(f64::INFINITY) - identity.size).abs() > 0.15
            || v["charShapeId"].as_u64() != identity.char_shape.map(u64::from)
            || v["paraShapeId"].as_u64() != identity.para_shape.map(u64::from)
        {
            return false;
        }
        if address.path.is_empty() {
            return v["paraIdx"].as_u64() == Some(address.paragraph as u64)
                && v.get("cellPath").is_none();
        }
        if v["parentParaIdx"].as_u64() != Some(address.paragraph as u64) {
            return false;
        }
        v["cellPath"].as_array().is_some_and(|path| {
            path.len() == address.path.len()
                && path.iter().zip(&address.path).all(|(v, p)| {
                    v["controlIndex"].as_u64() == Some(p.control as u64)
                        && v["cellIndex"].as_u64() == Some(p.cell as u64)
                        && v["cellParaIndex"].as_u64() == Some(p.paragraph as u64)
                })
        })
    });
    let entry = matching
        .next()
        .ok_or_else(|| anyhow!("정확한 문서 경로의 문자 경계 기하를 찾지 못했습니다."))?;
    ensure!(
        matching.all(|other| other["charX"] == entry["charX"]),
        "서로 다른 문자 경계 기하가 같은 원본 위치를 가리킵니다."
    );
    entry["charX"]
        .as_array()
        .ok_or_else(|| anyhow!("문자 경계 기하가 없습니다."))?
        .iter()
        .map(|v| {
            v.as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| anyhow!("문자 경계 기하가 올바르지 않습니다."))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};

    #[test]
    fn flow_uses_the_drawn_script_band_without_expanding_it_to_the_base_font() -> Result<()> {
        fn text(node: &RenderNode) -> Option<RenderNode> {
            if matches!(node.node_type, RenderNodeType::TextRun(_)) {
                Some(node.clone())
            } else {
                node.children.iter().find_map(text)
            }
        }
        let mut session = Session::new()?;
        session.execute(Command::Insert("X".into()))?;
        let tree = session.core.build_page_render_tree(0).map_err(crate::err)?;
        let template = text(&tree.root).ok_or_else(|| anyhow!("missing text fixture"))?;
        let flow = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        for (y, baseline, superscript, subscript, expected) in [
            (90.0, 10.0, false, false, false),
            (90.0, 10.0, true, false, true),
            (0.0, 4.5, false, false, false),
            (0.0, 4.5, false, true, true),
            (80.0, 10.0, false, false, true),
        ] {
            let mut node = template.clone();
            node.bbox.x = 10.0;
            node.bbox.y = y;
            node.bbox.width = 5.0;
            node.bbox.height = 5.0;
            let RenderNodeType::TextRun(run) = &mut node.node_type else {
                unreachable!()
            };
            run.baseline = baseline;
            run.style.font_size = 10.0;
            run.style.superscript = superscript;
            run.style.subscript = subscript;
            run.style.underline = hwpcraft_doc::model::style::UnderlineType::Bottom;
            run.layout_positions = Some(vec![0.0, 5.0]);
            let mut context = Collect {
                paper: flow,
                flow,
                layout: &Value::Null,
                runs: Vec::new(),
            };
            collect(
                &node,
                Walk {
                    body: true,
                    ..Default::default()
                },
                &mut context,
            )?;
            assert_eq!(context.runs.len(), 1);
            assert_eq!(
                context.runs[0].fully_in_flow, expected,
                "y={y}, baseline={baseline}, super={superscript}, sub={subscript}"
            );
        }
        Ok(())
    }

    fn path() -> TextPath {
        TextPath {
            section: 0,
            paragraph: 0,
            ..Default::default()
        }
    }
    fn run(start: usize, text: &str, y: f64) -> RunGeometry {
        let n = text.chars().count();
        RunGeometry {
            address: path(),
            scalar_range: start..start + n,
            bounds: Rect {
                x: 10.0,
                y,
                width: n as f64 * 5.0,
                height: 10.0,
            },
            text: text.into(),
            positions: (0..=n).map(|i| i as f64 * 5.0).collect(),
            baseline: 8.0,
            font_size: 10.0,
            vertical: false,
            rotation: 0.0,
            transformed: false,
            clip: None,
            fully_in_flow: true,
            visible: true,
        }
    }
    #[test]
    fn explicit_affinity_selects_each_side_and_keeps_paragraph_end() {
        let a = run(0, "ab", 10.0);
        let b = run(2, "cd", 30.0);
        let p = TextPosition {
            address: path(),
            offset: 2,
        };
        let candidates = |affinity| {
            [&a, &b]
                .into_iter()
                .filter_map(|r| r.candidate(0, &p, affinity))
                .collect()
        };
        assert!(
            matches!(choose(candidates(Affinity::Upstream)),Some(CaretLookup::Text(c)) if c.y==10.0&&c.x==20.0&&c.boundary==Boundary::RunEnd)
        );
        assert!(
            matches!(choose(candidates(Affinity::Downstream)),Some(CaretLookup::Text(c)) if c.y==30.0&&c.x==10.0&&c.boundary==Boundary::Scalar)
        );
        assert!(
            matches!(b.candidate(0,&TextPosition {offset:4,..p},Affinity::Downstream),Some((_,CaretLookup::Text(c))) if c.x==20.0&&c.boundary==Boundary::RunEnd)
        );
    }
    #[test]
    fn controls_and_vertical_text_have_explicit_native_fallback() {
        let p = TextPosition {
            address: path(),
            offset: 0,
        };
        assert!(matches!(
            run(0, "\u{fffc}", 10.0).candidate(0, &p, Affinity::Downstream),
            Some((_, CaretLookup::Native(NativeReason::ControlMarker)))
        ));
        assert!(
            matches!(run(0,"\t",10.0).candidate(0,&p,Affinity::Downstream),Some((_,CaretLookup::Text(c))) if c.advance==5.0)
        );
        let mut vertical = run(0, "가", 10.0);
        vertical.vertical = true;
        assert!(matches!(
            vertical.candidate(0, &p, Affinity::Downstream),
            Some((_, CaretLookup::Native(NativeReason::VerticalOrRotatedText)))
        ));
    }
    #[test]
    fn complete_continuation_wins_but_genuine_overflow_is_retained() {
        let p = TextPosition {
            address: path(),
            offset: 0,
        };
        let mut prior = run(0, "x", 1004.0);
        prior.fully_in_flow = false;
        let next = run(0, "x", 132.0);
        let a = prior.candidate(0, &p, Affinity::Downstream);
        let b = next.candidate(1, &p, Affinity::Downstream);
        assert!(
            matches!(choose(a.clone().into_iter().chain(b).collect()),Some(CaretLookup::Text(c)) if c.page==1)
        );
        assert!(
            matches!(choose(a.into_iter().collect()),Some(CaretLookup::Text(c)) if c.page==0&&!c.fully_in_flow)
        );
    }
    #[test]
    fn full_nested_path_and_legacy_conversion_do_not_drop_ancestors() {
        let p = TextPosition {
            address: TextPath {
                path: vec![
                    PathStep {
                        control: 2,
                        cell: 1,
                        paragraph: 0,
                    },
                    PathStep {
                        control: 0,
                        cell: 0,
                        paragraph: 0,
                    },
                ],
                ..path()
            },
            offset: 1,
        };
        let mut r = run(0, "ab", 10.0);
        r.address = p.address.clone();
        assert!(p.legacy_cursor().is_none());
        assert!(r.candidate(0, &p, Affinity::Downstream).is_some());
        let mut other = p.clone();
        other.address.path[0].cell = 2;
        assert!(r.candidate(0, &other, Affinity::Downstream).is_none());
    }
    #[test]
    fn real_page_is_built_once_and_changed_generation_invalidates() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("기하 캐시".into()))?;
        let cache = GeometryCache::default();
        let a = cache.page(&s.core, s.generation, 0)?;
        let b = cache.page(&s.core, s.generation, 0)?;
        assert!(Rc::ptr_eq(&a, &b));
        assert_eq!(cache.stats().tree_builds, 1);
        assert_eq!(cache.stats().layout_builds, 1);
        let first = a
            .first_body_position()
            .ok_or_else(|| anyhow!("no real body run"))?;
        assert!(matches!(
            cache.caret_on_page(&s.core, s.generation, 0, &first, Affinity::Downstream)?,
            CaretLookup::Text(_)
        ));
        s.execute(Command::Insert(" 변경".into()))?;
        cache.page(&s.core, s.generation, 0)?;
        assert_eq!(cache.stats().tree_builds, 2);
        assert_eq!(cache.stats().invalidations, 1);
        assert_eq!(cache.stats().resident_pages, 1);
        let mut other = Session::new()?;
        other.execute(Command::Insert("다른 문서".into()))?;
        cache.page(&other.core, s.generation, 0)?;
        assert_eq!(cache.stats().tree_builds, 3);
        assert_eq!(cache.stats().invalidations, 2);
        Ok(())
    }

    #[test]
    fn page_and_byte_limits_evict_real_pages() -> Result<()> {
        let mut s = Session::new()?;
        s.execute(Command::Insert("첫 쪽\n둘째 쪽".into()))?;
        let mut doc = s.core.document().clone();
        doc.sections[0].paragraphs[1].column_type =
            hwpcraft_doc::model::paragraph::ColumnBreakType::Page;
        s.core.set_document(doc);
        ensure!(s.core.page_count() >= 2, "fixture needs two pages");
        let cache = GeometryCache::new(Limits {
            pages: 1,
            ..Default::default()
        });
        let first = cache.page(&s.core, 1, 0)?;
        let second = cache.page(&s.core, 1, 1)?;
        assert_eq!(cache.stats().resident_pages, 1);
        assert_eq!(cache.stats().evictions, 1);
        cache.page(&s.core, 1, 0)?;
        assert_eq!(cache.stats().tree_builds, 3);
        assert_eq!(cache.stats().evictions, 2);
        let budget = first
            .estimated_bytes
            .max(second.estimated_bytes)
            .saturating_add(1);
        let cache = GeometryCache::new(Limits {
            pages: 12,
            estimated_bytes: budget,
            scan_pages: 8,
        });
        cache.page(&s.core, 1, 0)?;
        cache.page(&s.core, 1, 1)?;
        assert_eq!(cache.stats().resident_pages, 1);
        assert_eq!(cache.stats().evictions, 1);
        assert!(cache.stats().estimated_bytes <= budget);
        Ok(())
    }

    #[test]
    fn real_three_level_table_geometry_uses_the_entire_address() -> Result<()> {
        let mut s = Session::new()?;
        let mut address = serde_json::json!({"section":0,"paragraph":0,"path":[]});
        for width in [120, 70, 30] {
            let made = s.run(
                "context.table.create",
                serde_json::json!({"address":address,"rows":1,"columns":1,"width_mm":width}),
            )?;
            address = made["address"].clone();
        }
        s.run(
            "context.replace",
            serde_json::json!({"address":address,"start":0,"end":0,"text":"깊이😀"}),
        )?;
        let address: crate::DocumentAddress = serde_json::from_value(address)?;
        let position = TextPosition {
            address: TextPath {
                section: address.section,
                paragraph: address.paragraph,
                path: address
                    .path
                    .into_iter()
                    .map(|p| PathStep {
                        control: p.control,
                        cell: p.cell,
                        paragraph: p.paragraph,
                    })
                    .collect(),
                in_textbox: false,
            },
            offset: 1,
        };
        assert_eq!(position.address.path.len(), 3);
        assert!(position.legacy_cursor().is_none());
        let cache = GeometryCache::default();
        let CaretLookup::Text(caret) =
            cache.caret_on_page(&s.core, s.generation, 0, &position, Affinity::Downstream)?
        else {
            return Err(anyhow!("missing actual nested caret"));
        };
        assert!(caret.visible && caret.fully_in_flow);
        assert!(caret.advance > 0.0);
        let mut other = position.clone();
        other.address.path[0].cell += 1;
        assert!(matches!(
            cache.caret_on_page(&s.core, s.generation, 0, &other, Affinity::Downstream)?,
            CaretLookup::Native(NativeReason::NoTextRun)
        ));
        assert_eq!(cache.stats().tree_builds, 1);
        assert_eq!(cache.stats().layout_builds, 1);
        Ok(())
    }
    #[test]
    fn zero_budget_never_retains_an_oversize_page() -> Result<()> {
        let s = Session::new()?;
        let cache = GeometryCache::new(Limits {
            pages: 1,
            estimated_bytes: 1,
            scan_pages: 8,
        });
        cache.page(&s.core, s.generation, 0)?;
        assert_eq!(cache.stats().resident_pages, 0);
        assert_eq!(cache.stats().estimated_bytes, 0);
        assert_eq!(cache.stats().uncached_oversize, 1);
        Ok(())
    }
    #[test]
    fn an_inline_anchor_keeps_native_affinity_before_following_text() -> Result<()> {
        use hwpcraft_doc::model::control::Control;
        let mut s = Session::new()?;
        s.execute(Command::Insert("뒤".into()))?;
        let p = &mut s.core.document_mut().sections[0].paragraphs[0];
        p.controls.push(Control::Footnote(Box::default()));
        for offset in &mut p.char_offsets {
            *offset += 8;
        }
        p.char_count += 8;
        let position = TextPosition {
            address: path(),
            offset: 0,
        };
        assert_eq!(
            source_native_reason(&s.core, &position),
            Some(NativeReason::InlineControlBoundary)
        );
        let cache = GeometryCache::default();
        assert!(matches!(
            cache.caret_on_page(&s.core, s.generation, 0, &position, Affinity::Downstream)?,
            CaretLookup::Native(NativeReason::InlineControlBoundary)
        ));
        assert_eq!(
            cache.stats().tree_builds,
            0,
            "do not use the following literal run as the inline object's caret"
        );
        Ok(())
    }
}
