//! Owner: format area. 변경 추적 기록의 상태와 저장 보호.
//!
//! HwpCraft cannot edit Hangul's change tracking. HWP 5 documents keep it in
//! DocInfo records (TRACK_CHANGE_AUTHOR 97, TRACK_CHANGE 96), FileHeader bit 14,
//! the accepted result in `BodyText` and the marked text (deleted text and
//! range tags of kinds 16–19) in `ViewText`; RHWP edits the marked text. HWPX
//! keeps the marked text with `insertBegin`/`deleteBegin` marks, which RHWP
//! (patches 0082/0083, from the OWPML schema; no tracked HWPX sample exists) reads
//! into the same range tags and writes back. An HWP document saved as HWP, or an
//! HWPX document saved as HWPX, without any change keeps its tracking: RHWP (patch
//! 0070) writes every HWP section's BodyText and ViewText as read. Any other HWP/HWPX/DOCX save would
//! merge the changes into one text under stale tracking records, so it is
//! refused unless the caller accepts the merge, and an accepted save drops the
//! tracking records so the file is consistent.
use super::*;
use std::io::Read;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "document.track_changes",
        "변경 추적 기록 상태 (저장하면 결과본으로 합쳐지는지)",
        "",
    ),
    (
        "document.track_changes.accept",
        "변경 내용 적용 (삽입은 남기고 삭제한 글은 지움)",
        "",
    ),
    (
        "document.track_changes.reject",
        "변경 내용 취소 (삽입한 글은 지우고 삭제한 글은 되살림)",
        "",
    ),
    (
        "document.track_changes.record",
        "변경 추적 기록 켜기·끄기 (입력·지우기·문단 나누기를 변경으로 기록)",
        "",
    ),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "document.track_changes",
        "매개 변수 없음. tracked(변경 추적 기록 여부), format(hwp/hwpx), authors·changes(HWP 기록 수), ranges(HWP 변경 범위 수), marks(HWPX 삽입·삭제 표식 수), list(본문의 변경마다 change 번호, kind insert/delete/format, section·paragraph·start·end 글자 위치, text, author, date; 1000개까지, listTruncated), message. keptOnHwpSave(고치지 않은 HWP 문서라 HWP로 저장하면 변경 추적 기록을 그대로 두는지). tracked면 그 경우 말고는 HWP/HWPX 저장이 기본으로 거절되고, file.save·save_document의 acceptTrackChanges:true로만 저장합니다(변경 내용이 지금 보이는 결과로 합쳐지고 변경 추적 기록은 지워집니다).",
        r#"{}"#,
    ),
    (
        "document.track_changes.accept",
        "change(변경 번호, document.track_changes의 list[].change) 또는 all:true 중 하나. 삽입한 글은 그대로 두고 삭제한 글(삭제 표시가 덮은 문단 나눔 포함)은 지우며, 서식 변경은 지금 서식으로 확정합니다. 결과: resolved(처리한 변경 범위 수), remaining(남은 범위 수), tracked. 남은 범위가 없으면 변경 추적 기록(작성자·변경 기록·파일 머리 표시)도 지웁니다. 한 번에 되돌립니다. HWPX 문서의 삽입·삭제 표식(insertBegin·deleteBegin)도 같은 방식으로 처리합니다(OWPML 명세 기반).",
        r#"{"all":true}"#,
    ),
    (
        "document.track_changes.record",
        "on(true 켜기, false 끄기), author(작성자 이름, 생략하면 \"사용자\"). 켜면 문서가 변경 추적 문서가 되고(파일 머리 표시, 작성자 기록), 그 뒤 본문과 표 셀의 글자 입력·지우기·문단 나누기·합치기를 변경으로 기록합니다: 넣은 글은 삽입, 지운 글은 지우지 않고 삭제 표시(이번 기록에서 넣은 글을 지우면 바로 지움). 기록 중에는 그 밖의 편집(서식·개체·표 구조·구역·서식 있는 붙여넣기)은 거절합니다. 끄면 기록한 변경은 그대로 남습니다. 결과는 document.track_changes와 같습니다(recording 포함).",
        r#"{"on":true,"author":"검토자"}"#,
    ),
    (
        "document.track_changes.reject",
        "change(변경 번호) 또는 all:true 중 하나. 삽입한 글(삽입 표시가 덮은 문단 나눔 포함)은 지우고 삭제한 글은 되살립니다. 서식 변경은 옛 서식을 알 수 없어 취소하지 못하고 그대로 두며 결과의 unsupported에 셉니다. 결과: resolved, remaining, unsupported, tracked.",
        r#"{"change":1}"#,
    ),
];

/// HWP 5 DocInfo record tags (HWPTAG_BEGIN 16 + 80 / + 81).
const TRACK_CHANGE: u16 = 96;
const TRACK_CHANGE_AUTHOR: u16 = 97;
/// FileHeader property bit 14: 변경 추적 문서.
const HEADER_TRACKED: u32 = 1 << 14;
/// Paragraph range-tag kinds found only in change-tracked HWP 5 documents.
const TRACK_RANGE_KINDS: std::ops::RangeInclusive<u32> = 16..=19;

pub const REFUSED: &str = "변경 추적 기록이 있는 문서입니다. HWP·HWPX로 저장하면 변경 기록을 그대로 두지만, 이 저장(DOCX, 쪽·블록 저장, 암호를 건 HWP, 읽지 못한 HWPX 표식이 있는 문서)은 변경 기록을 담지 못합니다. 모든 변경을 적용한 결과본으로 저장하세요(MCP save_document acceptTrackChanges:true, CLI --accept-track-changes, 데스크톱 확인 창). 쪽·블록 저장은 그렇게 저장한 뒤에 하세요.";

