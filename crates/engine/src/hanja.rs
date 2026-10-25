//! Owner: review-tools area. 한자 변환: 한자로 바꾸기 (F9), 한자→한글, 한자→한자.
//! Other areas must not edit this file; request changes through the owner.
//!
//! Readings come from the Unicode Han Database (Unihan `kHangul`, Unicode License v3)
//! compiled into the binary from assets/data/hanja-readings.txt. There is no word
//! dictionary, so multi-syllable words are converted syllable by syllable.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// (command id, Korean label, shortcut) shown in help and command search.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "tools.hanja.candidates",
        "한자 후보 조회 (한글 음절별 한자, 한자의 음과 동음 한자)",
        "",
    ),
    (
        "tools.hanja.convert",
        "한자로 바꾸기 (선택·낱말, 입력 형식: 한자 / 한자(한글) / 한글(한자) / 한자[한글])",
        "F9",
    ),
    (
        "tools.hanja.to_hangul",
        "한자를 한글로 바꾸기 (선택·낱말·문서 전체, 두음 법칙 적용)",
        "",
    ),
];

/// Parameter help and JSON example per command, for `command_help`.
pub const PARAMS: &[(&str, &str, &str)] = &[
    (
        "tools.hanja.candidates",
        "text(선택, 64자 이하): 생략하면 선택 영역 또는 커서 위치의 낱말. 결과 items[]에 음절별 후보(hanja, ks_x_1001, education, personal_name)와 hangul(한자를 한글로 읽은 값)을 돌려줍니다.",
        r#"{"text":"한자"}"#,
    ),
    (
        "tools.hanja.convert",
        "hanja(필수): 바꿀 한자 문자열(고르지 않은 음절은 한글 그대로 가능). format: hanja | hanja_hangul | hangul_hanja | hanja_bracket (기본 hanja). hangul(선택): 괄호 안 한글 표기, 기본값은 바뀌는 글자의 독음. 선택 영역 또는 커서 위치의 낱말을 바꿉니다.",
        r#"{"hanja":"漢字","format":"hanja_hangul"}"#,
    ),
    (
        "tools.hanja.to_hangul",
        "scope: auto(선택 영역 또는 낱말, 기본) | selection | word | document. 한자를 독음으로 바꾸며 낱말 첫 음절에 두음 법칙을 적용합니다. document는 본문과 표 셀 전체를 한 번의 실행 취소로 바꿉니다.",
        r#"{"scope":"document"}"#,
    ),
];

const TABLE: &str = include_str!("../../../assets/data/hanja-readings.txt");

/// Source tags of a reading, from Unihan `kHangul`.
pub const KS_X_1001: u8 = 1;
pub const KS_X_1002: u8 = 2;
/// 한문 교육용 기초 한자
pub const EDUCATION: u8 = 4;
/// 인명용 한자
pub const PERSONAL_NAME: u8 = 8;
const REMOVED: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub syllable: char,
    pub tags: u8,
}

struct Table {
    by_hanja: HashMap<char, Vec<Reading>>,
    /// Candidates per Hangul syllable, best first.
    by_syllable: HashMap<char, Vec<(char, u8)>>,
}

fn parse_tags(s: &str) -> u8 {
    s.bytes().fold(0, |acc, b| {
        acc | match b {
            b'0' => KS_X_1001,
            b'1' => KS_X_1002,
            b'E' => EDUCATION,
            b'N' => PERSONAL_NAME,
            b'X' => REMOVED,
            _ => 0,
        }
    })
}
/// Lower is listed first: common KS X 1001 characters before rare name-only ones.
fn rank(tags: u8) -> u8 {
    if tags & KS_X_1001 != 0 && tags & EDUCATION != 0 {
        0
    } else if tags & KS_X_1001 != 0 {
        1
    } else if tags & EDUCATION != 0 {
        2
    } else if tags & KS_X_1002 != 0 {
        3
    } else {
        4
    }
}

fn table() -> &'static Table {
    static TABLE_CELL: OnceLock<Table> = OnceLock::new();
    TABLE_CELL.get_or_init(|| {
        let mut by_hanja: HashMap<char, Vec<Reading>> = HashMap::new();
        let mut by_syllable: HashMap<char, Vec<(char, u8)>> = HashMap::new();
        for line in TABLE.lines() {
            if line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let Some(hanja) = fields.next().and_then(|f| f.chars().next()) else {
                continue;
            };
            let mut readings = Vec::new();
            for field in fields {
                let Some((syllable, tags)) = field.split_once(':') else {
                    continue;
                };
                let Some(syllable) = syllable.chars().next() else {
                    continue;
                };
                let tags = parse_tags(tags);
                if tags & REMOVED != 0 {
                    continue;
                }
                readings.push(Reading { syllable, tags });
                by_syllable.entry(syllable).or_default().push((hanja, tags));
            }
            if !readings.is_empty() {
                by_hanja.insert(hanja, readings);
            }
        }
        for list in by_syllable.values_mut() {
            list.sort_by_key(|(c, tags)| (rank(*tags), *c));
        }
        Table {
            by_hanja,
            by_syllable,
        }
    })
}

