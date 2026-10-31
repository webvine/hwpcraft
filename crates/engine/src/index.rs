//! Owner: format area. 색인 (C47): index marks and the index (찾아보기) made from them.
//!
//! An index mark is Hangul's own point control after the marked word (`IndexMark`,
//! HWPX `<hp:indexmark><hp:firstKey>…</hp:firstKey><hp:secondKey>…</hp:secondKey>`,
//! HWP `idxm` with no CTRL_DATA record), keys as in Hancom's public IndexMark
//! parameter set (First, Second). RHWP reads and writes it but has no API to make
//! one, so a mark is made where a bookmark is (same 0x16 control slot) and then given
//! the IndexMark identity; deleting turns it back into a bookmark for RHWP's tested
//! removal.
//!
//! The index follows what Hangul 2024's 색인 만들기 (MakeIndex, default options)
//! writes, observed in memory: a heading per first sound, `(ᄇ)`, then one line per
//! first key, `사과<TAB>1, 3`; a first key used with second keys gets a line of its
//! own and `   과일<TAB>2` under it; an empty paragraph between groups; all in
//! 바탕글 with the default tab. Hangul writes it into a new document; HwpCraft writes
//! it at the caret like a table of contents.
use super::*;
use hwpcraft_doc::model::control::{Bookmark, IndexMark};
use serde::Deserialize;
use std::collections::BTreeMap;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("index.mark.insert", "색인 표시 넣기", ""),
    ("index.mark.list", "색인 표시 목록", ""),
    ("index.mark.update", "색인 표시 고치기", ""),
    ("index.mark.delete", "색인 표시 지우기", ""),
    ("tools.index", "색인 만들기 (찾아보기)", ""),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "index.mark.insert",
        "first(첫째 키, 1–200자; 본문 한 문단 안의 선택이 있으면 생략할 때 선택한 글), second(둘째 키, 선택, 200자까지). 선택 끝 또는 캐럿(본문)에 한글과 같은 색인 표시를 넣습니다(글자는 그대로, 한 번 실행 취소). 결과: section, paragraph, control.",
        r#"{"first":"사과","second":""}"#,
    ),
    (
        "index.mark.list",
        "매개 변수 없음. marks[]: section, paragraph, control, first, second, page(지금 조판의 쪽).",
        r#"{}"#,
    ),
    (
        "index.mark.update",
        "section, paragraph, control(index.mark.list의 주소)과 바꿀 first·second.",
        r#"{"section":0,"paragraph":0,"control":1,"first":"사과","second":"과일"}"#,
    ),
    (
        "index.mark.delete",
        "section, paragraph, control: 색인 표시를 지웁니다(글자는 그대로).",
        r#"{"section":0,"paragraph":0,"control":1}"#,
    ),
    (
        "tools.index",
        "매개 변수 없음. 문서의 색인 표시로 캐럿 위치(본문)에 색인을 넣습니다: 첫소리 묶음 머리 (ᄀ)…, 항목 '첫째 키<탭>쪽, 쪽', 둘째 키는 그 아래 '   둘째 키<탭>쪽'(한글 2024 색인 만들기 기본 모양, 바탕글). 한글은 새 문서에 만들고 HwpCraft는 캐럿 위치에 넣습니다(다시 부르면 새로 하나 더 넣음). 결과: entries, groups, section, first, paragraphs.",
        r#"{}"#,
    ),
];