/// Mark colours: inserted text is underlined in blue, deleted text struck
/// through in red and a format change underlined in green.
const INSERTED: &str = "#1f5fd6";
const DELETED: &str = "#d43a3a";
const FORMATTED: &str = "#2c9a52";
/// Caret rectangles measured for the marks (two per one-line change, about 0.3 ms
/// each); a document with more changes gets marks up to here.
const MARK_PROBES: usize = 4_000;
/// Changes listed by `document.track_changes`.
const MAX_LISTED: usize = 1000;

/// A TRACK_CHANGE record: u32 type (0x10 insert, 0x11 delete, 0x12 format), u16
/// year, month, day, hour, minute and the 1-based TRACK_CHANGE_AUTHOR number.
fn change_record(data: &[u8], authors: &[String]) -> (String, String) {
    let word = |at: usize| {
        data.get(at..at + 2)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    };
    let date = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        word(4),
        word(6),
        word(8),
        word(10),
        word(12)
    );
    let author = usize::from(word(14))
        .checked_sub(1)
        .and_then(|i| authors.get(i))
        .cloned()
        .unwrap_or_default();
    (author, date)
}

/// A TRACK_CHANGE_AUTHOR record: u16 name length, two bytes, the UTF-16 name.
pub(crate) fn author_name(data: &[u8]) -> String {
    let len = data
        .get(..2)
        .map_or(0, |b| usize::from(u16::from_le_bytes([b[0], b[1]])));
    let units: Vec<u16> = data
        .get(4..)
        .unwrap_or_default()
        .as_chunks::<2>()
        .0
        .iter()
        .take(len)
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    String::from_utf16_lossy(&units)
}

/// The scalar offset of the paragraph's stream position `position` (UTF-16 units,
/// counting the 8 units of each control).
fn scalar_at(p: &Paragraph, position: u32) -> usize {
    p.char_offsets
        .iter()
        .position(|&o| o >= position)
        .unwrap_or_else(|| p.text.chars().count())
}

/// A tracked range of a body paragraph or of a paragraph in a table cell of one.
struct TrackedRange {
    /// The paragraph (offset 0); `cell` set for a table cell's paragraph.
    at: Cursor,
    range: Range<usize>,
    /// Range-tag kind: 16 insert, 17 delete, 18 format change.
    kind: u32,
    /// 1-based TRACK_CHANGE number.
    change: usize,
    /// It covers the paragraph break (a split or a join).
    breaks: bool,
    /// The range tag as stored: stream positions and tag word.
    start: u32,
    end: u32,
    tag: u32,
}

/// `change` (a TRACK_CHANGE number) or `all: true`.
fn resolve_params(id: &str, params: &Value) -> Result<Option<usize>> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct P {
        change: Option<usize>,
        all: Option<bool>,
    }
    let p: P =
        serde_json::from_value(params.clone()).map_err(|e| anyhow!("{id} 매개 변수 오류: {e}"))?;
    match (p.change, p.all) {
        (Some(n), None | Some(false)) if n > 0 => Ok(Some(n)),
        (None, Some(true)) => Ok(None),
        _ => bail!("{id}: change(1부터의 변경 번호) 또는 all:true 중 하나를 주세요."),
    }
}

/// Every tracked range in document order: a body paragraph's table cells (one
/// table level) come right after it.
fn tracked_ranges(doc: &hwpcraft_doc::model::document::Document) -> Vec<TrackedRange> {
    let mut out = Vec::new();
    for (section, s) in doc.sections.iter().enumerate() {
        for (paragraph, p) in s.paragraphs.iter().enumerate() {
            let at = Cursor {
                section,
                paragraph,
                ..Default::default()
            };
            paragraph_ranges(at, p, &mut out);
            for (control, item) in p.controls.iter().enumerate() {
                let Control::Table(table) = item else {
                    continue;
                };
                for (index, cell) in table.cells.iter().enumerate() {
                    for (cell_paragraph, cp) in cell.paragraphs.iter().enumerate() {
                        let at = Cursor {
                            cell: Some(Cell {
                                control,
                                index,
                                paragraph: cell_paragraph,
                            }),
                            ..at
                        };
                        paragraph_ranges(at, cp, &mut out);
                    }
                }
            }
        }
    }
    out
}

/// The tracked ranges of the paragraph `p` at `at`.
fn paragraph_ranges(at: Cursor, p: &Paragraph, out: &mut Vec<TrackedRange>) {
    {
        {
            for tag in &p.range_tags {
                let kind = tag.tag >> 24;
                if !TRACK_RANGE_KINDS.contains(&kind) {
                    continue;
                }
                let (start, end) = (scalar_at(p, tag.start), scalar_at(p, tag.end));
                out.push(TrackedRange {
                    at,
                    range: start..end.max(start),
                    kind,
                    change: (tag.tag & 0x00FF_FFFF) as usize,
                    // The break is the paragraph's last stream unit.
                    breaks: p.char_count > 0 && tag.end >= p.char_count,
                    start: tag.start,
                    end: tag.end,
                    tag: tag.tag,
                });
            }
        }
    }
}

/// The paragraph at `at` (body, or a table cell's).
fn paragraph_at<'a>(
    doc: &'a hwpcraft_doc::model::document::Document,
    at: &Cursor,
) -> Option<&'a Paragraph> {
    let p = doc.sections.get(at.section)?.paragraphs.get(at.paragraph)?;
    match at.cell {
        None => Some(p),
        Some(c) => match p.controls.get(c.control)? {
            Control::Table(t) => t.cells.get(c.index)?.paragraphs.get(c.paragraph),
            _ => None,
        },
    }
}

/// Order key of a tracked paragraph: a body paragraph's cells after its own text.
fn paragraph_key(at: &Cursor) -> (usize, usize, usize, usize, usize, usize) {
    match at.cell {
        None => (at.section, at.paragraph, 0, 0, 0, 0),
        Some(c) => (at.section, at.paragraph, 1, c.control, c.index, c.paragraph),
    }
}

