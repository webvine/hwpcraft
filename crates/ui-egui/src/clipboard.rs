//! Owner: core area. System clipboard HTML flavour (arboard), so formatting
//! travels to and from other programs. egui only carries plain text itself.
//! Every failure degrades to plain text; nothing here panics.

/// Largest HTML we try to import; bigger payloads fall back to text.
const MAX_HTML: usize = 4 * 1024 * 1024;

thread_local! {
    /// One clipboard handle for the UI thread's lifetime. On Linux (X11 and
    /// Wayland) the copied data is served by the process that owns the selection,
    /// and arboard gives ownership up when its handle is dropped, so a per-copy
    /// handle lost the copy unless a clipboard manager grabbed it in time.
    /// Windows and macOS keep data after a drop; reusing the handle is harmless there.
    static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with the thread's clipboard, opening it on first use. `None` when the
/// system clipboard is unavailable; a failed call drops the handle so the next
/// call reconnects.
fn with_clipboard<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
    CLIPBOARD.with(|slot| {
        let mut slot = slot.try_borrow_mut().ok()?;
        if slot.is_none() {
            *slot = arboard::Clipboard::new().ok();
        }
        let out = f(slot.as_mut()?);
        if out.is_none() {
            *slot = None;
        }
        out
    })
}

/// Put `text` and, when given, its HTML form on the clipboard. `false` when the
/// clipboard was unavailable, so the caller can fall back to egui's text copy.
pub(crate) fn set(text: &str, html: Option<&str>) -> bool {
    with_clipboard(|clipboard| {
        match html {
            Some(html) => clipboard.set().html(html, Some(text)),
            None => clipboard.set().text(text),
        }
        .ok()
    })
    .is_some()
}

/// HTML currently on the clipboard, if another program (or we) put some there.
pub(crate) fn html() -> Option<String> {
    let html = with_clipboard(|clipboard| match clipboard.get().html() {
        Ok(html) => Some(Some(html)),
        // No HTML flavour is not a broken connection; keep the handle.
        Err(arboard::Error::ContentNotAvailable) => Some(None),
        Err(_) => None,
    })??;
    (!html.trim().is_empty() && html.len() <= MAX_HTML).then_some(html)
}