const MAX_KEY: usize = 200;
/// In front of a second key, as Hangul writes it.
const INDENT: &str = "   ";
const PAGE_PASSES: usize = 3;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Insert {
    first: Option<String>,
    #[serde(default)]
    second: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct At {
    section: usize,
    paragraph: usize,
    control: usize,
    first: Option<String>,
    second: Option<String>,
}

/// One index mark: where it is and its keys.
struct Mark {
    section: usize,
    paragraph: usize,
    control: usize,
    offset: usize,
    first: String,
    second: String,
}

fn key(value: &str, what: &str) -> Result<String> {
    let value = value.trim();
    if value.chars().count() > MAX_KEY || value.chars().any(char::is_control) {
        bail!("{what}는 제어 문자 없이 {MAX_KEY}자까지 씁니다.");
    }
    Ok(value.to_owned())
}

/// The group a key files under: a Hangul syllable's first sound as a leading jamo
/// (`사과` → ᄉ, as Hangul's index heads show), otherwise the first letter in capitals.
fn group_of(key: &str) -> String {
    let Some(c) = key.chars().next() else {
        return String::new();
    };
    match u32::from(c) {
        code @ 0xAC00..=0xD7A3 => {
            char::from_u32(0x1100 + (code - 0xAC00) / 588).map_or_else(String::new, String::from)
        }
        _ => c.to_uppercase().collect(),
    }
}

fn pages_text(pages: &[usize]) -> String {
    pages
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A first key's pages without a second key, and its second keys' pages.
type Entry = (Vec<usize>, BTreeMap<String, Vec<usize>>);

/// The index lines for `marks` with their pages: group heads, entries, sub-entries
/// and the empty paragraphs between groups, plus the number of entries and groups.
fn index_lines(marks: &[(String, String, usize)]) -> (Vec<String>, usize, usize) {
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for (first, second, page) in marks {
        let entry = entries.entry(first.clone()).or_default();
        let pages = if second.is_empty() {
            &mut entry.0
        } else {
            entry.1.entry(second.clone()).or_default()
        };
        if !pages.contains(page) {
            pages.push(*page);
            pages.sort_unstable();
        }
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (first, (pages, seconds)) in &entries {
        let lines = groups.entry(group_of(first)).or_default();
        lines.push(if pages.is_empty() {
            first.clone()
        } else {
            format!("{first}\t{}", pages_text(pages))
        });
        for (second, pages) in seconds {
            lines.push(format!("{INDENT}{second}\t{}", pages_text(pages)));
        }
    }
    let mut out = Vec::new();
    for (i, (head, lines)) in groups.iter().enumerate() {
        if i > 0 {
            out.push(String::new());
        }
        out.push(format!("({head})"));
        out.extend(lines.iter().cloned());
    }
    (out, entries.len(), groups.len())
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_index_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        let empty = || params.as_object().is_none_or(|p| p.is_empty());
        Some(match id {
            "index.mark.insert" => serde_json::from_value::<Insert>(params.clone())
                .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
                .and_then(|p| self.insert_index_mark(p)),
            "index.mark.list" if empty() => Ok(json!({ "marks": self.index_mark_list() })),
            "index.mark.update" | "index.mark.delete" => {
                serde_json::from_value::<At>(params.clone())
                    .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
                    .and_then(|p| {
                        if id == "index.mark.update" {
                            self.update_index_mark(p)
                        } else if p.first.is_some() || p.second.is_some() {
                            Err(anyhow!("{id}에는 first·second를 주지 않습니다."))
                        } else {
                            self.delete_index_mark(&p)
                        }
                    })
            }
            "tools.index" if empty() => self.make_index(),
            _ => Err(anyhow!("{id}에는 매개 변수가 없습니다.")),
        })
    }

    fn index_marks(&self) -> Vec<Mark> {
        let mut out = Vec::new();
        for (s, section) in self.core.document().sections.iter().enumerate() {
            for (p, paragraph) in section.paragraphs.iter().enumerate() {
                let positions = paragraph.control_text_positions();
                for (c, control) in paragraph.controls.iter().enumerate() {
                    if let Control::IndexMark(mark) = control {
                        out.push(Mark {
                            section: s,
                            paragraph: p,
                            control: c,
                            offset: positions.get(c).copied().unwrap_or(0),
                            first: mark.first_key.clone(),
                            second: mark.second_key.clone(),
                        });
                    }
                }
            }
        }
        out
    }

    fn mark_page(&self, mark: &Mark) -> Option<usize> {
        self.page_of(Cursor {
            section: mark.section,
            paragraph: mark.paragraph,
            offset: mark.offset,
            cell: None,
        })
        .map(|p| p + 1)
    }

    fn index_mark_list(&self) -> Vec<Value> {
        self.index_marks()
            .iter()
            .map(|m| {
                json!({"section": m.section, "paragraph": m.paragraph, "control": m.control,
                    "first": m.first, "second": m.second, "page": self.mark_page(m)})
            })
            .collect()
    }

    fn insert_index_mark(&mut self, p: Insert) -> Result<Value> {
        if self.cursor.cell.is_some() || self.span_anchor.is_some() {
            bail!("색인 표시는 본문 한 문단 안에 넣습니다. 표 밖에 캐럿을 두세요.");
        }
        let selected = self.has_selection().then(|| self.selected_text());
        let first = match p.first {
            Some(first) => key(&first, "첫째 키")?,
            None => key(selected.as_deref().unwrap_or_default(), "첫째 키")?,
        };
        if first.is_empty() {
            bail!("첫째 키(first)를 쓰거나 색인에 올릴 낱말을 선택하세요.");
        }
        let second = key(&p.second, "둘째 키")?;
        let at = Cursor {
            offset: self.selection().end,
            ..self.cursor
        };
        let marks = self.bookmarks()?;
        let mut n = 0;
        let temp = loop {
            let name = format!("__hwpcraft_index_mark_{n}");
            if !marks
                .as_array()
                .is_some_and(|a| a.iter().any(|b| b["name"] == name.as_str()))
            {
                break name;
            }
            n += 1;
        };
        let mut control = 0;
        self.transaction(|s| {
            s.clear_selection();
            let value: Value = serde_json::from_str(
                &s.core
                    .add_bookmark_native(at.section, at.paragraph, at.offset, &temp)
                    .map_err(err)?,
            )?;
            if value["ok"] != true {
                bail!(
                    "{}",
                    value["error"]
                        .as_str()
                        .unwrap_or("색인 표시를 넣지 못했습니다.")
                );
            }
            let paragraph =
                &mut s.core.document_mut().sections[at.section].paragraphs[at.paragraph];
            control = paragraph
                .controls
                .iter()
                .position(|c| matches!(c, Control::Bookmark(b) if b.name == temp))
                .ok_or_else(|| anyhow!("색인 표시를 넣지 못했습니다."))?;
            paragraph.controls[control] = Control::IndexMark(IndexMark {
                first_key: first.clone(),
                second_key: second.clone(),
            });
            // An HWP index mark has no CTRL_DATA record (the bookmark's name list).
            if let Some(slot) = paragraph.ctrl_data_records.get_mut(control) {
                *slot = None;
            }
            s.core.document_mut().sections[at.section].raw_stream = None;
            s.cursor = at;
            Ok(())
        })?;
        Ok(
            json!({"section": at.section, "paragraph": at.paragraph, "control": control,
            "first": first, "second": second}),
        )
    }

    /// The mark at `p`'s address, or why there is none.
    fn index_mark_at(&self, p: &At) -> Result<()> {
        let found = self
            .core
            .document()
            .sections
            .get(p.section)
            .and_then(|s| s.paragraphs.get(p.paragraph))
            .and_then(|para| para.controls.get(p.control));
        if matches!(found, Some(Control::IndexMark(_))) {
            Ok(())
        } else {
            bail!("그 주소에 색인 표시가 없습니다. index.mark.list로 주소를 확인하세요.")
        }
    }

    fn update_index_mark(&mut self, p: At) -> Result<Value> {
        self.index_mark_at(&p)?;
        let first = p.first.as_deref().map(|f| key(f, "첫째 키")).transpose()?;
        if first.as_deref() == Some("") {
            bail!("첫째 키는 비울 수 없습니다.");
        }
        let second = p.second.as_deref().map(|s| key(s, "둘째 키")).transpose()?;
        self.transaction(|s| {
            let document = s.core.document_mut();
            if let Control::IndexMark(mark) =
                &mut document.sections[p.section].paragraphs[p.paragraph].controls[p.control]
            {
                if let Some(first) = first {
                    mark.first_key = first;
                }
                if let Some(second) = second {
                    mark.second_key = second;
                }
            }
            document.sections[p.section].raw_stream = None;
            Ok(())
        })?;
        Ok(json!({ "marks": self.index_mark_list() }))
    }

    fn delete_index_mark(&mut self, p: &At) -> Result<Value> {
        self.index_mark_at(p)?;
        self.transaction(|s| {
            // RHWP removes a bookmark with its 8-unit slot; the mark has the same slot.
            s.core.document_mut().sections[p.section].paragraphs[p.paragraph].controls[p.control] =
                Control::Bookmark(Bookmark {
                    name: "__hwpcraft_index_mark".into(),
                });
            let value: Value = serde_json::from_str(
                &s.core
                    .delete_bookmark_native(p.section, p.paragraph, p.control)
                    .map_err(err)?,
            )?;
            if value["ok"] != true {
                bail!("색인 표시를 지우지 못했습니다.");
            }
            Ok(())
        })?;
        Ok(json!({ "marks": self.index_mark_list() }))
    }

    /// Each mark's keys and its page in the current layout.
    fn index_entries(&self) -> Vec<(String, String, usize)> {
        self.index_marks()
            .iter()
            .filter_map(|m| Some((m.first.clone(), m.second.clone(), self.mark_page(m)?)))
            .collect()
    }

    fn make_index(&mut self) -> Result<Value> {
        if self.cursor.cell.is_some() {
            bail!("색인은 본문에 넣습니다. 표 밖에 캐럿을 두세요.");
        }
        let (lines, entries, groups) = index_lines(&self.index_entries());
        if lines.is_empty() {
            bail!("색인 표시가 없습니다. 먼저 index.mark.insert로 낱말에 색인 표시를 넣으세요.");
        }
        let at = self.cursor;
        let first = at.paragraph + usize::from(at.offset > 0);
        let written: Vec<(String, usize, bool)> =
            lines.iter().map(|l| (l.clone(), 0, false)).collect();
        self.transaction(|s| {
            s.clear_selection();
            s.write_lines(at, first, &written, Default::default())?;
            // Pages move when the index pushes marked text down; rewrite them in place.
            for _ in 0..PAGE_PASSES {
                let (now, _, _) = index_lines(&s.index_entries());
                if now.len() != lines.len() {
                    bail!("색인을 넣는 동안 색인 표시가 바뀌었습니다.");
                }
                let paragraphs = &s.core.document().sections[at.section].paragraphs;
                let stale: Vec<(usize, String)> = now
                    .iter()
                    .enumerate()
                    .filter(|(i, line)| paragraphs[first + i].text != **line)
                    .map(|(i, line)| (first + i, line.clone()))
                    .collect();
                if stale.is_empty() {
                    break;
                }
                s.deferred_layout(|s| {
                    for (paragraph, line) in &stale {
                        let len = s.core.document().sections[at.section].paragraphs[*paragraph]
                            .text
                            .chars()
                            .count();
                        s.core
                            .delete_range_native(at.section, *paragraph, 0, *paragraph, len, None)
                            .map_err(err)?;
                        s.core
                            .insert_text_native(at.section, *paragraph, 0, line)
                            .map_err(err)?;
                    }
                    Ok(())
                })?;
                s.refresh_layout()?;
            }
            s.cursor = Cursor {
                section: at.section,
                paragraph: first + lines.len(),
                offset: 0,
                cell: None,
            };
            Ok(())
        })?;
        Ok(
            json!({"entries": entries, "groups": groups, "section": at.section,
            "first": first, "paragraphs": lines.len()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{group_of, index_lines};

    #[test]
    fn lines_follow_hangul_index_layout() {
        let marks = [
            ("사과".to_owned(), String::new(), 1),
            ("배".to_owned(), "과일".to_owned(), 2),
            ("사과".to_owned(), String::new(), 3),
            ("사과".to_owned(), String::new(), 1),
        ];
        let (lines, entries, groups) = index_lines(&marks);
        assert_eq!(
            lines,
            ["(ᄇ)", "배", "   과일\t2", "", "(ᄉ)", "사과\t1, 3"]
        );
        assert_eq!((entries, groups), (2, 2));
        assert_eq!(group_of("Apple"), "A");
        assert_eq!(group_of("까치"), "ᄁ");
    }
}
