//! HwpCraft-owned bounded body snippets; no filesystem or settings side effects.
//! Source and destination are whole body-paragraph boundaries. Unsupported
//! controls fail closed rather than becoming plain text.
use super::*;
use hwpcraft_doc::model::{
    document::Document,
    shape::{Caption, ShapeObject},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Cursor as ByteCursor, Read, Write},
};

pub(crate) const MAX_PAYLOAD: usize = 8 * 1024 * 1024;
pub(crate) const MAX_EXPANDED: usize = 32 * 1024 * 1024;
pub(crate) const MAX_ZIP_ITEMS: usize = 1_024;
pub(crate) const MAX_DEPTH: usize = 32;
pub(crate) const MAX_NODES: usize = 16_000;
const MAX_SVG: usize = 4 * 1024 * 1024;
const MAX_PARAGRAPHS: usize = 1_024;
const MAX_BINARY: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRange {
    pub section: usize,
    pub start: usize,
    pub end: usize,
}

pub(crate) struct CapturedFragment {
    pub payload: Vec<u8>,
    pub summary: String,
    pub paragraphs: usize,
    pub stats: Value,
}

pub(crate) fn resolve_source(session: &Session, range: Option<SourceRange>) -> Result<SourceRange> {
    guard_document(session)?;
    if session.cursor.cell.is_some() {
        bail!("본문 상용구는 표 밖 본문에서만 등록할 수 있습니다.");
    }
    let range = if let Some(range) = range {
        range
    } else if session.has_selection() {
        let (start, end) = session.span().unwrap_or_else(|| {
            let r = session.selection();
            (
                Cursor {
                    offset: r.start,
                    ..session.cursor
                },
                Cursor {
                    offset: r.end,
                    ..session.cursor
                },
            )
        });
        if start.cell.is_some()
            || end.cell.is_some()
            || start.section != end.section
            || start.offset != 0
        {
            bail!("본문의 완전한 문단들만 선택하세요.");
        }
        let end_para = session
            .core
            .document()
            .sections
            .get(end.section)
            .and_then(|section| section.paragraphs.get(end.paragraph))
            .ok_or_else(|| anyhow!("상용구 선택 끝 문단이 없습니다."))?;
        let exclusive = if end.offset == 0 && end.paragraph > start.paragraph {
            end.paragraph
        } else if end.offset == end_para.text.chars().count() {
            end.paragraph
                .checked_add(1)
                .ok_or_else(|| anyhow!("상용구 문단 범위 초과"))?
        } else {
            bail!("부분 글자 선택은 본문 상용구로 등록할 수 없습니다.");
        };
        SourceRange {
            section: start.section,
            start: start.paragraph,
            end: exclusive,
        }
    } else {
        SourceRange {
            section: session.cursor.section,
            start: session.cursor.paragraph,
            end: session
                .cursor
                .paragraph
                .checked_add(1)
                .ok_or_else(|| anyhow!("상용구 문단 범위 초과"))?,
        }
    };
    let section = session
        .core
        .document()
        .sections
        .get(range.section)
        .ok_or_else(|| anyhow!("상용구 구역이 없습니다."))?;
    if range.start >= range.end
        || range.end > section.paragraphs.len()
        || range.end - range.start > MAX_PARAGRAPHS
    {
        bail!("상용구는 같은 구역의 완전한 문단 1–1024개여야 합니다.");
    }
    if section.section_def.text_direction != 0 {
        bail!("세로쓰기 구역의 본문 상용구는 지원하지 않습니다.");
    }
    Ok(range)
}

fn generation(session: &Session, expected: u64) -> Result<()> {
    if session.generation != expected {
        bail!("문서가 변경돼 상용구 범위/위치를 다시 조회해야 합니다.");
    }
    Ok(())
}

fn guard_document(session: &Session) -> Result<()> {
    if session.core.document().header.distribution {
        bail!("배포용 보호 문서의 본문 상용구는 지원하지 않습니다.");
    }
    session.guard_protected()
}

/// Where a body snippet goes: before a paragraph, or into the middle of one (the
/// paragraph is split there first).
#[derive(Clone, Copy)]
enum Spot {
    Before(usize),
    Middle { paragraph: usize, offset: usize },
}

impl Spot {
    /// The paragraph the snippet's first paragraph takes the place of.
    fn before(self) -> usize {
        match self {
            Spot::Before(before) => before,
            Spot::Middle { paragraph, .. } => paragraph + 1,
        }
    }
}

