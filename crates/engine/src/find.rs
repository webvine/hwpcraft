//! Owner: review-tools area. Hangul-style find / replace: case, whole word, direction,
//! 띄어쓰기 무시, 아무개 문자 (? *), 조건식 (regular expression), 여러 단어 찾기,
//! 한글로 한자 찾기 and 서식 찾기 (character shape conditions).
use super::*;
use regex::Regex;
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

/// 찾기 options. New fields default to off so older callers keep the exact search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FindOptions {
    /// 대소문자 구별
    pub case_sensitive: bool,
    /// 온전한 낱말
    pub whole_word: bool,
    pub direction: FindDirection,
    /// 띄어쓰기 무시: spaces in the query and the text are skipped.
    pub ignore_spaces: bool,
    /// 아무개 문자: `?` matches one character, `*` any run inside a paragraph.
    pub wildcards: bool,
    /// 조건식: the query is a regular expression (wildcards are then literal regex syntax).
    pub regex: bool,
    /// 여러 단어 찾기: each space-separated word is searched on its own.
    pub any_word: bool,
    /// 한글로 한자 찾기: a Hangul syllable also matches hanja read that way.
    pub hanja_by_hangul: bool,
    /// 서식 찾기 (글자 모양): every character of a match must satisfy the set conditions.
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    /// 글자 크기 in points (whole points).
    pub size_pt: Option<u16>,
}
impl FindOptions {
    fn has_format(&self) -> bool {
        self.bold.is_some()
            || self.italic.is_some()
            || self.underline.is_some()
            || self.size_pt.is_some()
    }
}

/// A compiled query, built once per find / replace call.
pub(crate) struct Matcher {
    regex: Regex,
    options: FindOptions,
}

fn escape_class_char(c: char, out: &mut String) {
    if matches!(c, '\\' | ']' | '[' | '^' | '-' | '&' | '~') {
        out.push('\\');
    }
    out.push(c);
}

/// Regex source for one query character under the non-regex modes.
fn atom(c: char, o: FindOptions, out: &mut String) {
    if o.wildcards && c == '?' {
        out.push('.');
    } else if o.wildcards && c == '*' {
        out.push_str(".*?");
    } else if o.hanja_by_hangul && hanja::is_hangul_syllable(c) {
        let hanja = hanja::hanja_candidates(c);
        if hanja.is_empty() {
            out.push_str(&regex::escape(&c.to_string()));
        } else {
            out.push('[');
            escape_class_char(c, out);
            for (h, _) in hanja {
                escape_class_char(*h, out);
            }
            out.push(']');
        }
    } else {
        out.push_str(&regex::escape(&c.to_string()));
    }
}

impl Matcher {
    pub(crate) fn new(query: &str, o: FindOptions) -> Result<Self> {
        if query.is_empty() {
            bail!("찾을 내용을 입력하세요.");
        }
        let mut source = String::new();
        if !o.case_sensitive {
            source.push_str("(?i)");
        }
        if o.regex {
            source.push_str("(?:");
            source.push_str(query);
            source.push(')');
        } else {
            let words: Vec<&str> = if o.any_word {
                query.split_whitespace().collect()
            } else {
                vec![query]
            };
            if words.is_empty() {
                bail!("찾을 내용을 입력하세요.");
            }
            source.push_str("(?:");
            for (i, word) in words.iter().enumerate() {
                if i > 0 {
                    source.push('|');
                }
                let mut first = true;
                for c in word.chars() {
                    if o.ignore_spaces && c.is_whitespace() {
                        continue;
                    }
                    if o.ignore_spaces && !first {
                        source.push_str(r"\s*");
                    }
                    atom(c, o, &mut source);
                    first = false;
                }
            }
            source.push(')');
        }
        let regex = regex::RegexBuilder::new(&source)
            .size_limit(1 << 20)
            .build()
            .map_err(|e| anyhow!("조건식이 올바르지 않습니다: {e}"))?;
        Ok(Self { regex, options: o })
    }

