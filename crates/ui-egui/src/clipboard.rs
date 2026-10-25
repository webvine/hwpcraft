//! Owner: core area. System clipboard HTML flavour (arboard), so formatting
//! travels to and from other programs. egui only carries plain text itself.
//! Every failure degrades to plain text; nothing here panics.

/// Largest HTML we try to import; bigger payloads fall back to text.
const MAX_HTML: usize = 4 * 1024 * 1024;

/// Put `text` and, when given, its HTML form on the clipboard. `false` when the
/// clipboard was unavailable, so the caller can fall back to egui's text copy.
pub(crate) fn set(text: &str, html: Option<&str>) -> bool {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return false;
    };
    let result = match html {
        Some(html) => clipboard.set().html(html, Some(text)),
        None => clipboard.set().text(text),
    };
    result.is_ok()
}

/// HTML currently on the clipboard, if another program (or we) put some there.
pub(crate) fn html() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let html = clipboard.get().html().ok()?;
    (!html.trim().is_empty() && html.len() <= MAX_HTML).then_some(html)
}
