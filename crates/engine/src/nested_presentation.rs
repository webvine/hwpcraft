//! Read-only presentation repair for undersized, horizontal nested-table frames.
//!
//! Imported stored frames can be shorter than their actual nested content. Grow
//! minimum heights in a private model clone, using the same width-aware bottom-up
//! measurement as explicit cell edits. Never assign this core to the editable or
//! saved document. Unsupported roots retain their original presentation.
use super::*;
use hwpcraft_doc::model::{shape::SizeCriterion, table::Table};
use hwpcraft_doc::{RenderNode, RenderNodeType};
use serde::Serialize;
use std::collections::BTreeMap;

const MAX_DEPTH: usize = 32;
const MAX_ROOTS: usize = 64;
const MAX_PATHS: usize = 1024;
const MAX_PAGES: u32 = 128;
const MAX_NODES: usize = 500_000;
/// Work budget for the whole repair pass, in page layouts. Measuring one path
/// re-paginates a document clone once per paragraph of every ancestor cell (see
/// `reflow_layouts`), and every visibility check walks every page, so each step
/// costs the source's page count times its layout count. The pass runs
/// synchronously when a document opens or changes; unbounded, it froze the
/// window for minutes on long documents with many nested tables, and a single
/// giant-cell path alone ran for minutes. The budget is deterministic and
/// charged before each step, so a document always gets the same presentation.
/// Measured release cost is 0.8–3 ms per page layout (dense nested tables are
/// the expensive end), so 600 keeps the pass near 0.5–1.8 s. Roots past it keep
/// their original presentation.
const MAX_WORK: usize = 600;
/// Wall-clock safety stop for unusually expensive single pages; not the normal limit.
const MAX_TIME: std::time::Duration = std::time::Duration::from_secs(3);
const WORK_REASON: &str =
    "중첩 표 표시 보정 작업 한도를 넘었습니다. 나머지 표는 원본 배치를 유지합니다.";
const TIME_REASON: &str =
    "중첩 표 표시 보정 시간 한도를 넘었습니다. 나머지 표는 원본 배치를 유지합니다.";

/// Deterministic work accounting plus the wall-clock safety stop.
struct Budget {
    pages: usize,
    limit: usize,
    spent: usize,
    deadline: std::time::Instant,
}
impl Budget {
    /// Reserve one whole-document layout; the reason it is refused otherwise.
    fn charge(&mut self) -> std::result::Result<(), &'static str> {
        self.charge_layouts(1)
    }
    /// Reserve `layouts` whole-document layouts before doing them.
    fn charge_layouts(&mut self, layouts: usize) -> std::result::Result<(), &'static str> {
        let cost = self.pages.saturating_mul(layouts.max(1));
        if self.spent.saturating_add(cost) > self.limit {
            return Err(WORK_REASON);
        }
        if std::time::Instant::now() >= self.deadline {
            return Err(TIME_REASON);
        }
        self.spent += cost;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct NormalizationReport {
    pub roots_seen: usize,
    pub roots_adjusted: usize,
    pub paths_measured: usize,
    pub scan_truncated: bool,
    pub skipped: Vec<SkippedRoot>,
    /// The verdict of the previous build was reused (see `Verdicts`).
    pub reused_previous: bool,
}

/// Fingerprint of everything a "nothing to repair" verdict depends on that an
/// edit can change cheaply: page count, page definitions, paragraph counts and
/// each candidate root (address and full model contents, nested tables
/// included). It carries no document identity: a different document reuses a
/// verdict only if all of this matches, and the verdict is display-only (keep
/// original frames), never saved. MCP runs each document on its own thread, so
/// there the per-thread slot is per document; desktop tabs share one thread and
/// a tab switch simply misses and replaces the slot.
fn verdict_key(source: &DocumentCore, plans: &[Plan], work: usize) -> Result<u64> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    work.hash(&mut h);
    source.page_count().hash(&mut h);
    // Cheap document discriminators that typing does not change: another
    // document on the same thread (desktop tabs) almost never matches them all.
    format!("{:?}", source.document().provenance).hash(&mut h);
    source.document().sections.len().hash(&mut h);
    for section in &source.document().sections {
        section.paragraphs.len().hash(&mut h);
        format!("{:?}", section.section_def.page_def).hash(&mut h);
    }
    for plan in plans {
        (plan.root.section, plan.root.paragraph, plan.root.control).hash(&mut h);
        format!("{:?}", table_at(source, plan.root)?).hash(&mut h);
    }
    Ok(h.finish())
}