    /// Scalar ranges of every non-overlapping, non-empty match in `text`.
    pub(crate) fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        if text.is_empty() {
            return out;
        }
        // Byte offset → scalar offset, walked once in order.
        let mut byte_to_char: Vec<(usize, usize)> = text
            .char_indices()
            .enumerate()
            .map(|(k, (b, _))| (b, k))
            .collect();
        byte_to_char.push((text.len(), byte_to_char.len()));
        let scalar = |byte: usize| {
            byte_to_char
                .binary_search_by_key(&byte, |(b, _)| *b)
                .map(|i| byte_to_char[i].1)
                .unwrap_or(usize::MAX)
        };
        let chars: Vec<char> = text.chars().collect();
        let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
        for m in self.regex.find_iter(text) {
            if m.start() == m.end() {
                continue;
            }
            let (start, end) = (scalar(m.start()), scalar(m.end()));
            if start == usize::MAX || end == usize::MAX {
                continue;
            }
            if self.options.whole_word
                && (word(start.checked_sub(1).and_then(|p| chars.get(p))) || word(chars.get(end)))
            {
                continue;
            }
            out.push(start..end);
        }
        out
    }
}

/// Scalar start offsets of every match; a test-only shorthand over `Matcher`.
#[cfg(test)]
fn matches(text: &str, query: &str, o: FindOptions) -> Vec<usize> {
    Matcher::new(query, o)
        .map(|m| m.ranges(text).into_iter().map(|r| r.start).collect())
        .unwrap_or_default()
}

