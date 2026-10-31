//! Owner: format area. 문서 비교의 나란히 보기와 병합 (C49).
//!
//! `review.merge.plan` lines the current document's body paragraphs up with another
//! file's, section by section, and lists each difference with both texts side by
//! side; `review.merge` takes chosen differences over from the file in one undo
//! step. The alignment works like RHWP's comparison (common start and end, then the
//! longest common subsequence, with touching delete/add runs paired as changes), with
//! a paragraph's key made of its text, the kinds of its objects and its table cells'
//! text, so a paragraph whose table changed is a change too. Taking a paragraph over
//! imports it with RHWP's paragraph-block import (formatting, tables, pictures and
//! their resources come along), the way body snippets go in.
use super::*;
use crate::body_autotext_fragment::{SourceRange, import, move_target_prologue, normalize_prefix};
use hwpcraft_doc::model::document::Document;
use serde::Deserialize;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "review.merge.plan",
        "문서 비교 나란히 보기 (문단마다 현재 글과 비교 파일 글)",
        "",
    ),
    (
        "review.merge",
        "문서 비교 병합 (비교 파일의 변경 가져오기)",
        "",
    ),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "review.merge.plan",
        "path(필수): 비교할 HWP/HWPX/HML 파일. ignore_whitespace(선택), password(선택). 두 문서의 같은 구역 본문 문단을 맞대어 changes[]를 돌려줍니다: id, kind(changed 바뀜 | added 파일에만 있음 | removed 현재 문서에만 있음), section, paragraph(현재 문서 문단, added는 들어갈 자리), other_paragraph(파일 문단, removed는 null), current_text, other_text. 표 셀 글·개체 종류 차이도 바뀜으로 셉니다. 글상자·각주·머리말 안은 맞대지 않습니다.",
        r#"{"path":"original.hwpx"}"#,
    ),
    (
        "review.merge",
        "path(필수), ignore_whitespace?, password?, ids?(review.merge.plan의 id 목록, 생략하면 전부). 고른 차이를 비교 파일 쪽으로 맞춥니다: changed는 파일 문단으로 바꾸고, added는 파일 문단을 넣고, removed는 현재 문단을 지웁니다. 가져온 문단은 서식·표·그림을 함께 가져옵니다. 한 번 실행 취소. 결과: applied, changes(남은 차이 수).",
        r#"{"path":"original.hwpx","ids":[0,2]}"#,
    ),
];

/// Paragraph pairs the alignment may compare at once; beyond it, position by position.
const LCS_BUDGET: usize = 1_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    path: PathBuf,
    #[serde(default)]
    ignore_whitespace: bool,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Merge {
    path: PathBuf,
    #[serde(default)]
    ignore_whitespace: bool,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    ids: Option<Vec<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Changed,
    Added,
    Removed,
}

/// One difference: where it is in the current document (`at`: the paragraph, or for
/// an added one the paragraph it goes before) and in the file.
#[derive(Clone, Copy, Debug)]
struct Change {
    kind: Kind,
    section: usize,
    at: usize,
    other: Option<usize>,
}

fn normalize(text: &str, ignore_whitespace: bool) -> String {
    if ignore_whitespace {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        text.to_owned()
    }
}

/// A paragraph's comparison key: its text, its objects' kinds and its tables' cell text.
fn key(p: &Paragraph, ignore_whitespace: bool) -> String {
    let mut key = normalize(&p.text, ignore_whitespace);
    for control in &p.controls {
        key.push('\u{1}');
        match control {
            Control::Table(table) => {
                key.push_str(&format!("table{}x{}", table.row_count, table.col_count));
                for cell in &table.cells {
                    key.push('\u{2}');
                    for cp in &cell.paragraphs {
                        key.push_str(&key_of_text(cp, ignore_whitespace));
                        key.push('\u{3}');
                    }
                }
            }
            Control::SectionDef(_) | Control::ColumnDef(_) => {}
            other => key.push_str(control_kind(other)),
        }
    }
    key
}

fn key_of_text(p: &Paragraph, ignore_whitespace: bool) -> String {
    normalize(&p.text, ignore_whitespace)
}

fn control_kind(control: &Control) -> &'static str {
    match control {
        Control::Picture(_) => "picture",
        Control::Shape(_) => "shape",
        Control::Equation(_) => "equation",
        Control::Footnote(_) => "footnote",
        Control::Endnote(_) => "endnote",
        Control::Field(_) => "field",
        Control::Bookmark(_) => "bookmark",
        _ => "control",
    }
}