thread_local! {
    /// Last build that repaired nothing, by `verdict_key`. Typing outside the
    /// candidate tables re-ran every visibility check and failed attempt on
    /// each keystroke (0.6–2 s on 30–80 page documents) only to reach the same
    /// "keep the original frames" result; that verdict is reused instead. Any
    /// change to a candidate table, a page definition or the page count misses.
    static VERDICTS: std::cell::RefCell<Option<(u64, NormalizationReport)>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SkippedRoot {
    pub section: usize,
    pub paragraph: usize,
    pub control: usize,
    pub reason: String,
}

pub(crate) struct NestedPresentation {
    /// None means all source frames were sufficient or explicitly unsupported.
    pub core: Option<DocumentCore>,
    pub report: NormalizationReport,
}

#[derive(Clone, Copy, Debug)]
struct Root {
    section: usize,
    paragraph: usize,
    control: usize,
}
type TextKey = Vec<(usize, usize, usize)>;

struct Plan {
    root: Root,
    paths: Vec<DocumentAddress>,
    expected: BTreeMap<TextKey, String>,
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}

fn table_at(core: &DocumentCore, root: Root) -> Result<&Table> {
    match core
        .document()
        .sections
        .get(root.section)
        .and_then(|s| s.paragraphs.get(root.paragraph))
        .and_then(|p| p.controls.get(root.control))
    {
        Some(Control::Table(table)) => Ok(table),
        _ => bail!("중첩 표의 원본 주소를 찾지 못했습니다."),
    }
}

/// Whole-document layouts one `Session::context_reflow_ancestors(address)` call
/// performs: a native empty insertion (a full re-pagination) for every paragraph
/// of each ancestor cell on the path, plus one measurement layout per depth.
/// Read-only; an address that does not resolve costs what was walked so far.
fn reflow_layouts(
    doc: &hwpcraft_doc::model::document::Document,
    address: &DocumentAddress,
) -> usize {
    let Some(mut para) = doc
        .sections
        .get(address.section)
        .and_then(|s| s.paragraphs.get(address.paragraph))
    else {
        return 1;
    };
    let mut layouts = 0usize;
    for step in &address.path {
        let Some(Control::Table(table)) = para.controls.get(step.control) else {
            break;
        };
        let Some(cell) = table.cells.get(step.cell) else {
            break;
        };
        layouts = layouts.saturating_add(cell.paragraphs.len() + 1);
        match cell.paragraphs.get(step.paragraph) {
            Some(next) => para = next,
            None => break,
        }
    }
    layouts.max(1)
}

fn has_nested(table: &Table) -> bool {
    table
        .cells
        .iter()
        .flat_map(|c| &c.paragraphs)
        .flat_map(|p| &p.controls)
        .any(|c| matches!(c, Control::Table(_)))
}

fn collect(
    table: &Table,
    address: &DocumentAddress,
    control: usize,
    plan: &mut Plan,
) -> Result<()> {
    if address.path.len() >= MAX_DEPTH {
        bail!("중첩 표 깊이가 표시 보정 한도를 넘습니다.");
    }
    if table.common.locked || table.common.size_protect {
        bail!("보호되거나 크기가 고정된 표입니다.");
    }
    if table.common.height_criterion != SizeCriterion::Absolute
        || table.common.width_criterion != SizeCriterion::Absolute
    {
        bail!("상대 크기의 표는 자동 표시 보정을 지원하지 않습니다.");
    }
    if table.caption.is_some() {
        bail!("캡션이 있는 중첩 표는 원본 배치를 유지합니다.");
    }
    for (cell_index, cell) in table.cells.iter().enumerate() {
        if cell.cell_protect() {
            bail!("내용이 보호된 셀이 있습니다.");
        }
        if cell.text_direction != 0 {
            bail!("세로쓰기 셀은 원본 배치를 유지합니다.");
        }
        if cell.height >= 0x8000_0000 || cell.width == 0 || cell.paragraphs.is_empty() {
            bail!("자동 크기 또는 비어 있는 셀 구조는 원본 배치를 유지합니다.");
        }
        if usize::from(cell.row) + usize::from(cell.row_span).max(1) > usize::from(table.row_count)
        {
            bail!("표의 셀 행 범위가 올바르지 않습니다.");
        }
        let mut cell_address = address.clone();
        cell_address.path.push(CellStep {
            control,
            cell: cell_index,
            paragraph: 0,
        });
        if plan.paths.len() >= MAX_PATHS {
            bail!("중첩 셀 측정 한도를 넘습니다.");
        }
        // Reflowing a cell includes every paragraph in its paragraph list.
        plan.paths.push(cell_address.clone());
        for (paragraph_index, paragraph) in cell.paragraphs.iter().enumerate() {
            cell_address
                .path
                .last_mut()
                .ok_or_else(|| anyhow!("빈 셀 주소"))?
                .paragraph = paragraph_index;
            let key = cell_address
                .path
                .iter()
                .map(|s| (s.control, s.cell, s.paragraph))
                .collect();
            let expected = compact(&paragraph.text);
            if !expected.is_empty() {
                plan.expected.insert(key, expected);
            }
            for (child_index, child) in paragraph.controls.iter().enumerate() {
                if let Control::Table(child) = child {
                    collect(child, &cell_address, child_index, plan)?;
                }
            }
        }
    }
    Ok(())
}

fn heights(table: &Table, result: &mut Vec<u32>) {
    result.push(table.common.height);
    for cell in &table.cells {
        result.push(cell.height);
        for paragraph in &cell.paragraphs {
            for control in &paragraph.controls {
                if let Control::Table(table) = control {
                    heights(table, result);
                }
            }
        }
    }
}

/// Public core APIs expose the effective page width, so a private clone can keep
/// a caller's DPI instead of assuming that screen pixels always mean 96 DPI.
pub(crate) fn source_dpi(source: &DocumentCore) -> Result<f64> {
    if source.page_count() == 0 {
        return Ok(96.0);
    }
    let info: Value = serde_json::from_str(&source.get_page_info_native(0).map_err(err)?)?;
    let section = info["sectionIndex"].as_u64().unwrap_or(0) as usize;
    let page = &source
        .document()
        .sections
        .get(section)
        .ok_or_else(|| anyhow!("표시할 구역이 없습니다."))?
        .section_def
        .page_def;
    let units = if page.width == 0 || page.height == 0 {
        // Native PageAreas treats either missing dimension as portrait A4,
        // including landscape sources. Match that existing fallback exactly.
        59_528
    } else if page.landscape {
        page.height
    } else {
        page.width
    };
    let width = source
        .build_page_render_tree(0)
        .map_err(err)?
        .root
        .bbox
        .width;
    let dpi = width * 7200.0 / f64::from(units);
    if !(36.0..=1200.0).contains(&dpi) {
        bail!("표시 DPI를 안전하게 복원하지 못했습니다.");
    }
    Ok(dpi)
}

pub(crate) fn clone_core(source: &DocumentCore, dpi: f64) -> Result<DocumentCore> {
    let mut core = DocumentCore::new_empty();
    core.set_dpi(dpi);
    core.set_font_environment(source.font_environment().cloned())
        .map_err(err)?;
    core.set_document(source.document().clone());
    Ok(core)
}

fn within(inner: (f64, f64, f64, f64), outer: (f64, f64, f64, f64)) -> bool {
    inner.0 >= outer.0 - 1.0
        && inner.1 >= outer.1 - 1.0
        && inner.2 <= outer.2 + 1.0
        && inner.3 <= outer.3 + 1.0
}

fn visible_text(core: &DocumentCore, plan: &Plan) -> Result<BTreeMap<TextKey, String>> {
    fn visit(
        node: &RenderNode,
        plan: &Plan,
        active: bool,
        frames: &[(f64, f64, f64, f64)],
        actual: &mut BTreeMap<TextKey, String>,
        budget: &mut usize,
    ) -> Result<()> {
        if *budget == 0 {
            bail!("중첩 표 표시 노드의 검사 한도를 넘습니다.");
        }
        *budget -= 1;
        let active = active
            || matches!(&node.node_type, RenderNodeType::Table(t)
            if t.cell_context.is_none() && t.section_index == Some(plan.root.section)
                && t.para_index == Some(plan.root.paragraph) && t.control_index == Some(plan.root.control));
        if !active {
            for child in &node.children {
                visit(child, plan, false, frames, actual, budget)?;
            }
            return Ok(());
        }
        let bbox = (
            node.bbox.x,
            node.bbox.y,
            node.bbox.x + node.bbox.width,
            node.bbox.y + node.bbox.height,
        );
        if matches!(node.node_type, RenderNodeType::Table(_)) {
            if !node.visible {
                bail!("숨겨진 표는 원본 표시 상태를 유지합니다.");
            }
            if frames.iter().any(|frame| !within(bbox, *frame)) {
                bail!("중첩 표의 테두리가 조상 셀 표시 영역을 벗어납니다.");
            }
        }
        let mut frames = frames.to_vec();
        if let RenderNodeType::TableCell(cell) = &node.node_type {
            if cell.page_fragment {
                bail!("쪽 경계에서 나뉜 셀은 원본 배치를 유지합니다.");
            }
            frames.push(bbox);
        }
        if let RenderNodeType::TextRun(run) = &node.node_type
            && let Some(context) = &run.cell_context
            && !context.in_textbox
            && !compact(&run.text).is_empty()
        {
            if !node.visible || frames.iter().any(|frame| !within(bbox, *frame)) {
                bail!("중첩 셀의 글자가 조상 셀 표시 영역을 벗어납니다.");
            }
            let key = context
                .path
                .iter()
                .map(|s| (s.control_index, s.cell_index, s.cell_para_index))
                .collect();
            actual.entry(key).or_default().push_str(&compact(&run.text));
        }
        for child in &node.children {
            visit(child, plan, true, &frames, actual, budget)?;
        }
        Ok(())
    }
    if core.page_count() > MAX_PAGES {
        bail!("문서 쪽 수가 중첩 표 표시 검사 한도를 넘습니다.");
    }
    let mut actual = BTreeMap::new();
    let mut budget = MAX_NODES;
    for page in 0..core.page_count() {
        let tree = core.build_page_render_tree(page).map_err(err)?;
        visit(&tree.root, plan, false, &[], &mut actual, &mut budget)?;
    }
    for (key, text) in &plan.expected {
        if !actual.get(key).is_some_and(|shown| shown.contains(text)) {
            bail!("원본 중첩 셀의 글자가 모두 표시되지 않습니다.");
        }
    }
    Ok(actual)
}

/// Build a presentation snapshot without changing source paragraphs, resources,
/// raw streams, provenance or editing history. Failed roots are discarded as a
/// unit. Reports deliberately expose fallback rather than asserting full parity.
pub(crate) fn build(source: &DocumentCore) -> Result<NestedPresentation> {
    build_within(source, MAX_WORK, MAX_TIME)
}

fn build_within(
    source: &DocumentCore,
    work: usize,
    time: std::time::Duration,
) -> Result<NestedPresentation> {
    let mut budget = Budget {
        pages: source.page_count().max(1) as usize,
        limit: work,
        spent: 0,
        deadline: std::time::Instant::now() + time,
    };
    let mut report = NormalizationReport::default();
    let mut plans = Vec::new();
    let mut path_count = 0;
    'sections: for (section, sec) in source.document().sections.iter().enumerate() {
        for (paragraph, para) in sec.paragraphs.iter().enumerate() {
            for (control, node) in para.controls.iter().enumerate() {
                let Control::Table(table) = node else {
                    continue;
                };
                if !has_nested(table) {
                    continue;
                }
                let root = Root {
                    section,
                    paragraph,
                    control,
                };
                report.roots_seen += 1;
                if report.roots_seen > MAX_ROOTS {
                    report.scan_truncated = true;
                    report.skipped.push(SkippedRoot {
                        section, paragraph, control,
                        reason: "중첩 표 표시 보정 개수 한도를 넘었습니다. 나머지 표는 원본 배치를 유지합니다.".into(),
                    });
                    break 'sections;
                }
                let mut plan = Plan {
                    root,
                    paths: Vec::new(),
                    expected: BTreeMap::new(),
                };
                let result = if sec.section_def.text_direction != 0 {
                    Err(anyhow!("세로쓰기 구역은 원본 배치를 유지합니다."))
                } else {
                    collect(
                        table,
                        &DocumentAddress {
                            section,
                            paragraph,
                            path: Vec::new(),
                        },
                        control,
                        &mut plan,
                    )
                };
                match result {
                    Ok(()) if path_count + plan.paths.len() <= MAX_PATHS => {
                        path_count += plan.paths.len();
                        plans.push(plan);
                    }
                    Ok(()) => report.skipped.push(SkippedRoot {
                        section,
                        paragraph,
                        control,
                        reason: "문서 전체의 중첩 셀 측정 한도를 넘습니다.".into(),
                    }),
                    Err(error) => report.skipped.push(SkippedRoot {
                        section,
                        paragraph,
                        control,
                        reason: error.to_string(),
                    }),
                }
            }
        }
    }
    if plans.is_empty() {
        return Ok(NestedPresentation { core: None, report });
    }
    let key = verdict_key(source, &plans, budget.limit)?;
    if let Some(previous) = VERDICTS.with(|v| {
        v.borrow()
            .as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, r)| r.clone())
    }) {
        return Ok(NestedPresentation {
            core: None,
            report: NormalizationReport {
                reused_previous: true,
                ..previous
            },
        });
    }
    let built = repair(source, plans, report, &mut budget)?;
    VERDICTS.with(|v| {
        // A verdict cut short by the wall clock depends on timing; never reuse it.
        let timed_out = built.report.skipped.iter().any(|s| s.reason == TIME_REASON);
        *v.borrow_mut() = (built.core.is_none() && !timed_out).then(|| (key, built.report.clone()));
    });
    Ok(built)
}

