//! Owner: format area. 변경 추적 기록하며 편집하기 (한글 검토 ▸ 변경 추적).
//!
//! While recording, typed text gets range tag 16 (insert) and deleted text stays in
//! the document under range tag 17 (delete); each tag's data is the 1-based number
//! of a TRACK_CHANGE record (kind, date, author) — the HWP 5 layout Hangul 2024
//! writes (FileHeader bit 14, TRACK_CHANGE_AUTHOR 97, TRACK_CHANGE 96, record 94).
//! Deleting text this recording inserted removes it outright. A paragraph split is
//! an insert over the paragraph break, a join a delete over it. Body paragraphs and
//! table cell paragraphs are recorded; any other edit (formatting, objects, table
//! structure, sections, formatted paste…) is refused while recording. A save writes
//! the accepted result to BodyText and the marked text to ViewText
//! (`save_tracked_hwp`), or the marks to HWPX (RHWP 0083).
use super::*;
use chrono::{Datelike, Timelike};

const TRACK_CHANGE: u16 = 96;
const TRACK_CHANGE_AUTHOR: u16 = 97;
/// Present (16 zero bytes) in every tracked HWP Hangul 2024 wrote.
const TRACK_SETTINGS: u16 = 94;
const HEADER_TRACKED: u32 = 1 << 14;
const INSERT: u32 = 16;
const DELETE: u32 = 17;

pub const REFUSED_WHILE_RECORDING: &str = "변경 추적 기록 중에는 본문과 표 셀의 글자 입력·지우기·문단 나누기와 합치기만 기록합니다. 이 편집(서식·개체·표 구조·구역·서식 있는 붙여넣기 등)은 아직 기록하지 못하므로, 변경 추적 기록을 끈 뒤에 하세요(document.track_changes.record {\"on\":false}).";

/// The recording state of a session.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrackRecorder {
    author: String,
    /// TRACK_CHANGE numbers this recording made: deleting their inserted text
    /// removes it.
    own: std::collections::BTreeSet<u32>,
}

/// The stream position of scalar `i` of `p`; past the text, the paragraph break.
fn stream_pos(p: &Paragraph, i: usize) -> u32 {
    p.char_offsets
        .get(i)
        .copied()
        .unwrap_or_else(|| p.char_count.saturating_sub(1))
}

fn kind_of(tag: &hwpcraft_doc::model::paragraph::RangeTag) -> u32 {
    tag.tag >> 24
}

fn data_of(tag: &hwpcraft_doc::model::paragraph::RangeTag) -> u32 {
    tag.tag & 0x00FF_FFFF
}

impl Session {
    pub fn track_recording(&self) -> bool {
        self.track_recorder.is_some()
    }

