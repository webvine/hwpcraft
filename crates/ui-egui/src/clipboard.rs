//! Owner: core area. System clipboard HTML flavour and pictures (arboard), so
//! formatting and pictures travel to and from other programs. egui only carries
//! plain text itself. Every failure degrades to plain text; nothing here panics.

use std::path::PathBuf;

/// Largest HTML we try to import; bigger payloads fall back to text.
const MAX_HTML: usize = 4 * 1024 * 1024;
/// Largest copied bitmap we paste, the limits of 그림 넣기.
const MAX_PICTURE_SIDE: usize = 8192;
const MAX_PICTURE_PIXELS: usize = 16_000_000;
/// Picture files a paste puts in (그림 넣기 formats).
const PICTURE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "gif"];

thread_local! {
    /// One clipboard handle for the UI thread's lifetime. On Linux (X11 and
    /// Wayland) the copied data is served by the process that owns the selection,
    /// and arboard gives ownership up when its handle is dropped, so a per-copy
    /// handle lost the copy unless a clipboard manager grabbed it in time.
    /// Windows and macOS keep data after a drop; reusing the handle is harmless there.
    static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

/// Tests stand in for the system clipboard (what [`html`], [`text`] and
/// [`picture`] read), so they never read or replace the user's.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct Fake {
    pub(crate) text: Option<String>,
    pub(crate) png: Option<Vec<u8>>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static FAKE: std::cell::RefCell<Option<Fake>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn fake() -> Option<Fake> {
    FAKE.with(|fake| fake.borrow().clone())
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
    #[cfg(test)]
    if fake().is_some() {
        return None;
    }
    let html = with_clipboard(|clipboard| match clipboard.get().html() {
        Ok(html) => Some(Some(html)),
        // No HTML flavour is not a broken connection; keep the handle.
        Err(arboard::Error::ContentNotAvailable) => Some(None),
        Err(_) => None,
    })??;
    (!html.trim().is_empty() && html.len() <= MAX_HTML).then_some(html)
}

/// Plain text on the clipboard; `None` when there is none or no clipboard.
pub(crate) fn text() -> Option<String> {
    #[cfg(test)]
    if let Some(fake) = fake() {
        return fake.text;
    }
    let text = with_clipboard(|clipboard| match clipboard.get_text() {
        Ok(text) => Some(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Some(None),
        Err(_) => None,
    })??;
    (!text.is_empty()).then_some(text)
}

/// How long after Ctrl or Shift comes up a key coming up still counts as part of
/// a shortcut with it (fast typists let go of Ctrl first).
const SHORTCUT_RELEASE_SECONDS: f64 = 0.5;

/// Paste commands egui swallowed. egui-winit turns Ctrl+V, Shift+Insert (Windows)
/// and the Paste key into a text paste and drops the key press, sending nothing at
/// all when the clipboard holds no text (a copied picture, picture files). The key
/// coming up is all that is left of such a paste.
#[derive(Default)]
pub(crate) struct PasteKeys {
    /// A press of V, Insert, Paste reached us since that key last came up: typing
    /// or another shortcut, not a swallowed paste.
    pressed: [bool; 3],
    /// egui sent a text paste since a paste key last came up.
    pasted: bool,
    /// The modifiers held, and when Ctrl (Cmd on macOS) and Shift last came up
    /// (`egui::InputState::time`).
    held: egui::Modifiers,
    command_up: Option<f64>,
    shift_up: Option<f64>,
}

impl PasteKeys {
    /// Watch one input event of the frame at `now`; `true` when it ends a paste
    /// command egui sent nothing for.
    pub(crate) fn swallowed(&mut self, event: &egui::Event, now: f64) -> bool {
        use egui::{Event, Key};
        let recent = |up: Option<f64>| up.is_some_and(|up| now - up < SHORTCUT_RELEASE_SECONDS);
        match *event {
            Event::ModifiersChanged(new) => {
                if self.held.command && !new.command {
                    self.command_up = Some(now);
                }
                if self.held.shift && !new.shift {
                    self.shift_up = Some(now);
                }
                self.held = new;
                false
            }
            Event::Paste(_) => {
                self.pasted = true;
                false
            }
            Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } => {
                let index = match key {
                    Key::V => 0,
                    Key::Insert => 1,
                    Key::Paste => 2,
                    _ => return false,
                };
                if pressed {
                    self.pressed[index] = true;
                    return false;
                }
                let typed = std::mem::take(&mut self.pressed[index]);
                let pasted = std::mem::take(&mut self.pasted);
                let command = modifiers.command || recent(self.command_up);
                let shift = modifiers.shift || recent(self.shift_up);
                !typed
                    && !pasted
                    && match key {
                        Key::V => command,
                        // Ctrl+Insert is copy.
                        Key::Insert => cfg!(target_os = "windows") && shift && !command,
                        _ => true,
                    }
            }
            _ => false,
        }
    }
}

/// A picture on the clipboard.
pub(crate) enum Picture {
    /// Picture files copied in a file manager.
    Files(Vec<PathBuf>),
    /// A copied bitmap (a browser's 이미지 복사, a screen capture, Paint), as PNG.
    Png(Vec<u8>),
}

/// Whether the text that came with a copy is no more than a picture's address, as
/// browsers put beside a copied picture: then the picture is what was copied.
pub(crate) fn only_names_a_picture(text: &str) -> bool {
    let text = text.trim();
    text.is_empty()
        || (!text.contains(char::is_whitespace)
            && ["http://", "https://", "file:", "data:image/"]
                .iter()
                .any(|scheme| {
                    text.get(..scheme.len())
                        .is_some_and(|s| s.eq_ignore_ascii_case(scheme))
                }))
}

/// A PNG, JPG, BMP or GIF file by its extension.
pub(crate) fn is_picture_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PICTURE_EXTENSIONS.iter().any(|p| e.eq_ignore_ascii_case(p)))
}

