//! Owner: core area. 편집 도구: 정렬 (edit.sort), 조판 부호 지우기 (edit.delete_ctrl_marks).
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    /// 글자 순서 (가나다/ABC)
    #[default]
    Text,
    /// 숫자: the first number in the paragraph; paragraphs without one sort last.
    Number,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SortOptions {
    pub key: SortKey,
    pub descending: bool,
    pub case_sensitive: bool,
}

/// Control kinds offered by 조판 부호 지우기.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Table,
    Picture,
    Shape,
    Equation,
    Footnote,
    Endnote,
    Bookmark,
    Hyperlink,
    HiddenComment,
    Field,
}
impl MarkKind {
    pub const ALL: [MarkKind; 10] = [
        Self::Table,
        Self::Picture,
        Self::Shape,
        Self::Equation,
        Self::Footnote,
        Self::Endnote,
        Self::Bookmark,
        Self::Hyperlink,
        Self::HiddenComment,
        Self::Field,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "표",
            Self::Picture => "그림",
            Self::Shape => "그리기 개체·글상자",
            Self::Equation => "수식",
            Self::Footnote => "각주",
            Self::Endnote => "미주",
            Self::Bookmark => "책갈피",
            Self::Hyperlink => "하이퍼링크",
            Self::HiddenComment => "숨은 설명",
            Self::Field => "누름틀·필드",
        }
    }
    fn of(control: &Control) -> Option<Self> {
        Some(match control {
            Control::Table(_) => Self::Table,
            Control::Picture(_) => Self::Picture,
            Control::Shape(_) => Self::Shape,
            Control::Equation(_) => Self::Equation,
            Control::Footnote(_) => Self::Footnote,
            Control::Endnote(_) => Self::Endnote,
            Control::Bookmark(_) => Self::Bookmark,
            Control::Hyperlink(_) => Self::Hyperlink,
            Control::HiddenComment(_) => Self::HiddenComment,
            Control::Field(_) => Self::Field,
            _ => return None,
        })
    }
}

fn first_number(text: &str) -> Option<f64> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let negative = text[..start].ends_with('-');
    let digits: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .filter(|c| *c != ',')
        .collect();
    let n: f64 = digits.trim_end_matches('.').parse().ok()?;
    Some(if negative { -n } else { n })
}