/// Align two key lists: Some((i, j)) pairs, (Some(i), None) only in A, (None, Some(j))
/// only in B, in document order. Touching only-A/only-B runs are paired up.
fn align(a: &[String], b: &[String]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let mut end = 0;
    while end < a.len() - start
        && end < b.len() - start
        && a[a.len() - 1 - end] == b[b.len() - 1 - end]
    {
        end += 1;
    }
    let (ma, mb) = (&a[start..a.len() - end], &b[start..b.len() - end]);
    let mut middle: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    if ma.len().saturating_mul(mb.len()) <= LCS_BUDGET {
        let (n, m) = (ma.len(), mb.len());
        let mut table = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                table[i * (m + 1) + j] = if ma[i] == mb[j] {
                    table[(i + 1) * (m + 1) + j + 1] + 1
                } else {
                    table[(i + 1) * (m + 1) + j].max(table[i * (m + 1) + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && ma[i] == mb[j] {
                middle.push((Some(start + i), Some(start + j)));
                i += 1;
                j += 1;
            } else if j >= m
                || (i < n && table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + j + 1])
            {
                middle.push((Some(start + i), None));
                i += 1;
            } else {
                middle.push((None, Some(start + j)));
                j += 1;
            }
        }
    } else {
        for k in 0..ma.len().max(mb.len()) {
            middle.push((
                (k < ma.len()).then_some(start + k),
                (k < mb.len()).then_some(start + k),
            ));
        }
    }
    // Pair touching runs of only-A and only-B entries as changed paragraphs.
    let mut paired = Vec::new();
    let mut k = 0;
    while k < middle.len() {
        if middle[k].0.is_some() && middle[k].1.is_some() {
            paired.push(middle[k]);
            k += 1;
            continue;
        }
        let run_end = (k..middle.len())
            .find(|&e| middle[e].0.is_some() && middle[e].1.is_some())
            .unwrap_or(middle.len());
        let only_a: Vec<usize> = middle[k..run_end]
            .iter()
            .filter_map(|&(a, b)| if b.is_none() { a } else { None })
            .collect();
        let only_b: Vec<usize> = middle[k..run_end]
            .iter()
            .filter_map(|&(a, b)| if a.is_none() { b } else { None })
            .collect();
        for x in 0..only_a.len().max(only_b.len()) {
            paired.push((only_a.get(x).copied(), only_b.get(x).copied()));
        }
        k = run_end;
    }
    let mut out: Vec<(Option<usize>, Option<usize>)> =
        (0..start).map(|k| (Some(k), Some(k))).collect();
    out.extend(paired);
    out.extend((0..end).map(|k| (Some(a.len() - end + k), Some(b.len() - end + k))));
    out
}

fn changes(current: &Document, other: &Document, ignore_whitespace: bool) -> Vec<Change> {
    let mut out = Vec::new();
    for (section, (sa, sb)) in current.sections.iter().zip(&other.sections).enumerate() {
        let ka: Vec<String> = sa
            .paragraphs
            .iter()
            .map(|p| key(p, ignore_whitespace))
            .collect();
        let kb: Vec<String> = sb
            .paragraphs
            .iter()
            .map(|p| key(p, ignore_whitespace))
            .collect();
        let ops = align(&ka, &kb);
        for (n, op) in ops.iter().enumerate() {
            let change = match *op {
                (Some(i), Some(j)) if ka[i] != kb[j] => Change {
                    kind: Kind::Changed,
                    section,
                    at: i,
                    other: Some(j),
                },
                (Some(i), None) => Change {
                    kind: Kind::Removed,
                    section,
                    at: i,
                    other: None,
                },
                (None, Some(j)) => {
                    // It goes before the next paragraph of the current document.
                    let at = ops[n + 1..]
                        .iter()
                        .find_map(|(a, _)| *a)
                        .unwrap_or(sa.paragraphs.len());
                    Change {
                        kind: Kind::Added,
                        section,
                        at,
                        other: Some(j),
                    }
                }
                _ => continue,
            };
            out.push(change);
        }
    }
    out
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_doc_merge_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        match id {
            "review.merge.plan" => Some(
                serde_json::from_value::<Plan>(params.clone())
                    .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
                    .and_then(|p| self.merge_plan(p)),
            ),
            "review.merge" => Some(
                serde_json::from_value::<Merge>(params.clone())
                    .map_err(|e| anyhow!("{id}: 매개 변수 오류 ({e})"))
                    .and_then(|p| self.merge_from_file(p)),
            ),
            _ => None,
        }
    }

    fn open_other(&self, path: &Path, password: Option<&str>) -> Result<DocumentCore> {
        hwpcraft_formats::file_access::check_read(&path.to_string_lossy())
            .map_err(anyhow::Error::msg)?;
        if !path.is_file() {
            bail!("비교할 파일이 없습니다: {}", path.display());
        }
        Ok(match password {
            Some(pw) => hwpcraft_formats::open_with_password(path, pw)?,
            None => hwpcraft_formats::open(path)?,
        })
    }

    fn merge_plan(&mut self, p: Plan) -> Result<Value> {
        let other = self.open_other(&p.path, p.password.as_deref())?;
        let list = changes(self.core.document(), other.document(), p.ignore_whitespace);
        let text = |doc: &Document, section: usize, paragraph: Option<usize>| {
            paragraph
                .and_then(|i| doc.sections.get(section)?.paragraphs.get(i))
                .map(|para| para.text.clone())
        };
        let items: Vec<Value> = list
            .iter()
            .enumerate()
            .map(|(id, c)| {
                let current = (c.kind != Kind::Added).then_some(c.at);
                json!({
                    "id": id,
                    "kind": match c.kind { Kind::Changed => "changed", Kind::Added => "added", Kind::Removed => "removed" },
                    "section": c.section,
                    "paragraph": c.at,
                    "other_paragraph": c.other,
                    "current_text": text(self.core.document(), c.section, current),
                    "other_text": text(other.document(), c.section, c.other),
                })
            })
            .collect();
        let sections = (
            self.core.document().sections.len(),
            other.document().sections.len(),
        );
        Ok(json!({
            "path": p.path,
            "changes": items,
            "sections": {"current": sections.0, "other": sections.1},
            "note": (sections.0 != sections.1).then_some("구역 수가 달라 같은 번호의 구역끼리만 맞댔습니다."),
        }))
    }

    fn merge_from_file(&mut self, p: Merge) -> Result<Value> {
        self.guard_protected()?;
        let other = self.open_other(&p.path, p.password.as_deref())?;
        let list = changes(self.core.document(), other.document(), p.ignore_whitespace);
        let chosen: Vec<Change> = match &p.ids {
            None => list.clone(),
            Some(ids) => ids
                .iter()
                .map(|id| {
                    list.get(*id)
                        .copied()
                        .ok_or_else(|| anyhow!("없는 변경 id: {id}"))
                })
                .collect::<Result<_>>()?,
        };
        if chosen.is_empty() {
            return Ok(json!({"applied": 0, "changes": list.len()}));
        }
        // From the end of each section back, so earlier positions stay valid. An added
        // paragraph goes before `at`, so at equal positions it is applied last.
        let mut order = chosen.clone();
        order.sort_by(|a, b| {
            (b.section, b.at, b.kind != Kind::Added).cmp(&(a.section, a.at, a.kind != Kind::Added))
        });
        // A section's first paragraph carries its definitions, which a paragraph block
        // cannot take along: import from a copy without them, as body snippets do.
        let mut source = other.document().clone();
        for section in &mut source.sections {
            if let Some(first) = section.paragraphs.first_mut() {
                normalize_prefix(first, true)?;
            }
            section.raw_stream = None;
        }
        self.transaction(|s| {
            s.clear_selection();
            for c in &order {
                s.apply_change(&source, c)?;
            }
            s.cursor = Cursor::default();
            Ok(())
        })?;
        let left = changes(self.core.document(), other.document(), p.ignore_whitespace).len();
        Ok(json!({"applied": chosen.len(), "changes": left}))
    }

    fn apply_change(&mut self, other: &Document, c: &Change) -> Result<()> {
        let take = |s: &mut Session, before: usize| -> Result<()> {
            let j = c.other.ok_or_else(|| anyhow!("가져올 문단이 없습니다."))?;
            import(
                &mut s.core,
                other,
                SourceRange {
                    section: c.section,
                    start: j,
                    end: j + 1,
                },
                c.section,
                before,
            )?;
            if before == 0 {
                move_target_prologue(&mut s.core, c.section, 1)?;
            }
            Ok(())
        };
        match c.kind {
            Kind::Added => take(self, c.at),
            Kind::Changed => {
                take(self, c.at)?;
                self.delete_paragraph(c.section, c.at + 1)
            }
            Kind::Removed => self.delete_paragraph(c.section, c.at),
        }
    }

    /// Remove a body paragraph with everything in it, keeping its neighbours.
    fn delete_paragraph(&mut self, section: usize, paragraph: usize) -> Result<()> {
        let paragraphs = &self.core.document().sections[section].paragraphs;
        if paragraphs.len() < 2 {
            bail!("구역의 마지막 남은 문단은 지울 수 없습니다.");
        }
        if paragraph > 0 {
            self.core
                .delete_paragraph_native(section, paragraph)
                .map_err(err)?;
            return Ok(());
        }
        // The first paragraph carries the section's definitions: pull the second one's
        // content up into it instead, which only works while it holds no objects.
        if paragraphs[0]
            .controls
            .iter()
            .any(|c| !matches!(c, Control::SectionDef(_) | Control::ColumnDef(_)))
        {
            bail!("개체가 있는 구역 첫 문단은 병합으로 지울 수 없습니다.");
        }
        self.core
            .delete_range_native(section, 0, 0, 1, 0, None)
            .map_err(err)?;
        Ok(())
    }
}
