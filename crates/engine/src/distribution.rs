//! Owner: shell/page area. 배포용 문서 opens read-only. RHWP decrypts it for viewing but
//! does not report the author's copy and print restrictions, and saving would write an
//! ordinary, unprotected file, so everything that edits, saves, copies, prints or hands
//! out the text is refused; looking at pages, moving around and finding stay.
use super::*;

pub const DISTRIBUTION_READ_ONLY: &str =
    "배포용 문서는 읽기 전용입니다. 작성자의 허락 없이 편집·복사·인쇄할 수 없습니다.";

/// Commands a 배포용 문서 still runs: screen marks, caret and selection moves, finding,
/// and layout queries without document text. Their results lose any text (see
/// [`scrub_text`]).
const READ_ONLY_COMMANDS: &[&str] = &[
    "view.marks",
    "view.marks.get",
    "caret.get",
    "caret.move",
    "caret.move_cell",
    "select.all",
    "select.clear",
    "select.line",
    "select.paragraph",
    "select.word",
    "select.get",
    "selection.set",
    "selection.range",
    "edit.find",
    "document.statistics",
    "file.doc_info",
    "page.layout.get",
    "page.number.get",
    "page.section.get",
    "page.hide.get",
    "page.border_fill.get",
];

/// Result keys that carry document text.
const TEXT_KEYS: &[&str] = &[
    "text",
    "texts",
    "selectedText",
    "html",
    "excerpt",
    "snippet",
    "context",
];

impl Session {
    /// The open document is a 배포용 (distribution) document.
    pub fn is_distribution(&self) -> bool {
        self.core.document().header.distribution
    }

    /// Refuse edits, saves, copies and printing of a 배포용 문서.
    pub(crate) fn guard_distribution(&self) -> Result<()> {
        if self.is_distribution() {
            bail!("{DISTRIBUTION_READ_ONLY}");
        }
        Ok(())
    }

    /// Refuse a command a 배포용 문서 does not allow.
    pub(crate) fn guard_distribution_command(&self, id: &str) -> Result<()> {
        if self.is_distribution() && !READ_ONLY_COMMANDS.contains(&id) {
            bail!("{DISTRIBUTION_READ_ONLY}");
        }
        Ok(())
    }

    /// A command result with document text removed, for a 배포용 문서.
    pub(crate) fn scrub_distribution(&self, value: Value) -> Value {
        if self.is_distribution() {
            scrub_text(value)
        } else {
            value
        }
    }
}

fn scrub_text(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(k, _)| !TEXT_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k, scrub_text(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(scrub_text).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_removed_at_every_depth() {
        let v = scrub_text(json!({
            "pages": 3,
            "text": "본문",
            "items": [{"text": "x", "page": 1}, {"selectedText": "y"}],
        }));
        assert_eq!(v, json!({"pages": 3, "items": [{"page": 1}, {}]}));
    }
}