/// The picture on the clipboard: picture files first, else a bitmap. `None` when
/// there is neither; `Some(Err)` says why a bitmap cannot be pasted.
pub(crate) fn picture() -> Option<Result<Picture, String>> {
    #[cfg(test)]
    if let Some(fake) = fake() {
        return fake.png.map(|png| Ok(Picture::Png(png)));
    }
    let files: Vec<PathBuf> = with_clipboard(|clipboard| match clipboard.get().file_list() {
        Ok(files) => Some(files),
        Err(arboard::Error::ContentNotAvailable) => Some(Vec::new()),
        Err(_) => None,
    })
    .unwrap_or_default()
    .into_iter()
    .filter(|path| is_picture_file(path))
    .collect();
    if !files.is_empty() {
        return Some(Ok(Picture::Files(files)));
    }
    let image = with_clipboard(|clipboard| match clipboard.get_image() {
        Ok(image) => Some(Some(image)),
        Err(arboard::Error::ContentNotAvailable) => Some(None),
        Err(_) => None,
    })??;
    Some(png(image.width, image.height, image.bytes.into_owned()))
}

/// RGBA pixels as PNG. A bitmap whose alpha bytes are all zero (some programs
/// leave the channel unset) is taken as opaque; an opaque one is stored as RGB.
fn png(width: usize, height: usize, mut rgba: Vec<u8>) -> Result<Picture, String> {
    if width == 0
        || height == 0
        || width > MAX_PICTURE_SIDE
        || height > MAX_PICTURE_SIDE
        || width.saturating_mul(height) > MAX_PICTURE_PIXELS
    {
        return Err("붙일 그림은 8192픽셀 및 1,600만 화소 이하입니다.".into());
    }
    let alpha = |rgba: &[u8], value: u8| rgba.as_chunks::<4>().0.iter().all(|p| p[3] == value);
    if alpha(&rgba, 0) {
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
    }
    let (width, height) = (width as u32, height as u32);
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "클립보드 그림 데이터가 손상되었습니다.".to_owned())?;
    let image = if alpha(image.as_raw(), 255) {
        image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(image).into_rgb8())
    } else {
        image::DynamicImage::ImageRgba8(image)
    };
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|error| format!("클립보드 그림을 PNG로 바꾸지 못했습니다: {error}"))?;
    Ok(Picture::Png(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    /// Feeds `(time, event)` and returns the times of swallowed pastes.
    fn swallowed(events: &[(f64, egui::Event)]) -> Vec<f64> {
        let mut keys = PasteKeys::default();
        events
            .iter()
            .filter(|(now, event)| keys.swallowed(event, *now))
            .map(|(now, _)| *now)
            .collect()
    }

    #[test]
    fn a_paste_key_coming_up_without_its_press_is_a_swallowed_paste() {
        use egui::{Event, Key, Modifiers as M};
        let (none, ctrl) = (M::NONE, M::COMMAND);
        let held = Event::ModifiersChanged;
        // Ctrl+V: V coming up while Ctrl is held, or just after Ctrl came up first.
        assert_eq!(swallowed(&[(1.0, key(Key::V, false, ctrl))]), [1.0]);
        assert_eq!(
            swallowed(&[
                (1.0, held(ctrl)),
                (1.4, held(none)),
                (1.6, key(Key::V, false, none)),
            ]),
            [1.6]
        );
        // Typing v is no paste, even just after a shortcut (Ctrl+S, then v).
        assert!(
            swallowed(&[
                (1.0, held(ctrl)),
                (1.1, held(none)),
                (1.2, key(Key::V, true, none)),
                (1.3, key(Key::V, false, none)),
            ])
            .is_empty()
        );
        // V coming up long after Ctrl, its press unseen (another window had it).
        assert!(
            swallowed(&[
                (1.0, held(ctrl)),
                (1.1, held(none)),
                (3.0, key(Key::V, false, none)),
            ])
            .is_empty()
        );
        // A text paste was the paste; a picture paste right after still goes.
        assert_eq!(
            swallowed(&[
                (1.0, Event::Paste("글".into())),
                (1.1, key(Key::V, false, ctrl)),
                (1.3, key(Key::V, false, ctrl)),
            ]),
            [1.3]
        );
        // Held Ctrl+V repeats the text paste; one V coming up ends it.
        assert!(
            swallowed(&[
                (1.0, Event::Paste("글".into())),
                (1.5, Event::Paste("글".into())),
                (1.6, key(Key::V, false, ctrl)),
            ])
            .is_empty()
        );
        // The Paste key.
        assert_eq!(swallowed(&[(1.0, key(Key::Paste, false, none))]), [1.0]);
        // Shift+Insert pastes (Windows); Ctrl+Insert copies; Insert alone is typed.
        let shift_insert = swallowed(&[(1.0, key(Key::Insert, false, M::SHIFT))]);
        assert_eq!(shift_insert.is_empty(), !cfg!(target_os = "windows"));
        assert!(swallowed(&[(1.0, key(Key::Insert, false, ctrl | M::SHIFT))]).is_empty());
        assert!(
            swallowed(&[
                (1.0, key(Key::Insert, true, none)),
                (1.1, key(Key::Insert, false, none))
            ])
            .is_empty()
        );
        // A Shift+Insert text paste does not hold back the next Ctrl+V.
        assert_eq!(
            swallowed(&[
                (1.0, Event::Paste("글".into())),
                (1.1, key(Key::Insert, false, M::SHIFT)),
                (2.0, key(Key::V, false, ctrl)),
            ]),
            [2.0]
        );
    }

    #[test]
    fn a_picture_address_is_not_text_to_paste() {
        for text in [
            "",
            " \r\n",
            "https://example.com/a.png",
            "HTTP://x/y",
            "file:///C:/a.png",
            "data:image/png;base64,AAAA",
        ] {
            assert!(only_names_a_picture(text), "{text:?}");
        }
        for text in [
            "그림",
            "https://example.com/a.png 설명",
            "ftp://x/a.png",
            "http",
        ] {
            assert!(!only_names_a_picture(text), "{text:?}");
        }
    }

    fn decoded(picture: Result<Picture, String>) -> Result<image::DynamicImage, String> {
        match picture? {
            Picture::Png(bytes) => {
                image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                    .map_err(|error| error.to_string())
            }
            Picture::Files(_) => Err("files".into()),
        }
    }

    #[test]
    fn a_copied_bitmap_becomes_a_png_of_the_same_pixels() -> Result<(), String> {
        // Opaque: stored as RGB.
        let rgba: Vec<u8> = (0..6u8)
            .flat_map(|i| [i * 40, 255 - i * 40, 7, 255])
            .collect();
        let image = decoded(png(3, 2, rgba))?;
        assert_eq!(image.color(), image::ColorType::Rgb8);
        assert_eq!((image.width(), image.height()), (3, 2));
        assert_eq!(image.to_rgb8().get_pixel(2, 1).0, [200, 55, 7]);
        // Translucent pixels keep their alpha.
        let image = decoded(png(1, 2, vec![1, 2, 3, 128, 4, 5, 6, 255]))?;
        assert_eq!(image.to_rgba8().get_pixel(0, 0).0, [1, 2, 3, 128]);
        // No alpha at all is opaque, not invisible.
        let image = decoded(png(2, 1, vec![9, 8, 7, 0, 6, 5, 4, 0]))?;
        assert_eq!(image.to_rgb8().get_pixel(1, 0).0, [6, 5, 4]);
        assert_eq!(image.color(), image::ColorType::Rgb8);
        Ok(())
    }

    /// The system clipboard as another program left it: `HWPCRAFT_CLIPBOARD_EXPECT`
    /// is `png:<width>x<height>` or `files:<count>`. The test only reads, but
    /// putting a picture there first replaces the user's clipboard (사용자 클립보드를
    /// 바꿈): run it only with the user's permission (허락 후에만).
    #[test]
    #[ignore = "needs a picture put on the system clipboard, which replaces the user's: only with their permission"]
    fn the_system_clipboard_picture() -> Result<(), String> {
        let expect = std::env::var("HWPCRAFT_CLIPBOARD_EXPECT").map_err(|e| e.to_string())?;
        let got = match picture() {
            None => "none".to_owned(),
            Some(Err(error)) => format!("error:{error}"),
            Some(Ok(Picture::Files(files))) => format!("files:{}", files.len()),
            Some(Ok(picture @ Picture::Png(_))) => {
                let image = decoded(Ok(picture))?;
                format!("png:{}x{}", image.width(), image.height())
            }
        };
        assert_eq!(got, expect);
        Ok(())
    }

    #[test]
    fn an_oversized_or_broken_bitmap_says_why() {
        assert!(png(8193, 1, vec![0; 8193 * 4]).is_err());
        assert!(png(5000, 4000, Vec::new()).is_err());
        assert!(png(0, 1, Vec::new()).is_err());
        assert!(png(2, 2, vec![0; 3]).is_err());
    }
}