impl Session {
    /// Body paragraph range covered by the selection, or the whole section body.
    fn body_paragraphs(&self) -> Result<(usize, Range<usize>)> {
        if self.cursor.cell.is_some() {
            bail!("본문 문단에서 사용하세요. 표 셀 정렬은 표 도구를 사용합니다.");
        }
        let section = self.cursor.section;
        Ok(match self.span() {
            Some((a, b)) => (section, a.paragraph..b.paragraph + 1),
            None => {
                let n = self
                    .core
                    .document()
                    .sections
                    .get(section)
                    .map(|s| s.paragraphs.len())
                    .unwrap_or(0);
                (section, 0..n)
            }
        })
    }
    /// 정렬: reorder the selected body paragraphs (stable), keeping each paragraph's formats.
    pub fn sort_paragraphs(&mut self, o: SortOptions) -> Result<()> {
        let (section, range) = self.body_paragraphs()?;
        if range.len() < 2 {
            bail!("정렬할 문단을 둘 이상 선택하세요.");
        }
        let paragraphs = &self.core.document().sections[section].paragraphs[range.clone()];
        let structural = |c: &Control| matches!(c, Control::SectionDef(_) | Control::ColumnDef(_));
        // The section's first paragraph carries its section/column definitions.
        // Those stay in place; only the text content takes part in the sort.
        let lift = paragraphs[0].controls.iter().any(structural);
        if paragraphs[0].controls.iter().any(|c| !structural(c))
            || paragraphs[1..].iter().any(|p| !p.controls.is_empty())
        {
            bail!("표·개체가 있는 문단은 정렬하지 않습니다. 텍스트 문단만 선택하세요.");
        }
        self.transaction(|s| {
            let mut range = range;
            if lift {
                s.core
                    .split_paragraph_native(section, range.start, 0, None)
                    .map_err(err)?;
                range = range.start + 1..range.end + 1;
            }
            let paragraphs = &s.core.document().sections[section].paragraphs[range.clone()];
            if paragraphs.iter().any(|p| !p.controls.is_empty()) {
                bail!("구역 정의를 분리하지 못해 정렬하지 않았습니다.");
            }
            let key = |i: usize| {
                let t = paragraphs[i].text.as_str();
                if o.case_sensitive {
                    t.to_owned()
                } else {
                    t.to_lowercase()
                }
            };
            let mut order: Vec<usize> = (0..paragraphs.len()).collect();
            match o.key {
                SortKey::Text => order.sort_by_key(|&i| key(i)),
                SortKey::Number => order.sort_by(|&a, &b| {
                    match (
                        first_number(&paragraphs[a].text),
                        first_number(&paragraphs[b].text),
                    ) {
                        (Some(x), Some(y)) => x.total_cmp(&y),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                }),
            }
            if o.descending {
                order.reverse();
            }
            let sorted: Vec<_> = order.iter().map(|&i| paragraphs[i].clone()).collect();
            let mut doc = s.core.document().clone();
            let target = &mut doc.sections[section];
            target.raw_stream = None;
            for (slot, p) in target.paragraphs[range.clone()].iter_mut().zip(sorted) {
                *slot = p;
            }
            s.core.set_document(doc);
            if lift {
                s.core
                    .merge_paragraph_native(section, range.start)
                    .map_err(err)?;
                range = range.start - 1..range.end - 1;
            }
            let at = |paragraph: usize, offset: usize| Cursor {
                section,
                paragraph,
                offset,
                cell: None,
            };
            let len = s.paragraph(at(range.end - 1, 0))?.text.chars().count();
            s.clear_selection();
            s.cursor = at(range.start, 0);
            s.span_anchor = Some(at(range.end - 1, len));
            Ok(())
        })
    }
    /// Count controls of each kind the delete would remove (for the dialog).
    pub fn ctrl_mark_counts(&self) -> Result<Vec<(MarkKind, usize)>> {
        let (section, range) = self.body_paragraphs()?;
        let mut counts: Vec<(MarkKind, usize)> = MarkKind::ALL.iter().map(|k| (*k, 0)).collect();
        for p in &self.core.document().sections[section].paragraphs[range] {
            for c in &p.controls {
                if let Some(k) = MarkKind::of(c)
                    && let Some(slot) = counts.iter_mut().find(|(x, _)| *x == k)
                {
                    slot.1 += 1;
                }
            }
        }
        Ok(counts)
    }
    /// 조판 부호 지우기: remove the chosen control kinds from the selection (or section body).
    pub fn delete_ctrl_marks(&mut self, kinds: &[MarkKind]) -> Result<usize> {
        if kinds.is_empty() {
            bail!("지울 조판 부호를 하나 이상 고르세요.");
        }
        let (section, range) = self.body_paragraphs()?;
        let mut targets = Vec::new();
        for (pi, p) in self.core.document().sections[section].paragraphs[range.clone()]
            .iter()
            .enumerate()
        {
            for (ci, c) in p.controls.iter().enumerate() {
                if let Some(k) = MarkKind::of(c)
                    && kinds.contains(&k)
                {
                    targets.push((range.start + pi, ci, k));
                }
            }
        }
        let count = targets.len();
        if count == 0 {
            return Ok(0);
        }
        self.transaction(|s| {
            // Last first, so earlier paragraph/control indexes stay valid.
            for (p, c, k) in targets.into_iter().rev() {
                match k {
                    MarkKind::Footnote | MarkKind::Endnote => {
                        s.core.delete_footnote_native(section, p, c).map_err(err)?
                    }
                    _ => s.core.delete_control_native(section, p, c).map_err(err)?,
                };
            }
            s.clear_selection();
            s.cursor = Cursor {
                section,
                paragraph: range.start,
                offset: 0,
                cell: None,
            };
            Ok(())
        })?;
        Ok(count)
    }
}
