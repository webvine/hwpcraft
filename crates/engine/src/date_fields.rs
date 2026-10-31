//! Owner: format area. Document creation and last-save times (문서 요약 정보) and the
//! date fields that show them (C33).
//!
//! The times live where Hangul keeps them: HWPX `Contents/content.hpf` metadata
//! (`CreatedDate`, `ModifiedDate`, UTC as `2011-01-31T01:02:54Z`, plus `date` in the
//! local long form) and the HWP `\u{5}HwpSummaryInformation` property set
//! (PIDSI_CREATE_DTM 12, PIDSI_LASTSAVE_DTM 13 as FILETIME). RHWP keeps both as
//! they were read and never updates them, so `Session::save` stamps them through
//! [`Session::with_save_times`]: the creation time when the document has none, and
//! the save time. Opening alone changes nothing; a failed save puts the old values
//! back. `SOURCE_DATE_EPOCH` fixes the clock for reproducible output.
//! A test that compares two saves made at different moments (bytes or document
//! fingerprints) must fix the clock with [`fixed_clock`], which is process-wide.
//!
//! The fields are Hangul 2024's 날짜 코드 and 마지막 저장 날짜 as observed: a 문서 요약
//! field (`%smr`) with command `$createtime` / `$modifiedtime`, property 8, whose text
//! is the time in the local long form ("2026년 10월 30일 금요일 오후 11:27:37").
use super::*;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};
use hwpcraft_doc::model::control::{Field, FieldType, Parameter, ParameterList};
use hwpcraft_doc::model::document::Document;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "insert.date_field",
        "날짜 필드 넣기 (문서 만든 날짜·마지막 저장 날짜)",
        "",
    ),
    ("tools.date_field.update", "날짜 필드 새로 고침", ""),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "insert.date_field",
        "kind: created(기본, 한글의 날짜 코드 $createtime) | modified(마지막 저장 날짜 $modifiedtime). 본문 캐럿 위치에 문서 요약 필드로 넣고 보이는 글은 문서 요약 정보의 시각을 지역 시간 \"2026년 10월 30일 금요일 오후 11:27:37\" 형식으로 씁니다. 저장한 적 없는 새 문서의 만든 시각은 지금이고 첫 저장 때 기록됩니다. 마지막 저장 시각이 없으면 modified는 거절합니다. 넣을 문단에는 표·그림·책갈피 같은 개체가 없어야 합니다. 결과: field_id, kind, text.",
        r#"{"kind":"created"}"#,
    ),
    (
        "tools.date_field.update",
        "매개 변수 없음. 본문의 날짜 필드($createtime·$modifiedtime) 글을 지금 문서 요약 정보의 시각으로 다시 씁니다(한 번 실행 취소). 저장해도 필드 글은 바뀌지 않고 이 명령으로 바뀝니다. 결과: fields, updated.",
        r#"{}"#,
    ),
];

const SUMMARY_STREAM: &str = "\u{5}HwpSummaryInformation";
const CONTENT_HPF: &str = "Contents/content.hpf";
const PID_CREATED: u32 = 12;
const PID_SAVED: u32 = 13;
const VT_FILETIME: u32 = 0x40;
/// 100 ns ticks between 1601-01-01 (FILETIME) and 1970-01-01.
const FILETIME_UNIX: i64 = 116_444_736_000_000_000;
const CREATED: &str = "$createtime";
const MODIFIED: &str = "$modifiedtime";
/// Hangul's metadata block order for a new HWPX (as in Hangul-saved samples).
const NEW_METADATA: &str = r#"<opf:metadata><opf:title/><opf:language>ko</opf:language><opf:meta name="creator" content="text"/><opf:meta name="subject" content="text"/><opf:meta name="description" content="text"/><opf:meta name="lastsaveby" content="text"/><opf:meta name="CreatedDate" content="text"/><opf:meta name="ModifiedDate" content="text"/><opf:meta name="date" content="text"/><opf:meta name="keyword" content="text"/></opf:metadata>"#;

#[derive(Deserialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    #[default]
    Created,
    Modified,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Insert {
    #[serde(default)]
    kind: Kind,
}

/// A fixed clock (seconds since 1970) for tests, which cannot set the environment.
static FIXED_CLOCK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(i64::MIN);