/// Seal the sections of a tracked HWP document as read once it has opened. The
/// open fixups (memo text filled in from the stored records, RHWP repairing stale
/// stored lines) leave the file's bytes standing for the section, which is what
/// RHWP's seal is for ("after parsing and load fixups"), so an unchanged HWP save
/// writes BodyText and ViewText as they were (RHWP 0070). Only sections whose
/// ViewText RHWP kept: other documents keep the parser's seal.
pub(crate) fn seal_tracked_sections(core: &mut DocumentCore) {
    let doc = core.document_mut_keeping_layout();
    let views: Vec<usize> = doc
        .extra_streams
        .iter()
        .filter_map(|(path, _)| path.strip_prefix("/ViewText/Section")?.parse().ok())
        .collect();
    for index in views {
        if let Some(section) = doc.sections.get_mut(index)
            && section.raw_stream.is_some()
        {
            section.seal_raw_provenance();
        }
    }
}

/// HWPX marks RHWP did not read into range tags: none once any paragraph (body
/// or nested) holds a tracked range, as RHWP reads every `hp:t` the same way.
pub(crate) fn unread_track_marks(
    marks: usize,
    doc: &hwpcraft_doc::model::document::Document,
) -> usize {
    let tracked = |p: &Paragraph| p.range_tags.iter().any(|t| matches!(t.tag >> 24, 16 | 17));
    let read = doc
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter())
        .any(tracked)
        || !context_walk::nested_paragraphs(doc, &tracked).is_empty();
    if read { 0 } else { marks }
}