fn destination(session: &Session) -> Result<Spot> {
    guard_document(session)?;
    if session.cursor.cell.is_some() || session.has_selection() {
        bail!("본문 상용구는 선택 없이 표 밖 본문의 캐럿 자리에 넣으세요.");
    }
    let paragraph = session.paragraph(session.cursor)?;
    if session.cursor.offset == 0 {
        if session.cursor.paragraph == 0 {
            let prefix = paragraph
                .controls
                .iter()
                .take_while(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
                .count();
            if paragraph
                .controls
                .iter()
                .skip(prefix)
                .any(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
                || (prefix == 0
                    && (paragraph.raw_break_type & 3 != 0
                        || matches!(
                            paragraph.column_type,
                            hwpcraft_doc::model::paragraph::ColumnBreakType::Section
                                | hwpcraft_doc::model::paragraph::ColumnBreakType::MultiColumn
                        )))
            {
                bail!("대상 첫 문단의 구역/단 정의 배치는 본문 상용구에서 지원하지 않습니다.");
            }
        }
        Ok(Spot::Before(session.cursor.paragraph))
    } else if session.cursor.offset == paragraph.text.chars().count() {
        session
            .cursor
            .paragraph
            .checked_add(1)
            .map(Spot::Before)
            .ok_or_else(|| anyhow!("상용구 삽입 위치 초과"))
    } else {
        // The split takes RHWP's logical offset (an inline object counts once).
        Ok(Spot::Middle {
            paragraph: session.cursor.paragraph,
            offset: session.split_offset(session.cursor)?,
        })
    }
}

fn request(range: SourceRange, section: usize, before: usize) -> Value {
    json!({"sourceSection":range.section,"sourceStart":range.start,"sourceEnd":range.end,
        "targetSection":section,"insertBefore":before,"count":1,
        "limits":{"block":{"maxCopies":1,"maxParagraphs":MAX_PARAGRAPHS,"maxNodes":MAX_NODES,
            "maxDepth":MAX_DEPTH,"maxStructureBytes":MAX_EXPANDED,"maxMappingBytes":2*1024*1024,
            "maxDocumentNodes":100_000},"maxResourceMetadataBytes":8*1024*1024,"maxBinaryBytes":MAX_BINARY}})
}

pub(crate) fn import(
    core: &mut DocumentCore,
    source: &Document,
    range: SourceRange,
    section: usize,
    before: usize,
) -> Result<Value> {
    let raw = core
        .import_paragraph_block_json_native(
            source,
            &json!({"request":request(range,section,before),"dryRun":false}).to_string(),
        )
        .map_err(err)?;
    let result: Value = serde_json::from_str(&raw)?;
    Ok(result["operationResult"]["result"].clone())
}

pub(crate) fn capture(
    session: &Session,
    range: Option<SourceRange>,
    expected_generation: u64,
) -> Result<CapturedFragment> {
    generation(session, expected_generation)?;
    let range = resolve_source(session, range)?;
    let mut source = bounded_source_copy(session.core.document())?;
    normalize_prefix(
        &mut source.sections[range.section].paragraphs[range.start],
        range.start == 0,
    )?;
    let source_paras = &source.sections[range.section].paragraphs[range.start..range.end];
    let mut expected = Profile::new(MAX_NODES);
    expected.paras(source_paras, 1, true)?;
    if !expected.content {
        bail!("상용구로 등록할 본문 내용이 없습니다.");
    }
    let mut compact = blank()?;
    let report = import(&mut compact, &source, range, 0, 0)?;
    let start = report["inserted"]["start"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| anyhow!("상용구 import 시작 위치가 없습니다."))?;
    let end = report["inserted"]["end"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| anyhow!("상용구 import 끝 위치가 없습니다."))?;
    let mut document = compact.document().clone();
    let kept = document.sections[0]
        .paragraphs
        .get(start..end)
        .ok_or_else(|| anyhow!("상용구 import 범위가 잘못됐습니다."))?
        .to_vec();
    document.sections[0].paragraphs = kept;
    document.sections[0].raw_stream = None;
    document.sections[0].raw_provenance = None;
    // This document came from a fresh blank, never the source envelope.
    document.preview = None;
    document.extra_streams.clear();
    document.hwpx_aux_entries.clear();
    compact.set_document(document);
    let payload = hwpcraft_formats::auto_number_formats::export(&compact, None)?;
    if payload.len() > MAX_PAYLOAD {
        bail!("본문 상용구 HWPX는 8 MiB 이하입니다.");
    }
    let reloaded = decode(&payload)?;
    let mut actual = Profile::new(MAX_NODES);
    actual.paras(&reloaded.document().sections[0].paragraphs, 1, true)?;
    if reloaded.document().sections[0].paragraphs.len() != range.end - range.start
        || expected.text != actual.text
        || expected.tables != actual.tables
        || expected.pictures != actual.pictures
        || expected.shapes != actual.shapes
    {
        bail!("상용구 저장/재열기에서 본문·개체가 달라져 등록을 취소했습니다.");
    }
    let summary = actual.summary();
    Ok(CapturedFragment {
        payload,
        summary,
        paragraphs: range.end - range.start,
        stats: actual.json(),
    })
}

pub(crate) fn decode(payload: &[u8]) -> Result<DocumentCore> {
    validate_package(payload)?;
    let mut core = hwpcraft_formats::auto_number_formats::open(payload, None)?;
    let mut document = core.document().clone();
    if document.sections.len() != 1
        || document.sections[0].paragraphs.is_empty()
        || document.sections[0].paragraphs.len() > MAX_PARAGRAPHS
        || document.header.distribution
    {
        bail!("상용구 payload는 보호되지 않은 한 구역의 본문 문단이어야 합니다.");
    }
    normalize_prefix(&mut document.sections[0].paragraphs[0], true)?;
    let mut profile = Profile::new(MAX_NODES);
    profile.paras(&document.sections[0].paragraphs, 1, true)?;
    if !profile.content {
        bail!("상용구 payload에 본문 내용이 없습니다.");
    }
    // The same strict resource/identity preparation as insertion, without accepting
    // external BinData paths or silently resolving unavailable lazy storage.
    let target = blank()?;
    let options = serde_json::from_value(request(
        SourceRange {
            section: 0,
            start: 0,
            end: document.sections[0].paragraphs.len(),
        },
        0,
        0,
    ))?;
    target
        .preview_paragraph_block_import_native(&document, &options)
        .map_err(err)?;
    document.sections[0].raw_stream = None;
    document.sections[0].raw_provenance = None;
    core.set_document(document);
    Ok(core)
}

pub(crate) fn preview(
    session: &Session,
    payload: &[u8],
    expected_generation: u64,
) -> Result<Value> {
    generation(session, expected_generation)?;
    let before = destination(session)?.before();
    let core = decode(payload)?;
    let paragraphs = core.document().sections[0].paragraphs.len();
    let options = serde_json::from_value(request(
        SourceRange {
            section: 0,
            start: 0,
            end: paragraphs,
        },
        session.cursor.section,
        before,
    ))?;
    let report = session
        .core
        .preview_paragraph_block_import_native(core.document(), &options)
        .map_err(err)?;
    let mut profile = Profile::new(MAX_NODES);
    profile.paras(&core.document().sections[0].paragraphs, 1, true)?;
    let svg = core.render_page_svg_native(0).map_err(err)?;
    if svg.len() > MAX_SVG {
        bail!("상용구 미리 보기 SVG가 4 MiB 제한을 초과했습니다.");
    }
    Ok(json!({"svg":svg,"paragraphs":paragraphs,"summary":profile.summary(),"import":report}))
}

pub(crate) fn insert(
    session: &mut Session,
    payload: &[u8],
    expected_generation: u64,
) -> Result<Value> {
    generation(session, expected_generation)?;
    let spot = destination(session)?;
    let before = spot.before();
    let source = decode(payload)?;
    let paragraphs = source.document().sections[0].paragraphs.len();
    // A one-paragraph snippet without a table goes into the text like typing; a
    // longer one stands between the two halves of the split paragraph.
    let inline = paragraphs == 1
        && !source.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .any(|c| matches!(c, Control::Table(_)));
    let section = session.cursor.section;
    let mut report = Value::Null;
    session.transaction(|s| {
        if let Spot::Middle { paragraph, offset } = spot {
            s.core
                .split_paragraph_native(section, paragraph, offset, None)
                .map_err(err)?;
        }
        report = import(
            &mut s.core,
            source.document(),
            SourceRange {
                section: 0,
                start: 0,
                end: paragraphs,
            },
            section,
            before,
        )?;
        if before == 0 {
            move_target_prologue(&mut s.core, section, paragraphs)?;
        }
        // A short import must roll the transaction back, not panic inside it.
        let last = (before + paragraphs)
            .checked_sub(1)
            .ok_or_else(|| anyhow!("본문 상용구에 넣을 문단이 없습니다."))?;
        let length = |s: &Session, paragraph: usize| -> Result<usize> {
            Ok(s.core
                .document()
                .sections
                .get(section)
                .and_then(|sec| sec.paragraphs.get(paragraph))
                .ok_or_else(|| anyhow!("본문 상용구를 넣은 뒤 문단 위치를 찾지 못했습니다."))?
                .text
                .chars()
                .count())
        };
        let mut paragraph = last;
        let mut offset = length(s, last)?;
        if let Spot::Middle {
            paragraph: head, ..
        } = spot
            && inline
        {
            // Join the text after the caret, then the snippet, back into one paragraph.
            let head_length = length(s, head)?;
            s.core
                .merge_paragraph_native(section, last + 1)
                .map_err(err)?;
            s.core.merge_paragraph_native(section, last).map_err(err)?;
            paragraph = head;
            offset += head_length;
        }
        s.cursor = Cursor {
            section,
            paragraph,
            offset,
            cell: None,
        };
        s.clear_selection();
        Ok(())
    })?;
    Ok(
        json!({"paragraphs":paragraphs,"cursor":session.cursor,"generation":session.generation,"import":report}),
    )
}

/// Native block import deliberately only splices paragraphs. At section start,
/// move the target's original definitions rather than letting serialization add
/// a second definition before the displaced one. This is a same-document move:
/// resource IDs, document settings and all original body text stay unchanged.
pub(crate) fn move_target_prologue(
    core: &mut DocumentCore,
    section: usize,
    displaced: usize,
) -> Result<()> {
    use hwpcraft_doc::model::paragraph::ColumnBreakType;
    let paragraphs = &mut core
        .document_mut()
        .sections
        .get_mut(section)
        .ok_or_else(|| anyhow!("상용구 대상 구역이 없습니다."))?
        .paragraphs;
    if displaced == 0 || displaced >= paragraphs.len() {
        bail!("상용구 대상 첫 문단 위치가 잘못됐습니다.");
    }
    let (inserted, remaining) = paragraphs.split_at_mut(displaced);
    let first = &mut inserted[0];
    let previous = &mut remaining[0];
    let count = previous
        .controls
        .iter()
        .take_while(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
        .count();
    if count == 0 {
        return Ok(());
    }
    previous.align_ctrl_data_records();
    first.align_ctrl_data_records();
    let definitions: Vec<_> = previous.controls.drain(..count).collect();
    let records: Vec<_> = previous.ctrl_data_records.drain(..count).collect();
    for range in &mut previous.field_ranges {
        range.control_idx = range
            .control_idx
            .checked_sub(count)
            .ok_or_else(|| anyhow!("대상 필드 참조가 구역 정의를 가리킵니다."))?;
    }
    let width = u32::try_from(count)?
        .checked_mul(8)
        .ok_or_else(|| anyhow!("대상 구역 슬롯 크기 초과"))?;
    let released = width.min(
        previous
            .char_offsets
            .first()
            .copied()
            .unwrap_or_else(|| previous.char_count.saturating_sub(1)),
    );
    for offset in &mut previous.char_offsets {
        *offset = offset
            .checked_sub(released)
            .ok_or_else(|| anyhow!("대상 구역 자리표 오류"))?;
    }
    if let Some(kept) = previous
        .char_shapes
        .iter()
        .rposition(|run| run.start_pos <= released)
    {
        previous.char_shapes.drain(..kept);
    }
    for run in &mut previous.char_shapes {
        run.start_pos = run.start_pos.saturating_sub(released);
    }
    for tag in &mut previous.range_tags {
        tag.start = tag.start.saturating_sub(released);
        tag.end = tag.end.saturating_sub(released);
    }
    for mark in &mut previous.markpen_marks {
        if let Some(position) = &mut mark.utf16_pos {
            *position = position.saturating_sub(released);
        }
    }
    previous.char_count = previous.char_count.saturating_sub(released);
    if !previous
        .controls
        .iter()
        .any(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
    {
        previous.control_mask &= !(1 << 2);
    }
    // The inserted fragment has no structural prefix: its existing leading gap
    // can belong to an image/table and must not be reused for these new slots.
    for offset in &mut first.char_offsets {
        *offset = offset
            .checked_add(width)
            .ok_or_else(|| anyhow!("첫 문단 자리표 초과"))?;
    }
    for run in &mut first.char_shapes {
        if run.start_pos != 0 {
            run.start_pos = run
                .start_pos
                .checked_add(width)
                .ok_or_else(|| anyhow!("첫 문단 서식 위치 초과"))?;
        }
    }
    for tag in &mut first.range_tags {
        tag.start = tag
            .start
            .checked_add(width)
            .ok_or_else(|| anyhow!("첫 문단 범위 초과"))?;
        tag.end = tag
            .end
            .checked_add(width)
            .ok_or_else(|| anyhow!("첫 문단 범위 초과"))?;
    }
    for mark in &mut first.markpen_marks {
        if let Some(position) = &mut mark.utf16_pos {
            *position = position
                .checked_add(width)
                .ok_or_else(|| anyhow!("첫 문단 형광펜 위치 초과"))?;
        }
    }
    for range in &mut first.field_ranges {
        range.control_idx = range
            .control_idx
            .checked_add(count)
            .ok_or_else(|| anyhow!("첫 문단 필드 참조 초과"))?;
    }
    first.char_count = first
        .char_count
        .checked_add(width)
        .ok_or_else(|| anyhow!("첫 문단 문자 수 초과"))?;
    first.controls.splice(0..0, definitions);
    first.ctrl_data_records.splice(0..0, records);
    first.control_mask |= 1 << 2;
    first.raw_break_type = (first.raw_break_type & !3) | (previous.raw_break_type & 3);
    if matches!(
        previous.column_type,
        ColumnBreakType::Section | ColumnBreakType::MultiColumn
    ) {
        first.column_type = previous.column_type;
        previous.column_type = ColumnBreakType::None;
    }
    previous.raw_break_type &= !3;
    for paragraph in [first, previous] {
        paragraph.invalidate_layout_inputs();
        paragraph.line_segs.clear();
        paragraph.layout_space_metrics.clear();
        paragraph.hwpx_axis_shift = 0;
        paragraph.source_line_seg_vertical_pos = None;
        paragraph.layout_only_fill_lines = 0;
    }
    core.set_document(core.document().clone());
    Ok(())
}

/// Count serialized bytes without allocating a parallel JSON representation.
struct ByteBudget(usize);
impl Write for ByteBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.0 {
            return Err(std::io::Error::other("상용구 원본 구조 32 MiB 제한 초과"));
        }
        self.0 -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn bounded_source_copy(document: &Document) -> Result<Document> {
    let mut budget = ByteBudget(MAX_EXPANDED);
    let mut tree = Profile::new(100_000);
    for section in &document.sections {
        tree.paras(&section.paragraphs, 1, false)?;
        for master in &section.section_def.master_pages {
            tree.paras(&master.paragraphs, 1, false)?;
        }
        serde_json::to_writer(&mut budget, &section.paragraphs)?;
        serde_json::to_writer(&mut budget, &section.section_def)?;
    }
    let info = &document.doc_info;
    serde_json::to_writer(
        &mut budget,
        &(
            &info.bin_data_list,
            &info.font_faces,
            &info.border_fills,
            &info.char_shapes,
            &info.tab_defs,
            &info.numberings,
            &info.bullets,
            &info.para_shapes,
            &info.styles,
            &info.extra_records,
            &info.memo_properties_xml,
            &info.hwpx_head_tail,
        ),
    )?;
    if info
        .raw_stream
        .as_ref()
        .is_some_and(|raw| raw.len() > MAX_EXPANDED)
    {
        bail!("원본 DocInfo가 상용구 구조 제한을 초과했습니다.");
    }
    let loaded = document
        .bin_data_content
        .iter()
        .try_fold(0usize, |sum, binary| {
            let length = match &binary.data {
                hwpcraft_doc::model::bin_data::BinDataBytes::Loaded(bytes) => bytes.len(),
                _ => 0,
            };
            sum.checked_add(length)
                .ok_or_else(|| anyhow!("원본 바이너리 크기 초과"))
        })?;
    if loaded > MAX_EXPANDED {
        bail!("원본 메모리 바이너리가 상용구 복제 제한을 초과했습니다.");
    }
    // Retain the complete owned tree for strict outside-endpoint validation, but
    // not source previews, metadata/auxiliary ZIP entries or raw body streams.
    let sections = document
        .sections
        .iter()
        .map(|section| hwpcraft_doc::model::document::Section {
            section_def: section.section_def.clone(),
            paragraphs: section.paragraphs.clone(),
            raw_stream: None,
            raw_provenance: None,
        })
        .collect();
    Ok(Document {
        doc_info: info.clone(),
        sections,
        bin_data_content: document.bin_data_content.clone(),
        ..Default::default()
    })
}

pub(crate) fn normalize_prefix(paragraph: &mut Paragraph, allow: bool) -> Result<()> {
    let count = paragraph
        .controls
        .iter()
        .take_while(|control| matches!(control, Control::SectionDef(_) | Control::ColumnDef(_)))
        .count();
    if count == 0 {
        return Ok(());
    }
    if !allow || count > 2 {
        bail!("문단 중간의 구역/다단 정의는 상용구로 만들 수 없습니다.");
    }
    let mut section = false;
    let mut column = false;
    for control in &paragraph.controls[..count] {
        match control {
            Control::SectionDef(value) if !section && value.text_direction == 0 => section = true,
            Control::ColumnDef(value) if !column && value.column_count == 1 => column = true,
            _ => bail!("중복/세로쓰기/다단 정의는 본문 상용구로 만들 수 없습니다."),
        }
    }
    if !paragraph.field_ranges.is_empty() || !paragraph.orphan_field_ends.is_empty() {
        bail!("필드가 포함된 본문 상용구는 지원하지 않습니다.");
    }
    paragraph.align_ctrl_data_records();
    if paragraph.ctrl_data_records[..count]
        .iter()
        .flatten()
        .any(|raw| !raw.is_empty())
    {
        bail!("구역 정의에 미지원 CTRL_DATA가 있습니다.");
    }
    paragraph.controls.drain(..count);
    paragraph.ctrl_data_records.drain(..count);
    // A first-paragraph section-start marker belongs to the source owner,
    // rather than introducing a new section in the receiving document.
    if matches!(
        paragraph.column_type,
        hwpcraft_doc::model::paragraph::ColumnBreakType::Section
            | hwpcraft_doc::model::paragraph::ColumnBreakType::MultiColumn
    ) {
        paragraph.column_type = hwpcraft_doc::model::paragraph::ColumnBreakType::None;
    }
    paragraph.raw_break_type &= !3;
    let requested = u32::try_from(count)?
        .checked_mul(8)
        .ok_or_else(|| anyhow!("구역 슬롯 크기 초과"))?;
    let available = paragraph
        .char_offsets
        .first()
        .copied()
        .unwrap_or_else(|| paragraph.char_count.saturating_sub(1));
    let released = requested.min(available);
    for offset in &mut paragraph.char_offsets {
        *offset = offset
            .checked_sub(released)
            .ok_or_else(|| anyhow!("잘못된 구역 자리표"))?;
    }
    if let Some(first) = paragraph
        .char_shapes
        .iter()
        .rposition(|run| run.start_pos <= released)
    {
        paragraph.char_shapes.drain(..first);
    }
    for run in &mut paragraph.char_shapes {
        run.start_pos = run.start_pos.saturating_sub(released);
    }
    for range in &mut paragraph.range_tags {
        range.start = range.start.saturating_sub(released);
        range.end = range.end.saturating_sub(released);
    }
    for mark in &mut paragraph.markpen_marks {
        if let Some(position) = &mut mark.utf16_pos {
            *position = position.saturating_sub(released);
        }
    }
    paragraph.char_count = paragraph.char_count.saturating_sub(released);
    paragraph.control_mask = if paragraph.controls.is_empty() {
        0
    } else {
        1 << 11
    };
    if paragraph.text.contains('\t') {
        paragraph.control_mask |= 1 << 9;
    }
    if paragraph.text.contains('\n') {
        paragraph.control_mask |= 1 << 10;
    }
    paragraph.invalidate_layout_inputs();
    paragraph.line_segs.clear();
    paragraph.layout_space_metrics.clear();
    paragraph.hwpx_axis_shift = 0;
    paragraph.source_line_seg_vertical_pos = None;
    paragraph.layout_only_fill_lines = 0;
    Ok(())
}

struct Profile {
    left: usize,
    nodes: usize,
    paragraphs: usize,
    tables: usize,
    pictures: usize,
    shapes: usize,
    text: Vec<String>,
    content: bool,
}
impl Profile {
    fn new(left: usize) -> Self {
        Self {
            left,
            nodes: 0,
            paragraphs: 0,
            tables: 0,
            pictures: 0,
            shapes: 0,
            text: Vec::new(),
            content: false,
        }
    }
    fn node(&mut self, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH || self.left == 0 {
            bail!("상용구 구조는 깊이 32/노드 제한 이하여야 합니다.");
        }
        self.left -= 1;
        self.nodes += 1;
        Ok(())
    }
    fn caption(&mut self, caption: Option<&Caption>, depth: usize, strict: bool) -> Result<()> {
        if let Some(c) = caption {
            self.paras(&c.paragraphs, depth, strict)?;
        }
        Ok(())
    }
    fn shape(&mut self, shape: &ShapeObject, depth: usize, strict: bool) -> Result<()> {
        self.node(depth)?;
        if strict && shape.common().locked {
            bail!("보호된 개체는 상용구로 복사할 수 없습니다.");
        }
        if let Some(drawing) = shape.drawing() {
            if let Some(textbox) = &drawing.text_box {
                self.paras(&textbox.paragraphs, depth + 1, strict)?;
            }
            self.caption(drawing.caption.as_ref(), depth + 1, strict)?;
        }
        match shape {
            ShapeObject::Group(group) => {
                if strict {
                    bail!("그룹 개체는 본문 상용구 v1에서 지원하지 않습니다.");
                }
                for child in &group.children {
                    self.shape(child, depth + 1, strict)?;
                }
                self.caption(group.caption.as_ref(), depth + 1, strict)?;
            }
            ShapeObject::Chart(_) | ShapeObject::Ole(_) if strict => {
                bail!("차트/OLE는 본문 상용구에서 지원하지 않습니다.")
            }
            ShapeObject::Line(line) if strict && line.connector.is_some() => {
                bail!("연결선은 본문 상용구에서 지원하지 않습니다.")
            }
            ShapeObject::Picture(picture) => {
                self.pictures += 1;
                self.caption(picture.caption.as_ref(), depth + 1, strict)?;
            }
            ShapeObject::Chart(chart) => {
                self.caption(chart.caption.as_ref(), depth + 1, strict)?;
            }
            ShapeObject::Ole(ole) => {
                self.caption(ole.caption.as_ref(), depth + 1, strict)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn paras(&mut self, paras: &[Paragraph], depth: usize, strict: bool) -> Result<()> {
        for paragraph in paras {
            self.node(depth)?;
            self.paragraphs += 1;
            if strict {
                if !paragraph.field_ranges.is_empty() || !paragraph.orphan_field_ends.is_empty() {
                    bail!("필드가 포함된 상용구는 지원하지 않습니다.");
                }
                self.content |= !paragraph.text.is_empty() || !paragraph.controls.is_empty();
                self.text.push(paragraph.text.clone());
            }
            for control in &paragraph.controls {
                self.node(depth + 1)?;
                match control {
                    Control::Table(table) => {
                        self.tables += 1;
                        if strict
                            && (table.common.locked
                                || table.cells.iter().any(|cell| cell.cell_protect()))
                        {
                            bail!("보호된 표/셀은 상용구로 복사할 수 없습니다.");
                        }
                        for cell in &table.cells {
                            self.node(depth + 2)?;
                            self.paras(&cell.paragraphs, depth + 3, strict)?;
                        }
                        self.caption(table.caption.as_ref(), depth + 2, strict)?;
                    }
                    Control::Picture(picture) => {
                        if strict && picture.common.locked {
                            bail!("보호된 그림은 상용구로 복사할 수 없습니다.");
                        }
                        self.pictures += 1;
                        self.caption(picture.caption.as_ref(), depth + 2, strict)?;
                    }
                    Control::Shape(shape) => {
                        self.shapes += 1;
                        self.shape(shape, depth + 2, strict)?;
                    }
                    _ if strict => bail!("미지원 본문 상용구 컨트롤이 포함돼 있습니다."),
                    Control::Header(value) => self.paras(&value.paragraphs, depth + 2, false)?,
                    Control::Footer(value) => self.paras(&value.paragraphs, depth + 2, false)?,
                    Control::Footnote(value) => self.paras(&value.paragraphs, depth + 2, false)?,
                    Control::Endnote(value) => self.paras(&value.paragraphs, depth + 2, false)?,
                    Control::HiddenComment(value) => {
                        self.paras(&value.paragraphs, depth + 2, false)?
                    }
                    Control::Field(value) => {
                        self.paras(&value.memo_paragraphs, depth + 2, false)?
                    }
                    Control::SectionDef(value) => {
                        for master in &value.master_pages {
                            self.paras(&master.paragraphs, depth + 2, false)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    fn summary(&self) -> String {
        self.text
            .iter()
            .flat_map(|text| text.chars().chain(std::iter::once('\n')))
            .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
            .take(400)
            .collect::<String>()
            .trim_end()
            .to_owned()
    }
    fn json(&self) -> Value {
        json!({"nodes":self.nodes,"paragraphs":self.paragraphs,"tables":self.tables,"pictures":self.pictures,"shapes":self.shapes})
    }
}

fn validate_package(payload: &[u8]) -> Result<()> {
    if payload.len() > MAX_PAYLOAD || !payload.starts_with(b"PK\x03\x04") {
        bail!("본문 상용구 payload는 8 MiB 이하의 HWPX여야 합니다.");
    }
    let mut archive = zip::ZipArchive::new(ByteCursor::new(payload))?;
    if archive.len() > MAX_ZIP_ITEMS {
        bail!("상용구 ZIP 항목은 1024개 이하입니다.");
    }
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    let mut mime = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        if name.contains(['\\', ':'])
            || name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || name.chars().any(char::is_control)
            || !names.insert(name.to_ascii_lowercase())
        {
            bail!("상용구 ZIP 경로/중복 항목이 올바르지 않습니다.");
        }
        let known = matches!(
            name.as_str(),
            "mimetype"
                | "version.xml"
                | "settings.xml"
                | "META-INF/container.xml"
                | "META-INF/container.rdf"
                | "META-INF/manifest.xml"
                | "META-INF/rhwp-hwp5-origin"
                | "Contents/content.hpf"
                | "Contents/header.xml"
                | "Contents/section0.xml"
                | "Preview/PrvText.txt"
                | "Preview/PrvImage.png"
        ) || name.starts_with("BinData/");
        if !known {
            bail!("상용구에 미지원 보조 항목이 있습니다: {name}");
        }
        let left = MAX_EXPANDED
            .checked_sub(total)
            .ok_or_else(|| anyhow!("상용구 확장 크기 초과"))?;
        if entry.size() > u64::try_from(left)? {
            bail!("상용구 ZIP 확장은 32 MiB 이하입니다.");
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(u64::try_from(left)? + 1)
            .read_to_end(&mut bytes)?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow!("상용구 확장 크기 초과"))?;
        if total > MAX_EXPANDED {
            bail!("상용구 ZIP 확장은 32 MiB 이하입니다.");
        }
        if name == "mimetype" {
            if bytes != b"application/hwp+zip" {
                bail!("상용구 HWPX MIME이 올바르지 않습니다.");
            }
            mime = true;
        }
        if name == "META-INF/rhwp-hwp5-origin"
            && bytes != hwpcraft_doc::model::document::HWP5_ORIGIN_HWPX_PARAGRAPH_AXIS
        {
            bail!("상용구의 문단 좌표 출처 표시가 올바르지 않습니다.");
        }
        if name.ends_with(".xml") || name.ends_with(".hpf") {
            validate_xml_links(&bytes)?;
        }
    }
    if !mime
        || !names.contains("contents/header.xml")
        || !names.contains("contents/section0.xml")
        || !names.contains("contents/content.hpf")
    {
        bail!("상용구 HWPX 필수 항목이 없습니다.");
    }
    Ok(())
}

fn validate_xml_links(bytes: &[u8]) -> Result<()> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(bytes);
    let mut depth = 0usize;
    let mut elements = 0usize;
    loop {
        let event = reader.read_event()?;
        let opening = matches!(event, Event::Start(_));
        match event {
            Event::Start(event) | Event::Empty(event) => {
                if matches!(
                    event.local_name().as_ref(),
                    "script" | "Scripts" | "trackChange" | "formObject"
                ) {
                    bail!("상용구에 미지원 실행/추적/양식 요소가 있습니다.");
                }
                for attr in event.attributes() {
                    let attr = attr?;
                    let key = attr.key.local_name();
                    if matches!(
                        key.as_ref(),
                        "href" | "src" | "absPath" | "relPath" | "Target" | "TargetMode"
                    ) {
                        let value = attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)?;
                        if value.contains([':', '\\'])
                            || value.starts_with('/')
                            || value.split('/').any(|part| part == "..")
                            || value.eq_ignore_ascii_case("External")
                        {
                            bail!("상용구의 외부 파일/링크는 지원하지 않습니다.");
                        }
                    }
                }
                if matches!(
                    event.local_name().as_ref(),
                    "p" | "tbl" | "pic" | "rect" | "ellipse" | "line" | "polygon" | "curve" | "arc"
                ) {
                    elements = elements
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("상용구 XML 노드 수 초과"))?;
                    if elements > MAX_NODES {
                        bail!("상용구 XML 노드 제한 초과");
                    }
                }
                // XML nesting has its own ceiling before the native parser runs.
                if opening {
                    depth += 1;
                    if depth > 256 {
                        bail!("상용구 XML 깊이 제한 초과");
                    }
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Event::DocType(_) => bail!("상용구 XML DTD는 지원하지 않습니다."),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    fn at(paragraph: usize, offset: usize) -> Cursor {
        Cursor {
            paragraph,
            offset,
            ..Default::default()
        }
    }
    fn text(session: &Session) -> Vec<String> {
        session.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect()
    }
    fn captured(session: &Session, start: usize, end: usize) -> Result<CapturedFragment> {
        capture(
            session,
            Some(SourceRange {
                section: 0,
                start,
                end,
            }),
            session.generation,
        )
    }

    #[test]
    fn summary_omits_nonprinting_controls_without_rewriting_source_text() {
        let mut profile = Profile::new(MAX_NODES);
        let original = "원문\u{15}과\0설명\t😀\n다음".to_owned();
        profile.text.push(original.clone());
        assert_eq!(profile.summary(), "원문과설명\t😀\n다음");
        assert_eq!(profile.text[0], original);
    }

    #[test]
    fn source_budget_walk_includes_unselected_chart_and_ole_captions() -> Result<()> {
        use hwpcraft_doc::model::shape::{ChartShape, OleShape};
        let mut nested = Paragraph::new_empty();
        for depth in 0..40 {
            let caption = Some(Caption {
                paragraphs: vec![nested],
                ..Default::default()
            });
            let shape = if depth % 2 == 0 {
                ShapeObject::Chart(Box::new(ChartShape {
                    caption,
                    ..Default::default()
                }))
            } else {
                ShapeObject::Ole(Box::new(OleShape {
                    caption,
                    ..Default::default()
                }))
            };
            nested = Paragraph::new_empty();
            nested.controls.push(Control::Shape(Box::new(shape)));
        }
        let mut profile = Profile::new(100_000);
        assert!(profile.paras(&[nested], 1, false).is_err());
        Ok(())
    }

    fn first_boundary_case(blank_target: bool) -> Result<()> {
        let files = tempfile::tempdir()?;
        let mut source = Session::new()?;
        source.execute(Command::Insert("새 제목😀\n새 설명".into()))?;
        let title_length = source.core.document().sections[0].paragraphs[0]
            .text
            .chars()
            .count();
        source.select(at(0, 0), false)?;
        source.select(at(0, title_length), true)?;
        source.execute(Command::Character(json!({"bold":true,"fontSize":1800})))?;
        let snippet = captured(&source, 0, 2)?;
        let mut target = Session::new()?;
        if !blank_target {
            target.execute(Command::Insert("기존 첫 본문😀".into()))?;
        }
        let mut setup = target.page_setup()?;
        setup.width_mm = 180.0;
        setup.height_mm = 240.0;
        setup.top_mm = 23.0;
        setup.left_mm = 19.0;
        target.execute(Command::PageSetup(setup))?;
        target.select(at(0, 0), false)?;
        let original = text(&target);
        let paper = target.core.document().sections[0]
            .section_def
            .page_def
            .clone();
        let generation = target.generation;
        insert(&mut target, &snippet.payload, generation)?;
        let mut expected = vec!["새 제목😀".to_owned(), "새 설명".to_owned()];
        expected.extend(original.clone());
        assert_eq!(text(&target), expected);
        let paragraphs = &target.core.document().sections[0].paragraphs;
        assert_eq!(
            paragraphs[0]
                .controls
                .iter()
                .filter(|c| matches!(c, Control::SectionDef(_)))
                .count(),
            1,
            "target prologue must stay first"
        );
        assert!(
            !paragraphs.iter().skip(1).any(|p| p
                .controls
                .iter()
                .any(|c| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))),
            "no second prologue may be introduced"
        );
        for extension in ["hwp", "hwpx"] {
            let path = files.path().join(format!("first-boundary.{extension}"));
            target.save(&path)?;
            let reopened = hwpcraft_formats::open(&path)?;
            assert_eq!(
                reopened.document().sections[0]
                    .paragraphs
                    .iter()
                    .map(|p| p.text.clone())
                    .collect::<Vec<_>>(),
                expected
            );
            let actual = &reopened.document().sections[0].section_def.page_def;
            assert_eq!(
                (
                    actual.width,
                    actual.height,
                    actual.margin_left,
                    actual.margin_top
                ),
                (
                    paper.width,
                    paper.height,
                    paper.margin_left,
                    paper.margin_top
                )
            );
            let first = &reopened.document().sections[0].paragraphs[0];
            let shape = first
                .char_shape_id_at(0)
                .ok_or_else(|| anyhow!("missing inserted title shape"))?;
            assert_eq!(
                reopened.document().doc_info.char_shapes[shape as usize].base_size,
                1800
            );
            assert!(reopened.document().doc_info.char_shapes[shape as usize].bold);
        }
        target.execute(Command::Undo)?;
        assert_eq!(text(&target), original);
        target.execute(Command::Redo)?;
        assert_eq!(text(&target), expected);
        Ok(())
    }

    #[test]
    fn blank_target_keeps_real_empty_paragraph_paper_and_single_prologue() -> Result<()> {
        first_boundary_case(true)
    }

    #[test]
    fn insertion_before_first_text_keeps_prologue_and_existing_text() -> Result<()> {
        first_boundary_case(false)
    }

    #[test]
    fn nonleading_target_definition_is_rejected_before_insert_or_preview() -> Result<()> {
        let mut source = Session::new()?;
        source.execute(Command::Insert("삽입".into()))?;
        let payload = captured(&source, 0, 1)?.payload;
        let mut target = Session::new()?;
        target.execute(Command::Insert("보존".into()))?;
        target.run(
            "shape.insert",
            json!({"kind":"rectangle","width_mm":10.0,"height_mm":10.0}),
        )?;
        let mut doc = target.core.document().clone();
        let shape = doc.sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|c| matches!(c, Control::Shape(_)))
            .ok_or_else(|| anyhow!("missing test shape"))?;
        doc.sections[0].paragraphs[0].controls.swap(0, shape);
        target.core.set_document(doc);
        target.select(at(0, 0), false)?;
        let generation = target.generation;
        let before = text(&target);
        let resources = target.core.document().doc_info.char_shapes.len();
        assert!(preview(&target, &payload, generation).is_err());
        assert!(insert(&mut target, &payload, generation).is_err());
        assert_eq!(text(&target), before);
        assert_eq!(target.generation, generation);
        assert_eq!(target.core.document().doc_info.char_shapes.len(), resources);
        Ok(())
    }

    #[test]
    fn first_boundary_preserves_existing_memo_anchor_and_character_style() -> Result<()> {
        let files = tempfile::tempdir()?;
        let mut source = Session::new()?;
        source.execute(Command::Insert("앞 문단".into()))?;
        let payload = captured(&source, 0, 1)?.payload;
        let mut target = Session::new()?;
        target.execute(Command::Insert("기존😀 메모".into()))?;
        target.select(at(0, 0), false)?;
        target.select(at(0, 2), true)?;
        target.execute(Command::Character(json!({"bold":true,"fontSize":1400})))?;
        let memo = target.run(
            "memo.create",
            json!({"text":"원래 메모","author":"자체 검증"}),
        )?;
        let id = memo["id"].clone();
        let before = target.run("memo.get", json!({"id":id}))?;
        target.select(at(0, 0), false)?;
        let generation = target.generation;
        insert(&mut target, &payload, generation)?;
        let after = target.run("memo.get", json!({"id":id}))?;
        assert_eq!(after["anchor"], before["anchor"]);
        assert_eq!(after["text"], before["text"]);
        assert_eq!(after["author"], before["author"]);
        target.select(at(1, 0), false)?;
        target.select(at(1, 2), true)?;
        let format = target.run("format.character.get", json!({}))?;
        assert_eq!(format["bold"], true);
        assert_eq!(format["fontSize"], 1400);
        for extension in ["hwp", "hwpx"] {
            let path = files.path().join(format!("memo-start.{extension}"));
            target.save(&path)?;
            let mut reopened = Session::new()?;
            reopened.open(&path)?;
            let memo = reopened.run("memo.get", json!({"id":id}))?;
            assert_eq!(memo["anchor"], before["anchor"]);
            assert_eq!(memo["text"], before["text"]);
            assert_eq!(memo["author"], before["author"]);
        }
        target.execute(Command::Undo)?;
        assert_eq!(
            target.run("memo.get", json!({"id":id}))?["anchor"],
            before["anchor"]
        );
        target.execute(Command::Redo)?;
        assert_eq!(
            target.run("memo.get", json!({"id":id}))?["anchor"],
            before["anchor"]
        );
        Ok(())
    }

    #[test]
    fn source_resolution_current_full_reverse_partial_and_next_start() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("첫째😀\n둘째\n셋째".into()))?;
        assert_eq!(resolve_source(&session, None)?.start, 2);
        session.select(at(0, 0), false)?;
        session.select(at(2, 0), true)?;
        let range = resolve_source(&session, None)?;
        assert_eq!((range.start, range.end), (0, 2));
        session.select(at(1, 2), false)?;
        session.select(at(0, 0), true)?;
        let range = resolve_source(&session, None)?;
        assert_eq!((range.start, range.end), (0, 2));
        session.select(at(0, 1), false)?;
        session.select(at(0, 2), true)?;
        assert!(resolve_source(&session, None).is_err());
        assert!(
            capture(
                &session,
                Some(SourceRange {
                    section: 0,
                    start: 0,
                    end: 1
                }),
                session.generation + 1
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn first_paragraph_unicode_styles_empty_tail_and_private_envelope_survive() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert(
            "가😀나\t끝\n\n외부비밀-SOURCE-SECRET".into(),
        ))?;
        session.select(at(0, 0), false)?;
        session.select(at(0, 2), true)?;
        session.execute(Command::Character(
            json!({"bold":true,"fontSize":1800,"textColor":"#123456"}),
        ))?;
        let mut doc = session.core.document().clone();
        doc.extra_streams
            .push(("/Private".into(), b"SOURCE-SECRET-STREAM".to_vec()));
        doc.hwpx_aux_entries
            .push(("settings.xml".into(), b"SOURCE-SECRET-SETTINGS".to_vec()));
        session.core.set_document(doc);
        let before = text(&session);
        let generation = session.generation;
        let cursor = session.cursor;
        let snippet = captured(&session, 0, 2)?;
        assert_eq!(snippet.paragraphs, 2);
        assert!(snippet.summary.contains("가😀나"));
        assert_eq!(text(&session), before);
        assert_eq!(session.generation, generation);
        assert_eq!(session.cursor, cursor);
        let core = decode(&snippet.payload)?;
        assert_eq!(
            core.document().sections[0]
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>(),
            vec!["가😀나\t끝", ""]
        );
        let paragraph = &core.document().sections[0].paragraphs[0];
        let id = paragraph.char_shape_id_at(0).unwrap();
        let shape = &core.document().doc_info.char_shapes[id as usize];
        assert!(shape.bold);
        assert_eq!(shape.base_size, 1800);
        assert!(paragraph.char_offsets.first().copied().unwrap_or(0) < 8);
        let mut zip = zip::ZipArchive::new(ByteCursor::new(&snippet.payload))?;
        for index in 0..zip.len() {
            let mut bytes = Vec::new();
            zip.by_index(index)?.read_to_end(&mut bytes)?;
            assert!(!String::from_utf8_lossy(&bytes).contains("SOURCE-SECRET"));
        }
        Ok(())
    }

    #[test]
    fn table_picture_shape_reachable_resources_and_insert_history() -> Result<()> {
        let images = tempfile::tempdir()?;
        let included = images.path().join("included.png");
        let excluded = images.path().join("excluded.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])).save(&included)?;
        image::RgbaImage::from_pixel(3, 3, image::Rgba([0, 0, 255, 255])).save(&excluded)?;
        let mut session = Session::new()?;
        session.execute(Command::Insert("혼합 상용구".into()))?;
        session.select(at(0, 0), false)?;
        session.run(
            "shape.insert",
            json!({"kind":"rectangle","width_mm":15.0,"height_mm":10.0}),
        )?;
        session.run(
            "picture.insert",
            json!({"path":included,"width_mm":10.0,"treat_as_char":true}),
        )?;
        session.execute(Command::Table {
            rows: 1,
            columns: 1,
        })?;
        let (table_paragraph, table_control) = session.core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(i, p)| {
                p.controls
                    .iter()
                    .position(|c| matches!(c, Control::Table(_)))
                    .map(|c| (i, c))
            })
            .ok_or_else(|| anyhow!("missing fixture table"))?;
        session.select(
            Cursor {
                cell: Some(Cell {
                    control: table_control,
                    index: 0,
                    paragraph: 0,
                }),
                ..at(table_paragraph, 0)
            },
            false,
        )?;
        session.execute(Command::Insert("표 셀😀".into()))?;
        let wanted = text(&session);
        let end = wanted.len();
        let mut document = session.core.document().clone();
        let excluded_paragraph = Paragraph::new_empty_like(&document.sections[0].paragraphs[0]);
        document.sections[0].paragraphs.push(excluded_paragraph);
        session.core.set_document(document);
        session.select(at(end, 0), false)?;
        session.execute(Command::Insert("선택 밖 비밀 그림".into()))?;
        session.run(
            "picture.insert",
            json!({"path":excluded,"width_mm":10.0,"treat_as_char":true}),
        )?;
        let snippet = captured(&session, 0, end)?;
        assert_eq!(snippet.stats["tables"], 1);
        assert_eq!(snippet.stats["pictures"], 1);
        assert_eq!(snippet.stats["shapes"], 1);
        let source = decode(&snippet.payload)?;
        assert_eq!(source.document().bin_data_content.len(), 1);
        let mut target = Session::new()?;
        target.execute(Command::Insert("접두부".into()))?;
        let before = text(&target);
        let original_resources = target.core.document().doc_info.char_shapes.len();
        let generation = target.generation;
        let previewed = preview(&target, &snippet.payload, generation)?;
        assert!(
            previewed["svg"]
                .as_str()
                .is_some_and(|svg| svg.contains("<svg"))
        );
        assert_eq!(text(&target), before);
        assert_eq!(target.generation, generation);
        insert(&mut target, &snippet.payload, generation)?;
        let mut expected = vec!["접두부".to_owned()];
        expected.extend(wanted);
        assert_eq!(text(&target), expected);
        target.execute(Command::Undo)?;
        assert_eq!(text(&target), before);
        assert_eq!(
            target.core.document().doc_info.char_shapes.len(),
            original_resources
        );
        target.execute(Command::Redo)?;
        assert_eq!(text(&target), expected);
        for extension in ["hwp", "hwpx"] {
            let path = images.path().join(format!("snippet.{extension}"));
            target.save(&path)?;
            let reopened = hwpcraft_formats::open(&path)?;
            let mut profile = Profile::new(MAX_NODES);
            profile.paras(&reopened.document().sections[0].paragraphs[1..], 1, true)?;
            assert_eq!(
                (profile.tables, profile.pictures, profile.shapes),
                (1, 1, 1)
            );
        }
        // The first copied paragraph already has image/shape slots before text.
        // Adding a target prologue must reserve new slots rather than reuse them.
        let mut empty = Session::new()?;
        let generation = empty.generation;
        insert(&mut empty, &snippet.payload, generation)?;
        assert_eq!(
            empty.core.document().sections[0]
                .paragraphs
                .last()
                .map(|p| p.text.as_str()),
            Some("")
        );
        for extension in ["hwp", "hwpx"] {
            let path = images.path().join(format!("mixed-empty.{extension}"));
            empty.save(&path)?;
            let mut reopened = hwpcraft_formats::open(&path)?.document().clone();
            normalize_prefix(&mut reopened.sections[0].paragraphs[0], true)?;
            let mut profile = Profile::new(MAX_NODES);
            profile.paras(&reopened.sections[0].paragraphs, 1, true)?;
            assert_eq!(
                (profile.tables, profile.pictures, profile.shapes),
                (1, 1, 1)
            );
            assert!(profile.summary().contains("표 셀😀"));
        }
        Ok(())
    }

    #[test]
    fn unsupported_and_protected_content_fail_without_source_changes() -> Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("검토".into()))?;
        session.run("memo.create", json!({"text":"메모","author":"자체시험"}))?;
        let before = text(&session);
        let generation = session.generation;
        assert!(captured(&session, 0, 1).is_err());
        assert_eq!(text(&session), before);
        assert_eq!(session.generation, generation);
        let mut session = Session::new()?;
        session.execute(Command::Table {
            rows: 1,
            columns: 1,
        })?;
        let control = session.core.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|control| matches!(control, Control::Table(_)))
            .ok_or_else(|| anyhow!("missing table"))?;
        session.select(
            Cursor {
                cell: Some(Cell {
                    control,
                    index: 0,
                    paragraph: 0,
                }),
                ..at(0, 0)
            },
            false,
        )?;
        assert!(resolve_source(&session, None).is_err());
        session.select(at(0, 0), false)?;
        let mut doc = session.core.document().clone();
        for control in &mut doc.sections[0].paragraphs[0].controls {
            if let Control::Table(table) = control {
                table.common.locked = true;
            }
        }
        session.core.set_document(doc);
        assert!(captured(&session, 0, 1).is_err());
        Ok(())
    }

    #[test]
    fn external_binary_and_unsafe_packages_are_rejected() -> Result<()> {
        assert!(decode(&vec![0; MAX_PAYLOAD + 1]).is_err());
        assert!(validate_xml_links(br#"<item href="file:///outside.png"/>"#).is_err());
        assert!(
            validate_xml_links(br#"<!DOCTYPE x [<!ENTITY x SYSTEM "file:///outside">]><x/>"#)
                .is_err()
        );
        let mut writer = zip::ZipWriter::new(ByteCursor::new(Vec::new()));
        writer.start_file("../outside", zip::write::SimpleFileOptions::default())?;
        writer.write_all(b"x")?;
        assert!(decode(&writer.finish()?.into_inner()).is_err());
        let mut writer = zip::ZipWriter::new(ByteCursor::new(Vec::new()));
        for i in 0..=MAX_ZIP_ITEMS {
            writer.start_file(
                format!("BinData/{i}.png"),
                zip::write::SimpleFileOptions::default(),
            )?;
        }
        assert!(decode(&writer.finish()?.into_inner()).is_err());
        let images = tempfile::tempdir()?;
        let path = images.path().join("image.png");
        image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255])).save(&path)?;
        let mut session = Session::new()?;
        session.run("picture.insert", json!({"path":path,"width_mm":10.0}))?;
        let mut doc = session.core.document().clone();
        doc.doc_info.bin_data_list[0].abs_path = Some("C:/outside.png".into());
        session.core.set_document(doc);
        assert!(captured(&session, 0, 1).is_err());
        Ok(())
    }

    #[test]
    fn destination_selection_and_stale_requests_preserve_history_and_mid_caret_inserts()
    -> Result<()> {
        let mut source = Session::new()?;
        source.execute(Command::Insert("상용구".into()))?;
        let payload = captured(&source, 0, 1)?.payload;
        let mut target = Session::new()?;
        target.execute(Command::Insert("기존 본문".into()))?;
        let before = text(&target);
        let generation = target.generation;
        target.select(at(0, 0), false)?;
        target.select(at(0, 2), true)?;
        assert!(insert(&mut target, &payload, generation).is_err());
        target.select(at(0, 0), false)?;
        assert!(insert(&mut target, &payload, generation + 1).is_err());
        assert_eq!(text(&target), before);
        assert_eq!(target.generation, generation);
        // Inside a paragraph the snippet joins the text; one undo takes it out.
        target.select(at(0, 1), false)?;
        insert(&mut target, &payload, generation)?;
        assert_eq!(text(&target), vec!["기상용구존 본문"]);
        target.execute(Command::Undo)?;
        assert_eq!(text(&target), before);
        target.execute(Command::Undo)?;
        assert_eq!(text(&target), vec![""]);
        Ok(())
    }
}