/// Fix the clock the save-time stamps and date fields use (`None` releases it).
#[doc(hidden)]
pub fn fixed_clock(secs: Option<i64>) {
    FIXED_CLOCK.store(
        secs.unwrap_or(i64::MIN),
        std::sync::atomic::Ordering::SeqCst,
    );
}

/// The clock: a fixed test clock, `SOURCE_DATE_EPOCH` (seconds) when set, otherwise now.
fn now() -> DateTime<Utc> {
    let fixed = FIXED_CLOCK.load(std::sync::atomic::Ordering::SeqCst);
    if fixed != i64::MIN
        && let Some(time) = Utc.timestamp_opt(fixed, 0).single()
    {
        return time;
    }
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .and_then(|secs| Utc.timestamp_opt(secs, 0).single())
        .unwrap_or_else(Utc::now)
}

/// Hangul's long local form: "2026년 10월 30일 금요일 오후 11:27:37".
pub(crate) fn long_form(time: DateTime<Utc>) -> String {
    let local = time.with_timezone(&Local);
    let weekday =
        ["월", "화", "수", "목", "금", "토", "일"][local.weekday().num_days_from_monday() as usize];
    let (pm, hour) = local.hour12();
    format!(
        "{}년 {}월 {}일 {}요일 {} {}:{:02}:{:02}",
        local.year(),
        local.month(),
        local.day(),
        weekday,
        if pm { "오후" } else { "오전" },
        hour,
        local.minute(),
        local.second()
    )
}