/// Insert/delete marks in an HWPX package (sections only), read when it opens.
pub(crate) fn hwpx_track_marks(path: &Path) -> usize {
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return 0;
    };
    let names: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with("Contents/section") && n.ends_with(".xml"))
        .map(str::to_owned)
        .collect();
    let mut marks = 0;
    for name in names {
        let Ok(entry) = zip.by_name(&name) else {
            continue;
        };
        let mut xml = String::new();
        // Section XML is bounded by the package itself; 512 MB guards a bomb.
        if entry.take(512 << 20).read_to_string(&mut xml).is_err() {
            continue;
        }
        marks += xml.matches(":insertBegin").count() + xml.matches(":deleteBegin").count();
    }
    marks
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_track_changes_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        match id {
            "document.track_changes" => Some((|| {
                if params.as_object().is_some_and(|p| !p.is_empty()) {
                    bail!("document.track_changes에는 매개 변수가 없습니다.");
                }
                Ok(self.track_changes())
            })()),
            "document.track_changes.record" => Some((|| {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    on: bool,
                    author: Option<String>,
                }
                let p: P = serde_json::from_value(params.clone())
                    .map_err(|e| anyhow!("document.track_changes.record 매개 변수 오류: {e}"))?;
                self.record_track_changes(p.on, p.author)
            })()),
            "document.track_changes.accept" | "document.track_changes.reject" => {
                let accept = id.ends_with("accept");
                Some(
                    resolve_params(id, params)
                        .and_then(|which| self.resolve_track_changes(accept, which)),
                )
            }
            _ => None,
        }
    }

    /// Accept (`accept`) or reject the tracked ranges of change `which` (all when
    /// `None`), in one undo step. Accepting removes deleted text and keeps inserted
    /// text; rejecting removes inserted text and keeps deleted text. A range that
    /// covers its paragraph's break joins the next paragraph when its text goes.
    /// A format change cannot be rejected: its earlier shape is not known.
    pub fn resolve_track_changes(&mut self, accept: bool, which: Option<usize>) -> Result<Value> {
        self.resolve_track_changes_with(accept, which, true)
    }

    /// `resolve_track_changes`; `drop_records`: drop the tracking records once no
    /// tracked range is left (false for the accepted copy a tracked save writes).
    fn resolve_track_changes_with(
        &mut self,
        accept: bool,
        which: Option<usize>,
        drop_records: bool,
    ) -> Result<Value> {
        if self.track_marks > 0 {
            bail!(
                "HWPX 문서의 변경 추적 표식(insertBegin·deleteBegin)은 읽지 못해 적용·취소할 수 없습니다."
            );
        }
        let targets: Vec<TrackedRange> = tracked_ranges(self.core.document())
            .into_iter()
            .filter(|t| which.is_none_or(|n| t.change == n))
            .collect();
        if targets.is_empty() {
            match which {
                Some(n) => bail!("변경 번호 {n}의 변경 내용이 없습니다."),
                None => bail!("변경 추적 기록이 없습니다."),
            }
        }
        let unsupported = targets
            .iter()
            .filter(|t| !accept && !matches!(t.kind, 16 | 17))
            .count();
        let resolved = targets.len() - unsupported;
        if resolved == 0 {
            bail!("서식 변경은 옛 서식을 알 수 없어 취소하지 못합니다.");
        }
        // Accepting or rejecting is allowed while recording changes.
        self.track_scope(|s| {
            s.transaction(|s| {
                s.deferred_layout(|s| {
                    // Last paragraph first, so a join and a deletion leave earlier ones in
                    // place; a body paragraph's cells before its own text, which may hold
                    // their table.
                    let mut paragraphs: Vec<Cursor> = targets.iter().map(|t| t.at).collect();
                    paragraphs.sort_by_key(paragraph_key);
                    paragraphs.dedup();
                    for &at in paragraphs.iter().rev() {
                        let section = at.section;
                        let mine: Vec<&TrackedRange> = targets
                            .iter()
                            .filter(|t| t.at == at)
                            .filter(|t| accept || matches!(t.kind, 16 | 17))
                            .collect();
                        // Accepting removes deleted text, rejecting removes inserted text.
                        let removes_text =
                            |t: &TrackedRange| matches!((accept, t.kind), (true, 17) | (false, 16));
                        let mut cuts: Vec<Range<usize>> = mine
                            .iter()
                            .filter(|t| removes_text(t))
                            .map(|t| t.range.clone())
                            .collect();
                        let joins = mine.iter().any(|t| removes_text(t) && t.breaks);
                        let tags: Vec<u32> = mine.iter().map(|t| t.tag).collect();
                        let para = s.paragraph_mut(at)?;
                        para.range_tags.retain(|r| {
                            !(TRACK_RANGE_KINDS.contains(&(r.tag >> 24))
                                && tags.contains(&r.tag)
                                && mine.iter().any(|t| (t.start, t.end) == (r.start, r.end)))
                        });
                        s.core.document_mut().sections[section].raw_stream = None;
                        cuts.sort_by_key(|r| std::cmp::Reverse(r.start));
                        for cut in cuts.into_iter().filter(|r| !r.is_empty()) {
                            let deleted = match at.cell {
                                Some(c) => s.core.delete_text_in_cell_native(
                                    section,
                                    at.paragraph,
                                    c.control,
                                    c.index,
                                    c.paragraph,
                                    cut.start,
                                    cut.len(),
                                ),
                                None => s.core.delete_text_native(
                                    section,
                                    at.paragraph,
                                    cut.start,
                                    cut.len(),
                                ),
                            };
                            deleted.map_err(err)?;
                        }
                        if joins {
                            s.join_next_paragraph(at)?;
                        }
                    }
                    if drop_records && tracked_ranges(s.core.document()).is_empty() {
                        s.drop_track_changes();
                    }
                    Ok(())
                })
            })
        })?;
        let remaining = tracked_ranges(self.core.document()).len();
        self.cursor.offset = self.cursor.offset.min(self.len());
        Ok(json!({
            "resolved": resolved,
            "remaining": remaining,
            "unsupported": unsupported,
            "tracked": self.has_unkept_track_changes(),
        }))
    }

    /// The document's change-tracking state.
    pub fn track_changes(&self) -> Value {
        let doc = self.core.document();
        let count = |tag| {
            doc.doc_info
                .extra_records
                .iter()
                .filter(|r| r.tag_id == tag)
                .count()
        };
        let (authors, changes) = (count(TRACK_CHANGE_AUTHOR), count(TRACK_CHANGE));
        let ranges = tracked_ranges(doc).len();
        let header = doc.header.flags & HEADER_TRACKED != 0;
        let marks = self.track_marks;
        let tracked = header || authors + changes + ranges + marks > 0;
        let format = if marks > 0 { "hwpx" } else { "hwp" };
        let list = self.track_change_list();
        json!({
            "tracked": tracked,
            "format": format,
            "authors": authors,
            "changes": changes,
            "ranges": ranges,
            "marks": marks,
            "recording": self.track_recording(),
            "keptOnHwpSave": tracked && self.keeps_track_changes(Path::new("x.hwp")),
            "keptOnHwpxSave": tracked && self.keeps_track_changes(Path::new("x.hwpx")),
            "list": list,
            "listTruncated": ranges > MAX_LISTED,
            "message": if tracked { REFUSED } else { "" },
        })
    }

    /// The tracked changes of the body, in document order (HWP range tags): kind
    /// (insert, delete, format), section, paragraph, start..end (scalars), the text,
    /// and the author and date of its TRACK_CHANGE record.
    fn track_change_list(&self) -> Vec<Value> {
        let doc = self.core.document();
        let records: Vec<&[u8]> = doc
            .doc_info
            .extra_records
            .iter()
            .filter(|r| r.tag_id == TRACK_CHANGE)
            .map(|r| r.data.as_slice())
            .collect();
        let authors: Vec<String> = doc
            .doc_info
            .extra_records
            .iter()
            .filter(|r| r.tag_id == TRACK_CHANGE_AUTHOR)
            .map(|r| author_name(&r.data))
            .collect();
        tracked_ranges(doc)
            .into_iter()
            .take(MAX_LISTED)
            .map(|t| {
                let text: String = paragraph_at(doc, &t.at)
                    .map_or("", |p| p.text.as_str())
                    .chars()
                    .skip(t.range.start)
                    .take(t.range.len())
                    .collect();
                let (author, date) = t
                    .change
                    .checked_sub(1)
                    .and_then(|i| records.get(i))
                    .map(|data| change_record(data, &authors))
                    .unwrap_or_default();
                json!({
                    "change": t.change,
                    "paragraphBreak": t.breaks,
                    "kind": match t.kind {
                        16 => "insert",
                        17 => "delete",
                        18 => "format",
                        _ => "other",
                    },
                    "section": t.at.section,
                    "paragraph": t.at.paragraph,
                    "cell": t.at.cell,
                    "start": t.range.start,
                    "end": t.range.end,
                    "text": text,
                    "author": author,
                    "date": date,
                })
            })
            .collect()
    }

    /// The mark lines of the tracked changes, in page pixels: under inserted and
    /// format-changed text, through deleted text. Drawn with the markpen overlay.
    pub(crate) fn track_mark_rects(&self) -> Vec<HighlightRect> {
        let ranges = tracked_ranges(self.core.document());
        if ranges.is_empty() {
            return Vec::new();
        }
        let color = |kind| match kind {
            16 => INSERTED,
            17 => DELETED,
            _ => FORMATTED,
        };
        let probes: Vec<(Cursor, Range<usize>, &str)> = ranges
            .iter()
            .map(|t| (t.at, t.range.clone(), color(t.kind)))
            .collect();
        self.body_range_rects(&probes, MARK_PROBES)
            .into_iter()
            .map(|r| {
                // A vertical section's run goes down a column: the line runs beside it
                // (inserted, on the right as Hangul underlines vertical text) or through it.
                if r.vertical {
                    let thickness = (r.width / 14.0).clamp(1.0, 2.5);
                    let x = if r.color == DELETED {
                        r.x + r.width * 0.5 - thickness / 2.0
                    } else {
                        r.x + r.width - thickness
                    };
                    return HighlightRect {
                        x,
                        width: thickness,
                        solid: true,
                        ..r
                    };
                }
                let thickness = (r.height / 14.0).clamp(1.0, 2.5);
                let y = if r.color == DELETED {
                    r.y + r.height * 0.55 - thickness / 2.0
                } else {
                    r.y + r.height - thickness
                };
                HighlightRect {
                    y,
                    height: thickness,
                    solid: true,
                    ..r
                }
            })
            .collect()
    }

    /// Join the paragraph at `at` (body or table cell) with the next one, if any.
    pub(crate) fn join_next_paragraph(&mut self, at: Cursor) -> Result<()> {
        match at.cell {
            Some(c) => {
                let count = paragraph_at(self.core.document(), &Cursor { cell: None, ..at })
                    .and_then(|p| match p.controls.get(c.control) {
                        Some(Control::Table(t)) => t.cells.get(c.index),
                        _ => None,
                    })
                    .map_or(0, |cell| cell.paragraphs.len());
                if c.paragraph + 1 < count {
                    self.core
                        .merge_paragraph_in_cell_native(
                            at.section,
                            at.paragraph,
                            c.control,
                            c.index,
                            c.paragraph + 1,
                        )
                        .map_err(err)?;
                }
            }
            None => {
                let next = at.paragraph + 1;
                if next < self.core.document().sections[at.section].paragraphs.len() {
                    self.core
                        .merge_paragraph_native(at.section, next)
                        .map_err(err)?;
                }
            }
        }
        Ok(())
    }

    pub fn has_unkept_track_changes(&self) -> bool {
        self.track_changes()["tracked"] == true
    }

    /// Save as HWP/HWPX accepting every tracked change (deleted text goes, inserted
    /// text stays), then without the tracking records so the file is consistent.
    pub fn save_accepting_track_changes(&mut self, path: &Path) -> Result<()> {
        self.accept_all_track_changes()?;
        self.save(path)
    }

    /// Accept every tracked change and drop the tracking records, before a save or
    /// export that merges them. HWPX marks are refused: RHWP does not read them, so
    /// the deleted text they enclose would stay as ordinary text.
    pub(crate) fn accept_all_track_changes(&mut self) -> Result<()> {
        if self.track_marks > 0 {
            bail!(
                "HWPX 문서의 변경 추적 표식(insertBegin·deleteBegin)은 읽지 못해, 삭제한 글이 보통 글로 남습니다. 한글에서 변경 내용을 적용한 뒤 여세요."
            );
        }
        if !tracked_ranges(self.core.document()).is_empty() {
            self.resolve_track_changes(true, None)?;
        }
        self.drop_track_changes();
        Ok(())
    }

    /// Drop the tracking records so the shown text is the whole document.
    pub(crate) fn drop_track_changes(&mut self) {
        let doc = self.core.document_mut();
        doc.header.flags &= !HEADER_TRACKED;
        if let Some(raw) = doc.header.raw_data.as_mut()
            && raw.len() >= 40
        {
            let flags = u32::from_le_bytes([raw[36], raw[37], raw[38], raw[39]]) & !HEADER_TRACKED;
            raw[36..40].copy_from_slice(&flags.to_le_bytes());
        }
        // The HWPX header block (RHWP 0082) goes with the records.
        doc.doc_info.track_changes_xml = None;
        let before = doc.doc_info.extra_records.len();
        doc.doc_info
            .extra_records
            .retain(|r| !matches!(r.tag_id, TRACK_CHANGE | TRACK_CHANGE_AUTHOR));
        if doc.doc_info.extra_records.len() != before {
            doc.doc_info.raw_stream = None;
            doc.doc_info.raw_stream_dirty = true;
        }
        for section in &mut doc.sections {
            let mut touched = false;
            for paragraph in &mut section.paragraphs {
                let before = paragraph.range_tags.len();
                paragraph
                    .range_tags
                    .retain(|r| !TRACK_RANGE_KINDS.contains(&(r.tag >> 24)));
                touched |= paragraph.range_tags.len() != before;
            }
            if touched {
                section.raw_stream = None;
            }
        }
        self.track_marks = 0;
    }

    /// Every section is written as read (RHWP patch 0070: BodyText and ViewText):
    /// an HWP document with no change since it was opened or saved, and no tab
    /// width the save would have to fill in.
    fn unchanged_hwp(&self) -> bool {
        let doc = self.core.document();
        !self.dirty()
            && self.track_marks == 0
            && doc
                .sections
                .iter()
                .all(|s| s.raw_stream.is_some() && s.raw_provenance_permits_reuse())
            && !self.tab_widths_missing()
    }

    /// A save that keeps the document's change tracking: HWP without a document
    /// password (an unchanged document as read, RHWP 0070; otherwise BodyText from the
    /// accepted copy and ViewText from the live model, `save_tracked_hwp`), or HWPX
    /// (RHWP 0083 writes the marks and the header's tracking block). Not while HWPX
    /// marks RHWP could not read are in the file.
    pub(crate) fn keeps_track_changes(&self, path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .map(str::to_ascii_lowercase);
        self.track_marks == 0
            && match ext.as_deref() {
                Some("hwp") => self.password.is_none(),
                Some("hwpx") => true,
                _ => false,
            }
    }

    /// Does saving `path` keep the tracking (MCP `trackChangesKept`)?
    pub fn save_keeps_track_changes(&self, path: &Path) -> bool {
        self.has_unkept_track_changes() && self.keeps_track_changes(path)
    }

    /// An HWP save that writes the tracked document again (it changed since it was
    /// opened or saved): see `save_tracked_hwp`.
    pub(crate) fn rewrites_tracked_hwp(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("hwp"))
            && self.has_unkept_track_changes()
            && self.track_marks == 0
            && !self.unchanged_hwp()
    }

    /// Write the tracked document as HWP: BodyText from a copy with every tracked
    /// change accepted (records kept), ViewText from the live model, the way Hangul
    /// stores it. An edit made while recording was off is in both.
    pub(crate) fn save_tracked_hwp(&self, path: &Path) -> Result<()> {
        let mut core = DocumentCore::new_empty();
        core.set_source_format(self.core.source_format());
        core.set_document(self.core.document().clone());
        let mut copy = Session::with_core(core, None);
        if !tracked_ranges(copy.core.document()).is_empty() {
            copy.resolve_track_changes_with(true, None, false)?;
        }
        let mut body = copy.core.document().clone();
        // HWPX has no FileHeader: a document read from one gets the tracking bit here.
        body.header.flags |= HEADER_TRACKED;
        if let Some(raw) = body.header.raw_data.as_mut()
            && raw.len() >= 40
        {
            let flags = u32::from_le_bytes([raw[36], raw[37], raw[38], raw[39]]) | HEADER_TRACKED;
            raw[36..40].copy_from_slice(&flags.to_le_bytes());
        }
        hwpcraft_formats::save_tracked_hwp(&self.core, body, path)
    }

    /// Refuse an HWP/HWPX save that would carry tracking records it cannot keep.
    pub(crate) fn refuse_tracked_save(&self, path: &Path) -> Result<()> {
        let editable = path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
            ["hwp", "hwpx", "docx"]
                .iter()
                .any(|e| x.eq_ignore_ascii_case(e))
        });
        if editable && self.has_unkept_track_changes() {
            bail!(REFUSED);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "../../../rhwp/samples/issue5169_viewtext_changetracking.hwp";
    /// A tracked document without memos (one section, 1.4 MB).
    const KEPT: &str =
        "../../../rhwp/samples/issue6778/156757920-animal-welfare-husbandry-guidelines.hwp";

    /// The ViewText stream of section `index`, as RHWP keeps it for an HWP save.
    fn view_text(s: &Session, index: usize) -> Option<Vec<u8>> {
        let name = format!("/ViewText/Section{index}");
        s.core
            .document()
            .extra_streams
            .iter()
            .find(|(path, _)| *path == name)
            .map(|(_, bytes)| bytes.clone())
    }

    #[test]
    fn an_unchanged_tracked_hwp_saves_with_its_tracking() -> Result<()> {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join(KEPT);
        if !sample.exists() {
            return Ok(());
        }
        let mut s = Session::new()?;
        s.open(&sample)?;
        let state = s.track_changes();
        assert_eq!(state["keptOnHwpSave"], true, "{state}");
        let dir = tempfile::tempdir()?;
        // HWPX keeps it too (RHWP 0083); DOCX cannot hold it.
        let hwpx = dir.path().join("kept.hwpx");
        s.save(&hwpx)?;
        let mut as_hwpx = Session::new()?;
        as_hwpx.open(&hwpx)?;
        assert_eq!(as_hwpx.track_changes()["list"], state["list"]);
        let docx = dir.path().join("kept.docx");
        assert!(s.save(&docx).is_err());
        assert!(!docx.exists());
        let kept = dir.path().join("kept.hwp");
        s.save(&kept)?;
        let mut back = Session::new()?;
        back.open(&kept)?;
        assert_eq!(back.track_changes(), s.track_changes());
        // BodyText (the accepted result) and ViewText (the marks) as read.
        let (before, after) = (s.core.document(), back.core.document());
        assert_eq!(before.sections.len(), after.sections.len());
        for (index, (a, b)) in before.sections.iter().zip(&after.sections).enumerate() {
            assert!(a.raw_stream.is_some());
            assert_eq!(a.raw_stream, b.raw_stream, "BodyText {index}");
            assert!(view_text(&s, index).is_some(), "ViewText {index}");
            assert_eq!(view_text(&s, index), view_text(&back, index), "{index}");
            let text = |d: &hwpcraft_doc::model::document::Section| {
                d.paragraphs
                    .iter()
                    .map(|p| p.text.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(text(a), text(b), "section {index}");
        }
        Ok(())
    }

    #[test]
    fn an_edited_tracked_hwp_keeps_its_tracking_and_the_edit() -> Result<()> {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
        if !sample.exists() {
            return Ok(());
        }
        let mut s = Session::new()?;
        s.open(&sample)?;
        let state = s.run("document.track_changes", json!({}))?;
        assert_eq!(state["tracked"], true, "{state}");
        // Its memos are filled in from the stored records on open; the section is
        // still the file's, so an unchanged save would keep the tracking.
        assert_eq!(state["keptOnHwpSave"], true, "{state}");
        // An edit made while recording is off goes into the marked text and the
        // result alike: the saved BodyText has it, and accepting every change of the
        // saved file gives that BodyText.
        s.run("text.insert", json!({"text": "가"}))?;
        assert_eq!(s.track_changes()["keptOnHwpSave"], true);
        let dir = tempfile::tempdir()?;
        let plain = dir.path().join("plain.hwp");
        s.save(&plain)?;
        let mut back = Session::new()?;
        back.open(&plain)?;
        assert_eq!(back.track_changes()["ranges"], state["ranges"]);
        assert!(shown(&back)[0].starts_with('가'), "{:?}", shown(&back)[0]);
        let stored = stored_result(&back)?;
        assert!(stored[0].starts_with('가'), "{:?}", stored[0]);
        back.run("document.track_changes.accept", json!({"all": true}))?;
        assert_eq!(shown(&back), stored);
        // Page and block saves cannot hold the tracking.
        let pages = s.page_count()?;
        let whole = dir.path().join("pages.hwp");
        assert!(
            s.run("file.save_pages", json!({"path":whole,"from":1,"to":pages}))
                .is_err()
        );
        assert!(!whole.exists());
        // PDF and TXT exports are not affected.
        s.save(&dir.path().join("out.txt"))?;
        let accepted = dir.path().join("accepted.hwp");
        s.save_accepting_track_changes(&accepted)?;
        let mut reopened = Session::new()?;
        reopened.open(&accepted)?;
        assert_eq!(reopened.track_changes()["tracked"], false);
        // Once accepted, the document saves normally.
        s.save(&dir.path().join("again.hwpx"))?;
        Ok(())
    }

    /// Section 0 as Hangul stored the accepted result (BodyText, kept by RHWP 0070).
    fn stored_result(s: &Session) -> Result<Vec<String>> {
        let raw = s.core.document().sections[0]
            .raw_stream
            .clone()
            .ok_or_else(|| anyhow!("BodyText가 없습니다."))?;
        let records = hwpcraft_doc::HwpRecord::read_all(&raw).map_err(|e| anyhow!("{e}"))?;
        Ok(texts(&hwpcraft_doc::parse_hwp_paragraph_list(&records)))
    }

    /// Paragraph texts, each followed by its tables' cell paragraphs ("| " text).
    fn texts(paragraphs: &[Paragraph]) -> Vec<String> {
        let mut out = Vec::new();
        for p in paragraphs {
            out.push(p.text.clone());
            for control in &p.controls {
                if let Control::Table(t) = control {
                    for cell in &t.cells {
                        out.extend(cell.paragraphs.iter().map(|c| format!("| {}", c.text)));
                    }
                }
            }
        }
        out
    }

    fn shown(s: &Session) -> Vec<String> {
        texts(&s.core.document().sections[0].paragraphs)
    }

    #[test]
    fn tracked_changes_are_listed_and_marked() -> Result<()> {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join(KEPT);
        if !sample.exists() {
            return Ok(());
        }
        let mut s = Session::new()?;
        s.open(&sample)?;
        let state = s.track_changes();
        let list = state["list"].as_array().cloned().unwrap_or_default();
        assert_eq!(Some(list.len() as u64), state["ranges"].as_u64(), "{state}");
        let kinds: Vec<&str> = list.iter().filter_map(|c| c["kind"].as_str()).collect();
        assert!(
            kinds.contains(&"insert") && kinds.contains(&"delete"),
            "{kinds:?}"
        );
        for change in &list {
            let (start, end) = (change["start"].as_u64(), change["end"].as_u64());
            assert!(start <= end, "{change}");
            assert!(
                change["date"].as_str().is_some_and(|d| d.starts_with("20")),
                "{change}"
            );
            let at = Cursor {
                section: change["section"].as_u64().unwrap_or(0) as usize,
                paragraph: change["paragraph"].as_u64().unwrap_or(0) as usize,
                offset: 0,
                cell: serde_json::from_value(change["cell"].clone())?,
            };
            let text = paragraph_at(s.core.document(), &at).map_or("", |p| p.text.as_str());
            let expected: String = text
                .chars()
                .skip(start.unwrap_or(0) as usize)
                .take((end.unwrap_or(0) - start.unwrap_or(0)) as usize)
                .collect();
            assert_eq!(change["text"], expected, "{change}");
        }
        // Inserted text is underlined in blue, deleted text struck through in red,
        // on screen and in PDF output alike.
        let marks = s.track_mark_rects();
        assert!(marks.iter().any(|m| m.color == INSERTED), "{marks:?}");
        assert!(
            marks.iter().all(|m| m.solid && m.height <= 2.5),
            "{marks:?}"
        );
        let page = marks[0].page;
        assert!(s.render_page_svg(page)?.contains(r#"fill-opacity="1""#));
        Ok(())
    }

    #[test]
    fn accepting_and_rejecting_tracked_changes() -> Result<()> {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join(KEPT);
        if !sample.exists() {
            return Ok(());
        }
        let mut s = Session::new()?;
        s.open(&sample)?;
        let viewed = shown(&s);
        let stored = stored_result(&s)?;
        assert_ne!(viewed, stored);
        let ranges = s.track_changes()["ranges"].clone();
        assert!(s.run("document.track_changes.accept", json!({})).is_err());
        assert!(
            s.run("document.track_changes.accept", json!({"change": 9999}))
                .is_err()
        );

        // Accepting every change gives the result Hangul stored.
        let accepted = s.run("document.track_changes.accept", json!({"all": true}))?;
        assert_eq!(accepted["remaining"], 0, "{accepted}");
        assert_eq!(accepted["tracked"], false, "{accepted}");
        assert_eq!(shown(&s), stored);
        // One undo step brings the marked text and the tracking back.
        s.run("edit.undo", json!({}))?;
        assert_eq!(shown(&s), viewed);
        assert_eq!(s.track_changes()["ranges"], ranges);

        // Accepting one change resolves only its ranges.
        let first = s.track_changes()["list"][0]["change"].clone();
        let one = s.run("document.track_changes.accept", json!({"change": first}))?;
        assert!(one["remaining"].as_u64() < ranges.as_u64(), "{one}");
        assert_eq!(one["tracked"], true, "{one}");
        s.run("edit.undo", json!({}))?;

        // Rejecting every change removes the inserted text and keeps the deleted text.
        let inserted: usize = s.track_changes()["list"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["kind"] == "insert")
            .map(|c| (c["end"].as_u64().unwrap_or(0) - c["start"].as_u64().unwrap_or(0)) as usize)
            .sum();
        let rejected = s.run("document.track_changes.reject", json!({"all": true}))?;
        assert_eq!(rejected["remaining"], 0, "{rejected}");
        let count = |texts: &[String]| {
            texts
                .iter()
                .map(|t| t.strip_prefix("| ").unwrap_or(t).chars().count())
                .sum::<usize>()
        };
        assert_eq!(count(&shown(&s)), count(&viewed) - inserted);

        // An accepting save writes the stored result, without the tracking.
        s.run("edit.undo", json!({}))?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("accepted.hwp");
        s.save_accepting_track_changes(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(back.track_changes()["tracked"], false);
        assert_eq!(shown(&back), stored);
        Ok(())
    }

    /// An HWPX package with change tracking as the OWPML schema writes it: "마바"
    /// inserted (change 1), "자차카타" and the paragraph break deleted (change 2).
    /// No tracked HWPX sample exists; this follows ParaList/Header XML schema.
    fn tracked_hwpx(dir: &Path) -> Result<PathBuf> {
        use std::io::Write;
        let plain = dir.join("plain.hwpx");
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text": "가나다라 마바사아 자차카타\n다음 문단"}),
        )?;
        s.save(&plain)?;
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&plain)?)?;
        let path = dir.join("tracked.hwpx");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path)?);
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            let mut text = String::from_utf8_lossy(&data).into_owned();
            if name == "Contents/header.xml" {
                text = text.replacen(
                    "</hh:refList>",
                    concat!(
                        r#"<hh:trackChanges itemCnt="2">"#,
                        r#"<hh:trackChange type="Insert" date="2026-11-01T09:30:00Z" authorID="1" hide="0" id="1"/>"#,
                        r#"<hh:trackChange type="Delete" date="2026-11-01T09:31:00Z" authorID="1" hide="0" id="2"/>"#,
                        r#"</hh:trackChanges><hh:trackChangeAuthors itemCnt="1">"#,
                        r##"<hh:trackChangeAuthor name="검토자" mark="1" color="#FF0000" id="1"/>"##,
                        r#"</hh:trackChangeAuthors></hh:refList>"#,
                    ),
                    1,
                );
                data = text.into_bytes();
            } else if name == "Contents/section0.xml" {
                anyhow::ensure!(text.contains("가나다라 마바사아 자차카타"), "본문 글");
                text = text.replacen(
                    "가나다라 마바사아 자차카타",
                    concat!(
                        r#"가나다라 <hp:insertBegin paraend="0" TcId="1" Id="1"/>마바"#,
                        r#"<hp:insertEnd paraend="0" TcId="1" Id="1"/>사아 "#,
                        r#"<hp:deleteBegin paraend="0" TcId="2" Id="2"/>자차카타"#,
                        r#"<hp:deleteEnd paraend="1" TcId="2" Id="2"/>"#,
                    ),
                    1,
                );
                data = text.into_bytes();
            }
            let method = if name == "mimetype" {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            };
            writer.start_file(
                name,
                zip::write::SimpleFileOptions::default().compression_method(method),
            )?;
            writer.write_all(&data)?;
        }
        writer.finish()?;
        Ok(path)
    }

    #[test]
    fn hwpx_tracked_changes_are_listed_kept_and_resolved() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = tracked_hwpx(dir.path())?;
        let mut s = Session::new()?;
        s.open(&path)?;
        let state = s.track_changes();
        assert_eq!(state["tracked"], true, "{state}");
        assert_eq!(state["keptOnHwpxSave"], true, "{state}");
        let list = state["list"].as_array().cloned().unwrap_or_default();
        let brief: Vec<(String, String, String)> = list
            .iter()
            .map(|c| {
                (
                    c["kind"].as_str().unwrap_or("").to_owned(),
                    c["text"].as_str().unwrap_or("").to_owned(),
                    c["author"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        assert_eq!(
            brief,
            [
                ("insert".into(), "마바".into(), "검토자".into()),
                ("delete".into(), "자차카타".into(), "검토자".into()),
            ]
        );
        assert_eq!(list[1]["paragraphBreak"], true);
        assert_eq!(list[1]["date"], "2026-11-01 09:31");

        // Saved unchanged as HWPX, the tracking stays; as HWP it is refused.
        let kept = dir.path().join("kept.hwpx");
        s.save(&kept)?;
        // As HWP it keeps the tracking too: BodyText accepted, ViewText marked.
        let as_hwp = dir.path().join("as.hwp");
        s.save(&as_hwp)?;
        let mut hwp = Session::new()?;
        hwp.open(&as_hwp)?;
        assert_eq!(hwp.track_changes()["list"], state["list"]);
        assert_eq!(stored_result(&hwp)?, ["가나다라 마바사아 다음 문단"]);
        let mut back = Session::new()?;
        back.open(&kept)?;
        assert_eq!(back.track_changes()["list"], state["list"]);

        // Accepting every change drops "자차카타" and its paragraph break.
        back.run("document.track_changes.accept", json!({"all": true}))?;
        assert_eq!(shown(&back), ["가나다라 마바사아 다음 문단"]);
        let accepted = dir.path().join("accepted.hwpx");
        back.save(&accepted)?;
        let mut clean = Session::new()?;
        clean.open(&accepted)?;
        assert_eq!(clean.track_changes()["tracked"], false);
        assert_eq!(shown(&clean), ["가나다라 마바사아 다음 문단"]);
        Ok(())
    }

    #[test]
    fn untracked_documents_save_as_before() -> Result<()> {
        let mut s = Session::new()?;
        s.run("text.insert", json!({"text":"보통 문서"}))?;
        assert_eq!(s.track_changes()["tracked"], false);
        let dir = tempfile::tempdir()?;
        s.save(&dir.path().join("a.hwp"))?;
        s.save(&dir.path().join("a.hwpx"))?;
        assert!(s.run("document.track_changes", json!({"x":1})).is_err());
        Ok(())
    }
}