/// CJK ideograph ranges Hangul treats as 한자.
pub fn is_hanja(c: char) -> bool {
    matches!(u32::from(c),
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3134F)
}
pub fn is_hangul_syllable(c: char) -> bool {
    ('가'..='힣').contains(&c)
}
/// Hanja with the given Hangul reading, best candidates first.
pub fn hanja_candidates(syllable: char) -> &'static [(char, u8)] {
    table()
        .by_syllable
        .get(&syllable)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
/// Known readings of a hanja, in Unihan order.
pub fn hanja_readings(hanja: char) -> &'static [Reading] {
    table()
        .by_hanja
        .get(&hanja)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
/// The dictionary (본음) reading: the KS X 1001 education reading when there is one.
pub fn primary_reading(hanja: char) -> Option<char> {
    let readings = hanja_readings(hanja);
    readings
        .iter()
        .min_by_key(|r| rank(r.tags))
        .map(|r| r.syllable)
}

const INITIAL_N: u32 = 2;
const INITIAL_R: u32 = 5;
const INITIAL_NONE: u32 = 11;
fn decompose(c: char) -> Option<(u32, u32, u32)> {
    if !is_hangul_syllable(c) {
        return None;
    }
    let code = u32::from(c) - 0xAC00;
    Some((code / 588, (code % 588) / 28, code % 28))
}
fn compose(initial: u32, medial: u32, finale: u32) -> char {
    char::from_u32(0xAC00 + initial * 588 + medial * 28 + finale).unwrap_or('\u{FFFD}')
}
/// Vowels after which word-initial ㄹ/ㄴ become ㅇ (ㅑ ㅒ ㅕ ㅖ ㅛ ㅠ ㅣ).
fn iotised(medial: u32) -> bool {
    matches!(medial, 2 | 3 | 6 | 7 | 12 | 17 | 20)
}
/// 두음 법칙 for the first syllable of a word: 로→노, 녀→여, 리→이, 락→낙.
pub fn initial_sound_rule(c: char) -> char {
    let Some((initial, medial, finale)) = decompose(c) else {
        return c;
    };
    match initial {
        INITIAL_R if iotised(medial) => compose(INITIAL_NONE, medial, finale),
        INITIAL_R => compose(INITIAL_N, medial, finale),
        INITIAL_N if iotised(medial) => compose(INITIAL_NONE, medial, finale),
        _ => c,
    }
}
/// 렬·률 after a vowel or ㄴ become 열·율 (비율, 선열); after other finals they stay (확률).
fn medial_ryul_rule(prev: char, c: char) -> char {
    if !matches!(c, '렬' | '률') {
        return c;
    }
    match decompose(prev) {
        Some((_, _, finale)) if finale == 0 || finale == 4 => initial_sound_rule(c),
        _ => c,
    }
}

/// Hangul reading of `text`: hanja are replaced syllable by syllable, other
/// characters stay. Readings follow 두음 법칙 at the start of a hanja run.
pub fn to_hangul(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    for c in text.chars() {
        let converted = if is_hanja(c) {
            match primary_reading(c) {
                Some(reading) => match prev {
                    Some(p) if is_hanja(p) || is_hangul_syllable(p) => {
                        medial_ryul_rule(out.chars().next_back().unwrap_or(' '), reading)
                    }
                    _ => initial_sound_rule(reading),
                },
                None => c,
            }
        } else {
            c
        };
        out.push(converted);
        prev = Some(c);
    }
    out
}

/// Hangul's 입력 형식 list in the 한자로 바꾸기 dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HanjaFormat {
    /// 漢字
    #[default]
    Hanja,
    /// 漢字(한자)
    HanjaHangul,
    /// 한자(漢字)
    HangulHanja,
    /// 漢字[한자]
    HanjaBracket,
}
impl HanjaFormat {
    pub fn render(self, hanja: &str, hangul: &str) -> String {
        match self {
            HanjaFormat::Hanja => hanja.to_owned(),
            HanjaFormat::HanjaHangul => format!("{hanja}({hangul})"),
            HanjaFormat::HangulHanja => format!("{hangul}({hanja})"),
            HanjaFormat::HanjaBracket => format!("{hanja}[{hangul}]"),
        }
    }
}