/// The measuring half of `build_within`: clone, grow and verify each root.
fn repair(
    source: &DocumentCore,
    plans: Vec<Plan>,
    mut report: NormalizationReport,
    budget: &mut Budget,
) -> Result<NestedPresentation> {
    let dpi = source_dpi(source)?;
    let mut result: Option<DocumentCore> = None;
    let mut accepted = Vec::new();
    let mut plans = plans.into_iter();
    while let Some(plan) = plans.next() {
        if let Err(reason) = budget.charge() {
            report.scan_truncated = true;
            for late in std::iter::once(plan).chain(plans.by_ref()) {
                report.skipped.push(SkippedRoot {
                    section: late.root.section,
                    paragraph: late.root.paragraph,
                    control: late.root.control,
                    reason: reason.into(),
                });
            }
            break;
        }
        let current = result.as_ref().unwrap_or(source);
        // Already visible tables need no clone or stored-frame changes.
        if visible_text(current, &plan).is_ok() {
            continue;
        }
        let attempt = (|| -> Result<DocumentCore> {
            let mut before = Vec::new();
            heights(table_at(current, plan.root)?, &mut before);
            // Cloning re-paginates the whole document once.
            budget.charge().map_err(|reason| anyhow!(reason))?;
            let mut scratch = Session::with_core(clone_core(current, dpi)?, None);
            // Deepest first; the v1 helper then propagates actual child heights
            // all the way to the body table. Repeating an ancestor never shrinks.
            for address in plan.paths.iter().rev() {
                budget
                    .charge_layouts(reflow_layouts(scratch.core.document(), address))
                    .map_err(|reason| anyhow!(reason))?;
                scratch.context_reflow_ancestors(address)?;
                report.paths_measured += 1;
            }
            let mut after = Vec::new();
            heights(table_at(&scratch.core, plan.root)?, &mut after);
            if before == after {
                bail!("표 높이 보정으로 해결되지 않는 배치입니다.");
            }
            budget.charge().map_err(|reason| anyhow!(reason))?;
            visible_text(&scratch.core, &plan)?;
            for previous in &accepted {
                budget.charge().map_err(|reason| anyhow!(reason))?;
                visible_text(&scratch.core, previous)?;
            }
            Ok(scratch.core)
        })();
        match attempt {
            Ok(core) => {
                result = Some(core);
                report.roots_adjusted += 1;
                accepted.push(plan);
            }
            Err(error) => report.skipped.push(SkippedRoot {
                section: plan.root.section,
                paragraph: plan.root.paragraph,
                control: plan.root.control,
                reason: error.to_string(),
            }),
        }
    }
    Ok(NestedPresentation {
        core: result,
        report,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fixture() -> Result<DocumentCore> {
        // One shared source owns every border/font/style allocation. Building
        // each table in a separate session would leave dangling resource IDs.
        let mut s = Session::new()?;
        s.execute(Command::Insert("자체 생성 본문".into()))?;
        s.run(
            "table.create",
            json!({"rows":1,"columns":1,"width_mm":110.0,"treat_as_char":true}),
        )?;
        let first = DocumentAddress::from_cursor(s.cursor);
        s.run(
            "context.replace",
            json!({"address":first,"start":0,"end":0,"text":"첫째"}),
        )?;
        let middle = s.run(
            "context.table.create",
            json!({"address":first,"rows":1,"columns":1,"width_mm":65.0}),
        )?;
        let middle: DocumentAddress = serde_json::from_value(middle["address"].clone())?;
        s.run(
            "context.replace",
            json!({"address":middle,"start":0,"end":0,"text":"둘째"}),
        )?;
        let leaf = s.run(
            "context.table.create",
            json!({"address":middle,"rows":1,"columns":1,"width_mm":30.0}),
        )?;
        let leaf: DocumentAddress = serde_json::from_value(leaf["address"].clone())?;
        s.run(
            "context.replace",
            json!({"address":leaf,"start":0,"end":0,"text":"세 단계😀"}),
        )?;
        fn shorten(t: &mut Table) {
            t.common.height = 1200;
            if t.raw_ctrl_data.len() >= 20 {
                t.raw_ctrl_data[16..20].copy_from_slice(&1200_u32.to_le_bytes());
            }
            for cell in &mut t.cells {
                cell.height = 1200;
                for p in &mut cell.paragraphs {
                    for c in &mut p.controls {
                        if let Control::Table(t) = c {
                            shorten(t);
                        }
                    }
                }
            }
        }
        let mut document = s.core.document().clone();
        for sec in &mut document.sections {
            sec.raw_stream = None;
            for p in &mut sec.paragraphs {
                for c in &mut p.controls {
                    if let Control::Table(t) = c {
                        shorten(t);
                    }
                }
            }
        }
        // Rebuild in a new core so no previously grown layout/cache is reused.
        let mut core = DocumentCore::new_empty();
        core.set_document(document);
        // This fixture models an unchanged imported document with authoritative
        // stored frames. Serialization is fixture construction only; build()
        // itself never serializes or reparses its input.
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("self-created-short-frames.hwp");
        hwpcraft_formats::save(&core, &path)?;
        let stored = hwpcraft_formats::open(&path)?;
        // Persist generated line caches once as an ordinary saved HWP would.
        // The next open trusts these caches and reproduces the short frames.
        hwpcraft_formats::save(&stored, &path)?;
        let result = hwpcraft_formats::open(&path)?;
        assert_resource_references(&result);
        Ok(result)
    }

    fn assert_resource_references(core: &DocumentCore) {
        fn check(
            t: &Table,
            info: &hwpcraft_doc::model::document::DocInfo,
            ids: &mut std::collections::BTreeSet<u32>,
        ) {
            assert!(
                t.border_fill_id > 0 && usize::from(t.border_fill_id) <= info.border_fills.len()
            );
            assert!(
                ids.insert(t.common.instance_id),
                "duplicate table instance ID"
            );
            for cell in &t.cells {
                assert!(usize::from(cell.border_fill_id) <= info.border_fills.len());
                for p in &cell.paragraphs {
                    assert!(usize::from(p.para_shape_id) < info.para_shapes.len());
                    for shape in &p.char_shapes {
                        assert!((shape.char_shape_id as usize) < info.char_shapes.len());
                    }
                    for c in &p.controls {
                        if let Control::Table(t) = c {
                            check(t, info, ids);
                        }
                    }
                }
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        let document = core.document();
        for c in document
            .sections
            .iter()
            .flat_map(|s| &s.paragraphs)
            .flat_map(|p| &p.controls)
        {
            if let Control::Table(t) = c {
                check(t, &document.doc_info, &mut ids);
            }
        }
        assert_eq!(ids.len(), 3);
    }

    fn plan(core: &DocumentCore) -> Result<Plan> {
        for (section, sec) in core.document().sections.iter().enumerate() {
            for (paragraph, p) in sec.paragraphs.iter().enumerate() {
                for (control, c) in p.controls.iter().enumerate() {
                    if let Control::Table(t) = c
                        && has_nested(t)
                    {
                        let mut result = Plan {
                            root: Root {
                                section,
                                paragraph,
                                control,
                            },
                            paths: Vec::new(),
                            expected: BTreeMap::new(),
                        };
                        collect(
                            t,
                            &DocumentAddress {
                                section,
                                paragraph,
                                path: Vec::new(),
                            },
                            control,
                            &mut result,
                        )?;
                        return Ok(result);
                    }
                }
            }
        }
        bail!("nested fixture missing")
    }

    fn semantics(table: &Table) -> Value {
        Value::Array(table.cells.iter().map(|c| Value::Array(c.paragraphs.iter().map(|p| {
            json!({"text":p.text,"char_shapes":p.char_shapes,"para_shape":p.para_shape_id,
                "style":p.style_id,"controls":p.controls.iter().map(|c| {
                    if let Control::Table(t) = c { semantics(t) } else { serde_json::to_value(c).unwrap() }
                }).collect::<Vec<_>>()})
        }).collect())).collect())
    }

    fn emit(core: &DocumentCore, name: &str) -> Result<()> {
        if std::env::var("HWPCRAFT_WRITE_NESTED_PRESENTATION_FIXTURES").as_deref() != Ok("1") {
            return Ok(());
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let out = root.join("output/implementation/0.17/nested-presentation");
        std::fs::create_dir_all(&out)?;
        std::fs::write(
            out.join(format!("{name}.svg")),
            core.render_page_svg_native(0).map_err(err)?,
        )?;
        std::fs::write(
            out.join(format!("{name}.pdf")),
            core.render_pages_pdf_native_with_options(&[0], &hwpcraft_formats::pdf_options())
                .map_err(err)?,
        )?;
        Ok(())
    }

    #[test]
    fn undersized_frames_grow_on_clone_with_visible_three_level_glyphs() -> Result<()> {
        let source = fixture()?;
        let p = plan(&source)?;
        assert!(
            visible_text(&source, &p).is_err(),
            "fixture must reproduce clipped glyphs"
        );
        let before = format!("{:?}", source.document());
        let info = format!("{:?}", source.document().doc_info);
        let semantic = semantics(table_at(&source, p.root)?);
        let normalized = build(&source)?;
        assert_eq!(
            normalized.report.roots_adjusted, 1,
            "{:?}",
            normalized.report
        );
        assert!(
            normalized.report.skipped.is_empty(),
            "{:?}",
            normalized.report
        );
        let rendered = normalized.core.as_ref().unwrap();
        visible_text(rendered, &p)?;
        assert_eq!(semantics(table_at(rendered, p.root)?), semantic);
        assert_eq!(format!("{:?}", rendered.document().doc_info), info);
        assert_eq!(format!("{:?}", source.document()), before);
        assert_eq!(source.document().provenance, rendered.document().provenance);
        assert_eq!(
            source.document().is_hwpx_variant,
            rendered.document().is_hwpx_variant
        );
        assert_eq!(source.font_environment(), rendered.font_environment());
        emit(rendered, "generated-normalized")?;
        let second = build(rendered)?;
        assert!(
            second.core.is_none(),
            "visible frames must not keep growing"
        );
        assert_eq!(second.report.roots_adjusted, 0);
        Ok(())
    }

    /// Opening must not wait on the repair pass: with its budget spent, every
    /// undersized root keeps its original frame and is reported, no measurement
    /// runs, and the source stays untouched. The work limit is the normal,
    /// deterministic stop; the clock is only a safety stop.
    #[test]
    fn spent_budget_keeps_original_frames_and_reports_every_root() -> Result<()> {
        let source = fixture()?;
        let before = format!("{:?}", source.document());
        let hour = std::time::Duration::from_secs(3600);
        for (work, time, reason) in [
            (0, hour, WORK_REASON),
            (MAX_WORK, std::time::Duration::ZERO, TIME_REASON),
        ] {
            let normalized = build_within(&source, work, time)?;
            let report = &normalized.report;
            assert!(normalized.core.is_none(), "{report:?}");
            assert!(report.scan_truncated, "{report:?}");
            assert_eq!(report.roots_adjusted, 0);
            assert_eq!(report.paths_measured, 0);
            assert_eq!(report.skipped.len(), 1);
            assert_eq!(report.skipped[0].reason, reason);
        }
        assert_eq!(format!("{:?}", source.document()), before);
        // The default budget still repairs the same document, deterministically.
        for _ in 0..2 {
            let report = build(&source)?.report;
            assert_eq!(report.roots_adjusted, 1, "{report:?}");
            assert!(!report.scan_truncated, "{report:?}");
        }
        Ok(())
    }

    /// Typing outside the candidate tables reuses a "nothing repaired" verdict;
    /// editing inside a nested table, or a different work limit, recomputes.
    #[test]
    fn unchanged_candidates_reuse_the_previous_verdict() -> Result<()> {
        let hour = std::time::Duration::from_secs(3600);
        let mut s = Session::with_core(fixture()?, None);
        let first = build_within(&s.core, 0, hour)?;
        assert!(first.core.is_none());
        assert!(!first.report.reused_previous);
        // Type after the root table in its host paragraph.
        let p = plan(&s.core)?;
        let host = Cursor {
            section: p.root.section,
            paragraph: p.root.paragraph,
            offset: s
                .paragraph(Cursor {
                    section: p.root.section,
                    paragraph: p.root.paragraph,
                    ..Default::default()
                })?
                .text
                .chars()
                .count(),
            cell: None,
        };
        s.select(host, false)?;
        s.execute(Command::Insert("가".into()))?;
        let again = build_within(&s.core, 0, hour)?;
        assert!(again.report.reused_previous, "{:?}", again.report);
        assert_eq!(again.report.skipped.len(), first.report.skipped.len());
        // Another limit is another verdict.
        assert!(!build_within(&s.core, 1, hour)?.report.reused_previous);
        build_within(&s.core, 0, hour)?;
        // Editing inside the nested table changes the root's fingerprint.
        let leaf = plan(&s.core)?
            .paths
            .into_iter()
            .max_by_key(|a| a.path.len())
            .unwrap();
        s.run(
            "context.replace",
            json!({"address": leaf, "start": 0, "end": 0, "text": "추가"}),
        )?;
        let edited = build_within(&s.core, 0, hour)?;
        assert!(!edited.report.reused_previous, "{:?}", edited.report);
        // A clock-stopped verdict is never stored.
        build_within(&s.core, MAX_WORK, std::time::Duration::ZERO)?;
        assert!(
            !build_within(&s.core, MAX_WORK, std::time::Duration::ZERO)?
                .report
                .reused_previous
        );
        Ok(())
    }

    /// A single path through a giant ancestor cell is refused before it runs:
    /// its cost grows with the cell's paragraph count, not just one step.
    #[test]
    fn path_cost_counts_every_ancestor_cell_paragraph() -> Result<()> {
        let source = fixture()?;
        let p = plan(&source)?;
        let deepest = p.paths.iter().max_by_key(|a| a.path.len()).unwrap();
        let base = reflow_layouts(source.document(), deepest);
        assert!(base >= deepest.path.len() * 2, "{base} for {deepest:?}");
        // Add 200 paragraphs to the outermost ancestor cell.
        let mut document = source.document().clone();
        let host = &mut document.sections[deepest.section].paragraphs[deepest.paragraph];
        let step = &deepest.path[0];
        let Some(Control::Table(table)) = host.controls.get_mut(step.control) else {
            panic!("fixture root is a table");
        };
        let cell = &mut table.cells[step.cell];
        let filler = cell.paragraphs[0].clone();
        for _ in 0..200 {
            cell.paragraphs.push(filler.clone());
        }
        assert_eq!(reflow_layouts(&document, deepest), base + 200);
        let mut budget = Budget {
            pages: 39,
            limit: MAX_WORK,
            spent: 0,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(3600),
        };
        assert_eq!(
            budget.charge_layouts(reflow_layouts(&document, deepest)),
            Err(WORK_REASON),
            "a giant-cell path on a long document is refused upfront"
        );
        assert_eq!(budget.spent, 0);
        assert_eq!(budget.charge_layouts(base), Ok(()));
        Ok(())
    }

    #[test]
    fn protected_relative_vertical_and_caption_roots_have_explicit_fallback() -> Result<()> {
        for mode in 0..7 {
            let mut source = fixture()?;
            let root = plan(&source)?.root;
            let doc = source.document_mut();
            let sec = &mut doc.sections[root.section];
            let Control::Table(t) = &mut sec.paragraphs[root.paragraph].controls[root.control]
            else {
                unreachable!()
            };
            match mode {
                0 => t.common.size_protect = true,
                1 => t.common.locked = true,
                2 => t.cells[0].set_cell_protect(true),
                3 => t.common.height_criterion = SizeCriterion::Page,
                4 => t.cells[0].text_direction = 1,
                5 => sec.section_def.text_direction = 1,
                _ => t.caption = Some(Default::default()),
            }
            let before = format!("{:?}", source.document());
            let result = build(&source)?;
            assert!(result.core.is_none());
            assert_eq!(result.report.skipped.len(), 1);
            assert!(!result.report.skipped[0].reason.is_empty());
            assert_eq!(format!("{:?}", source.document()), before);
        }
        Ok(())
    }

    #[test]
    fn clone_keeps_nondefault_dpi_and_never_shrinks_declared_minima() -> Result<()> {
        let mut source = fixture()?;
        source.set_dpi(144.0);
        source
            .set_font_environment(Some(serde_json::from_value(json!({
                "id":"own-nested-presentation-test", "substitutions":{}
            }))?))
            .map_err(err)?;
        assert!((source_dpi(&source)? - 144.0).abs() < 0.01);
        let p = plan(&source)?;
        let mut before = Vec::new();
        heights(table_at(&source, p.root)?, &mut before);
        let normalized = build(&source)?;
        let core = normalized
            .core
            .as_ref()
            .ok_or_else(|| anyhow!("{:?}", normalized.report))?;
        assert!((source_dpi(core)? - 144.0).abs() < 0.01);
        assert_eq!(source.font_environment(), core.font_environment());
        let mut after = Vec::new();
        heights(table_at(core, p.root)?, &mut after);
        assert_eq!(before.len(), after.len());
        assert!(after.iter().zip(before).all(|(a, b)| *a >= b));
        visible_text(core, &p)?;
        Ok(())
    }

    #[test]
    fn dpi_inference_matches_native_a4_fallback_for_missing_page_dimensions() -> Result<()> {
        for landscape in [false, true] {
            for missing in 0..3 {
                let mut source = blank()?;
                let page = &mut source.document_mut().sections[0].section_def.page_def;
                page.landscape = landscape;
                if missing != 1 {
                    page.width = 0;
                }
                if missing != 0 {
                    page.height = 0;
                }
                source.set_dpi(144.0);
                // Native set_dpi does not invalidate an already clean page's
                // cached layout. Publish after setting DPI so the fixture has
                // one coherent native page/layout generation.
                source.set_document(source.document().clone());
                let inferred = source_dpi(&source)?;
                assert!(
                    (inferred - 144.0).abs() < 0.01,
                    "landscape={landscape}, missing={missing}, inferred={inferred}, tree={:?}, info={}",
                    source.build_page_render_tree(0).map_err(err)?.root.bbox,
                    source.get_page_info_native(0).map_err(err)?,
                );
                let cloned = clone_core(&source, source_dpi(&source)?)?;
                assert!((source_dpi(&cloned)? - 144.0).abs() < 0.01);
            }
        }
        Ok(())
    }

    #[test]
    fn source_session_history_selection_raw_streams_and_source_bytes_stay_unchanged() -> Result<()>
    {
        let core = fixture()?;
        let dir = tempfile::tempdir()?;
        for extension in ["hwp", "hwpx"] {
            let path = dir.path().join(format!("own-nested.{extension}"));
            hwpcraft_formats::save(&core, &path)?;
            let original_bytes = std::fs::read(&path)?;
            let imported = hwpcraft_formats::open(&path)?;
            let mut session = Session::with_core(imported, Some(path.clone()));
            let p = plan(&session.core)?;
            assert!(
                visible_text(&session.core, &p).is_err(),
                "{extension}: fixture must need repair"
            );
            // A selection must not reflow and accidentally repair the imported
            // source before the presentation-only path is exercised.
            session.select(
                Cursor {
                    section: 0,
                    paragraph: 0,
                    offset: 0,
                    cell: None,
                },
                false,
            )?;
            session.select(
                Cursor {
                    section: 0,
                    paragraph: 0,
                    offset: 1,
                    cell: None,
                },
                true,
            )?;
            let before = (
                format!("{:?}", session.core.document()),
                session.cursor,
                session.anchor,
                session.span_anchor,
                session.generation,
                session.revision,
                session.saved_revision,
                session.undo.len(),
                session.redo.len(),
                session.dirty(),
            );
            let result = build(&session.core)?;
            assert!(result.core.is_some(), "{:?}", result.report);
            visible_text(result.core.as_ref().unwrap(), &p)?;
            let runtime = session
                .page_field_layout()?
                .ok_or_else(|| anyhow!("{extension}: missing normalized runtime core"))?;
            visible_text(&runtime.core, &p)?;
            assert!(session.render_pdf(&[0])?.starts_with(b"%PDF-"));
            let after = (
                format!("{:?}", session.core.document()),
                session.cursor,
                session.anchor,
                session.span_anchor,
                session.generation,
                session.revision,
                session.saved_revision,
                session.undo.len(),
                session.redo.len(),
                session.dirty(),
            );
            assert_eq!(before, after);
            assert_eq!(std::fs::read(path)?, original_bytes);
        }
        Ok(())
    }

    #[test]
    fn runtime_outputs_use_normalized_nested_frames_without_any_page_fields() -> Result<()> {
        let source = fixture()?;
        let before = format!("{:?}", source.document());
        let source_info = format!("{:?}", source.document().doc_info);
        let session = Session::with_core(source, None);
        let p = plan(&session.core)?;
        assert!(visible_text(&session.core, &p).is_err());
        let layout = session
            .page_field_layout()?
            .ok_or_else(|| anyhow!("missing normalized runtime core"))?;
        assert_eq!(layout.field_count(), 0);
        visible_text(&layout.core, &p)?;
        assert_eq!(session.page_count()?, layout.core.page_count());
        let svg = session.render_page_svg(0)?;
        // Native SVG may emit one tspan per scalar rather than one word.
        // The render-tree check above verifies their complete visible order.
        assert!(["세", "단", "계"].iter().all(|glyph| svg.contains(glyph)));
        assert!(session.render_pdf(&[0])?.starts_with(b"%PDF-"));
        let report = session.presentation_report()?;
        assert_eq!(report["pageFields"], 0);
        assert_eq!(report["nestedTables"]["roots_adjusted"], 1, "{report}");
        assert_eq!(format!("{:?}", session.core.document()), before);
        assert_eq!(
            format!("{:?}", session.core.document().doc_info),
            source_info
        );
        assert!(!session.dirty());
        assert_eq!(session.generation, 0);
        assert!(!session.can_undo());
        Ok(())
    }

    #[test]
    fn runtime_markpen_uses_displayed_geometry_after_nested_frame_growth() -> Result<()> {
        use crate::presentation_geometry::{Affinity, CaretLookup};
        use hwpcraft_doc::model::paragraph::{CharShapeRef, MarkpenMark};
        for in_cell in [false, true] {
            let mut source = fixture()?;
            let mut tail = Paragraph {
                text: "아래 본문 강조".into(),
                has_para_text: true,
                char_shapes: vec![CharShapeRef {
                    start_pos: 0,
                    char_shape_id: 0,
                }],
                markpen_marks: vec![
                    MarkpenMark {
                        char_idx: 0,
                        color: Some("#ff00ff".into()),
                        utf16_pos: None,
                    },
                    MarkpenMark {
                        char_idx: 1,
                        color: None,
                        utf16_pos: None,
                    },
                ],
                ..Default::default()
            };
            let mut position = 0;
            for ch in tail.text.chars() {
                tail.char_offsets.push(position);
                position += ch.len_utf16() as u32;
            }
            tail.char_count = position + 1;
            let mut doc = source.document().clone();
            let root = plan(&source)?.root;
            let cursor = if in_cell {
                let Control::Table(table) = &mut doc.sections[root.section].paragraphs
                    [root.paragraph]
                    .controls[root.control]
                else {
                    unreachable!()
                };
                let index = table.cells[0].paragraphs.len();
                table.cells[0].paragraphs.push(tail);
                Cursor {
                    section: root.section,
                    paragraph: root.paragraph,
                    offset: 0,
                    cell: Some(Cell {
                        control: root.control,
                        index: 0,
                        paragraph: index,
                    }),
                }
            } else {
                let index = doc.sections[0].paragraphs.len();
                doc.sections[0].paragraphs.push(tail);
                Cursor {
                    paragraph: index,
                    ..Cursor::default()
                }
            };
            doc.sections[0].raw_stream = None;
            source.set_document(doc);
            let before = format!("{:?}", source.document());
            let session = Session::with_core(source, None);
            let p = plan(&session.core)?;
            assert!(visible_text(&session.core, &p).is_err());
            let layout = session
                .page_field_layout()?
                .ok_or_else(|| anyhow!("missing nested presentation"))?;
            visible_text(&layout.core, &p)?;
            let CaretLookup::Text(expected) = layout.geometry.caret_from_source(
                &layout.core,
                &session.core,
                layout.geometry_revision,
                &cursor.into(),
                Affinity::Downstream,
                None,
            )?
            else {
                bail!("highlight target has no owning displayed scalar");
            };
            assert!(expected.visible);
            let rects = session.output_highlight_rects();
            assert_eq!(rects.len(), 1, "{rects:?}");
            let rect = &rects[0];
            assert_eq!(rect.page, expected.page);
            assert!((rect.x - expected.x).abs() < 0.01);
            assert!((rect.y - expected.y).abs() < 0.01);
            assert!((rect.height - expected.height).abs() < 0.01);
            assert!((rect.width - expected.advance.abs().max(2.0)).abs() < 0.01);
            assert!(
                session
                    .render_page_svg(0)?
                    .contains("data-hwpcraft-markpen")
            );
            assert!(session.render_pdf(&[0])?.starts_with(b"%PDF-"));
            assert_eq!(format!("{:?}", session.core.document()), before);
        }
        Ok(())
    }

    #[test]
    #[ignore = "local self-created native HWP oracle, explicitly run when available"]
    fn native_three_depth_short_frame_oracle_is_visible_without_source_changes() -> Result<()> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../output/implementation/0.16/nested-interop/pre-growth-native-roundtrip.hwp",
        );
        let source = hwpcraft_formats::open(&path)?;
        let source_bytes = std::fs::read(&path)?;
        let p = plan(&source)?;
        assert!(visible_text(&source, &p).is_err());
        let before = format!("{:?}", source.document());
        let normalized = build(&source)?;
        assert_eq!(
            normalized.report.roots_adjusted, 1,
            "{:?}",
            normalized.report
        );
        assert!(normalized.report.skipped.is_empty());
        let rendered = normalized.core.as_ref().unwrap();
        visible_text(rendered, &p)?;
        assert_eq!(
            semantics(table_at(&source, p.root)?),
            semantics(table_at(rendered, p.root)?)
        );
        assert_eq!(format!("{:?}", source.document()), before);
        assert_eq!(std::fs::read(path)?, source_bytes);
        emit(rendered, "native-normalized")?;
        Ok(())
    }

    /// Timing probe for the per-edit cost on a long public document. Run with
    /// `HWPCRAFT_LATENCY_DOC=<path> cargo test --release -- --ignored latency`.
    #[test]
    #[ignore = "timing probe on a local long document"]
    fn latency_probe_on_long_document() -> Result<()> {
        let Ok(path) = std::env::var("HWPCRAFT_LATENCY_DOC") else {
            return Ok(());
        };
        let source = hwpcraft_formats::open(Path::new(&path))?;
        for round in 0..3 {
            let started = std::time::Instant::now();
            let built = build(&source)?;
            eprintln!(
                "round {round}: build {:?}, pages {}, core {}, report {}",
                started.elapsed(),
                source.page_count(),
                built.core.is_some(),
                serde_json::to_string(&built.report)?
            );
        }
        Ok(())
    }
}