fn iso(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn filetime(time: DateTime<Utc>) -> u64 {
    (time.timestamp() * 10_000_000 + i64::from(time.timestamp_subsec_nanos() / 100) + FILETIME_UNIX)
        .max(0) as u64
}

fn from_filetime(ticks: u64) -> Option<DateTime<Utc>> {
    let since_unix = i64::try_from(ticks).ok()? - FILETIME_UNIX;
    Utc.timestamp_opt(
        since_unix.div_euclid(10_000_000),
        (since_unix.rem_euclid(10_000_000) * 100) as u32,
    )
    .single()
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// Byte offset of the FILETIME value of `pid` in an OLE property set (first section).
fn filetime_slot(data: &[u8], pid: u32) -> Option<usize> {
    if data.get(..2) != Some(&[0xFE, 0xFF]) || u32_at(data, 24)? == 0 {
        return None;
    }
    let section = u32_at(data, 44)? as usize;
    let count = u32_at(data, section + 4)? as usize;
    (0..count.min(4096)).find_map(|i| {
        let entry = section + 8 + i * 8;
        (u32_at(data, entry)? == pid).then_some(())?;
        let at = section + u32_at(data, entry + 4)? as usize;
        (u32_at(data, at)? == VT_FILETIME && data.len() >= at + 12).then_some(at + 4)
    })
}

fn summary(doc: &Document) -> Option<&[u8]> {
    doc.extra_streams
        .iter()
        .find(|(path, _)| path.trim_start_matches('/') == SUMMARY_STREAM)
        .map(|(_, data)| data.as_slice())
}

fn summary_time(data: &[u8], pid: u32) -> Option<DateTime<Utc>> {
    let at = filetime_slot(data, pid)?;
    let ticks = u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?);
    (ticks != 0).then(|| from_filetime(ticks)).flatten()
}

fn hpf(doc: &Document) -> Option<&str> {
    doc.hwpx_aux_entries
        .iter()
        .find(|(path, _)| path == CONTENT_HPF)
        .and_then(|(_, data)| std::str::from_utf8(data).ok())
}

/// The text of `<opf:meta name="…">` in a metadata block, if it has any.
fn meta(xml: &str, name: &str) -> Option<String> {
    let open = format!("<opf:meta name=\"{name}\" content=\"text\">");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find("</opf:meta>")?;
    Some(xml[start..end].to_owned()).filter(|v| !v.is_empty())
}

/// Set `<opf:meta name="…">` (empty or not) to `value`; false when the block has none.
fn set_meta(xml: &mut String, name: &str, value: &str) -> bool {
    let empty = format!("<opf:meta name=\"{name}\" content=\"text\"/>");
    let open = format!("<opf:meta name=\"{name}\" content=\"text\">");
    let full = format!("{open}{value}</opf:meta>");
    if let Some(at) = xml.find(&empty) {
        xml.replace_range(at..at + empty.len(), &full);
        return true;
    }
    if let Some(at) = xml.find(&open)
        && let Some(len) = xml[at..].find("</opf:meta>")
    {
        xml.replace_range(at..at + len + "</opf:meta>".len(), &full);
        return true;
    }
    false
}

/// What `Session::save` changed, to put back if the save fails.
pub(crate) struct SavedTimes {
    summary: Option<(usize, Vec<u8>)>,
    hpf: Option<Option<(usize, Vec<u8>)>>,
}

/// The creation time RHWP's blank document carries; a new document shows it until
/// HwpCraft records a real one.
fn template_created() -> Option<DateTime<Utc>> {
    static TEMPLATE: std::sync::OnceLock<Option<DateTime<Utc>>> = std::sync::OnceLock::new();
    *TEMPLATE.get_or_init(|| {
        let core = blank().ok()?;
        summary(core.document()).and_then(|s| summary_time(s, PID_CREATED))
    })
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_date_fields_ext(
        &mut self,
        id: &str,
        params: &Value,
    ) -> Option<Result<Value>> {
        match id {
            "insert.date_field" => Some(
                serde_json::from_value::<Insert>(params.clone())
                    .map_err(|e| anyhow!("insert.date_field: 매개 변수 오류 ({e})"))
                    .and_then(|p| self.insert_date_field(p.kind)),
            ),
            "tools.date_field.update" => {
                Some(if params.as_object().is_some_and(|p| !p.is_empty()) {
                    Err(anyhow!("{id}에는 매개 변수가 없습니다."))
                } else {
                    self.update_date_fields()
                })
            }
            _ => None,
        }
    }

    /// The document's (creation, last save) times. A new document never saved has
    /// only RHWP's template times, which are not its own: none.
    pub fn document_times(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let doc = self.core.document();
        if let Some(xml) = hpf(doc) {
            let parse = |name| {
                meta(xml, name)
                    .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
                    .map(|t| t.with_timezone(&Utc))
            };
            let (created, saved) = (parse("CreatedDate"), parse("ModifiedDate"));
            if created.is_some() || saved.is_some() {
                return (created, saved);
            }
        }
        let Some(data) = summary(doc) else {
            return (None, None);
        };
        let created = summary_time(data, PID_CREATED);
        if self.path_is_new() && created == template_created() {
            return (None, None);
        }
        (created, summary_time(data, PID_SAVED))
    }

    fn path_is_new(&self) -> bool {
        self.path.is_none()
    }

    /// Run `save` with the document times stamped: creation (when missing) and now as
    /// the last save, in the HWP summary and the HWPX metadata. A failed save restores
    /// them. Only HWP/HWPX saves are stamped.
    pub(crate) fn with_save_times(
        &mut self,
        path: &Path,
        save: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let stamps = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("hwp") || e.eq_ignore_ascii_case("hwpx"));
        if !stamps {
            return save(self);
        }
        let saved = self.stamp_save_times(now());
        let result = save(self);
        if result.is_err() {
            self.restore_save_times(saved);
        }
        result
    }

    fn stamp_save_times(&mut self, at: DateTime<Utc>) -> SavedTimes {
        let created = self.document_times().0.unwrap_or(at);
        let doc = self.core.document_mut();
        let mut saved = SavedTimes {
            summary: None,
            hpf: None,
        };
        if let Some(i) = doc
            .extra_streams
            .iter()
            .position(|(path, _)| path.trim_start_matches('/') == SUMMARY_STREAM)
        {
            let data = &mut doc.extra_streams[i].1;
            let before = data.clone();
            for (pid, time) in [(PID_CREATED, created), (PID_SAVED, at)] {
                if let Some(slot) = filetime_slot(data, pid) {
                    data[slot..slot + 8].copy_from_slice(&filetime(time).to_le_bytes());
                }
            }
            saved.summary = Some((i, before));
        }
        let existing = doc
            .hwpx_aux_entries
            .iter()
            .position(|(path, _)| path == CONTENT_HPF);
        let mut xml = match existing {
            Some(i) => String::from_utf8(doc.hwpx_aux_entries[i].1.clone()).ok(),
            None => Some(format!("<opf:package>{NEW_METADATA}</opf:package>")),
        };
        if let Some(xml) = xml.as_mut() {
            let had_created = meta(xml, "CreatedDate").is_some();
            if !had_created {
                set_meta(xml, "CreatedDate", &iso(created));
            }
            set_meta(xml, "ModifiedDate", &iso(at));
            if existing.is_none() {
                set_meta(xml, "date", &long_form(created));
            }
        }
        if let Some(xml) = xml {
            match existing {
                Some(i) => {
                    let before =
                        std::mem::replace(&mut doc.hwpx_aux_entries[i].1, xml.into_bytes());
                    saved.hpf = Some(Some((i, before)));
                }
                None => {
                    doc.hwpx_aux_entries
                        .push((CONTENT_HPF.to_owned(), xml.into_bytes()));
                    saved.hpf = Some(None);
                }
            }
        }
        saved
    }

    fn restore_save_times(&mut self, saved: SavedTimes) {
        let doc = self.core.document_mut();
        if let Some((i, before)) = saved.summary {
            doc.extra_streams[i].1 = before;
        }
        match saved.hpf {
            Some(Some((i, before))) => doc.hwpx_aux_entries[i].1 = before,
            Some(None) => {
                doc.hwpx_aux_entries.retain(|(path, _)| path != CONTENT_HPF);
            }
            None => {}
        }
    }

    /// The text a date field shows now, or `None` when the document has no such time.
    fn date_field_text(&self, command: &str) -> Option<String> {
        let (created, saved) = self.document_times();
        match command {
            CREATED => Some(long_form(created.unwrap_or_else(now))),
            MODIFIED => saved.map(long_form),
            _ => None,
        }
    }

    fn insert_date_field(&mut self, kind: Kind) -> Result<Value> {
        self.guard_protected()?;
        if let Some(why) = self.cross_reference_blocked() {
            bail!(why.replace("상호 참조", "날짜 필드"));
        }
        let command = if kind == Kind::Created {
            CREATED
        } else {
            MODIFIED
        };
        let text = self.date_field_text(command).ok_or_else(|| {
            anyhow!("저장한 적 없는 문서라 마지막 저장 날짜가 없습니다. 먼저 저장하세요.")
        })?;
        let mut out = Value::Null;
        self.transaction(|s| {
            let id = s.write_text_field(&text, |f: &mut Field| {
                f.field_type = FieldType::Summary;
                f.ctrl_id = super::cross_ref::SUMMARY_CTRL;
                f.command = command.to_owned();
                f.properties = 8;
                f.parameters = ParameterList {
                    name: Some(String::new()),
                    items: vec![Parameter::String {
                        name: Some("Command".into()),
                        value: command.to_owned(),
                        preserve_space: false,
                    }],
                };
            })?;
            out = json!({"field_id": id, "kind": if kind == Kind::Created {"created"} else {"modified"}, "text": text});
            Ok(())
        })?;
        Ok(out)
    }

    /// Body date fields: (section, paragraph, field id, command, shown text).
    fn date_fields(&self) -> Vec<(usize, usize, u32, String, String)> {
        let mut out = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, para) in section.paragraphs.iter().enumerate() {
                for range in &para.field_ranges {
                    let Some(Control::Field(f)) = para.controls.get(range.control_idx) else {
                        continue;
                    };
                    if f.field_type != FieldType::Summary
                        || !matches!(f.command.as_str(), CREATED | MODIFIED)
                    {
                        continue;
                    }
                    let text: String = para
                        .text
                        .chars()
                        .skip(range.start_char_idx)
                        .take(range.end_char_idx.saturating_sub(range.start_char_idx))
                        .collect();
                    out.push((s, p, f.field_id, f.command.clone(), text));
                }
            }
        }
        out
    }

    fn update_date_fields(&mut self) -> Result<Value> {
        self.guard_protected()?;
        let fields = self.date_fields();
        let mut updated = 0;
        self.transaction(|s| {
            for (sec, para, id, command, text) in fields.iter().cloned() {
                let Some(value) = s.date_field_text(&command) else {
                    continue;
                };
                if value == text || text.is_empty() {
                    continue;
                }
                let saved = s.text_fields_as_links(sec, para);
                let result = s.core.replace_hyperlink_text_native(
                    &hwpcraft_doc::HyperlinkTarget::body(sec, para),
                    id,
                    &value,
                );
                s.restore_field_types(sec, para, &saved);
                result.map_err(err)?;
                updated += 1;
            }
            s.cursor.offset = s.cursor.offset.min(s.len());
            Ok(())
        })?;
        Ok(json!({"fields": fields.len(), "updated": updated}))
    }
}