impl Session {
    /// Character-shape ids per scalar offset of a paragraph, for 서식 찾기.
    fn shape_ok(&self, c: Cursor, range: &Range<usize>, o: FindOptions) -> bool {
        if !o.has_format() {
            return true;
        }
        let Ok(para) = self.paragraph(c) else {
            return false;
        };
        let shapes = &self.core.document().doc_info.char_shapes;
        range.clone().all(|k| {
            let pos = para.char_offsets.get(k).copied().unwrap_or(k as u32);
            let id = para
                .char_shapes
                .iter()
                .take_while(|r| r.start_pos <= pos)
                .last()
                .map(|r| r.char_shape_id)
                .unwrap_or(0);
            let Some(cs) = shapes.get(id as usize) else {
                return false;
            };
            o.bold.is_none_or(|b| cs.bold == b)
                && o.italic.is_none_or(|i| cs.italic == i)
                && o.underline.is_none_or(|u| {
                    (cs.underline_type != hwpcraft_doc::model::style::UnderlineType::None) == u
                })
                && o.size_pt.is_none_or(|pt| {
                    ((cs.base_size as f64) / 100.0).round() as i64 == i64::from(pt)
                })
        })
    }
    fn paragraph_ranges(&self, c: Cursor, m: &Matcher) -> Result<Vec<Range<usize>>> {
        let text = &self.paragraph(c)?.text;
        Ok(m.ranges(text)
            .into_iter()
            .filter(|r| self.shape_ok(c, r, m.options))
            .collect())
    }
    /// True when the current selection is exactly one match of the query.
    fn selection_matches(&self, m: &Matcher) -> bool {
        if self.span_anchor.is_some() {
            return false;
        }
        let r = self.selection();
        !r.is_empty()
            && self
                .paragraph_ranges(self.cursor, m)
                .is_ok_and(|ranges| ranges.contains(&r))
    }
    /// Select the next match. Returns false when the document has none.
    pub fn find(&mut self, query: &str, o: FindOptions) -> Result<bool> {
        if query.is_empty() {
            return Ok(false);
        }
        if query.contains(['\r', '\n']) {
            bail!("찾기는 한 문단 안의 텍스트만 지원합니다.");
        }
        let m = Matcher::new(query, o)?;
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
            let found = self.paragraph_ranges(c, &m)?;
            let first_visit = step == 0 && o.direction != FindDirection::All;
            let last_visit = step == n;
            let pick = if backward {
                found.into_iter().rev().find(|m| {
                    (!first_visit && !last_visit)
                        || (first_visit && m.start < r.start)
                        || (last_visit && m.start >= r.start)
                })
            } else {
                let after = r.end.max(self.cursor.offset);
                found.into_iter().find(|m| {
                    (!first_visit && !last_visit)
                        || (first_visit && m.start >= after)
                        || (last_visit && m.start < after)
                })
            };
            if let Some(m) = pick {
                self.clear_selection();
                self.cursor = Cursor { offset: m.end, ..c };
                self.anchor = Some(m.start);
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
        let m = Matcher::new(query, o)?;
        if self.selection_matches(&m) {
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
    /// The replacement is inserted literally (no regex group expansion).
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
        let m = Matcher::new(query, o)?;
        let mut planned = Vec::new();
        for (cursor, _) in self.entries() {
            // Protected cells keep their text, as in Hangul.
            if self.cell_protected(cursor) {
                continue;
            }
            for r in self.paragraph_ranges(cursor, &m)? {
                planned.push((cursor, r));
                if planned.len() > 10_000 {
                    bail!("한 번의 모두 바꾸기는 10,000건 이하입니다.");
                }
            }
        }
        let count = planned.len();
        let literal = !o.regex && !o.wildcards && !o.ignore_spaces && !o.hanja_by_hangul;
        if count == 0 || (literal && o.case_sensitive && query == replacement) {
            return Ok(if literal && query == replacement {
                0
            } else {
                count
            });
        }
        let caret = self.cursor;
        self.transaction(|s| {
            // Apply backwards so earlier addresses stay valid.
            for (cursor, r) in planned.into_iter().rev() {
                s.clear_selection();
                s.cursor = Cursor {
                    offset: r.end,
                    ..cursor
                };
                s.anchor = Some(r.start);
                s.insert(replacement)?;
            }
            s.cursor = caret;
            s.clear_selection();
            Ok(())
        })?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(text: &str, query: &str, o: FindOptions) -> Vec<Range<usize>> {
        Matcher::new(query, o)
            .map(|m| m.ranges(text))
            .unwrap_or_default()
    }

    #[test]
    fn literal_and_whole_word_match_the_previous_behaviour() {
        let o = FindOptions::default();
        assert_eq!(matches("Rust rusty RUST", "rust", o), [0, 5, 11]);
        let word = FindOptions {
            whole_word: true,
            ..o
        };
        assert_eq!(matches("Rust rusty RUST", "rust", word), [0, 11]);
        let exact = FindOptions {
            case_sensitive: true,
            ..o
        };
        assert_eq!(matches("Rust rusty RUST", "RUST", exact), [11]);
        assert!(matches("abc", "", o).is_empty());
    }

    #[test]
    fn ignore_spaces_wildcards_regex_and_any_word() {
        let o = FindOptions {
            ignore_spaces: true,
            ..Default::default()
        };
        assert_eq!(ranges("대 한 민국 대한민국", "대한민국", o), [0..6, 7..11]);
        let w = FindOptions {
            wildcards: true,
            ..Default::default()
        };
        assert_eq!(ranges("제1조 제12조 제x조", "제?조", w), [0..3, 9..12]);
        assert_eq!(
            ranges("제1조 제12조 제x조", "제*조", w),
            [0..3, 4..8, 9..12]
        );
        let r = FindOptions {
            regex: true,
            ..Default::default()
        };
        assert_eq!(ranges("2024년 3월 2025년", r"\d{4}년", r), [0..5, 9..14]);
        assert!(Matcher::new("(", r).is_err());
        let any = FindOptions {
            any_word: true,
            ..Default::default()
        };
        assert_eq!(
            ranges("사과 배 포도 배", "배 포도", any),
            [3..4, 5..7, 8..9]
        );
        // `*` is literal without the wildcard option.
        let literal = ranges("a*b ab", "a*b", FindOptions::default());
        assert_eq!((literal.len(), literal.first()), (1, Some(&(0..3))));
    }

    #[test]
    fn hangul_query_finds_hanja_when_asked() {
        let o = FindOptions {
            hanja_by_hangul: true,
            ..Default::default()
        };
        assert_eq!(ranges("韓國 한국 中國", "한국", o), [0..2, 3..5]);
        assert!(ranges("韓國", "한국", FindOptions::default()).is_empty());
    }
}