/// Scalar range of the Hangul/hanja word touching `offset` in `text`.
pub(crate) fn word_range(text: &str, offset: usize) -> Range<usize> {
    let chars: Vec<char> = text.chars().collect();
    let wordy = |c: &char| is_hangul_syllable(*c) || is_hanja(*c);
    let offset = offset.min(chars.len());
    let mut start = offset;
    while start > 0 && wordy(&chars[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < chars.len() && wordy(&chars[end]) {
        end += 1;
    }
    start..end
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidates {
    #[serde(default)]
    text: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Convert {
    hanja: String,
    #[serde(default)]
    format: HanjaFormat,
    /// Hangul shown next to the hanja; defaults to the replaced text's reading.
    #[serde(default)]
    hangul: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    /// Selection when there is one, otherwise the word at the caret.
    #[default]
    Auto,
    Selection,
    Word,
    Document,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToHangul {
    #[serde(default)]
    scope: Scope,
}

fn candidate_json(hanja: char, tags: u8) -> Value {
    json!({
        "hanja": hanja.to_string(),
        "ks_x_1001": tags & KS_X_1001 != 0,
        "education": tags & EDUCATION != 0,
        "personal_name": tags & PERSONAL_NAME != 0,
    })
}
/// Per-character candidate lists for `text`: hanja for a Hangul syllable, 동음 한자 for a hanja.
pub fn candidates_json(text: &str) -> Value {
    let items: Vec<Value> = text
        .chars()
        .map(|c| {
            if is_hangul_syllable(c) {
                let list: Vec<Value> = hanja_candidates(c)
                    .iter()
                    .map(|(h, t)| candidate_json(*h, *t))
                    .collect();
                json!({"source": c.to_string(), "reading": c.to_string(), "candidates": list})
            } else if is_hanja(c) {
                let readings: Vec<String> = hanja_readings(c)
                    .iter()
                    .map(|r| r.syllable.to_string())
                    .collect();
                let list: Vec<Value> = primary_reading(c)
                    .map(|r| {
                        hanja_candidates(r)
                            .iter()
                            .filter(|(h, _)| *h != c)
                            .map(|(h, t)| candidate_json(*h, *t))
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"source": c.to_string(), "reading": primary_reading(c).map(|r| r.to_string()),
                       "readings": readings, "candidates": list})
            } else {
                json!({"source": c.to_string(), "candidates": []})
            }
        })
        .collect();
    json!({"text": text, "hangul": to_hangul(text), "items": items})
}

impl Session {
    /// Return `None` for ids this module does not own.
    pub(crate) fn run_hanja_ext(&mut self, id: &str, params: &Value) -> Option<Result<Value>> {
        if !COMMANDS.iter().any(|(c, _, _)| *c == id) {
            return None;
        }
        Some(self.run_hanja_command(id, params.clone()))
    }

    /// Text and range the 한자 commands act on: a single-paragraph selection or the word at the caret.
    fn hanja_target(&self, scope: Scope) -> Result<Range<usize>> {
        if self.span_anchor.is_some() {
            bail!("한자 변환은 한 문단 안에서 선택한 글자에만 적용합니다.");
        }
        let selection = self.selection();
        let range = match scope {
            Scope::Selection => selection,
            Scope::Word => word_range(self.text(), self.cursor.offset),
            Scope::Auto if !selection.is_empty() => selection,
            Scope::Auto | Scope::Document => word_range(self.text(), self.cursor.offset),
        };
        if range.is_empty() {
            bail!(
                "한자로 바꿀 낱말이 없습니다. 한글이나 한자를 선택하거나 낱말 뒤에 커서를 두세요."
            );
        }
        Ok(range)
    }
    fn text_in(&self, range: &Range<usize>) -> String {
        self.text()
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect()
    }
    /// Replace `range` of the caret paragraph with `text` in one undo step.
    fn replace_range(&mut self, range: Range<usize>, text: &str) -> Result<()> {
        self.guard_protected()?;
        let text = text.to_owned();
        self.transaction(|s| {
            s.clear_selection();
            s.cursor.offset = range.end;
            s.anchor = Some(range.start);
            s.insert(&text)
        })
    }

    fn run_hanja_command(&mut self, id: &str, params: Value) -> Result<Value> {
        match id {
            "tools.hanja.candidates" => {
                let p: Candidates = serde_json::from_value(params)?;
                let text = match p.text {
                    Some(t) => t,
                    None => {
                        let range = self.hanja_target(Scope::Auto)?;
                        self.text_in(&range)
                    }
                };
                if text.chars().count() > 64 {
                    bail!("한자 후보는 64자 이하의 낱말에 대해 조회합니다.");
                }
                Ok(candidates_json(&text))
            }
            "tools.hanja.convert" => {
                let p: Convert = serde_json::from_value(params)?;
                if p.hanja.is_empty() || p.hanja.contains(['\r', '\n']) {
                    bail!("바꿀 한자를 입력하세요.");
                }
                if !p.hanja.chars().any(is_hanja)
                    || !p
                        .hanja
                        .chars()
                        .all(|c| is_hanja(c) || is_hangul_syllable(c))
                {
                    bail!(
                        "한자(또는 한자와 한글 음절)만 입력할 수 있습니다: {}",
                        p.hanja
                    );
                }
                let range = self.hanja_target(Scope::Auto)?;
                let original = self.text_in(&range);
                let hangul = p.hangul.unwrap_or_else(|| to_hangul(&original));
                if hangul.contains(['\r', '\n']) {
                    bail!("한글 표기는 한 줄 텍스트여야 합니다.");
                }
                let text = p.format.render(&p.hanja, &hangul);
                let start = range.start;
                self.replace_range(range, &text)?;
                Ok(json!({
                    "replaced": original,
                    "text": text,
                    "start": start,
                    "cursor": self.cursor,
                }))
            }
            "tools.hanja.to_hangul" => {
                let p: ToHangul = serde_json::from_value(params)?;
                if p.scope == Scope::Document {
                    return self.hanja_to_hangul_document();
                }
                let range = self.hanja_target(p.scope)?;
                let original = self.text_in(&range);
                let text = to_hangul(&original);
                let changed = text != original;
                if changed {
                    self.replace_range(range, &text)?;
                }
                Ok(json!({
                    "replaced": if changed { 1 } else { 0 },
                    "from": original,
                    "text": text,
                    "cursor": self.cursor,
                }))
            }
            _ => bail!("알 수 없는 한자 명령: {id}"),
        }
    }

    /// 한자→한글 for every hanja run in body and table paragraphs; one undo step.
    fn hanja_to_hangul_document(&mut self) -> Result<Value> {
        let mut planned: Vec<(Cursor, Range<usize>, String)> = Vec::new();
        for cursor in self.cursors() {
            if self.cell_protected(cursor) {
                continue;
            }
            let chars: Vec<char> = self.paragraph(cursor)?.text.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if !is_hanja(chars[i]) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len() && is_hanja(chars[i]) {
                    i += 1;
                }
                // Convert with the preceding syllable so 두음 법칙 sees the word start.
                let lead: String = chars[start.saturating_sub(1)..start].iter().collect();
                let run: String = chars[start..i].iter().collect();
                let converted = to_hangul(&format!("{lead}{run}"));
                let converted: String = converted.chars().skip(lead.chars().count()).collect();
                if converted != run {
                    planned.push((cursor, start..i, converted));
                }
                if planned.len() > 10_000 {
                    bail!("한 번에 바꾸는 한자 묶음은 10,000건 이하입니다.");
                }
            }
        }
        let count = planned.len();
        if count == 0 {
            return Ok(json!({"replaced": 0}));
        }
        let caret = self.cursor;
        self.transaction(|s| {
            s.with_deferred_layout(|s| {
                for (cursor, range, text) in planned.into_iter().rev() {
                    s.clear_selection();
                    s.cursor = Cursor {
                        offset: range.end,
                        ..cursor
                    };
                    s.anchor = Some(range.start);
                    s.insert(&text)?;
                }
                Ok(())
            })?;
            s.cursor = caret;
            s.clear_selection();
            Ok(())
        })?;
        Ok(json!({"replaced": count}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads_ks_x_1001_readings() {
        assert!(hanja_candidates('한').iter().any(|(c, _)| *c == '漢'));
        let (first, tags) = hanja_candidates('한')[0];
        assert!(tags & KS_X_1001 != 0 && tags & EDUCATION != 0, "{first}");
        assert_eq!(primary_reading('漢'), Some('한'));
        assert!(hanja_candidates('뷁').is_empty());
    }

    #[test]
    fn to_hangul_applies_initial_sound_rule() {
        assert_eq!(to_hangul("老人"), "노인");
        assert_eq!(to_hangul("敬老"), "경로");
        assert_eq!(to_hangul("李"), "이");
        assert_eq!(to_hangul("女子"), "여자");
        assert_eq!(to_hangul("比率"), "비율");
        assert_eq!(to_hangul("確率"), "확률");
        assert_eq!(to_hangul("大韓民國 2026"), "대한민국 2026");
    }

    #[test]
    fn word_range_covers_hangul_and_hanja_around_offset() {
        assert_eq!(word_range("한자 변환 test", 2), 0..2);
        assert_eq!(word_range("한자 변환 test", 3), 3..5);
        assert_eq!(word_range("한자 변환 test", 8), 8..8);
        assert_eq!(word_range("漢字한글", 2), 0..4);
    }
}
