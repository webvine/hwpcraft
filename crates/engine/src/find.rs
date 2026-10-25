//! Owner: core area. Hangul-style find / replace with case, whole-word and direction options.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindDirection {
    /// 아래쪽: from the caret to the end, then wrap.
    #[default]
    Down,
    /// 위쪽: from the caret back to the start, then wrap.
    Up,
    /// 문서 전체: from the document start.
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FindOptions {
    /// 대소문자 구별
    pub case_sensitive: bool,
    /// 온전한 낱말
    pub whole_word: bool,
    pub direction: FindDirection,
}

fn fold(c: char, case_sensitive: bool) -> char {
    if case_sensitive {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Scalar start offsets of every non-overlapping match in `text`.
pub(crate) fn matches(text: &str, query: &str, o: FindOptions) -> Vec<usize> {
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = query.chars().map(|c| fold(c, o.case_sensitive)).collect();
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let hit = hay[i..i + needle.len()]
            .iter()
            .zip(&needle)
            .all(|(a, b)| fold(*a, o.case_sensitive) == *b)
            && (!o.whole_word
                || (!word(i.checked_sub(1).and_then(|p| hay.get(p)))
                    && !word(hay.get(i + needle.len()))));
        if hit {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

impl Session {
    /// True when the current selection is exactly one match of `query`.
    fn selection_matches(&self, query: &str, o: FindOptions) -> bool {
        if self.span_anchor.is_some() {
            return false;
        }
        let r = self.selection();
        !r.is_empty() && matches(&self.selected_text(), query, o) == [0] && {
            let n = query.chars().count();
            r.len() == n
        }
    }
    /// Select the next match. Returns false when the document has none.
    pub fn find(&mut self, query: &str, o: FindOptions) -> Result<bool> {
        if query.is_empty() {
            return Ok(false);
        }
        if query.contains(['\r', '\n']) {
            bail!("찾기는 한 문단 안의 텍스트만 지원합니다.");
        }
        let entries = self.entries();
        if entries.is_empty() {
            return Ok(false);
        }
        let here = entries
            .iter()
            .position(|(c, _)| {
                c.section == self.cursor.section
                    && c.paragraph == self.cursor.paragraph
                    && c.cell == self.cursor.cell
            })
            .unwrap_or(0);
        let r = self.selection();
        let n = entries.len();
        let backward = o.direction == FindDirection::Up;
        let start = if o.direction == FindDirection::All {
            0
        } else {
            here
        };
        for step in 0..=n {
            let i = if backward {
                (start + n * 2 - step) % n
            } else {
                (start + step) % n
            };
            let c = entries[i].0;
            let found = matches(&self.paragraph(c)?.text, query, o);
            let first_visit = step == 0 && o.direction != FindDirection::All;
            let last_visit = step == n;
            let pick = if backward {
                found.into_iter().rev().find(|&m| {
                    (!first_visit && !last_visit)
                        || (first_visit && m < r.start)
                        || (last_visit && m >= r.start)
                })
            } else {
                let after = r.end.max(self.cursor.offset);
                found.into_iter().find(|&m| {
                    (!first_visit && !last_visit)
                        || (first_visit && m >= after)
                        || (last_visit && m < after)
                })
            };
            if let Some(m) = pick {
                self.clear_selection();
                self.cursor = Cursor {
                    offset: m + query.chars().count(),
                    ..c
                };
                self.anchor = Some(m);
                self.typing = None;
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// 바꾸기: replace the selected match (one undo step), then find the next one.
    /// Returns whether another match was selected.
    pub fn replace_next(&mut self, query: &str, replacement: &str, o: FindOptions) -> Result<bool> {
        if replacement.contains(['\r', '\n']) {
            bail!("바꿀 내용은 한 줄 텍스트만 지원합니다.");
        }
        if self.selection_matches(query, o) {
            self.guard_protected()?;
            let replacement = replacement.to_owned();
            self.transaction(|s| s.insert(&replacement))?;
        }
        let o = FindOptions {
            direction: if o.direction == FindDirection::Up {
                FindDirection::Up
            } else {
                FindDirection::Down
            },
            ..o
        };
        self.find(query, o)
    }
    /// 모두 바꾸기 with options; one undo step restores every replacement.
    pub fn replace_all_with(
        &mut self,
        query: &str,
        replacement: &str,
        o: FindOptions,
    ) -> Result<usize> {
        if query.is_empty() {
            bail!("찾을 내용을 입력하세요.");
        }
        if query.contains(['\r', '\n']) || replacement.contains(['\r', '\n']) {
            bail!("모두 바꾸기는 문단 안의 한 줄 텍스트만 지원합니다.");
        }
        if query.len() > 1_000_000 || replacement.len() > 1_000_000 {
            bail!("찾기와 바꾸기 텍스트는 각각 1 MB 이하입니다.");
        }
        let length = query.chars().count();
        let mut planned = Vec::new();
        for (cursor, _) in self.entries() {
            // Protected cells keep their text, as in Hangul.
            if self.cell_protected(cursor) {
                continue;
            }
            for m in matches(&self.paragraph(cursor)?.text, query, o) {
                planned.push((cursor, m));
                if planned.len() > 10_000 {
                    bail!("한 번의 모두 바꾸기는 10,000건 이하입니다.");
                }
            }
        }
        let count = planned.len();
        if count == 0 || (o.case_sensitive && query == replacement) {
            return Ok(if query == replacement { 0 } else { count });
        }
        let caret = self.cursor;
        self.transaction(|s| {
            // Apply backwards so earlier addresses stay valid.
            for (cursor, m) in planned.into_iter().rev() {
                s.clear_selection();
                s.cursor = Cursor {
                    offset: m + length,
                    ..cursor
                };
                s.anchor = Some(m);
                s.insert(replacement)?;
            }
            s.cursor = caret;
            s.clear_selection();
            Ok(())
        })?;
        Ok(count)
    }
}