    /// Run `f` as an edit recording may make (typing, deleting, splitting, joining,
    /// accepting or rejecting changes).
    pub(crate) fn track_scope<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let previous = std::mem::replace(&mut self.track_editing, true);
        let result = f(self);
        self.track_editing = previous;
        result
    }

    /// Refuse an edit recording cannot record (see `transaction`).
    pub(crate) fn guard_track_recording(&self) -> Result<()> {
        if self.track_recorder.is_some() && !self.track_editing {
            bail!(REFUSED_WHILE_RECORDING);
        }
        Ok(())
    }

    /// Start (`on`) or stop recording changes. Starting marks the document as
    /// change-tracked (FileHeader bit 14, the author's TRACK_CHANGE_AUTHOR record,
    /// record 94) in one undo step; stopping keeps every recorded change.
    pub fn record_track_changes(&mut self, on: bool, author: Option<String>) -> Result<Value> {
        if !on {
            self.track_recorder = None;
            return Ok(self.track_changes());
        }
        if self.track_marks > 0 {
            bail!(
                "이 HWPX 문서의 변경 추적 표식(insertBegin·deleteBegin)은 읽지 못해 변경 추적을 기록할 수 없습니다."
            );
        }
        let author = author
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "사용자".to_owned());
        if author.chars().count() > 64 || author.chars().any(char::is_control) {
            bail!("변경 추적 작성자 이름은 64자 이하이고 제어 문자가 없어야 합니다.");
        }
        self.track_scope(|s| {
            s.transaction(|s| {
                s.tracking_author_index(&author);
                Ok(())
            })
        })?;
        self.track_recorder = Some(TrackRecorder {
            author,
            own: Default::default(),
        });
        Ok(self.track_changes())
    }

    /// The 1-based TRACK_CHANGE_AUTHOR number of `author`, adding the record (and
    /// the FileHeader bit and record 94) when the document has none yet.
    fn tracking_author_index(&mut self, author: &str) -> u16 {
        let doc = self.core.document_mut();
        doc.header.flags |= HEADER_TRACKED;
        if let Some(raw) = doc.header.raw_data.as_mut()
            && raw.len() >= 40
        {
            let flags = u32::from_le_bytes([raw[36], raw[37], raw[38], raw[39]]) | HEADER_TRACKED;
            raw[36..40].copy_from_slice(&flags.to_le_bytes());
        }
        let records = &mut doc.doc_info.extra_records;
        let authors: Vec<usize> = (0..records.len())
            .filter(|&i| records[i].tag_id == TRACK_CHANGE_AUTHOR)
            .collect();
        if let Some(n) = authors
            .iter()
            .position(|&i| track_changes::author_name(&records[i].data) == author)
        {
            return n as u16 + 1;
        }
        // u16 length, two bytes, the UTF-16 name, then the eight bytes Hangul writes.
        let units: Vec<u16> = author.encode_utf16().collect();
        let mut data = Vec::with_capacity(12 + 2 * units.len());
        data.extend_from_slice(&(units.len() as u16).to_le_bytes());
        data.extend_from_slice(&[0, 0]);
        for unit in units {
            data.extend_from_slice(&unit.to_le_bytes());
        }
        data.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        // Hangul's order: authors, changes, record 94.
        let at = authors
            .last()
            .map(|&i| i + 1)
            .or_else(|| {
                records
                    .iter()
                    .position(|r| matches!(r.tag_id, TRACK_CHANGE | TRACK_SETTINGS))
            })
            .unwrap_or(records.len());
        records.insert(
            at,
            hwpcraft_doc::model::document::RawRecord {
                tag_id: TRACK_CHANGE_AUTHOR,
                level: 1,
                data,
            },
        );
        if !records.iter().any(|r| r.tag_id == TRACK_SETTINGS) {
            records.push(hwpcraft_doc::model::document::RawRecord {
                tag_id: TRACK_SETTINGS,
                level: 1,
                data: vec![0; 16],
            });
        }
        doc.doc_info.raw_stream = None;
        doc.doc_info.raw_stream_dirty = true;
        // The HWPX header block is rebuilt from the records.
        doc.doc_info.track_changes_xml = None;
        authors.len() as u16 + 1
    }

    /// The TRACK_CHANGE number for a recorded edit of `kind` (16 or 17) now: the
    /// last record when it is this recording's, of the same kind, author and minute.
    fn recorded_change(&mut self, kind: u32) -> Result<u32> {
        let author = self
            .track_recorder
            .as_ref()
            .map(|r| r.author.clone())
            .ok_or_else(|| anyhow!("변경 추적 기록 중이 아닙니다."))?;
        let author_index = self.tracking_author_index(&author);
        let now = date_fields::now().with_timezone(&chrono::Local);
        let mut data = Vec::with_capacity(26);
        data.extend_from_slice(&(if kind == INSERT { 0x10u32 } else { 0x11u32 }).to_le_bytes());
        for field in [
            now.year() as u16,
            now.month() as u16,
            now.day() as u16,
            now.hour() as u16,
            now.minute() as u16,
            author_index,
        ] {
            data.extend_from_slice(&field.to_le_bytes());
        }
        data.extend_from_slice(&[0; 10]);
        let own = self
            .track_recorder
            .as_ref()
            .map(|r| r.own.clone())
            .unwrap_or_default();
        let doc = self.core.document_mut();
        let records = &mut doc.doc_info.extra_records;
        let changes: Vec<usize> = (0..records.len())
            .filter(|&i| records[i].tag_id == TRACK_CHANGE)
            .collect();
        if let Some(&last) = changes.last()
            && records[last].data == data
            && own.contains(&(changes.len() as u32))
        {
            return Ok(changes.len() as u32);
        }
        let at = changes
            .last()
            .map(|&i| i + 1)
            .or_else(|| records.iter().position(|r| r.tag_id == TRACK_SETTINGS))
            .unwrap_or(records.len());
        records.insert(
            at,
            hwpcraft_doc::model::document::RawRecord {
                tag_id: TRACK_CHANGE,
                level: 1,
                data,
            },
        );
        doc.doc_info.raw_stream = None;
        doc.doc_info.raw_stream_dirty = true;
        doc.doc_info.track_changes_xml = None;
        let number = changes.len() as u32 + 1;
        if let Some(recorder) = self.track_recorder.as_mut() {
            recorder.own.insert(number);
        }
        Ok(number)
    }

    fn own_change(&self, data: u32) -> bool {
        self.track_recorder
            .as_ref()
            .is_some_and(|r| r.own.contains(&data))
    }

    /// Set a tracked range of `kind` and change `number` over scalars `range` of the
    /// paragraph at `at` (through its break when `to_break`), joined with the
    /// touching ranges of the same change.
    fn tag_range(
        &mut self,
        at: Cursor,
        kind: u32,
        number: u32,
        range: Range<usize>,
        to_break: bool,
    ) -> Result<()> {
        let para = self.paragraph_mut(at)?;
        let mut start = stream_pos(para, range.start);
        let mut end = if to_break {
            para.char_count
        } else {
            stream_pos(para, range.end)
        };
        let tag = (kind << 24) | number;
        para.range_tags.retain(|t| {
            let touching = t.tag == tag && t.start <= end && t.end >= start;
            if touching {
                start = start.min(t.start);
                end = end.max(t.end);
            }
            !touching
        });
        para.range_tags
            .push(hwpcraft_doc::model::paragraph::RangeTag { start, end, tag });
        // The stored section stays: a tag changes no layout, and clearing it would lay
        // the whole section out by the session-edit rules. Saves write the model.
        Ok(())
    }

    /// Record text just inserted at scalars `range` of the paragraph at `at`: other
    /// tracked ranges RHWP stretched over it give it back, and it becomes an insert.
    pub(crate) fn track_inserted(&mut self, at: Cursor, range: Range<usize>) -> Result<()> {
        if self.track_recorder.is_none() || range.is_empty() {
            return Ok(());
        }
        let number = self.recorded_change(INSERT)?;
        let para = self.paragraph_mut(at)?;
        let (s, e) = (stream_pos(para, range.start), stream_pos(para, range.end));
        let mut kept = Vec::new();
        for t in para.range_tags.drain(..) {
            let tracked = matches!(kind_of(&t), 16..=19);
            if !tracked || t.end <= s || t.start >= e {
                kept.push(t);
                continue;
            }
            if t.start < s {
                kept.push(hwpcraft_doc::model::paragraph::RangeTag { end: s, ..t });
            }
            if t.end > e {
                kept.push(hwpcraft_doc::model::paragraph::RangeTag { start: e, ..t });
            }
        }
        para.range_tags = kept;
        self.tag_range(at, INSERT, number, range, false)
    }

    /// Record the paragraph split that made the paragraph after `at`: an insert over
    /// the break of the paragraph at `at`.
    pub(crate) fn track_split(&mut self, at: Cursor) -> Result<()> {
        if self.track_recorder.is_none() {
            return Ok(());
        }
        let number = self.recorded_change(INSERT)?;
        let len = self.paragraph(at)?.text.chars().count();
        self.tag_range(at, INSERT, number, len..len, true)
    }

    /// Delete or Backspace while recording.
    pub(crate) fn track_delete(&mut self, back: bool) -> Result<()> {
        let c = self.cursor;
        let len = self.len();
        if back && c.offset > 0 {
            let removed = self.track_delete_range(c, c.offset - 1..c.offset)?;
            self.cursor.offset = c.offset - 1;
            let _ = removed;
            return Ok(());
        }
        if !back && c.offset < len {
            let removed = self.track_delete_range(c, c.offset..c.offset + 1)?;
            self.cursor.offset = if removed { c.offset } else { c.offset + 1 };
            return Ok(());
        }
        // At a paragraph edge: the break between this paragraph and its neighbour.
        let here = c.cell.map_or(c.paragraph, |t| t.paragraph);
        let neighbour = |n: usize| match c.cell {
            Some(t) => Cursor {
                cell: Some(Cell { paragraph: n, ..t }),
                offset: 0,
                ..c
            },
            None => Cursor {
                paragraph: n,
                offset: 0,
                ..c
            },
        };
        if back {
            let Some(prev) = here.checked_sub(1).map(neighbour) else {
                return Ok(());
            };
            let end = self.paragraph(prev)?.text.chars().count();
            self.track_break(prev)?;
            self.cursor = Cursor {
                offset: end,
                ..prev
            };
        } else {
            let next = neighbour(here + 1);
            if self.paragraph(next).is_err() {
                return Ok(());
            }
            if !self.track_break(c)? {
                self.cursor = next;
            }
        }
        Ok(())
    }

    /// Delete the break after the paragraph at `at`: a break this recording
    /// inserted joins the paragraphs (true), any other is marked deleted.
    fn track_break(&mut self, at: Cursor) -> Result<bool> {
        let para = self.paragraph(at)?;
        let char_count = para.char_count;
        let own = para
            .range_tags
            .iter()
            .find(|t| kind_of(t) == INSERT && t.end >= char_count && self.own_change(data_of(t)))
            .cloned();
        if let Some(tag) = own {
            let para = self.paragraph_mut(at)?;
            para.range_tags
                .retain(|t| (t.start, t.end, t.tag) != (tag.start, tag.end, tag.tag));
            if tag.start < char_count.saturating_sub(1) {
                // The insert also holds text: it keeps the text, not the break.
                para.range_tags
                    .push(hwpcraft_doc::model::paragraph::RangeTag {
                        end: char_count.saturating_sub(1),
                        ..tag
                    });
            }
            self.join_next_paragraph(at)?;
            return Ok(true);
        }
        let already = para
            .range_tags
            .iter()
            .any(|t| kind_of(t) == DELETE && t.end >= char_count);
        if !already {
            let number = self.recorded_change(DELETE)?;
            let len = self.paragraph(at)?.text.chars().count();
            self.tag_range(at, DELETE, number, len..len, true)?;
        }
        Ok(false)
    }

    /// Delete scalars `range` of the paragraph at `at` while recording: the parts
    /// this recording inserted go, the rest is marked deleted. True when nothing is
    /// left of the range in the text.
    fn track_delete_range(&mut self, at: Cursor, range: Range<usize>) -> Result<bool> {
        let para = self.paragraph(at)?;
        let len = para.text.chars().count();
        let range = range.start.min(len)..range.end.min(len);
        let char_at = |pos: u32| {
            para.char_offsets
                .iter()
                .position(|&o| o >= pos)
                .unwrap_or(len)
        };
        let own: Vec<Range<usize>> = para
            .range_tags
            .iter()
            .filter(|t| kind_of(t) == INSERT && self.own_change(data_of(t)))
            .map(|t| char_at(t.start)..char_at(t.end))
            .collect();
        let deleted: Vec<Range<usize>> = para
            .range_tags
            .iter()
            .filter(|t| kind_of(t) == DELETE)
            .map(|t| char_at(t.start)..char_at(t.end))
            .collect();
        let inside = |set: &[Range<usize>], i: usize| set.iter().any(|r| r.contains(&i));
        // Pieces of the range: removed (own insert), marked (the rest), or already deleted.
        let mut removed: Vec<Range<usize>> = Vec::new();
        let mut marked: Vec<Range<usize>> = Vec::new();
        for i in range.clone() {
            let target = if inside(&own, i) {
                &mut removed
            } else if inside(&deleted, i) {
                continue;
            } else {
                &mut marked
            };
            match target.last_mut() {
                Some(r) if r.end == i => r.end = i + 1,
                _ => target.push(i..i + 1),
            }
        }
        if !marked.is_empty() {
            let number = self.recorded_change(DELETE)?;
            for r in &marked {
                self.tag_range(at, DELETE, number, r.clone(), false)?;
            }
        }
        for r in removed.iter().rev() {
            let deleted = match at.cell {
                Some(t) => self.core.delete_text_in_cell_native(
                    at.section,
                    at.paragraph,
                    t.control,
                    t.index,
                    t.paragraph,
                    r.start,
                    r.len(),
                ),
                None => self
                    .core
                    .delete_text_native(at.section, at.paragraph, r.start, r.len()),
            };
            deleted.map_err(err)?;
        }
        Ok(marked.is_empty() && removed.iter().map(|r| r.len()).sum::<usize>() == range.len())
    }

    /// Delete the selection while recording: within one section, body or one cell.
    pub(crate) fn track_delete_selection(&mut self) -> Result<()> {
        let parts = self.selected_paragraphs()?;
        let Some((first, first_range)) = parts.first().cloned() else {
            return Ok(());
        };
        if parts.iter().any(|(c, _)| {
            c.section != first.section
                || c.cell.map(|t| (t.control, t.index)) != first.cell.map(|t| (t.control, t.index))
        }) {
            bail!("변경 추적 기록 중에는 구역이나 표 셀을 넘는 선택을 지울 수 없습니다.");
        }
        let last = parts.len() - 1;
        for (i, (c, r)) in parts.iter().enumerate().rev() {
            if i < last {
                self.track_break(*c)?;
            }
            self.track_delete_range(*c, r.clone())?;
        }
        self.cursor = Cursor {
            offset: first_range.start,
            ..first
        };
        self.clear_selection();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(paragraph: usize, offset: usize) -> Cursor {
        Cursor {
            paragraph,
            offset,
            ..Default::default()
        }
    }

    fn texts(s: &Session) -> Vec<String> {
        s.core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect()
    }

    /// (kind, text) of every tracked change.
    fn changes(s: &Session) -> Vec<(String, String)> {
        s.track_changes()["list"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c["kind"].as_str().unwrap_or("").to_owned(),
                    c["text"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn recorded_edits_are_tracked_saved_and_resolved() -> Result<()> {
        let mut s = Session::new()?;
        s.run(
            "text.insert",
            json!({"text": "가나다라 마바사아 자차카타\n둘째 문단"}),
        )?;
        let on = s.run(
            "document.track_changes.record",
            json!({"on": true, "author": "검토자"}),
        )?;
        assert_eq!(on["recording"], true, "{on}");
        assert_eq!(on["tracked"], true, "{on}");

        // Typing is an insert; deleting other text marks it deleted and keeps it.
        s.cursor = at(0, 5);
        s.run("text.insert", json!({"text": "추가 "}))?;
        s.select(at(0, 13), false)?;
        s.select(at(0, 17), true)?;
        s.run("text.delete", json!({}))?;
        assert_eq!(texts(&s)[0], "가나다라 추가 마바사아 자차카타");
        // Deleting text this recording inserted removes it.
        s.cursor = at(0, 8);
        s.run("text.backspace", json!({}))?;
        assert_eq!(texts(&s)[0], "가나다라 추가마바사아 자차카타");
        // A paragraph split is an insert over the break; Backspace at the start of
        // the next paragraph marks an existing break deleted instead of joining.
        s.cursor = at(1, 2);
        s.run("paragraph.split", json!({}))?;
        assert_eq!(
            texts(&s),
            ["가나다라 추가마바사아 자차카타", "둘째", " 문단"]
        );
        s.cursor = at(1, 0);
        s.run("text.backspace", json!({}))?;
        assert_eq!(s.cursor, at(0, 16));
        assert_eq!(
            changes(&s),
            [
                ("insert".to_owned(), "추가".to_owned()),
                ("delete".to_owned(), "자차카타".to_owned()),
                ("delete".to_owned(), "".to_owned()),
                ("insert".to_owned(), "".to_owned()),
            ]
        );
        let list = s.track_changes()["list"].clone();
        assert_eq!(list[0]["author"], "검토자");

        // Edits it cannot record are refused, and change nothing.
        assert!(s.execute(Command::PageBreak).is_err());
        assert_eq!(texts(&s).len(), 3);

        // Saved as HWP (BodyText accepted, ViewText marked) and as HWPX.
        let dir = tempfile::tempdir()?;
        for name in ["recorded.hwp", "recorded.hwpx"] {
            let path = dir.path().join(name);
            s.save(&path)?;
            let mut back = Session::new()?;
            back.open(&path)?;
            assert_eq!(changes(&back), changes(&s), "{name}");
            assert_eq!(texts(&back), texts(&s), "{name}");
        }

        // Accepting every change gives the edited text.
        s.run("document.track_changes.record", json!({"on": false}))?;
        s.run("document.track_changes.accept", json!({"all": true}))?;
        assert_eq!(texts(&s), ["가나다라 추가마바사아 둘째", " 문단"]);
        Ok(())
    }
    #[test]
    fn recorded_edits_in_a_table_cell() -> Result<()> {
        let mut s = Session::new()?;
        let t = s.run("insert.table", json!({"rows": 1, "columns": 2}))?;
        s.run(
            "table.cells.set",
            json!({"table": t["table"], "row": 0, "col": 0, "cells": [["셀 글자", "둘"]]}),
        )?;
        let host = s.core.document().sections[0]
            .paragraphs
            .iter()
            .position(|p| p.controls.iter().any(|c| matches!(c, Control::Table(_))))
            .ok_or_else(|| anyhow!("표가 없습니다."))?;
        let control = s.core.document().sections[0].paragraphs[host]
            .controls
            .iter()
            .position(|c| matches!(c, Control::Table(_)))
            .unwrap_or(0);
        let cell = |offset| Cursor {
            paragraph: host,
            offset,
            cell: Some(Cell {
                control,
                index: 0,
                paragraph: 0,
            }),
            ..Default::default()
        };
        s.run("document.track_changes.record", json!({"on": true}))?;
        s.cursor = cell(0);
        s.run("text.insert", json!({"text": "새 "}))?;
        s.cursor = cell(4);
        s.run("text.delete", json!({}))?;
        let list = s.track_changes()["list"].clone();
        let brief: Vec<(String, String, Value)> = list
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c["kind"].as_str().unwrap_or("").to_owned(),
                    c["text"].as_str().unwrap_or("").to_owned(),
                    c["cell"].clone(),
                )
            })
            .collect();
        let at = json!({"control": control, "index": 0, "paragraph": 0});
        assert_eq!(
            brief,
            [
                ("insert".to_owned(), "새 ".to_owned(), at.clone()),
                ("delete".to_owned(), "글".to_owned(), at),
            ]
        );
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("cell.hwp");
        s.save(&path)?;
        let mut back = Session::new()?;
        back.open(&path)?;
        assert_eq!(back.track_changes()["list"], list);
        back.run("document.track_changes.accept", json!({"all": true}))?;
        let text = |s: &Session| -> Option<String> {
            match s.core.document().sections[0].paragraphs[host]
                .controls
                .get(control)?
            {
                Control::Table(t) => Some(t.cells[0].paragraphs[0].text.clone()),
                _ => None,
            }
        };
        assert_eq!(text(&back).as_deref(), Some("새 셀 자"));
        Ok(())
    }
}
